use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const STORAGE_NAME_MAX_BYTES: usize = 64;

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

    // Runtime v3 packages created before storage declarations used this capability.
    if declared_collections
        .map(|collections| collections.is_empty())
        .unwrap_or(true)
        && has_legacy_world_records_capability(&theme)
    {
        return Ok(None);
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
        return Err("World record data must be a JSON object".to_string());
    };

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !data.contains_key(field) {
                return Err(format!("World record is missing required field `{field}`"));
            }
        }
    }

    let properties = schema.get("properties").and_then(Value::as_object);
    if schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        == Some(false)
    {
        if let Some(properties) = properties {
            if let Some(field) = data.keys().find(|field| !properties.contains_key(*field)) {
                return Err(format!("World record field `{field}` is not declared"));
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
                return Err(format!("storage collection `{name}` config must be an object"));
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
        return Err(format!("Unsupported world logic runtime `{runtime}`"));
    }
    if runtime == "sandbox-js-v1" {
        let source = object.get("source").and_then(Value::as_str).unwrap_or_default();
        if source.trim().is_empty() {
            return Err("sandbox-js-v1 requires a logic source file".to_string());
        }
        if source.len() > 256 * 1024 {
            return Err("World logic exceeds the 262144 byte limit".to_string());
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
        .ok_or_else(|| "World not found".to_string())?;
    serde_json::from_str(&raw).map_err(|error| format!("Invalid world UI config: {error}"))
}

fn has_legacy_world_records_capability(theme: &Value) -> bool {
    theme
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|items| items.iter().any(|item| item.as_str() == Some("supports_world_records")))
        .unwrap_or(false)
}

fn normalize_storage_name(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > STORAGE_NAME_MAX_BYTES
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
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
            return Err(format!("World record field `{field}` must be {expected}"));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(format!("World record field `{field}` has an unsupported value"));
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
            return Err(format!("World record field `{field}` is too short"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .map(|max| length > max)
            .unwrap_or(false)
        {
            return Err(format!("World record field `{field}` is too long"));
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .map(|min| number < min)
            .unwrap_or(false)
        {
            return Err(format!("World record field `{field}` is below its minimum"));
        }
        if schema
            .get("maximum")
            .and_then(Value::as_f64)
            .map(|max| number > max)
            .unwrap_or(false)
        {
            return Err(format!("World record field `{field}` is above its maximum"));
        }
    }
    Ok(())
}
