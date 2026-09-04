//! 本地工具执行器：impl_kind = "builtin_http" 的工具由 Rust 核心直接发起 HTTP，
//! 不经过外部 MCP server，因此桌面端与 Android 均可使用。
//!
//! impl_config 两种模式：
//! - generic（基础 HTTP 工具）：模型在调用参数里直接给 method/url/headers/params/body；
//!   配置里可用 allowed_hosts 限制可访问域名（空 = 不限制）。
//! - template（数据接口工具）：配置里写死 method/url/query/headers，字符串中的
//!   {{参数名}} 由调用入参替换（URL 编码）；可用 charset 指定响应编码（如 gbk），
//!   用 extractor 把响应整理成结构化 JSON。

use crate::models::mcp_tool::McpToolDefinition;
use serde_json::Value;

use super::executor::{cap_result_size, McpCallOutcome};

const LOCAL_TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const LOCAL_TOOL_MAX_RESULT_BYTES: usize = 32_768;

pub async fn execute_local_http_tool(tool: &McpToolDefinition, arguments: Value) -> McpCallOutcome {
    if !tool.enabled {
        return McpCallOutcome::failed(format!("工具未启用：{}", tool.name));
    }
    let config = if tool.impl_config.is_object() {
        tool.impl_config.clone()
    } else {
        serde_json::json!({})
    };
    let mode = config
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("generic")
        .trim()
        .to_string();
    let result = match mode.as_str() {
        "template" => execute_template(&config, &arguments).await,
        _ => execute_generic(&config, &arguments).await,
    };
    match result {
        Ok(value) => {
            let (result, truncated) = cap_result_size(value, LOCAL_TOOL_MAX_RESULT_BYTES);
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

// ---- generic：模型自由指定请求 ----

async fn execute_generic(config: &Value, arguments: &Value) -> Result<Value, String> {
    let url = arguments
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "缺少必填参数 url".to_string())?;
    check_allowed_hosts(config, url)?;
    let method = arguments
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let headers = arguments.get("headers").cloned().unwrap_or(Value::Null);
    let params = arguments.get("params").cloned().unwrap_or(Value::Null);
    let body = arguments.get("body").cloned();
    let response = send_request(method, url, &headers, &params, body).await?;
    decode_response(response, config.get("charset").and_then(Value::as_str), &Value::Null)
        .await
}

// ---- template：配置写死请求，入参只做占位替换 ----

async fn execute_template(config: &Value, arguments: &Value) -> Result<Value, String> {
    let url_template = config
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "工具配置缺少 url".to_string())?;
    let url = substitute_placeholders(url_template, arguments);
    check_allowed_hosts(config, &url)?;
    let method = config
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let headers = config.get("headers").cloned().unwrap_or(Value::Null);
    let mut query = serde_json::Map::new();
    if let Some(object) = config.get("query").and_then(Value::as_object) {
        for (key, value) in object {
            let rendered = match value {
                Value::String(text) => Value::String(substitute_placeholders(text, arguments)),
                other => other.clone(),
            };
            query.insert(key.clone(), rendered);
        }
    }
    let response = send_request(method, &url, &headers, &Value::Object(query), None).await?;
    let extractor = config.get("extractor").cloned().unwrap_or(Value::Null);
    let extractor = substitute_extractor_placeholders(&extractor, arguments);
    decode_response(
        response,
        config.get("charset").and_then(Value::as_str),
        &extractor,
    )
    .await
}

// ---- 请求与解析 ----

async fn send_request(
    method: &str,
    url: &str,
    headers: &Value,
    params: &Value,
    body: Option<Value>,
) -> Result<reqwest::Response, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("仅支持 http/https 地址：{url}"));
    }
    let method = reqwest::Method::from_bytes(method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|_| format!("不支持的 HTTP 方法：{method}"))?;
    let client = reqwest::Client::builder()
        .timeout(LOCAL_TOOL_TIMEOUT)
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))?;
    let mut request = client.request(method, url);
    if let Some(object) = headers.as_object() {
        for (key, value) in object {
            if let Some(text) = value.as_str() {
                request = request.header(key, text);
            }
        }
    }
    if let Some(object) = params.as_object() {
        let pairs: Vec<(String, String)> = object
            .iter()
            .map(|(key, value)| {
                let text = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (key.clone(), text)
            })
            .collect();
        request = request.query(&pairs);
    }
    if let Some(body) = body {
        if !body.is_null() {
            request = request.json(&body);
        }
    }
    request
        .send()
        .await
        .map_err(|error| format!("HTTP 请求失败：{error}"))
}

async fn decode_response(
    response: reqwest::Response,
    charset: Option<&str>,
    extractor: &Value,
) -> Result<Value, String> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取响应失败：{error}"))?;
    let text = decode_bytes(&bytes, charset);
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", truncate_for_error(&text)));
    }
    apply_extractor(&text, extractor)
}

