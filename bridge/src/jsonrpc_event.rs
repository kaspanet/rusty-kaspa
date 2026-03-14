use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stratum method types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StratumMethod {
    #[serde(rename = "mining.subscribe")]
    Subscribe,
    #[serde(rename = "mining.extranonce.subscribe")]
    ExtranonceSubscribe,
    #[serde(rename = "mining.authorize")]
    Authorize,
    #[serde(rename = "mining.submit")]
    Submit,
    #[serde(rename = "mining.set_difficulty")]
    SetDifficulty,
    #[serde(rename = "mining.notify")]
    Notify,
    #[serde(rename = "mining.set_extranonce")]
    SetExtranonce,
    #[serde(untagged)]
    Other(String),
}

impl From<&str> for StratumMethod {
    fn from(s: &str) -> Self {
        match s {
            "mining.subscribe" => StratumMethod::Subscribe,
            "mining.extranonce.subscribe" => StratumMethod::ExtranonceSubscribe,
            "mining.authorize" => StratumMethod::Authorize,
            "mining.submit" => StratumMethod::Submit,
            "mining.set_difficulty" => StratumMethod::SetDifficulty,
            "mining.notify" => StratumMethod::Notify,
            "mining.set_extranonce" => StratumMethod::SetExtranonce,
            other => StratumMethod::Other(other.to_string()),
        }
    }
}

impl From<StratumMethod> for String {
    fn from(m: StratumMethod) -> Self {
        match m {
            StratumMethod::Subscribe => "mining.subscribe".to_string(),
            StratumMethod::ExtranonceSubscribe => "mining.extranonce.subscribe".to_string(),
            StratumMethod::Authorize => "mining.authorize".to_string(),
            StratumMethod::Submit => "mining.submit".to_string(),
            StratumMethod::SetDifficulty => "mining.set_difficulty".to_string(),
            StratumMethod::Notify => "mining.notify".to_string(),
            StratumMethod::SetExtranonce => "mining.set_extranonce".to_string(),
            StratumMethod::Other(s) => s,
        }
    }
}

/// JSON-RPC event (request from client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcEvent {
    /// ID can be null, string, or number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(default = "default_version")]
    pub jsonrpc: String,
    pub method: String, // We'll parse this as string and convert to StratumMethod when needed
    pub params: Vec<Value>,
}

fn default_version() -> String {
    "2.0".to_string()
}

impl JsonRpcEvent {
    pub fn new(id: Option<String>, method: &str, params: Vec<Value>) -> Self {
        Self { id: id.map(Value::String), jsonrpc: "2.0".to_string(), method: method.to_string(), params }
    }

    pub fn method_enum(&self) -> StratumMethod {
        StratumMethod::from(self.method.as_str())
    }
}

/// JSON-RPC response (to client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// ID can be null, string, or number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Vec<Value>>,
}

impl JsonRpcResponse {
    pub fn new(event: &JsonRpcEvent, result: Option<Value>, error: Option<Vec<Value>>) -> Self {
        Self { id: event.id.clone(), result, error }
    }

    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self { id, result: Some(result), error: None }
    }

    pub fn error(id: Option<Value>, code: i32, message: &str, data: Option<Value>) -> Self {
        let mut error_vec = vec![Value::Number(code.into()), Value::String(message.to_string())];
        if let Some(d) = data {
            error_vec.push(d);
        } else {
            error_vec.push(Value::Null);
        }
        Self { id, result: None, error: Some(error_vec) }
    }
}

/// Sanitize JSON string by removing or replacing control characters
/// Control characters (0x00-0x1F) are not allowed in JSON strings except when escaped.
/// Tabs and other control characters inside JSON string values must be escaped or removed.
/// We replace them with spaces to preserve the structure while making it valid JSON.
fn sanitize_json_input(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            // Control characters (0x00-0x1F) are not allowed in JSON strings
            // Keep newline and carriage return for line endings, replace others with space
            // This handles cases like Goldshell ASICs that send tab characters in JSON strings
            if c.is_control() && c != '\n' && c != '\r' {
                ' ' // Replace with space to preserve JSON structure
            } else {
                c
            }
        })
        .collect()
}

/// Unmarshal a JSON-RPC event from a string
/// Automatically sanitizes control characters that are invalid in JSON
pub fn unmarshal_event(input: &str) -> Result<JsonRpcEvent, serde_json::Error> {
    // Check if sanitization is needed
    let needs_sanitization = input.chars().any(|c| c.is_control() && c != '\n' && c != '\r');

    if needs_sanitization {
        let sanitized = sanitize_json_input(input);
        // Try parsing sanitized version
        match serde_json::from_str(&sanitized) {
            Ok(result) => {
                tracing::debug!("JSON input sanitized (control characters replaced with spaces)");
                Ok(result)
            }
            Err(e) => {
                // If sanitized version still fails, return original error
                tracing::warn!("JSON sanitization applied but parsing still failed: {}", e);
                Err(e)
            }
        }
    } else {
        // No sanitization needed, parse directly
        serde_json::from_str(input)
    }
}

/// Unmarshal a JSON-RPC response from a string
pub fn unmarshal_response(input: &str) -> Result<JsonRpcResponse, serde_json::Error> {
    serde_json::from_str(input)
}
