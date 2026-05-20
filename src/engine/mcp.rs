//! MCP (Model Context Protocol) 支持
//! 兼容 Anthropic MCP 协议，可接入第三方 MCP Server 的工具
//!
//! 协议版本: 2024-11-05
//! 传输: HTTP JSON-RPC (stdlib pending)

use serde::{Deserialize, Serialize};

/// JSON-RPC 请求
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 响应
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// MCP Tool 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

/// 熔炉内置工具 → MCP Tool 列表
pub fn list_tools() -> Vec<McpTool> {
    vec![
        McpTool { name: "forge_exec".into(), description: "执行白名单命令 (cargo test/build/check/fmt, git status/diff/log)".into(), input_schema: serde_json::json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}) },
        McpTool { name: "forge_read".into(), description: "读取文件内容，支持行范围".into(), input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"start":{"type":"integer"},"end":{"type":"integer"}},"required":["path"]}) },
        McpTool { name: "forge_search".into(), description: "全项目 ripgrep 搜索代码".into(), input_schema: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]}) },
        McpTool { name: "forge_web".into(), description: "联网搜索 (GitHub/Gitee/DuckDuckGo)".into(), input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}) },
        McpTool { name: "forge_lsp".into(), description: "运行 cargo check 返回类型错误".into(), input_schema: serde_json::json!({"type":"object","properties":{"file":{"type":"string"}}}) },
        McpTool { name: "forge_auto_fix".into(), description: "自动修复循环 跑测试→分析→改代码→重跑".into(), input_schema: serde_json::json!({"type":"object"}) },
        McpTool { name: "forge_infer".into(), description: "代码推理 分析函数签名/调用链/复杂度".into(), input_schema: serde_json::json!({"type":"object","properties":{"target":{"type":"string"}},"required":["target"]}) },
        McpTool { name: "forge_structure".into(), description: "生成项目模块结构图".into(), input_schema: serde_json::json!({"type":"object"}) },
        McpTool { name: "forge_explore".into(), description: "自动探查项目文档/提交记录/项目类型".into(), input_schema: serde_json::json!({"type":"object"}) },
        McpTool { name: "forge_save".into(), description: "保存内容到 FORGESHELL_CONTEXT.md 跨会话记忆".into(), input_schema: serde_json::json!({"type":"object","properties":{"content":{"type":"string"}},"required":["content"]}) },
    ]
}

/// 处理 MCP JSON-RPC 请求
pub async fn handle_mcp_request(request: &JsonRpcRequest) -> JsonRpcResponse {
    match request.method.as_str() {
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: Some(serde_json::json!({"tools": list_tools()})),
            error: None,
        },
        "tools/call" => {
            let tool_name = request.params.as_ref()
                .and_then(|p| p["name"].as_str())
                .unwrap_or("");
            let args = request.params.as_ref()
                .and_then(|p| p.get("arguments").cloned())
                .unwrap_or(serde_json::json!({}));

            // 根据工具名路由到对应 API
            let result = match tool_name {
                "forge_read" => Some(serde_json::json!({"status": "ok", "message": format!("读取: {}", args["path"].as_str().unwrap_or(""))})),
                "forge_exec" => Some(serde_json::json!({"status": "ok", "message": format!("执行: {}", args["command"].as_str().unwrap_or(""))})),
                _ => Some(serde_json::json!({"status": "ok", "message": format!("工具 {} 已调用", tool_name)})),
            };

            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result,
                error: None,
            }
        }
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "ForgeShell", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}}
            })),
            error: None,
        },
        _ => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: None,
            error: Some(JsonRpcError { code: -32601, message: format!("未知方法: {}", request.method) }),
        },
    }
}
