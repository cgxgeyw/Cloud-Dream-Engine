//! 交互消息（第 5 项）：角色消息信封中的 interaction 字段。
//!
//! 信封：`{ speaker, content, narration, interaction? }`。
//! 模型返回 interaction 候选 → 宿主按类型结构校验（且世界包已声明允许的类型）
//! → 持久化到消息 metadata.interaction → 宿主渲染 → 玩家回答 → 幂等标记已回答
//! → 触发可选 interaction_answered 事件 → 把可读答案作为玩家消息提交下一回合。
//! 信封不放可执行 effects：状态变更继续走结构化提议 → Orchestrator 校验链路。

use serde::{Deserialize, Serialize};

pub const INTERACTION_KIND_CHOICE: &str = "choice";
pub const INTERACTION_KIND_MULTI_CHOICE: &str = "multi_choice";
pub const INTERACTION_KIND_FORM: &str = "form";
pub const INTERACTION_KIND_CONFIRM: &str = "confirm";
pub const INTERACTION_KIND_SLIDER: &str = "slider";

pub const INTERACTION_STATUS_PENDING: &str = "pending";
pub const INTERACTION_STATUS_ANSWERED: &str = "answered";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInteraction {
    pub interaction_id: String,
    pub kind: String,
    /// 展示给玩家的问题/说明。
    pub prompt: String,
    /// 类型相关配置：choice/multi_choice 为 options，form 为 fields，slider 为 min/max/step。
    pub config: serde_json::Value,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<String>,
}

fn clean_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn normalize_options(raw: &serde_json::Value) -> Result<serde_json::Value, String> {
    let items = raw
        .as_array()
        .ok_or_else(|| "options must be an array".to_string())?;
    let mut options = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let (id, label) = if let Some(text) = item.as_str() {
            (format!("opt-{index}"), text.trim().to_string())
        } else if item.is_object() {
            let label = item.get("label").map(clean_text).unwrap_or_default();
            let id = item
                .get("id")
                .map(clean_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("opt-{index}"));
            (id, label)
        } else {
            continue;
        };
        if label.is_empty() {
            continue;
        }
        options.push(serde_json::json!({ "id": id, "label": label }));
    }
    if options.is_empty() {
        return Err("options must contain at least one non-empty option".to_string());
    }
    if options.len() > 20 {
        return Err("options exceeds the 20 option limit".to_string());
    }
    Ok(serde_json::Value::Array(options))
}

fn normalize_fields(raw: &serde_json::Value) -> Result<serde_json::Value, String> {
    let items = raw
        .as_array()
        .ok_or_else(|| "fields must be an array".to_string())?;
    let mut fields = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let label = object.get("label").map(clean_text).unwrap_or_default();
        if label.is_empty() {
            continue;
        }
        let id = object
            .get("id")
            .map(clean_text)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("field-{index}"));
        let input = object
            .get("input")
            .map(clean_text)
            .filter(|value| matches!(value.as_str(), "text" | "number" | "textarea"))
            .unwrap_or_else(|| "text".to_string());
        let required = object
            .get("required")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        fields.push(serde_json::json!({
            "id": id,
            "label": label,
            "input": input,
            "required": required,
        }));
    }
    if fields.is_empty() {
        return Err("fields must contain at least one non-empty field".to_string());
    }
    if fields.len() > 10 {
        return Err("fields exceeds the 10 field limit".to_string());
    }
    Ok(serde_json::Value::Array(fields))
}

/// 校验并归一化模型返回的 interaction 候选。
/// 返回 (kind, prompt, config)；结构不合法时返回错误（调用方决定丢弃）。
pub fn validate_interaction_candidate(
    raw: &serde_json::Value,
) -> Result<(String, String, serde_json::Value), String> {
    let object = raw
        .as_object()
        .ok_or_else(|| "interaction must be an object".to_string())?;
    let kind = object.get("kind").map(clean_text).unwrap_or_default();
    let prompt = object
        .get("prompt")
        .or_else(|| object.get("title"))
        .or_else(|| object.get("question"))
        .map(clean_text)
        .unwrap_or_default();
    if prompt.len() > 500 {
        return Err("interaction prompt exceeds the 500 character limit".to_string());
    }
    let nested_config = object.get("config").and_then(|value| value.as_object());
    let config_value = |key: &str| {
        nested_config
            .and_then(|config| config.get(key))
            .or_else(|| object.get(key))
    };
    match kind.as_str() {
        INTERACTION_KIND_CHOICE | INTERACTION_KIND_MULTI_CHOICE => {
            let options = normalize_options(
                config_value("options")
                    .ok_or_else(|| "choice interaction requires options".to_string())?,
            )?;
            let mut config = serde_json::json!({ "options": options });
            if kind == INTERACTION_KIND_MULTI_CHOICE {
                if let Some(min) = config_value("min").and_then(|v| v.as_i64()) {
                    config["min"] = serde_json::json!(min.max(0));
                }
                if let Some(max) = config_value("max").and_then(|v| v.as_i64()) {
                    config["max"] = serde_json::json!(max.max(1));
                }
            }
            Ok((kind, prompt, config))
        }
        INTERACTION_KIND_FORM => {
            let fields = normalize_fields(
                config_value("fields")
                    .ok_or_else(|| "form interaction requires fields".to_string())?,
            )?;
            Ok((kind, prompt, serde_json::json!({ "fields": fields })))
        }
        INTERACTION_KIND_CONFIRM => {
            let confirm_label = config_value("confirm_label")
                .map(clean_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "确认".to_string());
            let cancel_label = config_value("cancel_label")
                .map(clean_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "取消".to_string());
            Ok((
                kind,
                prompt,
                serde_json::json!({ "confirm_label": confirm_label, "cancel_label": cancel_label }),
            ))
        }
        INTERACTION_KIND_SLIDER => {
            let min = config_value("min")
                .and_then(|value| value.as_f64())
                .ok_or_else(|| "slider interaction requires a numeric min".to_string())?;
            let max = config_value("max")
                .and_then(|value| value.as_f64())
                .ok_or_else(|| "slider interaction requires a numeric max".to_string())?;
            if !(min < max) {
                return Err("slider min must be less than max".to_string());
            }
            let step = config_value("step")
                .and_then(|value| value.as_f64())
                .filter(|value| *value > 0.0)
                .unwrap_or(1.0);
            let default = config_value("default")
                .and_then(|value| value.as_f64())
                .map(|value| value.clamp(min, max))
                .unwrap_or(min);
            Ok((
                kind,
                prompt,
                serde_json::json!({
                    "min": min,
                    "max": max,
                    "step": step,
                    "default": default,
                }),
            ))
        }
        _ => Err(format!("unsupported interaction kind `{kind}`")),
    }
}

