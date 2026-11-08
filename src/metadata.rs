use crate::error::AppError;
use crate::odoo::OdooClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct EnumValue {
    pub(crate) value: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct FieldSchema {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) string: Option<String>,
    pub(crate) required: bool,
    pub(crate) readonly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) relation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) selection: Vec<EnumValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) compute: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RelationSchema {
    pub(crate) field: String,
    pub(crate) relation: String,
    #[serde(rename = "type")]
    pub(crate) relation_type: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ModelSchema {
    pub(crate) model: String,
    pub(crate) fields: BTreeMap<String, FieldSchema>,
    pub(crate) required_fields: Vec<String>,
    pub(crate) relations: Vec<RelationSchema>,
    pub(crate) enums: BTreeMap<String, Vec<EnumValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) access: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RelationStep {
    pub(crate) from: String,
    pub(crate) field: String,
    pub(crate) relation: String,
    #[serde(rename = "type")]
    pub(crate) relation_type: String,
}

async fn load_schema(
    odoo: &OdooClient,
    model: &str,
    max_fields: usize,
    include_access: bool,
) -> Result<ModelSchema, AppError> {
    let metadata = odoo.get_all_metadata(model).await?;
    let metadata = metadata.as_object().ok_or_else(|| {
        AppError::protocol("Odoo fields_get returned a non-object field definition")
    })?;
    if metadata.len() > max_fields {
        return Err(AppError::input_validation(format!(
            "model {model:?} exposes {} fields, exceeding the configured maximum of {max_fields}",
            metadata.len()
        )));
    }

    let mut fields = BTreeMap::new();
    let mut required_fields = Vec::new();
    let mut relations = Vec::new();
    let mut enums = BTreeMap::new();
    for (name, definition) in metadata {
        let Some(definition) = definition.as_object() else {
            continue;
        };
        let field_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let relation = definition
            .get("relation")
            .and_then(Value::as_str)
            .map(str::to_string);
        let selection = parse_selection(definition.get("selection"));
        let required = definition
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let readonly = definition
            .get("readonly")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let field = FieldSchema {
            name: name.clone(),
            field_type: field_type.clone(),
            string: definition
                .get("string")
                .and_then(Value::as_str)
                .map(str::to_string),
            required,
            readonly,
            relation: relation.clone(),
            selection: selection.clone(),
            compute: definition
                .get("compute")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        if required {
            required_fields.push(name.clone());
        }
        if matches!(field_type.as_str(), "many2one" | "one2many" | "many2many")
            && let Some(relation) = relation
        {
            relations.push(RelationSchema {
                field: name.clone(),
                relation,
                relation_type: field_type,
            });
        }
        if !selection.is_empty() {
            enums.insert(name.clone(), selection);
        }
        fields.insert(name.clone(), field);
    }
    required_fields.sort();
    relations.sort_by(|left, right| left.field.cmp(&right.field));

    let access = if include_access {
        Some(odoo.get_model_access(model).await?)
    } else {
        None
    };

    Ok(ModelSchema {
        model: model.to_string(),
        fields,
        required_fields,
        relations,
        enums,
        access,
    })
}

fn parse_selection(value: Option<&Value>) -> Vec<EnumValue> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Value::Array(pair) if pair.len() == 2 => Some(EnumValue {
                value: pair[0].as_str()?.to_string(),
                label: pair[1].as_str()?.to_string(),
            }),
            Value::String(value) => Some(EnumValue {
                value: value.clone(),
                label: value.clone(),
            }),
            _ => None,
        })
        .collect()
}

struct CacheEntry {
    schema: ModelSchema,
    fetched_at: Instant,
}

static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) async fn schema(
    odoo: &OdooClient,
    model: &str,
    ttl_secs: u64,
    max_fields: usize,
    include_access: bool,
) -> Result<ModelSchema, AppError> {
    let key = format!("{}|{model}|{include_access}", odoo.cache_key());
    if ttl_secs > 0 {
        let cache = cache().lock().unwrap();
        if let Some(entry) = cache.get(&key)
            && entry.fetched_at.elapsed().as_secs() < ttl_secs
        {
            return Ok(entry.schema.clone());
        }
    }
    let schema = load_schema(odoo, model, max_fields, include_access).await?;
    if ttl_secs > 0 {
        cache().lock().unwrap().insert(
            key,
            CacheEntry {
                schema: schema.clone(),
                fetched_at: Instant::now(),
            },
        );
    }
    Ok(schema)
}

pub(crate) fn invalidate_all() {
    cache().lock().unwrap().clear();
}

pub(crate) fn invalidate_model(odoo: &OdooClient, model: &str) {
    let prefix = format!("{}|{model}|", odoo.cache_key());
    let mut cache = cache().lock().unwrap();
    cache.retain(|key, _| !key.starts_with(&prefix));
}

pub(crate) async fn enum_values(
    odoo: &OdooClient,
    model: &str,
    field: Option<&str>,
    ttl_secs: u64,
    max_fields: usize,
) -> Result<Value, AppError> {
    let schema = schema(odoo, model, ttl_secs, max_fields, false).await?;
    match field {
        Some(field) => {
            let values = schema.enums.get(field).ok_or_else(|| {
                AppError::input_validation(format!(
                    "field {field:?} on model {model:?} is not an enum"
                ))
            })?;
            Ok(json!({"model": model, "field": field, "values": values}))
        }
        None => Ok(json!({"model": model, "enums": schema.enums})),
    }
}

pub(crate) async fn required_fields(
    odoo: &OdooClient,
    model: &str,
    ttl_secs: u64,
    max_fields: usize,
) -> Result<Value, AppError> {
    let schema = schema(odoo, model, ttl_secs, max_fields, false).await?;
    Ok(json!({
        "model": model,
        "required_fields": schema.required_fields
    }))
}

pub(crate) async fn relation_path(
    odoo: &OdooClient,
    from: &str,
    to: &str,
    max_depth: usize,
    ttl_secs: u64,
    max_fields: usize,
) -> Result<Value, AppError> {
    if from == to {
        return Ok(json!({"from": from, "to": to, "path": []}));
    }
    let mut queue = VecDeque::from([(from.to_string(), Vec::<RelationStep>::new())]);
    let mut visited = HashSet::new();
    while let Some((model, path)) = queue.pop_front() {
        if path.len() >= max_depth {
            continue;
        }
        if !visited.insert(model.clone()) {
            continue;
        }
        let schema = schema(odoo, &model, ttl_secs, max_fields, false).await?;
        for relation in schema.relations {
            let mut next_path = path.clone();
            next_path.push(RelationStep {
                from: model.clone(),
                field: relation.field.clone(),
                relation: relation.relation.clone(),
                relation_type: relation.relation_type.clone(),
            });
            if relation.relation == to {
                return Ok(json!({"from": from, "to": to, "path": next_path}));
            }
            if !visited.contains(&relation.relation) {
                queue.push_back((relation.relation, next_path));
            }
        }
    }
    Ok(json!({"from": from, "to": to, "path": null}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_parses_pairs_and_plain_values() {
        let selection = parse_selection(Some(&json!([
            ["draft", "Draft"],
            ["done", "Done"],
            "legacy"
        ])));

        assert_eq!(selection.len(), 3);
        assert_eq!(selection[0].value, "draft");
        assert_eq!(selection[2].label, "legacy");
    }
}
