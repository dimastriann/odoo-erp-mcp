use crate::error::AppError;
use crate::odoo::OdooClient;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AggregateFunction {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    CountDistinct,
}

impl AggregateFunction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::Count => "count",
            Self::CountDistinct => "count_distinct",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Metric {
    pub(crate) field: String,
    pub(crate) function: AggregateFunction,
}

impl Metric {
    pub(crate) fn canonical(&self) -> String {
        format!("{}:{}", self.field, self.function.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GroupBy {
    pub(crate) field: String,
    pub(crate) granularity: Option<&'static str>,
}

impl GroupBy {
    pub(crate) fn canonical(&self) -> String {
        match self.granularity {
            Some(granularity) => format!("{}:{granularity}", self.field),
            None => self.field.clone(),
        }
    }
}

pub(crate) struct BusinessReport {
    pub(crate) name: &'static str,
    pub(crate) model: &'static str,
    pub(crate) description: &'static str,
    pub(crate) date_field: Option<&'static str>,
    pub(crate) groupby: &'static [&'static str],
    pub(crate) fields: &'static [&'static str],
    pub(crate) orderby: Option<&'static str>,
    pub(crate) limit: usize,
    pub(crate) domain: fn() -> Value,
}

fn sales_domain() -> Value {
    json!([["state", "in", ["sale", "done"]]])
}

fn pending_sales_domain() -> Value {
    json!([["state", "in", ["draft", "sent", "sale"]]])
}

fn crm_domain() -> Value {
    json!([["type", "=", "opportunity"]])
}

fn stock_domain() -> Value {
    json!([["location_id.usage", "=", "internal"]])
}

fn receivable_domain() -> Value {
    json!([
        ["account_id.account_type", "=", "asset_receivable"],
        ["reconciled", "=", false]
    ])
}

fn payable_domain() -> Value {
    json!([
        ["account_id.account_type", "=", "liability_payable"],
        ["reconciled", "=", false]
    ])
}

fn purchase_domain() -> Value {
    json!([["state", "in", ["draft", "sent", "to approve"]]])
}

fn deliveries_domain() -> Value {
    json!([
        ["picking_type_code", "=", "outgoing"],
        ["state", "in", ["confirmed", "assigned", "waiting"]]
    ])
}

fn crm_leads_domain() -> Value {
    json!([["type", "=", "lead"]])
}

fn activity_domain() -> Value {
    json!([["res_model", "=", "res.partner"]])
}

fn overdue_invoice_domain() -> Value {
    json!([
        ["move_type", "=", "out_invoice"],
        ["payment_state", "in", ["not_paid", "partial"]]
    ])
}

fn cash_flow_domain() -> Value {
    json!([[
        "account_id.account_type",
        "in",
        ["asset_cash", "liability_credit_card"]
    ]])
}

fn supplier_summary_domain() -> Value {
    json!([["state", "in", ["purchase", "done"]]])
}

fn supplier_price_domain() -> Value {
    json!([])
}

fn low_stock_domain() -> Value {
    json!([["qty_to_order", ">", 0]])
}

fn crm_conversion_domain() -> Value {
    json!([["type", "in", ["lead", "opportunity"]]])
}

pub(crate) const REPORTS: &[BusinessReport] = &[
    BusinessReport {
        name: "sales_summary",
        model: "sale.order",
        description: "Sales order totals grouped by order month",
        date_field: Some("date_order"),
        groupby: &["date_order:month"],
        fields: &["amount_total:sum", "id:count"],
        orderby: Some("date_order:month desc"),
        limit: 24,
        domain: sales_domain,
    },
    BusinessReport {
        name: "sales_revenue",
        model: "sale.order",
        description: "Total confirmed sales revenue",
        date_field: Some("date_order"),
        groupby: &[],
        fields: &["amount_total:sum", "id:count"],
        orderby: None,
        limit: 1,
        domain: sales_domain,
    },
    BusinessReport {
        name: "pending_sales_orders",
        model: "sale.order",
        description: "Open and pending sales orders grouped by state",
        date_field: Some("date_order"),
        groupby: &["state"],
        fields: &["amount_total:sum", "id:count"],
        orderby: None,
        limit: 20,
        domain: pending_sales_domain,
    },
    BusinessReport {
        name: "top_customers",
        model: "sale.order",
        description: "Customers ranked by confirmed sales revenue",
        date_field: Some("date_order"),
        groupby: &["partner_id"],
        fields: &["amount_total:sum"],
        orderby: Some("amount_total desc"),
        limit: 10,
        domain: sales_domain,
    },
    BusinessReport {
        name: "crm_pipeline",
        model: "crm.lead",
        description: "CRM opportunities grouped by stage",
        date_field: Some("create_date"),
        groupby: &["stage_id"],
        fields: &["expected_revenue:sum", "id:count"],
        orderby: None,
        limit: 50,
        domain: crm_domain,
    },
    BusinessReport {
        name: "inventory_summary",
        model: "stock.quant",
        description: "Internal stock quantities grouped by product",
        date_field: None,
        groupby: &["product_id"],
        fields: &["quantity:sum"],
        orderby: Some("quantity desc"),
        limit: 100,
        domain: stock_domain,
    },
    BusinessReport {
        name: "account_receivable",
        model: "account.move.line",
        description: "Outstanding receivable balances grouped by partner",
        date_field: Some("date"),
        groupby: &["partner_id"],
        fields: &["amount_residual:sum"],
        orderby: Some("amount_residual desc"),
        limit: 100,
        domain: receivable_domain,
    },
    BusinessReport {
        name: "account_payable",
        model: "account.move.line",
        description: "Outstanding payable balances grouped by partner",
        date_field: Some("date"),
        groupby: &["partner_id"],
        fields: &["amount_residual:sum"],
        orderby: Some("amount_residual desc"),
        limit: 100,
        domain: payable_domain,
    },
    BusinessReport {
        name: "inactive_customers",
        model: "sale.order",
        description: "Customers whose last confirmed order is before the supplied cutoff",
        date_field: Some("date_order"),
        groupby: &["partner_id"],
        fields: &["date_order:max"],
        orderby: Some("date_order asc"),
        limit: 100,
        domain: sales_domain,
    },
    BusinessReport {
        name: "current_stock",
        model: "stock.quant",
        description: "Current internal stock quantities grouped by product",
        date_field: None,
        groupby: &["product_id"],
        fields: &["quantity:sum"],
        orderby: Some("quantity desc"),
        limit: 100,
        domain: stock_domain,
    },
    BusinessReport {
        name: "low_stock",
        model: "stock.warehouse.orderpoint",
        description: "Products with a positive quantity to reorder",
        date_field: None,
        groupby: &["product_id"],
        fields: &["qty_to_order:sum"],
        orderby: Some("qty_to_order desc"),
        limit: 100,
        domain: low_stock_domain,
    },
    BusinessReport {
        name: "reserved_stock",
        model: "stock.quant",
        description: "Reserved internal stock quantities grouped by product",
        date_field: None,
        groupby: &["product_id"],
        fields: &["reserved_quantity:sum"],
        orderby: Some("reserved_quantity desc"),
        limit: 100,
        domain: stock_domain,
    },
    BusinessReport {
        name: "pending_deliveries",
        model: "stock.picking",
        description: "Pending outgoing deliveries grouped by state",
        date_field: Some("scheduled_date"),
        groupby: &["state"],
        fields: &["id:count"],
        orderby: None,
        limit: 20,
        domain: deliveries_domain,
    },
    BusinessReport {
        name: "crm_leads",
        model: "crm.lead",
        description: "Leads grouped by stage",
        date_field: Some("create_date"),
        groupby: &["stage_id"],
        fields: &["id:count"],
        orderby: None,
        limit: 50,
        domain: crm_leads_domain,
    },
    BusinessReport {
        name: "crm_conversion",
        model: "crm.lead",
        description: "Lead-to-opportunity conversion rate",
        date_field: Some("create_date"),
        groupby: &["type"],
        fields: &["id:count"],
        orderby: None,
        limit: 2,
        domain: crm_conversion_domain,
    },
    BusinessReport {
        name: "customer_activity",
        model: "mail.activity",
        description: "Partner activities grouped by activity type",
        date_field: Some("date_deadline"),
        groupby: &["activity_type_id"],
        fields: &["id:count"],
        orderby: None,
        limit: 50,
        domain: activity_domain,
    },
    BusinessReport {
        name: "overdue_invoices",
        model: "account.move",
        description: "Open customer invoices grouped by partner",
        date_field: Some("invoice_date_due"),
        groupby: &["partner_id"],
        fields: &["amount_residual:sum"],
        orderby: Some("amount_residual desc"),
        limit: 100,
        domain: overdue_invoice_domain,
    },
    BusinessReport {
        name: "cash_flow",
        model: "account.move.line",
        description: "Cash and bank balances grouped by month",
        date_field: Some("date"),
        groupby: &["date:month"],
        fields: &["balance:sum"],
        orderby: Some("date desc"),
        limit: 24,
        domain: cash_flow_domain,
    },
    BusinessReport {
        name: "supplier_summary",
        model: "purchase.order",
        description: "Suppliers ranked by confirmed purchase spend",
        date_field: Some("date_order"),
        groupby: &["partner_id"],
        fields: &["amount_total:sum"],
        orderby: Some("amount_total desc"),
        limit: 100,
        domain: supplier_summary_domain,
    },
    BusinessReport {
        name: "supplier_prices",
        model: "product.supplierinfo",
        description: "Supplier price ranges grouped by supplier",
        date_field: Some("date_start"),
        groupby: &["partner_id"],
        fields: &["price:avg", "price:min", "price:max"],
        orderby: Some("price desc"),
        limit: 100,
        domain: supplier_price_domain,
    },
    BusinessReport {
        name: "purchase_pending",
        model: "purchase.order",
        description: "Pending purchase orders grouped by state",
        date_field: Some("date_order"),
        groupby: &["state"],
        fields: &["amount_total:sum", "id:count"],
        orderby: None,
        limit: 20,
        domain: purchase_domain,
    },
];

fn currency_from_companies(companies: &Value) -> Option<Value> {
    companies
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("currency_id"))
        .cloned()
}

