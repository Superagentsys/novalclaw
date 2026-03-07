use crate::channels::{ChannelKind, InboundMessage};
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
        return build_route_decision(config, agent_name);
    }

    if let Some(agent_name) = resolve_agent_from_bindings(config, inbound) {
        return build_route_decision(config, &agent_name);
    }

    let fallback_agent_name = config
        .acp
        .default_agent
        .clone()
        .filter(|name| config.agents.contains_key(name))
        .unwrap_or_else(|| config.agent.name.clone());

    build_route_decision(config, &fallback_agent_name)
}

fn build_route_decision(config: &Config, agent_name: &str) -> RouteDecision {
    let fallback_provider = config
        .agent_defaults_extended
        .model
        .as_ref()
        .and_then(|m| m.provider.clone())
        .or_else(|| config.default_provider.clone());
    let fallback_model = config
        .agent_defaults_extended
        .model
        .as_ref()
        .and_then(|m| m.model.clone())
        .or_else(|| config.default_model.clone());

    if let Some(delegate) = config.agents.get(agent_name) {
        return RouteDecision {
            agent_name: agent_name.to_string(),
            provider: delegate.provider.clone().or(fallback_provider),
            model: delegate.model.clone().or(fallback_model),
        };
    }

    RouteDecision {
        agent_name: agent_name.to_string(),
        provider: fallback_provider,
        model: fallback_model,
    }
}

fn resolve_agent_from_bindings(config: &Config, inbound: &InboundMessage) -> Option<String> {
    for entry in &config.bindings {
        let Some(rule) = entry.match_rule.as_ref() else {
            continue;
        };
        if !match_channel(rule.channel.as_deref(), inbound) {
            continue;
        }
        if !match_scalar_rule(rule.account_id.as_deref(), metadata_str(inbound, &["account_id", "accountId"])) {
            continue;
        }
        if !match_scalar_rule(rule.guild_id.as_deref(), metadata_str(inbound, &["guild_id", "guildId"])) {
            continue;
        }
        if !match_scalar_rule(rule.team_id.as_deref(), metadata_str(inbound, &["team_id", "teamId"])) {
            continue;
        }
        if !match_peer(rule.peer.as_ref(), inbound) {
            continue;
        }
        if !match_roles(&rule.roles, inbound) {
            continue;
        }
        if let Some(agent_id) = entry.agent_id.clone() {
            return Some(agent_id);
        }
    }
    None
}

fn match_channel(expected: Option<&str>, inbound: &InboundMessage) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let actual = channel_name(&inbound.channel);
    actual == expected.to_lowercase()
}

fn channel_name(channel: &ChannelKind) -> String {
    match channel {
        ChannelKind::Cli => "cli".to_string(),
        ChannelKind::Web => "web".to_string(),
        ChannelKind::Telegram => "telegram".to_string(),
        ChannelKind::Discord => "discord".to_string(),
        ChannelKind::Slack => "slack".to_string(),
        ChannelKind::Whatsapp => "whatsapp".to_string(),
        ChannelKind::Matrix => "matrix".to_string(),
        ChannelKind::Email => "email".to_string(),
        ChannelKind::Webhook => "webhook".to_string(),
        ChannelKind::Other(value) => value.to_lowercase(),
    }
}

fn match_scalar_rule(expected: Option<&str>, actual: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    if expected == "*" {
        return true;
    }
    actual == Some(expected)
}

fn match_peer(peer_rule: Option<&crate::config::schema::BindingPeerConfig>, inbound: &InboundMessage) -> bool {
    let Some(peer_rule) = peer_rule else {
        return true;
    };
    let actual_kind = metadata_str(inbound, &["peer_kind", "peerKind"]);
    let actual_id = metadata_str(inbound, &["peer_id", "peerId"]);
    if !match_scalar_rule(peer_rule.kind.as_deref(), actual_kind) {
        return false;
    }
    match_scalar_rule(peer_rule.id.as_deref(), actual_id)
}

