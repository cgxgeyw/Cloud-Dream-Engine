use crate::models::session::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAttributeDefinition {
    pub id: String,
    pub name: String,
    pub value_type: String,
    pub order: i32,
    pub enabled: bool,
    pub required: bool,
    pub placeholder: String,
    pub default_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDefinition {
    pub id: String,
    pub name: String,
    pub genre: String,
    pub background_prompt: String,
    pub opening_scene: String,
    pub summary: String,
    pub time_system: String,
    pub map_nodes: serde_json::Value,
    pub triggers: Vec<String>,
    pub custom_tabs: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub world_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    #[serde(default)]
    pub character_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    pub time_config: serde_json::Value,
    pub director_config: serde_json::Value,
    pub ui_theme_config: serde_json::Value,
    pub director_system_prompt_base: String,
    pub director_runtime_system_prompt: String,
    pub opening_messages: Vec<WorldOpeningMessage>,
    pub opening_character_ids: Vec<String>,
    pub player_character_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldOpeningMessage {
    pub role: String,
    pub content: String,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldCreateRequest {
    pub name: String,
    pub genre: String,
    pub background_prompt: String,
    pub opening_scene: String,
    pub summary: String,
    pub time_system: String,
    pub map_nodes: serde_json::Value,
    pub triggers: Vec<String>,
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub world_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    pub character_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    pub time_config: serde_json::Value,
    pub director_config: serde_json::Value,
    pub ui_theme_config: serde_json::Value,
    pub opening_messages: Vec<WorldOpeningMessage>,
    pub opening_character_ids: Vec<String>,
    pub player_character_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUpdateRequest {
    pub name: Option<String>,
    pub genre: Option<String>,
    pub background_prompt: Option<String>,
    pub opening_scene: Option<String>,
    pub summary: Option<String>,
    pub time_system: Option<String>,
    pub map_nodes: Option<serde_json::Value>,
    pub triggers: Option<Vec<String>>,
    pub custom_tabs: Option<std::collections::HashMap<String, String>>,
    pub world_custom_attribute_definitions: Option<Vec<CustomAttributeDefinition>>,
    pub character_custom_attribute_definitions: Option<Vec<CustomAttributeDefinition>>,
    pub time_config: Option<serde_json::Value>,
    pub director_config: Option<serde_json::Value>,
    pub ui_theme_config: Option<serde_json::Value>,
    pub opening_messages: Option<Vec<WorldOpeningMessage>>,
    pub opening_character_ids: Option<Vec<String>>,
    pub player_character_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPromptTracePreview {
    pub speaker: Option<String>,
    pub prompt_trace: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldOpeningPromptPreviewResponse {
    pub opening_calls_llm: bool,
    pub sample_player_input: String,
    pub planned_speakers: Vec<String>,
    pub world_director_prompt_trace: serde_json::Value,
    pub character_prompt_traces: Vec<CharacterPromptTracePreview>,
    pub opening_messages: Vec<ChatMessage>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryFileResponse {
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedFileResponse {
    pub filename: String,
    pub saved_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPackageManifest {
    pub format: String,
    pub version: u32,
    pub world: Option<serde_json::Value>,
    pub world_file: Option<String>,
    pub desktop_ui_file: Option<String>,
    pub mobile_ui_file: Option<String>,
    pub characters_file: Option<String>,
    pub character_files: Vec<WorldPackageCharacterFileEntry>,
    pub assets: Vec<WorldPackageAssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPackageCharacterFileEntry {
    pub source_character_id: String,
    pub character_name: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPackageAssetEntry {
    pub source_path: String,
    pub archive_path: String,
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPackageWorldData {
    pub name: String,
    pub genre: String,
    pub background_prompt: String,
    pub opening_scene: String,
    pub summary: String,
    pub time_system: String,
    pub map_nodes: serde_json::Value,
    pub triggers: Vec<String>,
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub world_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    pub character_custom_attribute_definitions: Vec<CustomAttributeDefinition>,
    pub time_config: serde_json::Value,
    pub director_config: serde_json::Value,
    pub ui_assets_config: serde_json::Value,
    pub opening_messages: Vec<WorldOpeningMessage>,
    pub opening_character_names: Vec<String>,
    pub player_character_name: Option<String>,
    pub opening_character_source_ids: Vec<String>,
    pub player_character_source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiDocumentRequest {
    pub source: String,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiBundleValidationRequest {
    pub desktop_file: String,
    pub mobile_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiCompileRequest {
    pub source: String,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiCompatibilityTarget {
    pub name: String,
    pub supported_schema_versions: Vec<u32>,
    pub supported_components: Vec<String>,
    pub supported_actions: Vec<String>,
    pub supported_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyWorldPackageUiCompatibilityRequest {
    pub desktop_file: String,
    pub mobile_file: String,
    pub target: Option<WorldUiCompatibilityTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiDocumentValidationResult {
    pub ok: bool,
    pub platform: Option<String>,
    pub schema_version: Option<u32>,
    pub components: Vec<String>,
    pub actions: Vec<String>,
    pub capabilities: Vec<String>,
    pub errors: Vec<WorldUiDiagnostic>,
    pub warnings: Vec<WorldUiDiagnostic>,
    pub normalized_document: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiBundleValidationResult {
    pub ok: bool,
    pub desktop: WorldUiDocumentValidationResult,
    pub mobile: WorldUiDocumentValidationResult,
    pub errors: Vec<WorldUiDiagnostic>,
    pub warnings: Vec<WorldUiDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiCompileResult {
    pub ok: bool,
    pub platform: Option<String>,
    pub schema_version: Option<u32>,
    pub normalized_ast: Option<serde_json::Value>,
    pub component_dependencies: Vec<String>,
    pub action_dependencies: Vec<String>,
    pub capability_requirements: Vec<String>,
    pub diagnostics: Vec<WorldUiDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiCompatibilityDocumentReport {
    pub platform: String,
    pub ok: bool,
    pub schema_version: Option<u32>,
    pub component_dependencies: Vec<String>,
    pub action_dependencies: Vec<String>,
    pub capability_requirements: Vec<String>,
    pub unsupported_schema_versions: Vec<u32>,
    pub unsupported_components: Vec<String>,
    pub unsupported_actions: Vec<String>,
    pub unsupported_capabilities: Vec<String>,
    pub diagnostics: Vec<WorldUiDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldUiCompatibilityReport {
    pub ok: bool,
    pub target: WorldUiCompatibilityTarget,
    pub documents: Vec<WorldUiCompatibilityDocumentReport>,
    pub diagnostics: Vec<WorldUiDiagnostic>,
}
