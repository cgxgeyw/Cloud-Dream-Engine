use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const STORAGE_NAME_MAX_BYTES: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_collections_require_explicit_storage_declarations() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        crate::db::schema::create_tables(&conn).expect("create schema");
        conn.execute(
            "INSERT INTO worlds (id, name, ui_theme_config_json) VALUES ('storage-test', 'Storage', ?1)",
            params![serde_json::json!({ "capabilities": ["supports_world_records"] }).to_string()],
        )
        .expect("insert world");
        assert!(require_record_collection(&conn, "storage-test", "entries").is_err());
        conn.execute(
            "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = 'storage-test'",
            params![serde_json::json!({
                "storage": { "collections": { "entries": { "schema": { "type": "object" } } } }
            })
            .to_string()],
        )
        .expect("declare collection");
        assert_eq!(
            require_record_collection(&conn, "storage-test", "entries").unwrap(),
            Some(serde_json::json!({ "type": "object" })),
        );
        assert!(require_record_collection(&conn, "storage-test", "other").is_err());
    }
}

pub fn require_record_collection(
    conn: &Connection,
    world_id: &str,
    collection: &str,
) -> Result<Option<Value>, String> {
    let theme = load_world_ui_theme(conn, world_id)?;
    let collection = normalize_storage_name(collection, "collection")?;
    let declared_collections = theme
        .get("storage")
        .and_then(|value| value.get("collections"))
        .and_then(Value::as_object);
    if let Some(config) = declared_collections.and_then(|collections| {
        collections
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(&collection))
            .map(|(_, config)| config)
    }) {
        return Ok(config.get("schema").cloned());
    }

    Err(format!(
        "World package did not declare storage collection `{collection}`"
    ))
}

pub fn require_kv_namespace(
    conn: &Connection,
    world_id: &str,
    namespace: &str,
) -> Result<String, String> {
    let theme = load_world_ui_theme(conn, world_id)?;
    let namespace = normalize_storage_name(namespace, "KV namespace")?;
    let declared = theme
        .get("storage")
        .and_then(|value| value.get("kv_namespaces"))
        .and_then(Value::as_array)
        .map(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .map(|value| value.trim().eq_ignore_ascii_case(&namespace))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !declared {
        return Err(format!(
            "World package did not declare KV namespace `{namespace}`"
        ));
    }
    Ok(namespace)
}

pub fn validate_record_against_schema(data: &Value, schema: Option<&Value>) -> Result<(), String> {
    let Some(schema) = schema.and_then(Value::as_object) else {
        return Ok(());
    };
    let Some(data) = data.as_object() else {
        return Err("世界记录数据必须是 JSON 对象".to_string());
    };

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !data.contains_key(field) {
                return Err(format!("世界记录缺少必填字段 {field}"));
            }
        }
    }

    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
        if let Some(properties) = properties {
            if let Some(field) = data.keys().find(|field| !properties.contains_key(*field)) {
                return Err(format!("世界记录字段 {field} 未声明"));
            }
        }
    }

    if let Some(properties) = properties {
        for (field, field_schema) in properties {
            if let Some(value) = data.get(field) {
                validate_field(field, value, field_schema)?;
            }
        }
    }
    Ok(())
}

