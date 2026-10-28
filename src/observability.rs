use serde_json::{Value, json};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Metrics {
    started_at: u64,
    requests_total: AtomicU64,
    request_errors_total: AtomicU64,
    tool_calls_total: AtomicU64,
    tool_failures_total: AtomicU64,
    rpc_calls_total: AtomicU64,
    rpc_duration_ms_total: AtomicU64,
    retries_total: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            started_at: now_seconds(),
            requests_total: AtomicU64::new(0),
            request_errors_total: AtomicU64::new(0),
            tool_calls_total: AtomicU64::new(0),
            tool_failures_total: AtomicU64::new(0),
            rpc_calls_total: AtomicU64::new(0),
            rpc_duration_ms_total: AtomicU64::new(0),
            retries_total: AtomicU64::new(0),
        }
    }
}

impl Metrics {
    pub(crate) fn record_request(&self, failed: bool) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.request_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_tool_call(&self, failed: bool) {
        self.tool_calls_total.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.tool_failures_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_rpc(&self, duration_ms: u64) {
        self.rpc_calls_total.fetch_add(1, Ordering::Relaxed);
        self.rpc_duration_ms_total
            .fetch_add(duration_ms, Ordering::Relaxed);
    }

    pub(crate) fn record_retry(&self) {
        self.retries_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> Value {
        json!({
            "requests_total": self.requests_total.load(Ordering::Relaxed),
            "request_errors_total": self.request_errors_total.load(Ordering::Relaxed),
            "tool_calls_total": self.tool_calls_total.load(Ordering::Relaxed),
            "tool_failures_total": self.tool_failures_total.load(Ordering::Relaxed),
            "rpc_calls_total": self.rpc_calls_total.load(Ordering::Relaxed),
            "rpc_duration_ms_total": self.rpc_duration_ms_total.load(Ordering::Relaxed),
            "retries_total": self.retries_total.load(Ordering::Relaxed),
            "uptime_seconds": now_seconds().saturating_sub(self.started_at)
        })
    }
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

pub(crate) fn metrics() -> &'static Metrics {
    METRICS.get_or_init(Metrics::default)
}

pub(crate) fn log_event(level: &str, event: &str, fields: Value) {
    let fields = crate::audit::redact_details(fields);
    let payload = json!({
        "timestamp": now_seconds(),
        "level": level,
        "event": event,
        "fields": fields
    });
    eprintln!("{payload}");
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_contains_stable_counters() {
        let metrics = Metrics::default();
        metrics.record_request(false);
        metrics.record_request(true);
        metrics.record_tool_call(true);
        metrics.record_rpc(25);
        metrics.record_retry();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot["requests_total"], 2);
        assert_eq!(snapshot["request_errors_total"], 1);
        assert_eq!(snapshot["tool_calls_total"], 1);
        assert_eq!(snapshot["tool_failures_total"], 1);
        assert_eq!(snapshot["rpc_calls_total"], 1);
        assert_eq!(snapshot["rpc_duration_ms_total"], 25);
        assert_eq!(snapshot["retries_total"], 1);
        assert!(snapshot["uptime_seconds"].is_u64());
    }
}