/// 校验玩家回答是否符合 interaction 的类型与配置。
pub fn validate_interaction_answer(
    interaction: &MessageInteraction,
    answer: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let config = &interaction.config;
    match interaction.kind.as_str() {
        INTERACTION_KIND_CHOICE => {
            let id = answer
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "choice answer must be an option id string".to_string())?;
            let valid = config
                .get("options")
                .and_then(|value| value.as_array())
                .map(|options| {
                    options
                        .iter()
                        .any(|option| option.get("id").and_then(|v| v.as_str()) == Some(id))
                })
                .unwrap_or(false);
            if !valid {
                return Err(format!("choice answer `{id}` is not one of the options"));
            }
            Ok(serde_json::Value::String(id.to_string()))
        }
        INTERACTION_KIND_MULTI_CHOICE => {
            let items = answer
                .as_array()
                .ok_or_else(|| "multi_choice answer must be an array of option ids".to_string())?;
            let options = config
                .get("options")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "interaction config is missing options".to_string())?;
            let mut ids: Vec<String> = Vec::new();
            for item in items {
                let Some(id) = item.as_str().map(str::trim).filter(|v| !v.is_empty()) else {
                    continue;
                };
                if !options
                    .iter()
                    .any(|option| option.get("id").and_then(|v| v.as_str()) == Some(id))
                {
                    return Err(format!(
                        "multi_choice answer `{id}` is not one of the options"
                    ));
                }
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
            if ids.is_empty() {
                return Err("multi_choice answer must select at least one option".to_string());
            }
            let min = config.get("min").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            let max = config
                .get("max")
                .and_then(|v| v.as_i64())
                .map(|v| v as usize)
                .unwrap_or(usize::MAX);
            if ids.len() < min || ids.len() > max {
                return Err(format!(
                    "multi_choice answer must select between {min} and {max} options"
                ));
            }
            Ok(serde_json::json!(ids))
        }
        INTERACTION_KIND_FORM => {
            let object = answer
                .as_object()
                .ok_or_else(|| "form answer must be an object".to_string())?;
            let fields = config
                .get("fields")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "interaction config is missing fields".to_string())?;
            let mut normalized = serde_json::Map::new();
            for field in fields {
                let id = field
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let required = field
                    .get("required")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let value = object.get(id).map(clean_text).unwrap_or_default();
                if required && value.is_empty() {
                    let label = field.get("label").and_then(|v| v.as_str()).unwrap_or(id);
                    return Err(format!("form field `{label}` is required"));
                }
                if value.len() > 2000 {
                    return Err(format!(
                        "form field `{id}` exceeds the 2000 character limit"
                    ));
                }
                normalized.insert(id.to_string(), serde_json::Value::String(value));
            }
            Ok(serde_json::Value::Object(normalized))
        }
        INTERACTION_KIND_CONFIRM => {
            if !answer.is_boolean() {
                return Err("confirm answer must be a boolean".to_string());
            }
            Ok(answer.clone())
        }
        INTERACTION_KIND_SLIDER => {
            let value = answer
                .as_f64()
                .ok_or_else(|| "slider answer must be a number".to_string())?;
            let min = config.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = config.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if value < min || value > max {
                return Err(format!("slider answer must be between {min} and {max}"));
            }
            Ok(serde_json::json!(value))
        }
        other => Err(format!("unsupported interaction kind `{other}`")),
    }
}

/// Interaction kinds emitted by the world director itself.
pub fn declared_director_interaction_kinds(director_config: &serde_json::Value) -> Vec<String> {
    declared_interaction_kinds_for(director_config, "director_interaction_kinds")
}

fn declared_interaction_kinds_for(director_config: &serde_json::Value, key: &str) -> Vec<String> {
    director_config
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| {
                    matches!(
                        *item,
                        INTERACTION_KIND_CHOICE
                            | INTERACTION_KIND_MULTI_CHOICE
                            | INTERACTION_KIND_FORM
                            | INTERACTION_KIND_CONFIRM
                            | INTERACTION_KIND_SLIDER
                    )
                })
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
