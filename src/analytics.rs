use crate::error::AppError;
use crate::odoo::OdooClient;
use serde_json::{Value, json};

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

pub(crate) async fn run(
    odoo: &OdooClient,
    name: &str,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: Option<u64>,
    max_limit: u64,
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
    let rows = odoo
        .read_group(
            report.model,
            domain.clone(),
            json!(report.fields),
            json!(report.groupby),
            None,
            limit,
            report.orderby,
        )
        .await?;
    let rows = rows
        .as_array()
        .ok_or_else(|| AppError::protocol("Odoo read_group returned a non-array result"))?;
    Ok(json!({
        "report": report.name,
        "model": report.model,
        "description": report.description,
        "domain": domain,
        "groupby": report.groupby,
        "fields": report.fields,
        "limit": limit,
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
    fn dates_must_use_iso_format() {
        assert!(validate_date("2026-11-14").is_ok());
        assert!(validate_date("14-11-2026").is_err());
        assert!(validate_date("2026-1-4").is_err());
    }
}
