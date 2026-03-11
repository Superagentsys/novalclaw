use prometheus::{IntCounter, IntGauge, Registry, opts};
use std::sync::OnceLock;

static REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();

pub struct MetricsRegistry {
    pub registry: Registry,
    pub requests_total: IntCounter,
    pub active_sessions: IntGauge,
    pub tool_calls_total: IntCounter,
    pub errors_total: IntCounter,
    pub provider_calls_total: IntCounter,
}

impl MetricsRegistry {
    fn new() -> Self {
        let registry = Registry::new();

        let requests_total =
            IntCounter::with_opts(opts!("omninova_requests_total", "Total inbound requests"))
                .expect("metric");
        let active_sessions =
            IntGauge::with_opts(opts!("omninova_active_sessions", "Currently active sessions"))
                .expect("metric");
        let tool_calls_total =
            IntCounter::with_opts(opts!("omninova_tool_calls_total", "Total tool invocations"))
                .expect("metric");
        let errors_total =
            IntCounter::with_opts(opts!("omninova_errors_total", "Total errors"))
                .expect("metric");
        let provider_calls_total =
            IntCounter::with_opts(opts!(
                "omninova_provider_calls_total",
                "Total LLM provider calls"
            ))
            .expect("metric");

        registry.register(Box::new(requests_total.clone())).ok();
        registry.register(Box::new(active_sessions.clone())).ok();
        registry.register(Box::new(tool_calls_total.clone())).ok();
        registry.register(Box::new(errors_total.clone())).ok();
        registry
            .register(Box::new(provider_calls_total.clone()))
            .ok();

        Self {
            registry,
            requests_total,
            active_sessions,
            tool_calls_total,
            errors_total,
            provider_calls_total,
        }
    }
}

pub fn metrics() -> &'static MetricsRegistry {
    REGISTRY.get_or_init(MetricsRegistry::new)
}

pub fn encode_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = metrics().registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).ok();
    String::from_utf8(buffer).unwrap_or_default()
}
