use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry};
use std::sync::OnceLock;

static REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();

pub struct MetricsRegistry {
    pub registry: Registry,
    pub inbound_requests: IntCounterVec,
    pub inbound_errors: IntCounterVec,
    pub tool_calls: IntCounterVec,
    pub provider_calls: IntCounterVec,
    pub audit_events: IntCounterVec,
    pub approval_events: IntCounterVec,
    pub estop_events: IntCounterVec,
    pub active_sessions: IntGauge,
    pub inbound_duration: HistogramVec,
}

impl MetricsRegistry {
    fn new() -> Self {
        let registry = Registry::new();

        let inbound_requests = IntCounterVec::new(
            Opts::new("omninova_inbound_requests_total", "Total inbound gateway requests"),
            &["channel"],
        )
        .expect("metric");
        let inbound_errors = IntCounterVec::new(
            Opts::new("omninova_inbound_errors_total", "Total inbound gateway errors"),
            &["stage"],
        )
        .expect("metric");
        let tool_calls = IntCounterVec::new(
            Opts::new("omninova_tool_calls_total", "Total tool invocations"),
            &["tool", "status"],
        )
        .expect("metric");
        let provider_calls = IntCounterVec::new(
            Opts::new("omninova_provider_calls_total", "Total LLM provider chat calls"),
            &["provider", "status"],
        )
        .expect("metric");
        let audit_events = IntCounterVec::new(
            Opts::new("omninova_audit_events_total", "Total audit trail events written"),
            &["event"],
        )
        .expect("metric");
        let approval_events = IntCounterVec::new(
            Opts::new("omninova_approval_events_total", "Approval workflow events"),
            &["action"],
        )
        .expect("metric");
        let estop_events = IntCounterVec::new(
            Opts::new("omninova_estop_events_total", "Emergency stop events"),
            &["action"],
        )
        .expect("metric");
        let active_sessions = IntGauge::with_opts(Opts::new(
            "omninova_active_sessions",
            "Currently active gateway sessions",
        ))
        .expect("metric");
        let inbound_duration = HistogramVec::new(
            HistogramOpts::new(
                "omninova_inbound_duration_seconds",
                "Inbound gateway request duration in seconds",
            )
            .buckets(vec![
                0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0,
            ]),
            &["channel", "status"],
        )
        .expect("metric");

        registry.register(Box::new(inbound_requests.clone())).ok();
        registry.register(Box::new(inbound_errors.clone())).ok();
        registry.register(Box::new(tool_calls.clone())).ok();
        registry.register(Box::new(provider_calls.clone())).ok();
        registry.register(Box::new(audit_events.clone())).ok();
        registry.register(Box::new(approval_events.clone())).ok();
        registry.register(Box::new(estop_events.clone())).ok();
        registry.register(Box::new(active_sessions.clone())).ok();
        registry.register(Box::new(inbound_duration.clone())).ok();

        Self {
            registry,
            inbound_requests,
            inbound_errors,
            tool_calls,
            provider_calls,
            audit_events,
            approval_events,
            estop_events,
            active_sessions,
            inbound_duration,
        }
    }
}

pub fn metrics() -> &'static MetricsRegistry {
    REGISTRY.get_or_init(MetricsRegistry::new)
}

pub fn record_inbound_request(channel: &str) {
    metrics()
        .inbound_requests
        .with_label_values(&[channel])
        .inc();
}

pub fn record_inbound_error(stage: &str) {
    metrics()
        .inbound_errors
        .with_label_values(&[stage])
        .inc();
}

pub fn record_tool_call(tool: &str, status: &str) {
    metrics()
        .tool_calls
        .with_label_values(&[tool, status])
        .inc();
}

pub fn record_provider_call(provider: &str, status: &str) {
    metrics()
        .provider_calls
        .with_label_values(&[provider, status])
        .inc();
}

pub fn record_audit_event(event: &str) {
    metrics()
        .audit_events
        .with_label_values(&[event])
        .inc();
}

pub fn record_approval_event(action: &str) {
    metrics()
        .approval_events
        .with_label_values(&[action])
        .inc();
}

pub fn record_estop_event(action: &str) {
    metrics()
        .estop_events
        .with_label_values(&[action])
        .inc();
}

pub fn set_active_sessions(count: i64) {
    metrics().active_sessions.set(count);
}

pub fn record_inbound_duration(channel: &str, status: &str, seconds: f64) {
    metrics()
        .inbound_duration
        .with_label_values(&[channel, status])
        .observe(seconds);
}

pub fn encode_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = metrics().registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).ok();
    String::from_utf8(buffer).unwrap_or_default()
}
