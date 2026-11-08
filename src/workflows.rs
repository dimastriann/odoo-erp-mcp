use crate::error::AppError;
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkflowArgument {
    pub(crate) name: &'static str,
    pub(crate) required: bool,
    pub(crate) description: &'static str,
}

const NO_ARGS: &[WorkflowArgument] = &[];

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkflowDefinition {
    pub(crate) name: &'static str,
    pub(crate) model: &'static str,
    pub(crate) method: &'static str,
    pub(crate) description: &'static str,
    pub(crate) arguments: &'static [WorkflowArgument],
}

pub(crate) const WORKFLOWS: &[WorkflowDefinition] = &[
    WorkflowDefinition {
        name: "sale_order_confirm",
        model: "sale.order",
        method: "action_confirm",
        description: "Confirm a draft sales order",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "sale_order_cancel",
        model: "sale.order",
        method: "action_cancel",
        description: "Cancel a sales order",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "invoice_post",
        model: "account.move",
        method: "action_post",
        description: "Post a draft customer invoice or vendor bill",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "account_move_cancel",
        model: "account.move",
        method: "button_cancel",
        description: "Cancel a posted accounting entry",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "stock_picking_validate",
        model: "stock.picking",
        method: "button_validate",
        description: "Validate a stock transfer",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "purchase_order_confirm",
        model: "purchase.order",
        method: "button_confirm",
        description: "Confirm a draft purchase order",
        arguments: NO_ARGS,
    },
    WorkflowDefinition {
        name: "post_sale_order_message",
        model: "sale.order",
        method: "message_post",
        description: "Post a chatter message on a sales order",
        arguments: &[
            WorkflowArgument {
                name: "body",
                required: true,
                description: "Message body",
            },
            WorkflowArgument {
                name: "subtype_xmlid",
                required: false,
                description: "Optional mail message subtype",
            },
        ],
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
                    "description": workflow.description,
                    "arguments": workflow.arguments.iter().map(|argument| json!({
                        "name": argument.name,
                        "required": argument.required,
                        "description": argument.description
                    })).collect::<Vec<_>>()
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

pub(crate) fn validate_kwargs(
    workflow: &WorkflowDefinition,
    kwargs: Option<&Map<String, Value>>,
) -> Result<Map<String, Value>, AppError> {
    let kwargs = kwargs.cloned().unwrap_or_default();
    for key in kwargs.keys() {
        if !workflow
            .arguments
            .iter()
            .any(|argument| argument.name == key)
        {
            return Err(AppError::input_validation(format!(
                "workflow '{}' does not accept argument '{key}'",
                workflow.name
            )));
        }
    }
    for argument in workflow.arguments {
        if argument.required && !kwargs.contains_key(argument.name) {
            return Err(AppError::input_validation(format!(
                "workflow '{}' requires argument '{}'",
                workflow.name, argument.name
            )));
        }
    }
    Ok(kwargs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_registry_is_strict_and_describes_arguments() {
        let workflow = find("post_sale_order_message").unwrap();
        assert!(validate_target(workflow, "sale.order", "message_post").is_ok());
        assert!(validate_target(workflow, "res.partner", "message_post").is_err());
        assert!(validate_kwargs(workflow, None).is_err());
        assert!(
            validate_kwargs(
                workflow,
                Some(&Map::from_iter([("body".to_string(), json!("hello"))]))
            )
            .is_ok()
        );
        assert!(
            validate_kwargs(
                workflow,
                Some(&Map::from_iter([("unknown".to_string(), json!(true))]))
            )
            .is_err()
        );
        assert!(!find("unknown").is_some());
    }
}