pub fn validate_storage_config(storage: &Value) -> Result<(), String> {
    if storage.is_null() {
        return Ok(());
    }
    let object = storage
        .as_object()
        .ok_or_else(|| "storage must be an object".to_string())?;
    if let Some(namespaces) = object.get("kv_namespaces") {
        let namespaces = namespaces
            .as_array()
            .ok_or_else(|| "storage.kv_namespaces must be an array".to_string())?;
        for namespace in namespaces {
            let namespace = namespace
                .as_str()
                .ok_or_else(|| "storage.kv_namespaces entries must be strings".to_string())?;
            normalize_storage_name(namespace, "KV namespace")?;
        }
    }
    if let Some(collections) = object.get("collections") {
        let collections = collections
            .as_object()
            .ok_or_else(|| "storage.collections must be an object".to_string())?;
        for (name, config) in collections {
            normalize_storage_name(name, "collection")?;
            if !config.is_object() {
                return Err(format!(
                    "storage collection `{name}` config must be an object"
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_logic_config(logic: &Value) -> Result<(), String> {
    if logic.is_null() {
        return Ok(());
    }
    let object = logic
        .as_object()
        .ok_or_else(|| "logic must be an object".to_string())?;
    let runtime = object
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("disabled");
    if !matches!(runtime, "disabled" | "sandbox-js-v1") {
        return Err(format!("不支持的世界逻辑运行时：{runtime}"));
    }
    if let Some(execution) = object.get("execution").and_then(Value::as_str) {
        if !matches!(execution, "host" | "disabled") {
            return Err(format!("不支持的世界逻辑执行模式：{execution}"));
        }
    }
    let validate_handler = |path: &str, value: &Value| -> Result<(), String> {
        let handler = value
            .as_str()
            .ok_or_else(|| format!("{path} must be a handler name string"))?;
        if handler.is_empty()
            || handler.len() > 128
            || !handler
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(format!("{path} handler name is invalid"));
        }
        Ok(())
    };
    for key in ["ui_handlers"] {
        if let Some(handlers) = object.get(key) {
            let handlers = handlers
                .as_array()
                .ok_or_else(|| format!("logic.{key} must be an array"))?;
            for (index, handler) in handlers.iter().enumerate() {
                validate_handler(&format!("logic.{key}[{index}]"), handler)?;
            }
        }
    }
    if let Some(tools) = object.get("tools") {
        let tools = tools
            .as_array()
            .ok_or_else(|| "logic.tools must be an array".to_string())?;
        for (index, tool) in tools.iter().enumerate() {
            let tool = tool
                .as_object()
                .ok_or_else(|| format!("logic.tools[{index}] must be an object"))?;
            let handler = tool
                .get("handler")
                .ok_or_else(|| format!("logic.tools[{index}].handler is required"))?;
            validate_handler(&format!("logic.tools[{index}].handler"), handler)?;
            if let Some(speakers) = tool.get("speakers") {
                let speakers = speakers
                    .as_array()
                    .ok_or_else(|| format!("logic.tools[{index}].speakers must be an array"))?;
                if speakers.iter().any(|speaker| speaker.as_str().is_none()) {
                    return Err(format!(
                        "logic.tools[{index}].speakers entries must be strings"
                    ));
                }
            }
        }
    }
    if let Some(view_handler) = object
        .get("agent_turn")
        .and_then(Value::as_object)
        .and_then(|agent_turn| agent_turn.get("view_handler"))
    {
        validate_handler("logic.agent_turn.view_handler", view_handler)?;
    }
    if let Some(protocol) = object.get("turn_protocol") {
        let protocol = protocol
            .as_object()
            .ok_or_else(|| "logic.turn_protocol must be an object".to_string())?;
        if let Some(state_handler) = protocol.get("state_handler") {
            validate_handler("logic.turn_protocol.state_handler", state_handler)?;
        } else {
            return Err("logic.turn_protocol.state_handler is required".to_string());
        }
        for key in ["phase_field", "active_actor_field"] {
            if let Some(value) = protocol.get(key) {
                let field = value
                    .as_str()
                    .filter(|value| !value.trim().is_empty() && value.len() <= 128)
                    .ok_or_else(|| {
                        format!("logic.turn_protocol.{key} must be a non-empty string")
                    })?;
                if !field.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                }) {
                    return Err(format!(
                        "logic.turn_protocol.{key} contains invalid characters"
                    ));
                }
            }
        }
        if let Some(values) = protocol.get("actor_values") {
            let values = values
                .as_object()
                .ok_or_else(|| "logic.turn_protocol.actor_values must be an object".to_string())?;
            for (actor, value) in values {
                if !matches!(actor.as_str(), "player" | "agent" | "system")
                    || value.as_str().is_none()
                {
                    return Err(
                        "logic.turn_protocol.actor_values must map player/agent/system to strings"
                            .to_string(),
                    );
                }
            }
        }
        if let Some(handlers) = protocol.get("command_handlers") {
            let handlers = handlers.as_object().ok_or_else(|| {
                "logic.turn_protocol.command_handlers must be an object".to_string()
            })?;
            for (handler, actor) in handlers {
                validate_handler(
                    "logic.turn_protocol.command_handlers key",
                    &Value::String(handler.clone()),
                )?;
                if !matches!(actor.as_str(), Some("player" | "agent" | "system")) {
                    return Err(format!(
                        "logic.turn_protocol.command_handlers.{handler} has invalid actor"
                    ));
                }
            }
        }
        if let Some(values) = protocol.get("terminal_phase_values") {
            let values = values.as_array().ok_or_else(|| {
                "logic.turn_protocol.terminal_phase_values must be an array".to_string()
            })?;
            if values.iter().any(|value| value.as_str().is_none()) {
                return Err(
                    "logic.turn_protocol.terminal_phase_values entries must be strings".to_string(),
                );
            }
        }
        if let Some(handlers) = protocol.get("initialize_handlers") {
            let handlers = handlers.as_array().ok_or_else(|| {
                "logic.turn_protocol.initialize_handlers must be an array".to_string()
            })?;
            for (index, handler) in handlers.iter().enumerate() {
                validate_handler(
                    &format!("logic.turn_protocol.initialize_handlers[{index}]"),
                    handler,
                )?;
            }
        }
    }
    if let Some(events) = object.get("events") {
        let events = events
            .as_object()
            .ok_or_else(|| "logic.events must be an object".to_string())?;
        const KNOWN_EVENTS: [&str; 3] = ["session_start", "turn_completed", "interaction_answered"];
        for (name, handler) in events {
            if !KNOWN_EVENTS.contains(&name.as_str()) {
                return Err(format!("未知的世界逻辑事件：{name}"));
            }
            let handler = handler
                .as_str()
                .ok_or_else(|| format!("logic.events.{name} must be a handler name string"))?;
            validate_handler(
                &format!("logic.events.{name}"),
                &Value::String(handler.to_string()),
            )?;
        }
    }
    if runtime == "sandbox-js-v1" {
        let source = object
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if source.trim().is_empty() {
            return Err("sandbox-js-v1 requires a logic source file".to_string());
        }
        if source.len() > 256 * 1024 {
            return Err("世界逻辑脚本超过 262144 字节上限".to_string());
        }
    }
    Ok(())
}

fn load_world_ui_theme(conn: &Connection, world_id: &str) -> Result<Value, String> {
    let raw = conn
        .query_row(
            "SELECT ui_theme_config_json FROM worlds WHERE id = ?1",
            params![world_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "世界不存在".to_string())?;
    serde_json::from_str(&raw).map_err(|error| format!("Invalid world UI config: {error}"))
}

fn normalize_storage_name(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > STORAGE_NAME_MAX_BYTES
        || !normalized.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(format!(
            "World storage {label} must use 1-{STORAGE_NAME_MAX_BYTES} ASCII letters, numbers, dots, dashes, or underscores"
        ));
    }
    Ok(normalized)
}

fn validate_field(field: &str, value: &Value, schema: &Value) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let matches = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "null" => value.is_null(),
            _ => false,
        };
        if !matches {
            return Err(format!("世界记录字段 {field} 必须为 {expected}"));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(format!("世界记录字段 {field} 的值不受支持"));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .map(|min| length < min)
            .unwrap_or(false)
        {
            return Err(format!("世界记录字段 {field} 过短"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .map(|max| length > max)
            .unwrap_or(false)
        {
            return Err(format!("世界记录字段 {field} 过长"));
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .map(|min| number < min)
            .unwrap_or(false)
        {
            return Err(format!("世界记录字段 {field} 低于最小值"));
        }
        if schema
            .get("maximum")
            .and_then(Value::as_f64)
            .map(|max| number > max)
            .unwrap_or(false)
        {
            return Err(format!("世界记录字段 {field} 超过最大值"));
        }
    }
    Ok(())
}