fn decode_bytes(bytes: &[u8], charset: Option<&str>) -> String {
    match charset.map(|value| value.trim().to_ascii_lowercase()) {
        Some(name) if name == "gbk" || name == "gb18030" || name == "gb2312" => {
            let (text, _, _) = encoding_rs::GBK.decode(bytes);
            text.into_owned()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn apply_extractor(text: &str, extractor: &Value) -> Result<Value, String> {
    let kind = extractor
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("text")
        .trim()
        .to_string();
    match kind.as_str() {
        "json" => {
            let value: Value = serde_json::from_str(text)
                .map_err(|error| format!("响应不是合法 JSON：{error}"))?;
            let path = extractor
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let picked = if path.is_empty() {
                value
            } else {
                pick_json_path(&value, path)
                    .cloned()
                    .ok_or_else(|| format!("JSON 路径不存在：{path}"))?
            };
            Ok(apply_tail(picked, extractor.get("tail")))
        }
        "split_map" => apply_split_map(text, extractor),
        _ => Ok(Value::String(text.trim().to_string())),
    }
}

/// 先用正则抓出一段文本，再按分隔符切开，把指定下标映射成字段名。
/// fields 形如 {"1": "名称", "3": "现价"}。
fn apply_split_map(text: &str, extractor: &Value) -> Result<Value, String> {
    let pattern = extractor
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or_else(|| "split_map 缺少 pattern".to_string())?;
    let regex = regex::Regex::new(pattern)
        .map_err(|error| format!("split_map pattern 无效：{error}"))?;
    let segment = regex
        .captures(text)
        .and_then(|captures| captures.get(1).or_else(|| captures.get(0)))
        .map(|item| item.as_str())
        .ok_or_else(|| "split_map 未匹配到内容".to_string())?;
    let delimiter = extractor
        .get("delimiter")
        .and_then(Value::as_str)
        .unwrap_or("~");
    let parts: Vec<&str> = segment.split(delimiter).collect();
    let fields = extractor
        .get("fields")
        .and_then(Value::as_object)
        .ok_or_else(|| "split_map 缺少 fields".to_string())?;
    let mut output = serde_json::Map::new();
    for (index_text, name) in fields {
        let index: usize = index_text
            .trim()
            .parse()
            .map_err(|_| format!("split_map fields 下标无效：{index_text}"))?;
        let field_name = name
            .as_str()
            .ok_or_else(|| format!("split_map fields 字段名无效：{index_text}"))?;
        if let Some(value) = parts.get(index) {
            output.insert(
                field_name.to_string(),
                Value::String(value.trim().to_string()),
            );
        }
    }
    Ok(Value::Object(output))
}

/// 点分路径取值，数字段按数组下标处理，例如 "data.klines" 或 "result.data.0"。
fn pick_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        current = match current {
            Value::Object(object) => object.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// 数组结果只保留末尾 N 条（按时间升序的接口，末尾才是最近数据）。
/// 兜底接口忽略条数参数、返回全量历史的情况：不截尾时 32KB 大小上限会把
/// 结果从头部截断，模型只能看到最早的历史数据（如日 K 只剩上市初期）。
/// 上限 320 条：K 线类数据每条约 75 字节，320 条约 24KB，仍在结果大小上限内。
const EXTRACTOR_TAIL_MAX_ITEMS: usize = 320;

fn apply_tail(value: Value, tail: Option<&Value>) -> Value {
    let Some(limit) = tail.and_then(parse_tail_count) else {
        return value;
    };
    let limit = limit.min(EXTRACTOR_TAIL_MAX_ITEMS);
    match value {
        Value::Array(items) if items.len() > limit => {
            Value::Array(items[items.len() - limit..].to_vec())
        }
        other => other,
    }
}

fn parse_tail_count(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number.as_u64().map(|count| count as usize),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
    .filter(|count| *count > 0)
}

/// extractor 的顶层字符串值同样支持 {{参数名}} 占位替换（如 "tail": "{{lmt}}"）。
fn substitute_extractor_placeholders(extractor: &Value, arguments: &Value) -> Value {
    match extractor {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let rendered = match value {
                        Value::String(text) => {
                            Value::String(substitute_placeholders(text, arguments))
                        }
                        other => other.clone(),
                    };
                    (key.clone(), rendered)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn check_allowed_hosts(config: &Value, url: &str) -> Result<(), String> {
    let Some(hosts) = config.get("allowed_hosts").and_then(Value::as_array) else {
        return Ok(());
    };
    let hosts: Vec<&str> = hosts.iter().filter_map(Value::as_str).collect();
    if hosts.is_empty() {
        return Ok(());
    }
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .and_then(|authority| authority.split(':').next())
        .unwrap_or("")
        .to_ascii_lowercase();
    if hosts.iter().any(|item| host == item.to_ascii_lowercase()) {
        Ok(())
    } else {
        Err(format!("目标域名不在工具允许范围内：{host}"))
    }
}

/// 替换 {{参数名}} 占位符；替换值按 query 组件规则 percent-encode。
fn substitute_placeholders(template: &str, arguments: &Value) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let key = after[..end].trim();
                let value = arguments
                    .get(key)
                    .map(|value| match value {
                        Value::String(text) => text.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                output.push_str(&percent_encode(&value));
                rest = &after[end + 2..];
            }
            None => {
                output.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    output.push_str(rest);
    output
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn truncate_for_error(text: &str) -> String {
    text.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn substitutes_and_encodes_placeholders() {
        let args = json!({"code": "sh600519", "keyword": "贵州 茅台"});
        assert_eq!(
            substitute_placeholders("https://example.test/q={{code}}&k={{keyword}}", &args),
            "https://example.test/q=sh600519&k=%E8%B4%B5%E5%B7%9E%20%E8%8C%85%E5%8F%B0"
        );
        assert_eq!(
            substitute_placeholders("q={{missing}}", &args),
            "q="
        );
    }

    #[test]
    fn decodes_gbk_bytes() {
        // "贵州茅台" 的 GBK 编码。
        let bytes = [0xB9u8, 0xF3, 0xD6, 0xDD, 0xC3, 0xA9, 0xCC, 0xA8];
        assert_eq!(decode_bytes(&bytes, Some("gbk")), "贵州茅台");
        assert_eq!(decode_bytes("贵州茅台".as_bytes(), None), "贵州茅台");
    }

    #[test]
    fn json_extractor_follows_dot_path() {
        let text = r#"{"data":{"klines":["a","b"]}}"#;
        let extractor = json!({"type": "json", "path": "data.klines"});
        assert_eq!(
            apply_extractor(text, &extractor).expect("extract"),
            json!(["a", "b"])
        );
    }

    #[test]
    fn json_extractor_tail_keeps_latest_items() {
        // 接口忽略条数参数返回全量历史（时间升序）时，tail 保留末尾的最近数据。
        let items: Vec<String> = (0..500).map(|index| format!("bar-{index}")).collect();
        let text = serde_json::json!({"data": {"klines": items}}).to_string();
        let extractor = json!({"type": "json", "path": "data.klines", "tail": 60});
        let result = apply_extractor(&text, &extractor).expect("extract");
        let array = result.as_array().expect("array");
        assert_eq!(array.len(), 60);
        assert_eq!(array.first().unwrap(), &json!("bar-440"));
        assert_eq!(array.last().unwrap(), &json!("bar-499"));
    }

    #[test]
    fn json_extractor_tail_accepts_numeric_string_and_clamps() {
        let items: Vec<String> = (0..500).map(|index| format!("bar-{index}")).collect();
        let text = serde_json::json!({"data": {"klines": items}}).to_string();
        // 占位替换后 tail 是字符串；超过 320 时被钳制，避免结果再次触碰大小上限。
        let extractor = json!({"type": "json", "path": "data.klines", "tail": "500"});
        let result = apply_extractor(&text, &extractor).expect("extract");
        assert_eq!(result.as_array().expect("array").len(), 320);
    }

    #[test]
    fn json_extractor_tail_ignores_non_array_and_invalid_value() {
        let text = r#"{"data":{"name":"贵州茅台"}}"#;
        let extractor = json!({"type": "json", "path": "data", "tail": 10});
        assert_eq!(
            apply_extractor(text, &extractor).expect("extract"),
            json!({"name": "贵州茅台"})
        );
        let extractor = json!({"type": "json", "path": "data", "tail": ""});
        assert_eq!(
            apply_extractor(text, &extractor).expect("extract"),
            json!({"name": "贵州茅台"})
        );
    }

    #[test]
    fn extractor_placeholders_are_substituted() {
        let extractor = json!({"type": "json", "path": "data.klines", "tail": "{{lmt}}"});
        let args = json!({"lmt": 60});
        assert_eq!(
            substitute_extractor_placeholders(&extractor, &args),
            json!({"type": "json", "path": "data.klines", "tail": "60"})
        );
    }

    #[test]
    fn split_map_maps_indexes_to_fields() {
        let text = r#"v_sh600519="1~贵州茅台~600519~1358.98~1350.60";"#;
        let extractor = json!({
            "type": "split_map",
            "pattern": "v_\\w+=\"([^\"]*)\"",
            "delimiter": "~",
            "fields": {"1": "名称", "3": "现价", "4": "昨收"}
        });
        assert_eq!(
            apply_extractor(text, &extractor).expect("extract"),
            json!({"名称": "贵州茅台", "现价": "1358.98", "昨收": "1350.60"})
        );
    }

    #[test]
    fn allowed_hosts_blocks_unlisted_domains() {
        let config = json!({"allowed_hosts": ["qt.gtimg.cn"]});
        assert!(check_allowed_hosts(&config, "https://qt.gtimg.cn/q=sh600519").is_ok());
        assert!(check_allowed_hosts(&config, "https://evil.test/").is_err());
        // 未配置 allowed_hosts = 不限制。
        assert!(check_allowed_hosts(&json!({}), "https://evil.test/").is_ok());
    }
}
