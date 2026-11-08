use crate::error::AppError;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkflowDefinition {
    pub(crate) name: &'static str,
    pub(crate) model: &'static str,
    pub(crate) method: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) const WORKFLOWS: &[WorkflowDefinition] = &[
    WorkflowDefinition {
        name: "sale_order_confirm",
        model: "sale.order",
        method: "action_confirm",
        description: "Confirm a draft sales order",
    },
    WorkflowDefinition {
        name: "sale_order_cancel",
        model: "sale.order",
        method: "action_cancel",
        description: "Cancel a sales order",
    },
    WorkflowDefinition {
        name: "invoice_post",
        model: "account.move",
        method: "action_post",
        description: "Post a draft customer invoice or vendor bill",
    },
    WorkflowDefinition {
        name: "account_move_cancel",
        model: "account.move",
        method: "button_cancel",
        description: "Cancel a posted accounting entry",
    },
    WorkflowDefinition {
        name: "stock_picking_validate",
        model: "stock.picking",
        method: "button_validate",
        description: "Validate a stock transfer",
    },
    WorkflowDefinition {
        name: "purchase_order_confirm",
        model: "purchase.order",
        method: "button_confirm",
        description: "Confirm a draft purchase order",
    },
];

pub(crate) fn find(name: &str) -> Option<&'static WorkflowDefinition> {
    WORKFLOWS.iter().find(|workflow| workflow.name == name)
}

pub(crate) fn definitions() -> Value {
    Value::Array(
        WORKFLOWS
            .iter()
            .map(|workflow| {
                json!({
                    "name": workflow.name,
                    "model": workflow.model,
                    "method": workflow.method,
                    "description": workflow.description
                })
            })
            .collect(),
    )
}

pub(crate) fn validate_target(
    workflow: &WorkflowDefinition,
    model: &str,
    method: &str,
) -> Result<(), AppError> {
    if workflow.model != model || workflow.method != method {
        return Err(AppError::input_validation(format!(
            "workflow '{}' only permits {}.{}",
            workflow.name, workflow.model, workflow.method
        )));
    }
    Ok(())
}
