use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::gateway::GatewayRuntime;

pub async fn pairing_guard(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let cfg = runtime.get_config().await;
    if !cfg.gateway.require_pairing {
        return Ok(next.run(request).await);
    }

    let path = request.uri().path();
    if is_public_path(path) {
        return Ok(next.run(request).await);
    }

    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        Some(_token) => {
            // Permissive for now - accept any Bearer token when pairing is enabled.
            // Full implementation would verify against stored pairing secrets.
            Ok(next.run(request).await)
        }
        None => {
            let query_token = request
                .uri()
                .query()
                .and_then(|q| {
                    q.split('&')
                        .find_map(|pair| pair.strip_prefix("token="))
                });
            match query_token {
                Some(_) => Ok(next.run(request).await),
                None => Err(StatusCode::UNAUTHORIZED),
            }
        }
    }
}

fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/health"
            | "/pair"
            | "/webhook"
            | "/webhook/wechat"
            | "/webhook/feishu"
            | "/webhook/lark"
            | "/webhook/dingtalk"
    )
}
