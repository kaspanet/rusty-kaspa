//! Stratum protocol message types and parsing

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stratum request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumRequest {
    pub id: Option<u64>,
    pub method: String,
    pub params: Vec<Value>,
}

/// Stratum response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumResponse {
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<StratumErrorResponse>,
}

/// Stratum error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumErrorResponse {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// Stratum notification/event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumNotification {
    pub method: String,
    pub params: Vec<Value>,
}

/// Mining subscribe request parameters
#[derive(Debug, Clone)]
pub struct MiningSubscribeParams {
    pub user_agent: Option<String>,
    pub protocol: Option<String>,
}

/// Mining authorize request parameters
#[derive(Debug, Clone)]
pub struct MiningAuthorizeParams {
    pub username: String,
    pub password: String,
}

/// Mining submit request parameters
#[derive(Debug, Clone)]
pub struct MiningSubmitParams {
    pub username: String,
    pub job_id: String,
    pub nonce: String,
}

impl TryFrom<&StratumRequest> for MiningSubscribeParams {
    type Error = String;

    fn try_from(req: &StratumRequest) -> Result<Self, Self::Error> {
        if req.method != "mining.subscribe" {
            return Err("Not a mining.subscribe request".to_string());
        }

        let user_agent = req.params.first().and_then(|v| v.as_str()).map(|s| s.to_string());
        let protocol = req.params.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(MiningSubscribeParams { user_agent, protocol })
    }
}

impl TryFrom<&StratumRequest> for MiningAuthorizeParams {
    type Error = String;

    fn try_from(req: &StratumRequest) -> Result<Self, Self::Error> {
        if req.method != "mining.authorize" {
            return Err("Not a mining.authorize request".to_string());
        }

        let username =
            req.params.first().and_then(|v| v.as_str()).ok_or_else(|| "Missing username parameter".to_string())?.to_string();
        let password = req.params.get(1).and_then(|v| v.as_str()).ok_or_else(|| "Missing password parameter".to_string())?.to_string();

        Ok(MiningAuthorizeParams { username, password })
    }
}

impl TryFrom<&StratumRequest> for MiningSubmitParams {
    type Error = String;

    fn try_from(req: &StratumRequest) -> Result<Self, Self::Error> {
        if req.method != "mining.submit" {
            return Err("Not a mining.submit request".to_string());
        }

        let username =
            req.params.first().and_then(|v| v.as_str()).ok_or_else(|| "Missing username parameter".to_string())?.to_string();
        let job_id = req.params.get(1).and_then(|v| v.as_str()).ok_or_else(|| "Missing job_id parameter".to_string())?.to_string();
        let nonce = req.params.get(2).and_then(|v| v.as_str()).ok_or_else(|| "Missing nonce parameter".to_string())?.to_string();

        Ok(MiningSubmitParams { username, job_id, nonce })
    }
}

/// Parse a JSON-RPC message from bytes
pub fn parse_message(data: &[u8]) -> Result<StratumRequest, serde_json::Error> {
    serde_json::from_slice(data)
}

/// Create a success response
pub fn create_success_response(id: Option<u64>, result: Value) -> StratumResponse {
    StratumResponse { id, result: Some(result), error: None }
}

/// Create an error response
pub fn create_error_response(id: Option<u64>, code: i32, message: String) -> StratumResponse {
    StratumResponse { id, result: None, error: Some(StratumErrorResponse { code, message, data: None }) }
}

/// Create a notification message
pub fn create_notification(method: String, params: Vec<Value>) -> StratumNotification {
    StratumNotification { method, params }
}
