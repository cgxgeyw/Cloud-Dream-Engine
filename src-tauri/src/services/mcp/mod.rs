//! MCP（Model Context Protocol）客户端与执行器（第 7 项）。
//!
//! - `client`：JSON-RPC 2.0 over stdio / Streamable HTTP，负责 initialize、tools/list、tools/call。
//! - `executor`：把世界包声明的工具调用映射到已配置的 server，附带超时与结果大小上限。

pub mod client;
pub mod executor;

pub use executor::execute_mcp_tool_call;
