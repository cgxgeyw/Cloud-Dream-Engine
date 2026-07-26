use crate::models::mcp_server::McpServerConfig;
use crate::models::mcp_tool::McpToolDefinition;
use serde_json::Value;

/// 一次 MCP 工具调用的结果，直接摊进导演的 tool_results 留痕。
#[derive(Debug, Clone)]
pub struct McpCallOutcome {
    pub ok: bool,
    pub result: Value,
    pub error: Option<String>,
    pub truncated: bool,
}

impl McpCallOutcome {
    fn failed(error: String) -> Self {
        Self {
            ok: false,
            result: Value::Null,
            error: Some(error),
            truncated: false,
        }
    }
}

/// 执行一次自定义 MCP 工具调用：校验 → 连接 → tools/call → 结果裁剪。
/// 超时由 client 层按 server 配置控制；这里只负责结果大小上限。
pub async fn execute_mcp_tool_call(
    tool: &McpToolDefinition,
    server: &McpServerConfig,
    arguments: Value,
) -> McpCallOutcome {
    if !tool.enabled {
        return McpCallOutcome::failed(format!("MCP 工具未启用：{}", tool.name));
    }
    if let Err(error) = server.validate_ready() {
        return McpCallOutcome::failed(error);
    }
    let remote_tool_name = if tool.tool_name.trim().is_empty() {
        tool.name.trim()
    } else {
        tool.tool_name.trim()
    };
    match super::client::call_server_tool(server, remote_tool_name, arguments).await {
        Ok(result) => {
            let (result, truncated) = cap_result_size(result, server.resolved_max_result_bytes());
            McpCallOutcome {
                ok: true,
                result,
                error: None,
                truncated,
            }
        }
        Err(error) => McpCallOutcome::failed(error),
    }
}

/// 只保留「可执行」的工具：内置工具原样保留，自定义工具必须绑定一个
/// 已启用、本平台支持、必填字段齐全的 server。执行器落地后才下发给模型（第 7 项）。
pub fn filter_executable_tools(
    tools: &[McpToolDefinition],
    servers: &[McpServerConfig],
) -> Vec<McpToolDefinition> {
    tools
        .iter()
        .filter(|tool| {
            if crate::models::mcp_tool::is_builtin_mcp_tool_id(&tool.id) {
                return true;
            }
            servers
                .iter()
                .find(|server| server.id == tool.server_id)
                .map(|server| server.validate_ready().is_ok())
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// 结果超过上限时，替换为截断说明 + 前缀文本，避免把上下文冲爆。
fn cap_result_size(result: Value, max_bytes: usize) -> (Value, bool) {
    let serialized = serde_json::to_string(&result).unwrap_or_default();
    if serialized.len() <= max_bytes {
        return (result, false);
    }
    let mut prefix = String::new();
    for ch in serialized.chars() {
        if prefix.len() + ch.len_utf8() > max_bytes {
            break;
        }
        prefix.push(ch);
    }
    (
        serde_json::json!({
            "truncated": true,
            "original_bytes": serialized.len(),
            "max_bytes": max_bytes,
            "preview": prefix,
        }),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, transport: &str, enabled: bool) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            transport: transport.to_string(),
            command: "node".to_string(),
            args: vec![],
            env: serde_json::json!({}),
            url: "https://example.test/mcp".to_string(),
            headers: serde_json::json!({}),
            auth_token: String::new(),
            enabled,
            timeout_ms: 15_000,
            max_result_bytes: 32_768,
        }
    }

    fn tool(id: &str, server_id: &str) -> McpToolDefinition {
        McpToolDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            server_name: String::new(),
            tool_name: "remote_tool".to_string(),
            enabled: true,
            exposure_policy: serde_json::json!({}),
            risk_level: "low".to_string(),
            trigger_keywords: vec![],
            input_schema: serde_json::json!({ "type": "object" }),
            server_id: server_id.to_string(),
        }
    }

    #[test]
    fn builtin_tools_stay_exposed_without_server_binding() {
        let tools = vec![tool("mcp-tool-change-scene", "")];
        let filtered = filter_executable_tools(&tools, &[]);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn custom_tools_need_a_ready_server() {
        let tools = vec![
            tool("custom-bound", "srv-http"),
            tool("custom-unbound", ""),
            tool("custom-disabled", "srv-off"),
        ];
        let servers = vec![
            server("srv-http", "http", true),
            server("srv-off", "http", false),
        ];
        let filtered = filter_executable_tools(&tools, &servers);
        let ids: Vec<&str> = filtered.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["custom-bound"]);
    }

    #[test]
    fn http_transport_is_supported_everywhere() {
        assert!(server("srv", "http", true).validate_ready().is_ok());
    }

    #[test]
    fn stdio_ready_state_follows_platform_support() {
        let config = server("srv", "stdio", true);
        let ready = config.validate_ready();
        if cfg!(target_os = "android") {
            // 安卓无法起子进程，必须给出明确不支持提示而不是静默失败。
            let error = ready.expect_err("stdio should be rejected on android");
            assert!(error.contains("本平台不支持 stdio 传输"));
        } else {
            assert!(ready.is_ok());
        }
    }

    #[test]
    fn unknown_transport_is_rejected() {
        let mut config = server("srv", "http", true);
        config.transport = "grpc".to_string();
        assert!(config.validate_ready().is_err());
    }

    #[test]
    fn keeps_small_results_intact() {
        let value = serde_json::json!({ "content": "ok" });
        let (result, truncated) = cap_result_size(value.clone(), 1024);
        assert_eq!(result, value);
        assert!(!truncated);
    }

    #[test]
    fn caps_oversized_results_with_preview() {
        let value = serde_json::json!({ "content": "x".repeat(500) });
        let (result, truncated) = cap_result_size(value, 100);
        assert!(truncated);
        assert_eq!(result.get("truncated").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("max_bytes").and_then(Value::as_u64), Some(100));
        let preview = result.get("preview").and_then(Value::as_str).unwrap();
        assert!(preview.len() <= 100);
    }
}