fn transform_report_rows(name: &str, rows: &[Value]) -> Result<Value, AppError> {
    match name {
        "crm_conversion" => {
            let mut leads = 0;
            let mut opportunities = 0;
            for row in rows {
                let count = ["id_count", "__count", "type_count"]
                    .iter()
                    .find_map(|key| row.get(*key).and_then(Value::as_u64))
                    .unwrap_or(0);
                match row.get("type").and_then(Value::as_str) {
                    Some("lead") => leads += count,
                    Some("opportunity") => opportunities += count,
                    _ => {}
                }
            }
            let total = leads + opportunities;
            let conversion_rate = if total == 0 {
                0.0
            } else {
                opportunities as f64 / total as f64
            };
            Ok(json!([{
                "leads": leads,
                "opportunities": opportunities,
                "total": total,
                "conversion_rate": conversion_rate
            }]))
        }
        _ => Ok(Value::Array(rows.to_vec())),
    }
}

pub(crate) fn find(name: &str) -> Option<&'static BusinessReport> {
    REPORTS.iter().find(|report| report.name == name)
}

pub(crate) fn definitions() -> Value {
    Value::Array(
        REPORTS
            .iter()
            .map(|report| {
                json!({
                    "name": report.name,
                    "model": report.model,
                    "description": report.description,
                    "date_field": report.date_field,
                    "groupby": report.groupby,
                    "fields": report.fields
                })
            })
            .collect(),
    )
}

