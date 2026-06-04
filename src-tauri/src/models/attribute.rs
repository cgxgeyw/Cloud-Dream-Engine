use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
