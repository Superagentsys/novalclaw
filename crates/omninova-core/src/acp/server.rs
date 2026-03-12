use crate::acp::types::{AcpRequest, AcpResponse, AcpError};
use anyhow::Result;

pub struct AcpServer {
    // Handlers would go here
}

impl AcpServer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn handle(&self, request: AcpRequest) -> Result<AcpResponse> {
        // Dispatch based on method
        let result = match request.method.as_str() {
            "ping" => Some(serde_json::json!("pong")),
            _ => None, // Method not found
        };

        if let Some(res) = result {
            Ok(AcpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(res),
                error: None,
            })
        } else {
            Ok(AcpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(AcpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
            })
        }
    }
}