pub(crate) fn validate_date(value: &str) -> Result<(), AppError> {
    let bytes = value.as_bytes();
    let valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(AppError::input_validation(
            "date must use the YYYY-MM-DD format",
        ))
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.'))
}

pub(crate) fn parse_metric(raw: &str) -> Result<Metric, AppError> {
    let (field, function) = raw.rsplit_once(':').ok_or_else(|| {
        AppError::input_validation(format!("metric {raw:?} must use field:aggregate syntax"))
    })?;
    if !valid_identifier(field) {
        return Err(AppError::input_validation(format!(
            "invalid metric field {field:?}"
        )));
    }
    let function = match function {
        "sum" => AggregateFunction::Sum,
        "avg" => AggregateFunction::Avg,
        "min" => AggregateFunction::Min,
        "max" => AggregateFunction::Max,
        "count" => AggregateFunction::Count,
        "count_distinct" => AggregateFunction::CountDistinct,
        other => {
            return Err(AppError::input_validation(format!(
                "unsupported aggregate function {other:?}"
            )));
        }
    };
    Ok(Metric {
        field: field.to_string(),
        function,
    })
}

pub(crate) fn parse_groupby(raw: &str) -> Result<GroupBy, AppError> {
    let (field, granularity) = match raw.rsplit_once(':') {
        Some((field, granularity)) => (field, Some(granularity)),
        None => (raw, None),
    };
    if !valid_identifier(field) {
        return Err(AppError::input_validation(format!(
            "invalid groupby field {field:?}"
        )));
    }
    let granularity = match granularity {
        None => None,
        Some("day") => Some("day"),
        Some("week") => Some("week"),
        Some("month") => Some("month"),
        Some("quarter") => Some("quarter"),
        Some("year") => Some("year"),
        Some(other) => {
            return Err(AppError::input_validation(format!(
                "unsupported groupby granularity {other:?}"
            )));
        }
    };
    Ok(GroupBy {
        field: field.to_string(),
        granularity,
    })
}

