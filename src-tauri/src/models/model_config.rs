use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub model_type: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: i32,
    pub streaming_enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigCreateRequest {
    pub name: String,
    pub model_type: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: i32,
    pub streaming_enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelConfigUpdateRequest {
    pub name: Option<String>,
    pub model_type: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub max_tokens: Option<i32>,
    pub streaming_enabled: Option<bool>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTestResponse {
    pub ok: bool,
    pub detail: String,
    pub debug_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageModelTestRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageModelTestResponse {
    pub ok: bool,
    pub detail: String,
    pub debug_lines: Vec<String>,
    pub asset_path: Option<String>,
    pub image_url: Option<String>,
    pub seed: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDiscoverRequest {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDiscoverResponse {
    pub ok: bool,
    pub detail: String,
    pub model_ids: Vec<String>,
    pub debug_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelFileStatus {
    pub name: String,
    pub relative_path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelStatus {
    pub model_id: String,
    pub display_name: String,
    pub installed: bool,
    pub detail: String,
    pub local_dir: String,
    pub total_size_bytes: u64,
    pub files: Vec<EmbeddingModelFileStatus>,
}
