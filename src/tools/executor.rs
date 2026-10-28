use crate::context::RequestContext;
use crate::domain::{DomainLimits, validate_domain_security};
use crate::lifecycle::approval::{self, ApprovalId};
use crate::lifecycle::idempotency::{
    IdempotencyKey, IdempotencyRecord, IdempotencyState, IdempotencyStorage,
    InMemoryIdempotencyStorage,
};
use crate::lifecycle::operation::{Operation, OperationExecutor, OperationKind, OperationPayload};
use crate::lifecycle::verification::{self, VerificationStatus};
use crate::odoo::OdooClient;
use crate::risk::RiskEvaluator;
use crate::tools::arguments::{
    CheckAccessArgs, CopyArgs, CreateArgs, CreateBatchArgs, DeleteArgs, DeleteBatchArgs,
    GetViewsArgs, ListModelsArgs, ListReportsArgs, ModelFieldsArgs, NameSearchArgs, OnchangeArgs,
    ReadArgs, ReadGroupArgs, RenderReportArgs, SearchArgs, SearchDomainArgs, SearchReadArgs,
    UpdateArgs, UpdateBatchArgs,
};
use crate::tools::catalog::ToolName;
use crate::tools::context::validate_context;
use crate::tools::fields::{validate_field_count, validate_field_names};
use crate::tools::pagination::{add_total_count, fetch_limit, paginated_result, resolve_limit};
use crate::tools::protection::{QueryLimits, validate_response_record_count};
use crate::tools::records::validate_read_id_count;
use crate::tools::result::ToolExecutionResult;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn validate_idempotency_key(key: Option<&str>) -> Result<(), String> {
    key.map(IdempotencyKey::new)
        .transpose()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn idempotency_storage() -> &'static Mutex<InMemoryIdempotencyStorage> {
    static STORAGE: OnceLock<Mutex<InMemoryIdempotencyStorage>> = OnceLock::new();
    STORAGE.get_or_init(|| Mutex::new(InMemoryIdempotencyStorage::default()))
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

async fn execute_idempotent(
    odoo: &OdooClient,
    operation: &Operation,
    context: &RequestContext,
    key: Option<String>,
) -> Result<Value, AppError> {
    let Some(key) = key else {
        return execute_verified(odoo, operation).await;
    };
    let key = IdempotencyKey::new(key)?;
    {
        let mut storage = idempotency_storage()
            .lock()
            .map_err(|_| AppError::internal("Idempotency storage lock is poisoned"))?;
        if let Some(existing) = storage.get(&key)? {
            existing.ensure_reuse_allowed(operation, context)?;
            match existing.state {
                IdempotencyState::Succeeded => {
                    return existing
                        .stored_result()
                        .cloned()
                        .ok_or_else(|| AppError::internal("Stored idempotency result is missing"));
                }
                IdempotencyState::Pending => {
                    return Err(AppError::authorization(
                        "An identical idempotent request is already in progress",
                    ));
                }
                IdempotencyState::Failed | IdempotencyState::Unknown => {
                    return Err(AppError::internal(
                        "Idempotent request has an unresolved state",
                    ));
                }
                IdempotencyState::Expired => {}
            }
        }
        let record =
            IdempotencyRecord::for_operation(key.clone(), operation, context, now_seconds(), 900)?;
        storage.insert(record)?;
    }
    let result = execute_verified(odoo, operation).await;
    let mut storage = idempotency_storage()
        .lock()
        .map_err(|_| AppError::internal("Idempotency storage lock is poisoned"))?;
    let mut record = storage
        .get(&key)?
        .ok_or_else(|| AppError::internal("Idempotency record disappeared during execution"))?;
    match &result {
        Ok(value) => record.succeed(value.clone())?,
        Err(error) => record.fail(json!({"error": error.to_string()}))?,
    }
    storage.update(record)?;
    result
}

async fn execute_verified(odoo: &OdooClient, operation: &Operation) -> Result<Value, AppError> {
    let response = OperationExecutor::new(odoo).execute(operation).await?;
    let verification = match operation.kind {
        OperationKind::Create | OperationKind::Copy => {
            let id = response.as_i64().ok_or_else(|| {
                AppError::protocol("Mutation did not return a record ID for verification")
            })?;
            verification::verify_create(odoo, operation, id).await?
        }
        OperationKind::Update => verification::verify_update(odoo, operation).await?,
        OperationKind::Delete => verification::verify_delete(odoo, operation).await?,
    };
    if verification.status != VerificationStatus::Verified {
        return Err(AppError::internal(format!(
            "Mutation verification failed: {:?}",
            verification.mismatches
        )));
    }
    Ok(response)
}

fn authorize_mutation(
    operation: &Operation,
    context: &RequestContext,
    approval_id: Option<String>,
    approval_ttl_secs: u64,
) -> Result<(), ToolExecutionResult> {
    if let Some(approval_id) = approval_id {
        let approval_id =
            ApprovalId::try_parse(&approval_id).map_err(ToolExecutionResult::Failure)?;
        approval::consume_approval(approval_id, operation, context)
            .map_err(ToolExecutionResult::Failure)?;
        return Ok(());
    }
    let assessment = RiskEvaluator::default().assess(operation);
    if assessment.level >= crate::risk::RiskLevel::High {
        let ttl = i64::try_from(approval_ttl_secs).unwrap_or(i64::MAX);
        let request = approval::request_approval(operation, context, ttl, now_seconds())
            .map_err(ToolExecutionResult::Failure)?;
        return Err(ToolExecutionResult::ApprovalRequired(json!({
            "status": "approval_required",
            "approval": request,
            "retry_with": {"approval_id": request.id.to_string()}
        })));
    }
    Ok(())
}

async fn execute_mutation(
    odoo: &OdooClient,
    operation: &Operation,
    context: &RequestContext,
    key: Option<String>,
    approval_id: Option<String>,
    approval_ttl_secs: u64,
) -> ToolExecutionResult {
    if let Err(result) = authorize_mutation(operation, context, approval_id, approval_ttl_secs) {
        return result;
    }
    ToolExecutionResult::from_app_error(execute_idempotent(odoo, operation, context, key).await)
}

fn derive_batch_key(key: &Option<String>, index: usize) -> Option<String> {
    key.as_ref().map(|key| format!("{key}:{index}"))
}

fn batch_item_result(index: usize, result: &ToolExecutionResult) -> Value {
    match result {
        ToolExecutionResult::Success(value) => json!({
            "index": index,
            "status": "succeeded",
            "result": value
        }),
        ToolExecutionResult::ApprovalRequired(value) => json!({
            "index": index,
            "status": "approval_required",
            "detail": value
        }),
        ToolExecutionResult::Failure(error) => json!({
            "index": index,
            "status": "failed",
            "error": error.structured_response()
        }),
    }
}

fn batch_result(operation: &str, items: Vec<Value>) -> ToolExecutionResult {
    let succeeded = items
        .iter()
        .filter(|item| item["status"] == "succeeded")
        .count();
    let failed = items.len().saturating_sub(succeeded);
    ToolExecutionResult::Success(json!({
        "batch": {
            "operation": operation,
            "requested": items.len(),
            "succeeded": succeeded,
            "failed": failed,
            "items": items
        }
    }))
}

async fn execute_create_batch(
    odoo: &OdooClient,
    args: CreateBatchArgs,
    context: &RequestContext,
    limits: QueryLimits,
    approval_ttl_secs: u64,
) -> ToolExecutionResult {
    if args.vals_list.is_empty() {
        return ToolExecutionResult::Failure(AppError::input_validation(
            "vals_list must contain at least one record",
        ));
    }
    if args.vals_list.len() > limits.max_batch_size {
        return ToolExecutionResult::Failure(AppError::input_validation(format!(
            "batch size {} exceeds the configured maximum of {}",
            args.vals_list.len(),
            limits.max_batch_size
        )));
    }
    let batch_payload = OperationPayload::new(json!({"vals_list": args.vals_list}))
        .expect("batch create payload is an object");
    let batch_operation = Operation::new(OperationKind::Create, args.model.clone(), batch_payload);
    if let Err(result) = authorize_mutation(
        &batch_operation,
        context,
        args.approval_id,
        approval_ttl_secs,
    ) {
        return result;
    }
    let mut items = Vec::with_capacity(args.vals_list.len());
    for (index, vals) in args.vals_list.into_iter().enumerate() {
        let payload = OperationPayload::new(json!({"vals": vals}))
            .expect("single create payload is an object");
        let operation = Operation::new(OperationKind::Create, args.model.clone(), payload);
        let result = execute_idempotent(
            odoo,
            &operation,
            context,
            derive_batch_key(&args.idempotency_key, index),
        )
        .await;
        items.push(batch_item_result(
            index,
            &ToolExecutionResult::from_app_error(result),
        ));
    }
    batch_result("create", items)
}

async fn execute_update_batch(
    odoo: &OdooClient,
    args: UpdateBatchArgs,
    context: &RequestContext,
    limits: QueryLimits,
    approval_ttl_secs: u64,
) -> ToolExecutionResult {
    if args.records.is_empty() {
        return ToolExecutionResult::Failure(AppError::input_validation(
            "records must contain at least one entry",
        ));
    }
    if args.records.len() > limits.max_batch_size {
        return ToolExecutionResult::Failure(AppError::input_validation(format!(
            "batch size {} exceeds the configured maximum of {}",
            args.records.len(),
            limits.max_batch_size
        )));
    }
    let ids = args
        .records
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    let records = args
        .records
        .iter()
        .map(|record| json!({"id": record.id, "vals": record.vals}))
        .collect::<Vec<_>>();
    let batch_payload = OperationPayload::new(json!({"ids": ids, "records": records}))
        .expect("batch update payload is an object");
    let batch_operation = Operation::new(OperationKind::Update, args.model.clone(), batch_payload);
    if let Err(result) = authorize_mutation(
        &batch_operation,
        context,
        args.approval_id,
        approval_ttl_secs,
    ) {
        return result;
    }
    let mut items = Vec::with_capacity(args.records.len());
    for (index, record) in args.records.into_iter().enumerate() {
        let payload = OperationPayload::new(json!({"ids": [record.id], "vals": record.vals}))
            .expect("single update payload is an object");
        let operation = Operation::new(OperationKind::Update, args.model.clone(), payload);
        let result = execute_idempotent(
            odoo,
            &operation,
            context,
            derive_batch_key(&args.idempotency_key, index),
        )
        .await;
        items.push(batch_item_result(
            index,
            &ToolExecutionResult::from_app_error(result),
        ));
    }
    batch_result("update", items)
}

async fn execute_delete_batch(
    odoo: &OdooClient,
    args: DeleteBatchArgs,
    context: &RequestContext,
    limits: QueryLimits,
    approval_ttl_secs: u64,
) -> ToolExecutionResult {
    if args.ids.is_empty() {
        return ToolExecutionResult::Failure(AppError::input_validation(
            "ids must contain at least one record",
        ));
    }
    if args.ids.len() > limits.max_batch_size {
        return ToolExecutionResult::Failure(AppError::input_validation(format!(
            "batch size {} exceeds the configured maximum of {}",
            args.ids.len(),
            limits.max_batch_size
        )));
    }
    let batch_payload =
        OperationPayload::new(json!({"ids": args.ids})).expect("batch delete payload is an object");
    let batch_operation = Operation::new(OperationKind::Delete, args.model.clone(), batch_payload);
    if let Err(result) = authorize_mutation(
        &batch_operation,
        context,
        args.approval_id,
        approval_ttl_secs,
    ) {
        return result;
    }
    let mut items = Vec::with_capacity(args.ids.len());
    for (index, id) in args.ids.into_iter().enumerate() {
        let payload = OperationPayload::new(json!({"ids": [id]}))
            .expect("single delete payload is an object");
        let operation = Operation::new(OperationKind::Delete, args.model.clone(), payload);
        let result = execute_idempotent(
            odoo,
            &operation,
            context,
            derive_batch_key(&args.idempotency_key, index),
        )
        .await;
        items.push(batch_item_result(
            index,
            &ToolExecutionResult::from_app_error(result),
        ));
    }
    batch_result("delete", items)
}

fn validate_query_domain(domain: &Value, limits: QueryLimits) -> Result<(), String> {
    validate_domain_security(
        domain,
        DomainLimits {
            max_depth: limits.max_domain_depth,
            max_terms: limits.max_domain_terms,
            max_in_values: limits.max_domain_in_values,
        },
    )
}

pub(crate) async fn execute_tool(
    name: ToolName,
    arguments: Value,
    context: &RequestContext,
    odoo: &OdooClient,
    limits: QueryLimits,
    approval_ttl_secs: u64,
) -> ToolExecutionResult {
    let model = arguments
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if name.requires_model() && model.is_empty() {
        return ToolExecutionResult::Failure(AppError::input_validation(
            "Error: Missing required parameter 'model'",
        ));
    }
    let odoo_context = arguments.get("context").and_then(Value::as_object).cloned();
    if let Err(error) = validate_context(odoo_context.as_ref(), limits) {
        return ToolExecutionResult::Failure(AppError::input_validation(format!(
            "Error: Invalid context: {error}"
        )));
    }
    let scoped_odoo = odoo.with_context(odoo_context);
    let odoo = &scoped_odoo;

    match name {
        ToolName::SearchRead => {
            let args: SearchReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("search-read", error),
            };
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("search-read domain", error);
            }
            if let Err(error) = validate_field_count(&args.fields, limits.max_requested_fields) {
                return ToolExecutionResult::invalid_arguments("search-read fields", error);
            }
            if let Err(error) = validate_field_names(&args.fields) {
                return ToolExecutionResult::invalid_arguments("search-read fields", error);
            }
            let limit = match resolve_limit(args.limit, limits.max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search-read limit", error);
                }
            };
            let count_domain = args.include_total.then(|| args.domain.clone());
            let result = odoo
                .search_read(
                    &args.model,
                    args.domain,
                    args.fields,
                    args.offset,
                    fetch_limit(limit),
                )
                .await;
            let mut page = validate_response_record_count(
                paginated_result(result, args.offset, limit),
                limits.max_response_records,
            );
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count(&args.model, domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::SearchCount => {
            let args: SearchDomainArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search-count", error);
                }
            };
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("search-count domain", error);
            }
            ToolExecutionResult::from_app_error(odoo.search_count(&args.model, args.domain).await)
        }
        ToolName::ReadGroup => {
            let args: ReadGroupArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read-group", error),
            };
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("read-group domain", error);
            }
            if let Err(error) = validate_field_count(&args.fields, limits.max_requested_fields) {
                return ToolExecutionResult::invalid_arguments("read-group fields", error);
            }
            if let Err(error) = validate_field_names(&args.fields) {
                return ToolExecutionResult::invalid_arguments("read-group fields", error);
            }
            let result = odoo
                .read_group(&args.model, args.domain, args.fields, args.groupby)
                .await;
            ToolExecutionResult::from_app_error(validate_response_record_count(
                result,
                limits.max_response_records,
            ))
        }
        ToolName::Create => {
            let args: CreateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("create", error),
            };
            if let Err(error) = validate_idempotency_key(args.idempotency_key.as_deref()) {
                return ToolExecutionResult::invalid_arguments("create", error);
            }
            let payload = OperationPayload::new(json!({ "vals": args.vals }))
                .expect("create arguments always form an object payload");
            let operation = Operation::new(OperationKind::Create, args.model, payload);
            execute_mutation(
                odoo,
                &operation,
                context,
                args.idempotency_key,
                args.approval_id,
                approval_ttl_secs,
            )
            .await
        }
        ToolName::Copy => {
            let args: CopyArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("copy", error),
            };
            if let Err(error) = validate_idempotency_key(args.idempotency_key.as_deref()) {
                return ToolExecutionResult::invalid_arguments("copy", error);
            }
            let payload = OperationPayload::new(json!({
                "id": args.id,
                "vals": args.vals,
            }))
            .expect("copy arguments always form an object payload");
            let operation = Operation::new(OperationKind::Copy, args.model, payload);
            execute_mutation(
                odoo,
                &operation,
                context,
                args.idempotency_key,
                args.approval_id,
                approval_ttl_secs,
            )
            .await
        }
        ToolName::Update => {
            let args: UpdateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("update", error),
            };
            if let Err(error) = validate_idempotency_key(args.idempotency_key.as_deref()) {
                return ToolExecutionResult::invalid_arguments("update", error);
            }
            let payload = OperationPayload::new(json!({
                "ids": args.ids,
                "vals": args.vals,
            }))
            .expect("update arguments always form an object payload");
            let operation = Operation::new(OperationKind::Update, args.model, payload);
            execute_mutation(
                odoo,
                &operation,
                context,
                args.idempotency_key,
                args.approval_id,
                approval_ttl_secs,
            )
            .await
        }
        ToolName::Delete => {
            let args: DeleteArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("delete", error),
            };
            if let Err(error) = validate_idempotency_key(args.idempotency_key.as_deref()) {
                return ToolExecutionResult::invalid_arguments("delete", error);
            }
            let payload = OperationPayload::new(json!({ "ids": args.ids }))
                .expect("delete arguments always form an object payload");
            let operation = Operation::new(OperationKind::Delete, args.model, payload);
            execute_mutation(
                odoo,
                &operation,
                context,
                args.idempotency_key,
                args.approval_id,
                approval_ttl_secs,
            )
            .await
        }
        ToolName::GetMetadata => {
            let args: ModelFieldsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("metadata", error),
            };
            if let Err(error) = validate_field_count(&args.fields, limits.max_requested_fields) {
                return ToolExecutionResult::invalid_arguments("metadata fields", error);
            }
            if let Err(error) = validate_field_names(&args.fields) {
                return ToolExecutionResult::invalid_arguments("metadata fields", error);
            }
            ToolExecutionResult::from_app_error(odoo.get_metadata(&args.model, args.fields).await)
        }
        ToolName::Search => {
            let args: SearchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("search", error),
            };
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("search domain", error);
            }
            let limit = match resolve_limit(args.limit, limits.max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search limit", error);
                }
            };
            let count_domain = args.include_total.then(|| args.domain.clone());
            let result = odoo
                .search(&args.model, args.domain, args.offset, fetch_limit(limit))
                .await;
            let mut page = validate_response_record_count(
                paginated_result(result, args.offset, limit),
                limits.max_response_records,
            );
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count(&args.model, domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::Read => {
            let args: ReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read", error),
            };
            if let Err(error) = validate_read_id_count(&args.ids, limits.max_read_ids) {
                return ToolExecutionResult::invalid_arguments("read IDs", error);
            }
            if let Err(error) = validate_field_count(&args.fields, limits.max_requested_fields) {
                return ToolExecutionResult::invalid_arguments("read fields", error);
            }
            if let Err(error) = validate_field_names(&args.fields) {
                return ToolExecutionResult::invalid_arguments("read fields", error);
            }
            let result = odoo.read(&args.model, args.ids, args.fields).await;
            ToolExecutionResult::from_app_error(validate_response_record_count(
                result,
                limits.max_response_records,
            ))
        }
        ToolName::GetServerInfo => ToolExecutionResult::from_app_error(odoo.server_info().await),
        ToolName::ListModels => {
            let args: ListModelsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("list-models", error),
            };
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("list-models domain", error);
            }
            let limit = match resolve_limit(args.limit, limits.max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("list-models limit", error);
                }
            };
            let count_domain = args.include_total.then(|| args.domain.clone());
            let result = odoo
                .list_models(args.domain, args.offset, fetch_limit(limit))
                .await;
            let mut page = validate_response_record_count(
                paginated_result(result, args.offset, limit),
                limits.max_response_records,
            );
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count("ir.model", domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::CheckAccess => {
            let args: CheckAccessArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("check-access", error),
            };
            if !matches!(
                args.operation.as_str(),
                "read" | "write" | "create" | "unlink"
            ) {
                return ToolExecutionResult::Failure(AppError::input_validation(
                    "operation must be one of read, write, create, or unlink",
                ));
            }
            let ids = args.ids.unwrap_or_default();
            if let Err(error) = validate_read_id_count(&ids, limits.max_read_ids) {
                return ToolExecutionResult::invalid_arguments("check-access IDs", error);
            }
            let result = odoo
                .check_access(&args.model, &args.operation, ids.clone())
                .await;
            match result {
                Ok(mut value) => {
                    value["model"] = Value::String(args.model);
                    value["operation"] = Value::String(args.operation);
                    value["checked_ids"] = json!(ids);
                    ToolExecutionResult::Success(value)
                }
                Err(error) => ToolExecutionResult::Failure(error),
            }
        }
        ToolName::Onchange => {
            let args: OnchangeArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("onchange", error),
            };
            ToolExecutionResult::from_app_error(
                odoo.onchange(
                    &args.model,
                    Value::Object(args.values),
                    json!(args.field_names),
                    Value::Object(args.fields_spec),
                )
                .await,
            )
        }
        ToolName::NameSearch => {
            let args: NameSearchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("name-search", error),
            };
            if args.name.trim().is_empty() {
                return ToolExecutionResult::Failure(AppError::input_validation(
                    "name must not be empty",
                ));
            }
            if let Err(error) = validate_query_domain(&args.domain, limits) {
                return ToolExecutionResult::invalid_arguments("name-search domain", error);
            }
            let limit = match resolve_limit(args.limit, limits.max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("name-search limit", error);
                }
            };
            ToolExecutionResult::from_app_error(
                odoo.name_search(
                    &args.model,
                    &args.name,
                    args.domain,
                    args.operator.as_deref(),
                    limit,
                )
                .await,
            )
        }
        ToolName::GetViews => {
            let args: GetViewsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("get-views", error),
            };
            if args.views.as_array().is_none_or(|views| views.is_empty()) {
                return ToolExecutionResult::Failure(AppError::input_validation(
                    "views must contain at least one view description",
                ));
            }
            ToolExecutionResult::from_app_error(
                odoo.get_views(&args.model, args.views, args.options).await,
            )
        }
        ToolName::ListReports => {
            let args: ListReportsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("list-reports", error);
                }
            };
            let mut domain = args.domain;
            if let Some(model) = args.model {
                let clause = json!(["model", "=", model]);
                if let Some(clauses) = domain.as_array_mut() {
                    clauses.push(clause);
                } else {
                    domain = json!([clause]);
                }
            }
            if let Err(error) = validate_query_domain(&domain, limits) {
                return ToolExecutionResult::invalid_arguments("list-reports domain", error);
            }
            let limit = match resolve_limit(args.limit, limits.max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("list-reports limit", error);
                }
            };
            let count_domain = args.include_total.then(|| domain.clone());
            let result = odoo
                .list_reports(domain, args.offset, fetch_limit(limit))
                .await;
            let mut page = validate_response_record_count(
                paginated_result(result, args.offset, limit),
                limits.max_response_records,
            );
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count("ir.actions.report", domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::RenderReport => {
            let args: RenderReportArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("render-report", error);
                }
            };
            if let Err(error) = validate_read_id_count(&args.ids, limits.max_read_ids) {
                return ToolExecutionResult::invalid_arguments("render-report IDs", error);
            }
            ToolExecutionResult::from_app_error(
                odoo.render_report(
                    &args.report_name,
                    &args.ids,
                    &args.format.to_lowercase(),
                    args.data.as_ref(),
                )
                .await,
            )
        }
        ToolName::CreateBatch => {
            let args: CreateBatchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("create-batch", error);
                }
            };
            execute_create_batch(odoo, args, context, limits, approval_ttl_secs).await
        }
        ToolName::UpdateBatch => {
            let args: UpdateBatchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("update-batch", error);
                }
            };
            execute_update_batch(odoo, args, context, limits, approval_ttl_secs).await
        }
        ToolName::DeleteBatch => {
            let args: DeleteBatchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("delete-batch", error);
                }
            };
            execute_delete_batch(odoo, args, context, limits, approval_ttl_secs).await
        }
        ToolName::ListInstances => ToolExecutionResult::Failure(AppError::internal(
            "odoo-list-instances must be handled before the Odoo client is resolved",
        )),
    }
}
use crate::error::AppError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockOdooServer, authentication_success, json_rpc_success};
    use serde_json::json;

    const TEST_QUERY_LIMITS: QueryLimits = QueryLimits {
        max_query_limit: 1_000,
        max_requested_fields: 100,
        max_read_ids: 100,
        max_domain_depth: 8,
        max_domain_terms: 100,
        max_response_records: 1_000,
        max_domain_in_values: 1_000,
        max_context_keys: 32,
        max_context_bytes: 8_192,
        max_batch_size: 100,
    };

    #[tokio::test]
    async fn search_pagination_contract_includes_has_more_without_count() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([10, 11, 12])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::Search,
            json!({"model": "res.partner", "limit": 2}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::Success(response) = result else {
            panic!("search should return a paginated response");
        };

        assert_eq!(response["items"], json!([10, 11]));
        assert_eq!(response["pagination"]["has_more"], true);
        assert!(response["pagination"].get("total").is_none());
        let requests = server.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["params"]["args"][6]["limit"], 3);
    }

    #[tokio::test]
    async fn total_count_is_opt_in_and_uses_the_same_domain() {
        let domain = json!([["active", "=", true]]);
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([10])),
            json_rpc_success(json!(25)),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::Search,
            json!({
                "model": "res.partner",
                "domain": domain,
                "include_total": true
            }),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::Success(response) = result else {
            panic!("search should return a paginated response");
        };

        assert_eq!(response["pagination"]["total"], 25);
        let requests = server.requests().await;
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[1]["params"]["args"][5][0], domain);
        assert_eq!(requests[2]["params"]["args"][5][0], domain);
    }

    #[tokio::test]
    async fn list_models_uses_ir_model_with_pagination() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([{"id": 1, "model": "res.partner"}])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::ListModels,
            json!({"limit": 2}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        assert!(
            matches!(result, ToolExecutionResult::Success(_)),
            "{result:?}"
        );
        let requests = server.requests().await;
        assert_eq!(requests[1]["params"]["args"][3], "ir.model");
        assert_eq!(requests[1]["params"]["args"][4], "search_read");
        assert_eq!(requests[1]["params"]["args"][6]["limit"], 3);
    }

    #[tokio::test]
    async fn onchange_and_access_checks_use_public_odoo_methods() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!({"value": {"name": "A"}})),
            json_rpc_success(json!(true)),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let onchange = execute_tool(
            ToolName::Onchange,
            json!({
                "model": "res.partner",
                "values": {"name": "A"},
                "field_names": ["name"],
                "fields_spec": {"name": {}}
            }),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(onchange, ToolExecutionResult::Success(_)));

        let access = execute_tool(
            ToolName::CheckAccess,
            json!({"model": "res.partner", "operation": "read"}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(access, ToolExecutionResult::Success(value) if value["allowed"] == true));

        let requests = server.requests().await;
        assert_eq!(requests[1]["params"]["args"][4], "onchange");
        assert_eq!(requests[2]["params"]["args"][4], "has_access");
    }

    #[test]
    fn batch_results_report_per_item_status() {
        let result = batch_result(
            "update",
            vec![
                json!({"index": 0, "status": "succeeded", "result": true}),
                json!({"index": 1, "status": "failed", "error": {"code": "ODOO_VALIDATION_ERROR"}}),
            ],
        );

        let ToolExecutionResult::Success(value) = result else {
            panic!("batch results are successful envelopes");
        };
        assert_eq!(value["batch"]["requested"], 2);
        assert_eq!(value["batch"]["succeeded"], 1);
        assert_eq!(value["batch"]["failed"], 1);
        assert_eq!(value["batch"]["items"][1]["status"], "failed");
    }

    #[tokio::test]
    async fn delete_batch_requires_approval_then_reports_each_record() {
        use crate::lifecycle::approval::{ApprovalId, approve_approval};

        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!(true)),
            json_rpc_success(json!([])),
            json_rpc_success(json!(true)),
            json_rpc_success(json!([])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let arguments = json!({"model": "res.partner", "ids": [9301, 9302]});
        let pending = execute_tool(
            ToolName::DeleteBatch,
            arguments.clone(),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::ApprovalRequired(value) = pending else {
            panic!("delete batch must require approval");
        };
        assert_eq!(server.requests().await.len(), 1);
        let approval_id = value["approval"]["id"].as_str().unwrap().to_string();
        approve_approval(ApprovalId::try_parse(&approval_id).unwrap()).unwrap();
        let mut approved = arguments;
        approved["approval_id"] = json!(approval_id);

        let result = execute_tool(
            ToolName::DeleteBatch,
            approved,
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        let ToolExecutionResult::Success(value) = result else {
            panic!("approved batch must execute");
        };
        assert_eq!(value["batch"]["requested"], 2);
        assert_eq!(value["batch"]["succeeded"], 2);
        assert_eq!(value["batch"]["failed"], 0);
    }

    #[tokio::test]
    async fn malformed_domains_never_reach_odoo() {
        let server = MockOdooServer::start(authentication_success(7)).await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let calls = [
            (
                ToolName::SearchRead,
                json!({"model": "res.partner", "domain": ["name", "=", "Alpha"], "fields": ["id"]}),
            ),
            (
                ToolName::SearchCount,
                json!({"model": "res.partner", "domain": ["name", "=", "Alpha"]}),
            ),
            (
                ToolName::ReadGroup,
                json!({"model": "res.partner", "domain": ["name", "=", "Alpha"], "fields": ["id"], "groupby": []}),
            ),
            (
                ToolName::Search,
                json!({"model": "res.partner", "domain": ["name", "=", "Alpha"]}),
            ),
        ];

        for (tool, arguments) in calls {
            let result = execute_tool(
                tool,
                arguments,
                &RequestContext::new(),
                &client,
                TEST_QUERY_LIMITS,
                900,
            )
            .await;
            let ToolExecutionResult::Failure(AppError::InputValidation { .. }) = result else {
                panic!("malformed domain must be rejected as invalid input");
            };
        }

        assert_eq!(server.requests().await.len(), 1);
    }

    #[tokio::test]
    async fn create_runs_through_the_operation_executor() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!({"name": {"readonly": false}})),
            json_rpc_success(json!(42)),
            json_rpc_success(json!([{"id": 42}])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::Create,
            json!({"model": "res.partner", "vals": {"name": "Alpha"}}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        assert!(matches!(result, ToolExecutionResult::Success(value) if value == json!(42)));
        let requests = server.requests().await;
        assert_eq!(requests[2]["params"]["args"][3], "res.partner");
        assert_eq!(requests[2]["params"]["args"][4], "create");
        assert_eq!(
            requests[2]["params"]["args"][5][0],
            json!({"name": "Alpha"})
        );
    }

    #[tokio::test]
    async fn rejected_approval_never_executes_the_mutation() {
        use crate::lifecycle::approval::{ApprovalId, reject_approval};

        let server = MockOdooServer::start(authentication_success(7)).await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let arguments = json!({"model": "res.partner", "ids": [9101, 9102]});
        let pending = execute_tool(
            ToolName::Delete,
            arguments.clone(),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::ApprovalRequired(value) = pending else {
            panic!("delete must require approval");
        };
        let approval_id = value["approval"]["id"].as_str().unwrap().to_string();
        reject_approval(ApprovalId::try_parse(&approval_id).unwrap()).unwrap();
        let mut approved_arguments = arguments;
        approved_arguments["approval_id"] = json!(approval_id);

        let result = execute_tool(
            ToolName::Delete,
            approved_arguments,
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        assert!(matches!(
            result,
            ToolExecutionResult::Failure(AppError::Authorization { .. })
        ));
        assert_eq!(server.requests().await.len(), 1);
    }

    #[tokio::test]
    async fn approval_is_bound_to_the_requesting_actor() {
        use crate::context::{ActorIdentity, AgentIdentity, ClientIdentity, IdentitySource};
        use crate::lifecycle::approval::{ApprovalId, approve_approval};

        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!(true)),
            json_rpc_success(json!([])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let owner = RequestContext::identified(
            ClientIdentity::default(),
            AgentIdentity::default(),
            ActorIdentity::claimed(
                Some("owner".to_string()),
                None,
                IdentitySource::RequestMetadata,
            ),
            "test-instance".to_string(),
        );
        let other = RequestContext::identified(
            ClientIdentity::default(),
            AgentIdentity::default(),
            ActorIdentity::claimed(
                Some("other".to_string()),
                None,
                IdentitySource::RequestMetadata,
            ),
            "test-instance".to_string(),
        );
        let arguments = json!({"model": "res.partner", "ids": [9201, 9202]});
        let pending = execute_tool(
            ToolName::Delete,
            arguments.clone(),
            &owner,
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::ApprovalRequired(value) = pending else {
            panic!("delete must require approval");
        };
        let approval_id = value["approval"]["id"].as_str().unwrap().to_string();
        approve_approval(ApprovalId::try_parse(&approval_id).unwrap()).unwrap();
        let mut approved_arguments = arguments;
        approved_arguments["approval_id"] = json!(approval_id);

        let denied = execute_tool(
            ToolName::Delete,
            approved_arguments.clone(),
            &other,
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(
            denied,
            ToolExecutionResult::Failure(AppError::Authorization { .. })
        ));
        assert_eq!(server.requests().await.len(), 1);

        let allowed = execute_tool(
            ToolName::Delete,
            approved_arguments,
            &owner,
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(allowed, ToolExecutionResult::Success(value) if value == json!(true)));
    }

    #[tokio::test]
    async fn update_runs_through_the_operation_executor() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!({"active": {"readonly": false}})),
            json_rpc_success(json!(true)),
            json_rpc_success(json!([{"id": 7, "active": false}, {"id": 8, "active": false}])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::Update,
            json!({"model": "res.partner", "ids": [7, 8], "vals": {"active": false}}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        assert!(matches!(result, ToolExecutionResult::Success(value) if value == json!(true)));
        let requests = server.requests().await;
        assert_eq!(requests[2]["params"]["args"][3], "res.partner");
        assert_eq!(requests[2]["params"]["args"][4], "write");
        assert_eq!(requests[2]["params"]["args"][5][0], json!([7, 8]));
        assert_eq!(
            requests[2]["params"]["args"][5][1],
            json!({"active": false})
        );
    }

    #[tokio::test]
    async fn high_risk_delete_requires_single_use_approval_before_rpc() {
        use crate::lifecycle::approval::{ApprovalId, approve_approval};

        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!(true)),
            json_rpc_success(json!([])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let arguments = json!({"model": "res.partner", "ids": [9001, 9002]});

        let pending = execute_tool(
            ToolName::Delete,
            arguments.clone(),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        let ToolExecutionResult::ApprovalRequired(value) = pending else {
            panic!("delete must require approval");
        };
        assert_eq!(value["status"], "approval_required");
        assert_eq!(server.requests().await.len(), 1, "no mutation RPC may run");

        let approval_id = value["approval"]["id"].as_str().unwrap().to_string();
        approve_approval(ApprovalId::try_parse(&approval_id).unwrap()).unwrap();
        let mut approved_arguments = arguments.clone();
        approved_arguments["approval_id"] = json!(approval_id);

        let result = execute_tool(
            ToolName::Delete,
            approved_arguments.clone(),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(result, ToolExecutionResult::Success(value) if value == json!(true)));
        let requests = server.requests().await;
        assert_eq!(requests[1]["params"]["args"][3], "res.partner");
        assert_eq!(requests[1]["params"]["args"][4], "unlink");
        assert_eq!(requests[1]["params"]["args"][5][0], json!([9001, 9002]));

        let replay = execute_tool(
            ToolName::Delete,
            approved_arguments,
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;
        assert!(matches!(
            replay,
            ToolExecutionResult::Failure(AppError::Authorization { .. })
        ));
    }

    #[tokio::test]
    async fn copy_runs_through_the_operation_executor() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!({"name": {"readonly": false}})),
            json_rpc_success(json!(84)),
            json_rpc_success(json!([{"id": 84}])),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        let result = execute_tool(
            ToolName::Copy,
            json!({"model": "res.partner", "id": 42, "vals": {"name": "Alpha copy"}}),
            &RequestContext::new(),
            &client,
            TEST_QUERY_LIMITS,
            900,
        )
        .await;

        assert!(matches!(result, ToolExecutionResult::Success(value) if value == json!(84)));
        let requests = server.requests().await;
        assert_eq!(requests[2]["params"]["args"][3], "res.partner");
        assert_eq!(requests[2]["params"]["args"][4], "copy");
        assert_eq!(requests[2]["params"]["args"][5][0], 42);
        assert_eq!(
            requests[2]["params"]["args"][5][1],
            json!({"name": "Alpha copy"})
        );
    }
}
