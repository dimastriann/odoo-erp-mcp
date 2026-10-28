use crate::config::Config;
use crate::context::RequestContext;
use crate::error::AppError;
use crate::lifecycle::operation::Operation;
use crate::risk::RiskLevel;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const AUDIT_VERSION: u8 = 1;
const DEFAULT_RETENTION: usize = 10_000;
const MAX_DETAIL_STRING: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuditEventKind {
    PolicyDecision,
    ToolDisabled,
    ApprovalRequested,
    ApprovalDecided,
    ExecutionStarted,
    ExecutionSucceeded,
    ExecutionFailed,
    VerificationSucceeded,
    VerificationFailed,
    ToolCallCompleted,
    ConfigChanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuditOutcome {
    Allowed,
    Denied,
    Pending,
    Success,
    Failure,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AuditEvent {
    pub(crate) version: u8,
    pub(crate) id: String,
    pub(crate) timestamp: i64,
    pub(crate) kind: AuditEventKind,
    pub(crate) outcome: AuditOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) risk: Option<RiskLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_ms: Option<u64>,
    #[serde(default)]
    pub(crate) details: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) previous_hash: Option<String>,
    pub(crate) hash: String,
}

impl AuditEvent {
    pub(crate) fn new(kind: AuditEventKind, outcome: AuditOutcome) -> Self {
        Self {
            version: AUDIT_VERSION,
            id: Uuid::new_v4().to_string(),
            timestamp: now_seconds(),
            kind,
            outcome,
            request_id: None,
            actor: None,
            agent: None,
            client: None,
            instance: None,
            operation_id: None,
            model: None,
            risk: None,
            duration_ms: None,
            details: Value::Null,
            previous_hash: None,
            hash: String::new(),
        }
    }

    pub(crate) fn with_context(mut self, context: &RequestContext) -> Self {
        self.request_id = Some(context.request_id.to_string());
        self.actor = context.actor.subject.clone();
        self.agent = context.agent.name.clone();
        self.client = context.client.name.clone();
        self.instance = Some(context.instance.clone());
        self
    }

    pub(crate) fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    pub(crate) fn with_operation(mut self, operation: &Operation) -> Self {
        self.operation_id = Some(operation.id.to_string());
        self.model = Some(operation.model.clone());
        self
    }

    pub(crate) fn with_risk(mut self, risk: RiskLevel) -> Self {
        self.risk = Some(risk);
        self
    }

    pub(crate) fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub(crate) fn with_details(mut self, details: Value) -> Self {
        self.details = redact_details(details);
        self
    }

    pub(crate) fn seal(mut self, previous_hash: Option<String>) -> Self {
        self.previous_hash = previous_hash;
        self.hash = self.compute_hash();
        self
    }

    pub(crate) fn verify_hash(&self) -> bool {
        self.hash == self.compute_hash()
    }

    fn compute_hash(&self) -> String {
        let mut copy = self.clone();
        copy.hash.clear();
        let bytes = serde_json::to_vec(&copy).expect("audit event must serialize");
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn redact_details(value: Value) -> Value {
    match value {
        Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| {
                    if sensitive_key(&key) {
                        (key, Value::String("[REDACTED]".to_string()))
                    } else {
                        (key, redact_details(value))
                    }
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_details).collect()),
        Value::String(value) if value.chars().count() > MAX_DETAIL_STRING => {
            let truncated = value.chars().take(MAX_DETAIL_STRING).collect::<String>();
            Value::String(format!("{truncated}..."))
        }
        value => value,
    }
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

pub(crate) struct LocalAuditStorage {
    path: PathBuf,
    events: Vec<AuditEvent>,
    retention: usize,
}

impl LocalAuditStorage {
    pub(crate) fn open(path: impl Into<PathBuf>, retention: usize) -> Result<Self, AppError> {
        let path = path.into();
        let mut events: Vec<AuditEvent> = Vec::new();
        if path.exists() {
            let content =
                fs::read_to_string(&path).map_err(|error| AppError::internal(error.to_string()))?;
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                let event: AuditEvent = serde_json::from_str(line)
                    .map_err(|error| AppError::internal(format!("invalid audit event: {error}")))?;
                if !event.verify_hash() {
                    return Err(AppError::internal("audit event hash mismatch"));
                }
                if let Some(previous) = events.last() {
                    if event.previous_hash.as_deref() != Some(previous.hash.as_str()) {
                        return Err(AppError::internal("audit hash chain is broken"));
                    }
                }
                events.push(event);
            }
        }
        let mut storage = Self {
            path,
            events,
            retention: retention.max(1),
        };
        storage.compact_if_needed()?;
        Ok(storage)
    }

    pub(crate) fn append(&mut self, event: AuditEvent) -> Result<AuditEvent, AppError> {
        let previous_hash = self.events.last().map(|event| event.hash.clone());
        let event = event.seal(previous_hash);
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::internal(error.to_string()))?;
        }
        let line =
            serde_json::to_string(&event).map_err(|error| AppError::internal(error.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| AppError::internal(error.to_string()))?;
        writeln!(file, "{line}").map_err(|error| AppError::internal(error.to_string()))?;
        self.events.push(event.clone());
        self.compact_if_needed()?;
        Ok(event)
    }

