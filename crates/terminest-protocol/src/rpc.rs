use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard JSON-RPC 2.0 version string.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard and domain-specific JSON-RPC error codes.
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // TermiNest domain error codes (-32000 to -32099)
    pub const SESSION_UNAVAILABLE: i32 = -32000;
    pub const EXECUTION_TIMEOUT: i32 = -32001;
    pub const CAPTURE_FAILURE: i32 = -32002;
    pub const INPUT_FAILURE: i32 = -32003;
    pub const PERMISSION_DENIED: i32 = -32004;
}

/// A JSON-RPC 2.0 request identifier (string or integer).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
}

/// A standard JSON-RPC 2.0 Request object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest<T = serde_json::Value> {
    pub jsonrpc: String,
    pub method: String,
    pub params: T,
    pub id: RequestId,
}

impl<T> JsonRpcRequest<T> {
    pub fn new(method: impl Into<String>, params: T, id: RequestId) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
            id,
        }
    }
}

/// A standard JSON-RPC 2.0 Response object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse<R = serde_json::Value> {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: RequestId,
}

impl<R> JsonRpcResponse<R> {
    pub fn success(id: RequestId, result: R) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: RequestId, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

/// A standard JSON-RPC 2.0 Error object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i32, message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            error_codes::METHOD_NOT_FOUND,
            format!("Method '{}' not found", method),
        )
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(error_codes::INVALID_PARAMS, msg)
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::new(error_codes::INTERNAL_ERROR, msg)
    }
}

/// 2D coordinate point on guest display surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

// ---------------------------------------------------------------------------
// Typed Control Payloads for TNST-v1 Channel 0 RPC
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: u32,
    pub username: String,
    pub desktop_env: String,
    pub display_width: u32,
    pub display_height: u32,
    pub dpi: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandshakeParams {
    pub version: String,
    pub os_family: String,
    pub os_distro: String,
    pub kernel: String,
    pub session: SessionInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandshakeResult {
    pub status: String,
    pub host_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_cid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PingParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingResult {
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseClickParams {
    pub x: i32,
    pub y: i32,
    pub button: String,
    pub clicks: u32,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseClickResult {
    pub status: String,
    pub cursor_position: Point,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseMoveParams {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseMoveResult {
    pub status: String,
    pub cursor_position: Point,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseDragParams {
    pub start_x: i32,
    pub start_y: i32,
    pub end_x: i32,
    pub end_y: i32,
    #[serde(default = "default_mouse_button")]
    pub button: String,
}

fn default_mouse_button() -> String {
    "left".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseDragResult {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeySequenceParams {
    pub key: String,
    #[serde(default)]
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeySequenceResult {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeTextParams {
    pub text: String,
    #[serde(default)]
    pub delay_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeTextResult {
    pub status: String,
    pub characters_typed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecParams {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_timeout_ms() -> u64 {
    30000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureFrameParams {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_quality")]
    pub quality: u8,
}

fn default_format() -> String {
    "jpeg".to_string()
}

fn default_quality() -> u8 {
    80
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureFrameResult {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamStartParams {
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_fps() -> u32 {
    30
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamStartResult {
    pub status: String,
    pub target_fps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamStopParams {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamStopResult {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardReadParams {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardReadResult {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardWriteParams {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardWriteResult {
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_request_serialization() {
        let req = JsonRpcRequest::new(
            "guest.handshake",
            HandshakeParams {
                version: "1.0.0".to_string(),
                os_family: "linux".to_string(),
                os_distro: "Ubuntu 24.04 LTS".to_string(),
                kernel: "6.8.0-40-generic".to_string(),
                session: SessionInfo {
                    session_id: 1,
                    username: "terminest".to_string(),
                    desktop_env: "wayland".to_string(),
                    display_width: 1920,
                    display_height: 1080,
                    dpi: 96,
                },
            },
            RequestId::String("init-001".to_string()),
        );

        let json = serde_json::to_string(&req).expect("Failed to serialize handshake request");
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"guest.handshake\""));
        assert!(json.contains("\"os_distro\":\"Ubuntu 24.04 LTS\""));

        let deserialized: JsonRpcRequest<HandshakeParams> =
            serde_json::from_str(&json).expect("Failed to deserialize handshake request");
        assert_eq!(deserialized, req);
    }

    #[test]
    fn test_mouse_click_roundtrip() {
        let req = JsonRpcRequest::new(
            "desktop.mouse_click",
            MouseClickParams {
                x: 450,
                y: 280,
                button: "left".to_string(),
                clicks: 1,
                modifiers: vec![],
            },
            RequestId::String("req-042".to_string()),
        );

        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest<MouseClickParams> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.params.x, 450);
        assert_eq!(parsed.params.y, 280);

        let resp = JsonRpcResponse::success(
            RequestId::String("req-042".to_string()),
            MouseClickResult {
                status: "success".to_string(),
                cursor_position: Point { x: 450, y: 280 },
            },
        );

        let resp_json = serde_json::to_string(&resp).unwrap();
        let parsed_resp: JsonRpcResponse<MouseClickResult> =
            serde_json::from_str(&resp_json).unwrap();
        assert_eq!(parsed_resp.result.unwrap().cursor_position.x, 450);
    }

    #[test]
    fn test_error_response_serialization() {
        let err_resp: JsonRpcResponse<()> = JsonRpcResponse::error(
            RequestId::Number(99),
            JsonRpcError::new(error_codes::SESSION_UNAVAILABLE, "Session is locked"),
        );

        let json = serde_json::to_string(&err_resp).unwrap();
        assert!(json.contains("\"code\":-32000"));
        assert!(json.contains("Session is locked"));
        assert!(!json.contains("\"result\""));
    }
}
