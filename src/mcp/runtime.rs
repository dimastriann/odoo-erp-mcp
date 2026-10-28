use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc;

#[derive(Clone)]
pub(crate) struct McpRuntime {
    sender: mpsc::UnboundedSender<Value>,
    initialized: Arc<AtomicBool>,
    revision: Arc<AtomicU64>,
}

impl McpRuntime {
    pub(crate) fn new(sender: mpsc::UnboundedSender<Value>) -> Self {
        Self {
            sender,
            initialized: Arc::new(AtomicBool::new(false)),
            revision: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(crate) fn send(&self, value: Value) {
        let _ = self.sender.send(value);
    }

    pub(crate) fn mark_initialized(&self) {
        self.initialized.store(true, Ordering::SeqCst);
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    pub(crate) fn config_changed(&self) {
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        if self.initialized.load(Ordering::SeqCst) {
            self.send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/tools/list_changed",
                "params": {"_meta": {"configRevision": revision}}
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn config_changes_emit_one_list_changed_notification_after_initialize() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let runtime = McpRuntime::new(sender);
        assert_eq!(runtime.revision(), 1);

        runtime.config_changed();
        assert!(
            receiver.try_recv().is_err(),
            "no notification before initialize"
        );

        runtime.mark_initialized();
        runtime.config_changed();

        let notification = receiver.try_recv().expect("notification should be sent");
        assert_eq!(notification["method"], "notifications/tools/list_changed");
        assert_eq!(notification["params"]["_meta"]["configRevision"], 3);
        assert_eq!(runtime.revision(), 3);
    }
}