pub(crate) async fn run(
    odoo: &OdooClient,
    name: &str,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: Option<u64>,
    max_limit: u64,
    company_id: Option<i64>,
) -> Result<Value, AppError> {
    let report = find(name)
        .ok_or_else(|| AppError::input_validation(format!("unknown business report {name:?}")))?;
    let mut domain = (report.domain)();
    if let Some(date_field) = report.date_field {
        let clauses = domain
            .as_array_mut()
            .ok_or_else(|| AppError::internal("business report domain must be an array"))?;
        if let Some(date_from) = date_from {
            validate_date(date_from)?;
            clauses.push(json!([date_field, ">=", date_from]));
        }
        if let Some(date_to) = date_to {
            validate_date(date_to)?;
            clauses.push(json!([date_field, "<=", date_to]));
        }
    }
    let limit = limit.unwrap_or(report.limit as u64).min(max_limit).max(1);
    let fields = report
        .fields
        .iter()
        .map(|field| parse_metric(field).map(|metric| metric.canonical()))
        .collect::<Result<Vec<_>, _>>()?;
    let groupby = report
        .groupby
        .iter()
        .map(|field| parse_groupby(field).map(|groupby| groupby.canonical()))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = odoo
        .read_group(
            report.model,
            domain.clone(),
            json!(fields),
            json!(groupby),
            None,
            limit,
            report.orderby,
        )
        .await?;
    let rows = rows
        .as_array()
        .ok_or_else(|| AppError::protocol("Odoo read_group returned a non-array result"))?;
    let rows = transform_report_rows(report.name, rows)?;
    let currency = match company_id {
        Some(company_id) => {
            let companies = odoo
                .read("res.company", vec![company_id], json!(["currency_id"]))
                .await?;
            currency_from_companies(&companies)
        }
        None => None,
    };
    Ok(json!({
        "report": report.name,
        "model": report.model,
        "description": report.description,
        "domain": domain,
        "groupby": groupby,
        "fields": fields,
        "limit": limit,
        "company_id": company_id,
        "currency": currency,
        "rows": rows
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_have_stable_names_and_targets() {
        assert!(find("sales_summary").is_some());
        assert!(find("account_receivable").is_some());
        assert!(find("unknown").is_none());
        assert_eq!(definitions().as_array().unwrap().len(), REPORTS.len());
    }

    #[test]
    fn company_currency_is_extracted_from_read_results() {
        let companies = json!([{"id": 1, "currency_id": [2, "USD"]}]);
        assert_eq!(currency_from_companies(&companies), Some(json!([2, "USD"])));
        assert!(currency_from_companies(&json!([])).is_none());
    }

    #[test]
    fn crm_conversion_is_derived_from_group_counts() {
        let rows = vec![
            json!({"type": "lead", "id_count": 10}),
            json!({"type": "opportunity", "id_count": 5}),
        ];
        let converted = transform_report_rows("crm_conversion", &rows).unwrap();
        assert_eq!(converted[0]["leads"], 10);
        assert_eq!(converted[0]["opportunities"], 5);
        assert_eq!(converted[0]["total"], 15);
        assert!((converted[0]["conversion_rate"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn typed_metrics_and_groupbys_are_validated() {
        assert_eq!(
            parse_metric("amount_total:sum").unwrap().canonical(),
            "amount_total:sum"
        );
        assert_eq!(
            parse_groupby("date_order:month").unwrap().canonical(),
            "date_order:month"
        );
        assert!(parse_metric("amount_total").is_err());
        assert!(parse_metric("amount_total:median").is_err());
        assert!(parse_metric("amount_total;sum").is_err());
        assert!(parse_groupby("date_order:century").is_err());
    }

    #[test]
    fn dates_must_use_iso_format() {
        assert!(validate_date("2026-11-14").is_ok());
        assert!(validate_date("14-11-2026").is_err());
        assert!(validate_date("2026-1-4").is_err());
    }
}