fn match_roles(expected_roles: &[String], inbound: &InboundMessage) -> bool {
    if expected_roles.is_empty() {
        return true;
    }
    let Some(value) = inbound.metadata.get("roles") else {
        return false;
    };
    let Some(actual_roles) = value.as_array() else {
        return false;
    };
    expected_roles.iter().any(|expected| {
        actual_roles
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|actual| actual == expected)
    })
}

fn metadata_str<'a>(inbound: &'a InboundMessage, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| inbound.metadata.get(*key).and_then(serde_json::Value::as_str))
}

#[cfg(test)]
mod tests {
    use super::resolve_agent_route;
    use crate::channels::{ChannelKind, InboundMessage};
    use crate::config::schema::{
        AgentDefaultsExtendedConfig, AgentModelConfig, BindingEntry, BindingMatchConfig, Config,
        DelegateAgentConfig,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn explicit_agent_overrides_bindings() {
        let mut config = Config::default();
        config.agents.insert(
            "alpha".into(),
            DelegateAgentConfig {
                provider: Some("p-alpha".into()),
                model: Some("m-alpha".into()),
                ..DelegateAgentConfig::default()
            },
        );
        config.bindings.push(BindingEntry {
            agent_id: Some("beta".into()),
            match_rule: Some(BindingMatchConfig {
                channel: Some("discord".into()),
                ..BindingMatchConfig::default()
            }),
            ..BindingEntry::default()
        });

        let mut metadata = HashMap::new();
        metadata.insert("agent".into(), json!("alpha"));
        let inbound = InboundMessage {
            channel: ChannelKind::Discord,
            text: "hello".into(),
            metadata,
            ..InboundMessage::default()
        };

        let route = resolve_agent_route(&config, &inbound);
        assert_eq!(route.agent_name, "alpha");
        assert_eq!(route.provider.as_deref(), Some("p-alpha"));
        assert_eq!(route.model.as_deref(), Some("m-alpha"));
    }

    #[test]
    fn bindings_match_channel_and_account() {
        let mut config = Config::default();
        config.agents.insert(
            "ops".into(),
            DelegateAgentConfig {
                model: Some("ops-model".into()),
                ..DelegateAgentConfig::default()
            },
        );
        config.bindings.push(BindingEntry {
            agent_id: Some("ops".into()),
            match_rule: Some(BindingMatchConfig {
                channel: Some("discord".into()),
                account_id: Some("acct-1".into()),
                ..BindingMatchConfig::default()
            }),
            ..BindingEntry::default()
        });

        let mut metadata = HashMap::new();
        metadata.insert("accountId".into(), json!("acct-1"));
        let inbound = InboundMessage {
            channel: ChannelKind::Discord,
            text: "ping".into(),
            metadata,
            ..InboundMessage::default()
        };

        let route = resolve_agent_route(&config, &inbound);
        assert_eq!(route.agent_name, "ops");
        assert_eq!(route.model.as_deref(), Some("ops-model"));
    }

    #[test]
    fn fallback_to_acp_default_agent_when_present() {
        let mut config = Config::default();
        config.acp.default_agent = Some("worker".into());
        config.agents.insert(
            "worker".into(),
            DelegateAgentConfig {
                provider: Some("worker-provider".into()),
                ..DelegateAgentConfig::default()
            },
        );

        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "run".into(),
            ..InboundMessage::default()
        };

        let route = resolve_agent_route(&config, &inbound);
        assert_eq!(route.agent_name, "worker");
        assert_eq!(route.provider.as_deref(), Some("worker-provider"));
    }

    #[test]
    fn default_model_falls_back_to_agents_defaults() {
        let mut config = Config::default();
        config.default_provider = None;
        config.default_model = None;
        config.agent_defaults_extended = AgentDefaultsExtendedConfig {
            model: Some(AgentModelConfig {
                provider: Some("defaults-provider".into()),
                model: Some("defaults-model".into()),
            }),
            ..AgentDefaultsExtendedConfig::default()
        };

        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "hello".into(),
            ..InboundMessage::default()
        };

        let route = resolve_agent_route(&config, &inbound);
        assert_eq!(route.provider.as_deref(), Some("defaults-provider"));
        assert_eq!(route.model.as_deref(), Some("defaults-model"));
    }
}
