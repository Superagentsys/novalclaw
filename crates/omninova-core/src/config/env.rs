use crate::config::schema::Config;
use tracing::debug;

/// Apply environment variable overrides to a loaded Config.
/// Mirrors OpenClaw's `apply_env_overrides` behaviour.
pub fn apply_env_overrides(cfg: &mut Config) {
    // --- API key ---
    env_opt("OMNINOVA_API_KEY", |v| cfg.api_key = Some(v));
    env_opt("API_KEY", |v| {
        if cfg.api_key.is_none() {
            cfg.api_key = Some(v);
        }
    });

    // --- Provider / Model ---
    env_opt("OMNINOVA_PROVIDER", |v| cfg.default_provider = Some(v));
    env_opt("OMNINOVA_MODEL_PROVIDER", |v| cfg.default_provider = Some(v));
    env_opt("MODEL_PROVIDER", |v| {
        if cfg.default_provider.is_none() {
            cfg.default_provider = Some(v);
        }
    });
    env_opt("PROVIDER", |v| {
        if cfg.default_provider.as_deref() == Some("openrouter") {
            cfg.default_provider = Some(v);
        }
    });

    env_opt("OMNINOVA_MODEL", |v| cfg.default_model = Some(v));
    env_opt("MODEL", |v| {
        if cfg.default_model.is_none() {
            cfg.default_model = Some(v);
        }
    });

    // Also accept OPENAI_* for convenience / backward compat
    env_opt("OPENAI_API_KEY", |v| {
        if cfg.api_key.is_none() {
            cfg.api_key = Some(v);
        }
    });
    env_opt("OPENAI_BASE_URL", |v| {
        if cfg.api_url.is_none() {
            cfg.api_url = Some(v);
        }
    });
    env_opt("OPENAI_MODEL", |v| {
        if cfg.default_model.is_none() {
            cfg.default_model = Some(v);
        }
    });

    // --- Temperature ---
    env_opt("OMNINOVA_TEMPERATURE", |v| {
        if let Ok(t) = v.parse::<f64>() {
            cfg.default_temperature = t;
        }
    });

    // --- Workspace ---
    env_opt("OMNINOVA_WORKSPACE", |v| {
        cfg.workspace_dir = v.into();
    });

    // --- Skills ---
    env_opt("OMNINOVA_OPEN_SKILLS_ENABLED", |v| {
        cfg.skills.open_skills_enabled = v == "true" || v == "1";
    });
    env_opt("OMNINOVA_OPEN_SKILLS_DIR", |v| {
        cfg.skills.open_skills_dir = Some(v);
    });
    env_opt("OMNINOVA_SKILLS_PROMPT_MODE", |v| {
        cfg.skills.prompt_injection_mode = Some(v);
    });

    // --- Gateway ---
    env_opt("OMNINOVA_GATEWAY_PORT", |v| {
        if let Ok(p) = v.parse::<u16>() {
            cfg.gateway.port = p;
        }
    });
    env_opt("PORT", |v| {
        if let Ok(p) = v.parse::<u16>() {
            cfg.gateway.port = p;
        }
    });
    env_opt("OMNINOVA_GATEWAY_HOST", |v| cfg.gateway.host = v);
    env_opt("HOST", |v| {
        if cfg.gateway.host == "127.0.0.1" {
            cfg.gateway.host = v;
        }
    });
    env_opt("OMNINOVA_ALLOW_PUBLIC_BIND", |v| {
        cfg.gateway.allow_public_bind = v == "true" || v == "1";
    });

    // --- Runtime reasoning ---
    env_opt("OMNINOVA_REASONING_ENABLED", |v| {
        cfg.runtime.reasoning_enabled = v == "true" || v == "1";
    });
    env_opt("REASONING_ENABLED", |v| {
        cfg.runtime.reasoning_enabled = v == "true" || v == "1";
    });

    // --- Vision ---
    env_opt("OMNINOVA_MODEL_SUPPORT_VISION", |v| {
        cfg.model_support_vision = Some(v == "true" || v == "1");
    });
    env_opt("MODEL_SUPPORT_VISION", |v| {
        if cfg.model_support_vision.is_none() {
            cfg.model_support_vision = Some(v == "true" || v == "1");
        }
    });

    // --- Web search ---
    env_opt("OMNINOVA_WEB_SEARCH_ENABLED", |v| {
        cfg.web_search.enabled = v == "true" || v == "1";
    });
    env_opt("WEB_SEARCH_ENABLED", |v| {
        cfg.web_search.enabled = v == "true" || v == "1";
    });
    env_opt("OMNINOVA_WEB_SEARCH_PROVIDER", |v| {
        cfg.web_search.provider = Some(v);
    });
    env_opt("WEB_SEARCH_PROVIDER", |v| {
        if cfg.web_search.provider.is_none() {
            cfg.web_search.provider = Some(v);
        }
    });
    env_opt("OMNINOVA_BRAVE_API_KEY", |v| {
        cfg.web_search.brave_api_key = Some(v);
    });
    env_opt("BRAVE_API_KEY", |v| {
        if cfg.web_search.brave_api_key.is_none() {
            cfg.web_search.brave_api_key = Some(v);
        }
    });

    // --- Storage ---
    env_opt("OMNINOVA_STORAGE_PROVIDER", |v| {
        cfg.storage.provider.config.provider = Some(v);
    });
    env_opt("OMNINOVA_STORAGE_DB_URL", |v| {
        cfg.storage.provider.config.db_url = Some(v);
    });

    // --- Proxy ---
    env_opt("OMNINOVA_PROXY_ENABLED", |v| {
        cfg.proxy.enabled = v == "true" || v == "1";
    });
    env_opt("OMNINOVA_HTTP_PROXY", |v| cfg.proxy.http_proxy = Some(v));
    env_opt("HTTP_PROXY", |v| {
        if cfg.proxy.http_proxy.is_none() {
            cfg.proxy.http_proxy = Some(v);
        }
    });
    env_opt("OMNINOVA_HTTPS_PROXY", |v| cfg.proxy.https_proxy = Some(v));
    env_opt("HTTPS_PROXY", |v| {
        if cfg.proxy.https_proxy.is_none() {
            cfg.proxy.https_proxy = Some(v);
        }
    });
    env_opt("OMNINOVA_ALL_PROXY", |v| cfg.proxy.all_proxy = Some(v));
    env_opt("ALL_PROXY", |v| {
        if cfg.proxy.all_proxy.is_none() {
            cfg.proxy.all_proxy = Some(v);
        }
    });
    env_opt("OMNINOVA_NO_PROXY", |v| cfg.proxy.no_proxy = Some(v));
    env_opt("NO_PROXY", |v| {
        if cfg.proxy.no_proxy.is_none() {
            cfg.proxy.no_proxy = Some(v);
        }
    });
    env_opt("OMNINOVA_PROXY_SCOPE", |v| cfg.proxy.scope = Some(v));
    env_opt("OMNINOVA_PROXY_SERVICES", |v| {
        cfg.proxy.services = v.split(',').map(|s| s.trim().to_string()).collect();
    });

    // --- Memory ---
    env_opt("OMNINOVA_MEMORY_BACKEND", |v| cfg.memory.backend = v);
    env_opt("QDRANT_URL", |v| cfg.memory.qdrant_url = Some(v));
    env_opt("QDRANT_COLLECTION", |v| cfg.memory.qdrant_collection = Some(v));
    env_opt("QDRANT_API_KEY", |v| cfg.memory.qdrant_api_key = Some(v));
}

fn env_opt(key: &str, apply: impl FnOnce(String)) {
    if let Ok(val) = std::env::var(key) {
        if !val.trim().is_empty() {
            debug!("env override: {key}");
            apply(val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_api_key_override() {
        let mut cfg = Config::default();
        env::set_var("OMNINOVA_API_KEY", "test-key-123");
        apply_env_overrides(&mut cfg);
        assert_eq!(cfg.api_key.as_deref(), Some("test-key-123"));
        env::remove_var("OMNINOVA_API_KEY");
    }

    #[test]
    fn test_gateway_port_override() {
        let mut cfg = Config::default();
        env::set_var("PORT", "9999");
        apply_env_overrides(&mut cfg);
        assert_eq!(cfg.gateway.port, 9999);
        env::remove_var("PORT");
    }
}
