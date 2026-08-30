use crate::acp::types::{AcpRequest, AcpResponse};
use anyhow::Result;

pub struct AcpClient {
    // In a real implementation, this might hold a connection or channel
}

impl AcpClient {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn call(&self, request: AcpRequest) -> Result<AcpResponse> {
        // Mock implementation
        Ok(AcpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(serde_json::Value::Null),
            error: None,
        })
    }
}
