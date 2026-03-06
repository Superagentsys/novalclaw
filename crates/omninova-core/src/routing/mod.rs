use crate::channels::InboundMessage;
use crate::config::Config;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteDecision {
    pub agent_name: String,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// Resolve target agent/provider/model for an inbound message.
pub fn resolve_agent_route(config: &Config, inbound: &InboundMessage) -> RouteDecision {
    if let Some(agent_name) = inbound
        .metadata
        .get("agent")
        .and_then(serde_json::Value::as_str)
    {
        if let Some(delegate) = config.agents.get(agent_name) {
            return RouteDecision {
                agent_name: agent_name.to_string(),
                provider: delegate.provider.clone().or_else(|| config.default_provider.clone()),
                model: delegate.model.clone().or_else(|| config.default_model.clone()),
            };
        }
    }

    RouteDecision {
        agent_name: config.agent.name.clone(),
        provider: config.default_provider.clone(),
        model: config.default_model.clone(),
    }
}
