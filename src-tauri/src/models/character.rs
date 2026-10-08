use serde::{Deserialize, Serialize};

fn normalize_runtime_prompt_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

pub fn resolve_character_system_prompt_template(value: Option<&str>) -> String {
    normalize_runtime_prompt_value(value)
}

pub fn resolve_character_response_contract_prompt(value: Option<&str>) -> String {
    normalize_runtime_prompt_value(value)
}

pub fn resolve_character_narration_prompt(value: Option<&str>) -> String {
    normalize_runtime_prompt_value(value)
}

pub fn resolve_character_runtime_system_prompt(value: Option<&str>) -> String {
    normalize_runtime_prompt_value(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDefinition {
    pub id: String,
    pub name: String,
    pub world_id: String,
    pub role: String,
    pub background_prompt: String,
    pub model: String,
    pub memory_strategy: String,
    pub recent_dialogue_rounds: i32,
    pub attributes: Vec<String>,
    pub portrait_assets: Vec<String>,
    #[serde(default)]
    pub avatar_asset: String,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
    #[serde(default)]
    pub runtime_system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCreateRequest {
    pub name: String,
    pub role: String,
    pub background_prompt: String,
    pub model: String,
    pub memory_strategy: String,
    pub recent_dialogue_rounds: i32,
    pub attributes: Vec<String>,
    pub portrait_assets: Vec<String>,
    #[serde(default)]
    pub avatar_asset: String,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
    #[serde(default)]
    pub runtime_system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterUpdateRequest {
    pub name: Option<String>,
    pub role: Option<String>,
    pub background_prompt: Option<String>,
    pub model: Option<String>,
    pub memory_strategy: Option<String>,
    pub recent_dialogue_rounds: Option<i32>,
    pub attributes: Option<Vec<String>>,
    pub portrait_assets: Option<Vec<String>>,
    pub avatar_asset: Option<String>,
    pub system_prompt_template: Option<String>,
    pub response_contract_prompt: Option<String>,
    pub narration_prompt: Option<String>,
    pub runtime_system_prompt: Option<String>,
}

/// 整包覆盖式更新：世界包导入走「同名角色就地 update」，字段一个不落全带上。
impl From<&CharacterCreateRequest> for CharacterUpdateRequest {
    fn from(request: &CharacterCreateRequest) -> Self {
        Self {
            name: Some(request.name.clone()),
            role: Some(request.role.clone()),
            background_prompt: Some(request.background_prompt.clone()),
            model: Some(request.model.clone()),
            memory_strategy: Some(request.memory_strategy.clone()),
            recent_dialogue_rounds: Some(request.recent_dialogue_rounds),
            attributes: Some(request.attributes.clone()),
            portrait_assets: Some(request.portrait_assets.clone()),
            avatar_asset: Some(request.avatar_asset.clone()),
            system_prompt_template: Some(request.system_prompt_template.clone()),
            response_contract_prompt: Some(request.response_contract_prompt.clone()),
            narration_prompt: Some(request.narration_prompt.clone()),
            runtime_system_prompt: Some(request.runtime_system_prompt.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTemplateExport {
    pub name: String,
    pub role: String,
    pub background_prompt: String,
    pub model: String,
    pub memory_strategy: String,
    pub recent_dialogue_rounds: i32,
    pub attributes: Vec<String>,
    pub portrait_assets: Vec<String>,
    #[serde(default)]
    pub avatar_asset: String,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
    #[serde(default)]
    pub runtime_system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterImportRequest {
    pub target_world_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTemplateImportRequest {
    pub name: String,
    pub role: String,
    pub background_prompt: String,
    pub model: String,
    pub memory_strategy: String,
    pub recent_dialogue_rounds: i32,
    pub attributes: Vec<String>,
    pub portrait_assets: Vec<String>,
    #[serde(default)]
    pub avatar_asset: String,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
    pub runtime_system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPackageData {
    pub source_character_id: String,
    pub name: String,
    pub role: String,
    pub background_prompt: String,
    pub model: String,
    pub memory_strategy: String,
    pub recent_dialogue_rounds: i32,
    pub attributes: Vec<String>,
    pub portrait_assets: Vec<String>,
    #[serde(default)]
    pub avatar_asset: String,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
    pub runtime_system_prompt: String,
}
