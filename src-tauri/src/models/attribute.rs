use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ATTRIBUTE_SCOPE_WORLD: &str = "world";
pub const ATTRIBUTE_SCOPE_CHARACTER: &str = "character";
pub const ATTRIBUTE_SCOPE_SESSION: &str = "session";
pub const ATTRIBUTE_SCOPE_SESSION_CHARACTER: &str = "session_character";

pub const ATTRIBUTE_VALUE_TYPE_TEXT: &str = "text";
pub const ATTRIBUTE_VALUE_TYPE_NUMBER: &str = "number";
pub const ATTRIBUTE_VALUE_TYPE_BOOLEAN: &str = "boolean";
pub const ATTRIBUTE_VALUE_TYPE_LIST: &str = "list";
pub const ATTRIBUTE_VALUE_TYPE_JSON: &str = "json";

pub fn normalize_attribute_scope(scope: &str) -> Option<String> {
    match scope.trim() {
        ATTRIBUTE_SCOPE_WORLD => Some(ATTRIBUTE_SCOPE_WORLD.to_string()),
        ATTRIBUTE_SCOPE_CHARACTER => Some(ATTRIBUTE_SCOPE_CHARACTER.to_string()),
        ATTRIBUTE_SCOPE_SESSION => Some(ATTRIBUTE_SCOPE_SESSION.to_string()),
        ATTRIBUTE_SCOPE_SESSION_CHARACTER => Some(ATTRIBUTE_SCOPE_SESSION_CHARACTER.to_string()),
        _ => None,
    }
}

pub fn normalize_attribute_value_type(value_type: &str) -> Option<String> {
    match value_type.trim() {
        ATTRIBUTE_VALUE_TYPE_TEXT => Some(ATTRIBUTE_VALUE_TYPE_TEXT.to_string()),
        ATTRIBUTE_VALUE_TYPE_NUMBER => Some(ATTRIBUTE_VALUE_TYPE_NUMBER.to_string()),
        ATTRIBUTE_VALUE_TYPE_BOOLEAN => Some(ATTRIBUTE_VALUE_TYPE_BOOLEAN.to_string()),
        ATTRIBUTE_VALUE_TYPE_LIST => Some(ATTRIBUTE_VALUE_TYPE_LIST.to_string()),
        ATTRIBUTE_VALUE_TYPE_JSON => Some(ATTRIBUTE_VALUE_TYPE_JSON.to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeSchema {
    pub id: String,
    pub scope: String,
    pub key: String,
    pub label: String,
    pub value_type: String,
    pub description: String,
    pub default_value: serde_json::Value,
    pub enum_options: Vec<String>,
    pub display_policy: HashMap<String, serde_json::Value>,
    pub access_policy: HashMap<String, serde_json::Value>,
    pub mutation_policy: HashMap<String, serde_json::Value>,
    pub influence_policy: HashMap<String, serde_json::Value>,
    pub projection_policy: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeSchemaCreateRequest {
    pub scope: String,
    pub key: String,
    pub label: String,
    pub value_type: String,
    pub description: String,
    pub default_value: serde_json::Value,
    pub enum_options: Vec<String>,
    pub display_policy: HashMap<String, serde_json::Value>,
    pub access_policy: HashMap<String, serde_json::Value>,
    pub mutation_policy: HashMap<String, serde_json::Value>,
    pub influence_policy: HashMap<String, serde_json::Value>,
    pub projection_policy: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeValue {
    pub id: String,
    pub schema_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub value: serde_json::Value,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeValueUpsertRequest {
    pub schema_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub value: serde_json::Value,
    pub source: String,
}