    pub(crate) fn list(&self, limit: usize) -> Vec<AuditEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub(crate) fn export(&self) -> Vec<AuditEvent> {
        self.events.clone()
    }

    fn compact_if_needed(&mut self) -> Result<(), AppError> {
        if self.events.len() <= self.retention {
            return Ok(());
        }
        let keep_from = self.events.len() - self.retention;
        self.events.drain(..keep_from);
        let temporary = self.path.with_extension("tmp");
        let mut file =
            fs::File::create(&temporary).map_err(|error| AppError::internal(error.to_string()))?;
        for event in &self.events {
            let line = serde_json::to_string(event)
                .map_err(|error| AppError::internal(error.to_string()))?;
            writeln!(file, "{line}").map_err(|error| AppError::internal(error.to_string()))?;
        }
        fs::rename(&temporary, &self.path).map_err(|error| AppError::internal(error.to_string()))
    }
}

static RETENTION: AtomicUsize = AtomicUsize::new(DEFAULT_RETENTION);

pub(crate) fn initialize(retention: usize) {
    if retention > 0 {
        RETENTION.store(retention, Ordering::SeqCst);
    }
}

fn audit_path() -> PathBuf {
    Config::get_path().with_file_name("audit.log.jsonl")
}

fn global_storage() -> &'static Mutex<LocalAuditStorage> {
    static STORAGE: OnceLock<Mutex<LocalAuditStorage>> = OnceLock::new();
    STORAGE.get_or_init(|| {
        let path = audit_path();
        let storage = LocalAuditStorage::open(&path, RETENTION.load(Ordering::SeqCst))
            .unwrap_or_else(|error| {
                let fallback = std::env::temp_dir()
                    .join(format!("odoo-erp-mcp-audit-{}.jsonl", std::process::id()));
                eprintln!(
                    "Warning: failed to open audit storage at {:?}: {error}; using {:?}",
                    path, fallback
                );
                LocalAuditStorage::open(fallback, RETENTION.load(Ordering::SeqCst))
                    .expect("fallback audit storage must be available")
            });
        Mutex::new(storage)
    })
}

pub(crate) fn record(event: AuditEvent) -> Result<AuditEvent, AppError> {
    let mut storage = global_storage()
        .lock()
        .map_err(|_| AppError::internal("Audit storage lock is poisoned"))?;
    storage.append(event)
}

pub(crate) fn query(limit: usize) -> Result<Vec<AuditEvent>, AppError> {
    let storage = global_storage()
        .lock()
        .map_err(|_| AppError::internal("Audit storage lock is poisoned"))?;
    Ok(storage.list(limit.clamp(1, 1_000)))
}

pub(crate) fn export() -> Result<Vec<AuditEvent>, AppError> {
    let storage = global_storage()
        .lock()
        .map_err(|_| AppError::internal("Audit storage lock is poisoned"))?;
    Ok(storage.export())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_are_versioned_and_hash_chained() {
        let path = std::env::temp_dir().join(format!("audit-{}.jsonl", Uuid::new_v4()));
        let mut storage = LocalAuditStorage::open(&path, 100).unwrap();
        let first = storage
            .append(AuditEvent::new(
                AuditEventKind::PolicyDecision,
                AuditOutcome::Allowed,
            ))
            .unwrap();
        let second = storage
            .append(AuditEvent::new(
                AuditEventKind::ToolCallCompleted,
                AuditOutcome::Success,
            ))
            .unwrap();

        assert_eq!(first.version, AUDIT_VERSION);
        assert_eq!(second.previous_hash.as_deref(), Some(first.hash.as_str()));
        assert!(second.verify_hash());

        let reopened = LocalAuditStorage::open(&path, 100).unwrap();
        assert_eq!(reopened.export(), vec![first, second]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sensitive_details_are_redacted_and_truncated() {
        let value = redact_details(json!({
            "password": "secret",
            "api_key": "key",
            "nested": {"token": "value", "name": "Alpha"},
            "long": "x".repeat(600)
        }));

        assert_eq!(value["password"], "[REDACTED]");
        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["nested"]["token"], "[REDACTED]");
        assert_eq!(value["nested"]["name"], "Alpha");
        assert!(value["long"].as_str().unwrap().ends_with("..."));
    }

    #[test]
    fn retention_keeps_the_newest_events() {
        let path = std::env::temp_dir().join(format!("audit-{}.jsonl", Uuid::new_v4()));
        let mut storage = LocalAuditStorage::open(&path, 2).unwrap();
        for _ in 0..3 {
            storage
                .append(AuditEvent::new(
                    AuditEventKind::ToolCallCompleted,
                    AuditOutcome::Success,
                ))
                .unwrap();
        }

        assert_eq!(storage.export().len(), 2);
        assert_eq!(storage.list(10).len(), 2);
        let _ = fs::remove_file(path);
    }
}
