use crate::protocol::*;
use crate::tools;
use serde_json::{json, Value};
use wake_core::Database;

pub struct McpServer {
    db: Database,
    initialized: bool,
}

impl McpServer {
    pub fn new(db: Database) -> Self {
        Self { db, initialized: false }
    }

    pub fn handle_message(&mut self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Notifications (no id) don't get responses
        if request.id.is_none() {
            self.handle_notification(&request.method);
            return None;
        }

        let id = request.id.clone();

        // Before initialization, only allow initialize
        if !self.initialized && request.method != "initialize" {
            return Some(JsonRpcResponse::error(id, INVALID_REQUEST, "Server not initialized"));
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(request.params),
            "ping" => Ok(json!({})),
            _ => Err((METHOD_NOT_FOUND, format!("Unknown method: {}", request.method))),
        };

        Some(match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        })
    }

    fn handle_notification(&mut self, method: &str) {
        match method {
            "notifications/initialized" => {
                self.initialized = true;
            }
            "notifications/cancelled" => {
                // Ignore cancellation requests
            }
            _ => {
                // Unknown notifications are ignored per spec
            }
        }
    }

    fn handle_initialize(&mut self, params: Value) -> Result<Value, (i32, String)> {
        let _params: InitializeParams =
            serde_json::from_value(params).map_err(|e| (INVALID_PARAMS, e.to_string()))?;

        // We accept any protocol version and respond with what we support
        let result = InitializeResult {
            protocol_version: "2024-11-05",
            capabilities: ServerCapabilities { tools: ToolsCapability { list_changed: false } },
            server_info: ServerInfo { name: "wake-mcp", version: env!("CARGO_PKG_VERSION") },
        };

        // Mark as initialized after successful initialize response
        // (client should send notifications/initialized next)
        self.initialized = true;

        serde_json::to_value(result).map_err(|e| (INTERNAL_ERROR, e.to_string()))
    }

    fn handle_tools_list(&self) -> Result<Value, (i32, String)> {
        let result = ToolsListResult { tools: tools::list_tools() };
        serde_json::to_value(result).map_err(|e| (INTERNAL_ERROR, e.to_string()))
    }

    fn handle_tools_call(&self, params: Value) -> Result<Value, (i32, String)> {
        let call: ToolCallParams =
            serde_json::from_value(params).map_err(|e| (INVALID_PARAMS, e.to_string()))?;

        let result = tools::call_tool(&self.db, &call.name, call.arguments);

        serde_json::to_value(result).map_err(|e| (INTERNAL_ERROR, e.to_string()))
    }
}
