use serde::{Deserialize, Serialize};

pub const DEFAULT_CHARACTER_SYSTEM_PROMPT_TEMPLATE: &str = r#"你是{{speaker}}。

角色身份 / 职责：{{role}}

{{background_prompt}}

你必须始终站在该角色视角回应，不要代替玩家决定行动。

如果需要输出对白或行动，只输出该角色本轮会表达的内容。"#;

pub const DEFAULT_CHARACTER_RESPONSE_CONTRACT_PROMPT: &str =
    "只返回一个 JSON 对象，包含字符串字段：speaker、content、intent、emotion、narration。不要输出 markdown。";

pub const DEFAULT_CHARACTER_NARRATION_PROMPT: &str = r#"除扮演当前角色说话外，你还需要同时输出 narration 字段，用一两句简洁旁白补充这一轮发言后场景里真实发生的环境变化、动作结果和气氛变化。

要求：
1. narration 不能复述 content 里的对白。
2. narration 只描述当前角色视角下可以确定的外部变化，不代替其他角色发言，不补写玩家未做出的行动。
3. 如果这一轮没有新的环境变化，narration 返回空字符串。"#;

fn resolve_runtime_prompt_value(value: Option<&str>, default_value: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_value)
        .to_string()
}

pub fn resolve_character_system_prompt_template(value: Option<&str>) -> String {
    resolve_runtime_prompt_value(value, DEFAULT_CHARACTER_SYSTEM_PROMPT_TEMPLATE)
}

pub fn resolve_character_response_contract_prompt(value: Option<&str>) -> String {
    resolve_runtime_prompt_value(value, DEFAULT_CHARACTER_RESPONSE_CONTRACT_PROMPT)
}

pub fn resolve_character_narration_prompt(value: Option<&str>) -> String {
    resolve_runtime_prompt_value(value, DEFAULT_CHARACTER_NARRATION_PROMPT)
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
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
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
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
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
    pub custom_tabs: Option<std::collections::HashMap<String, String>>,
    pub system_prompt_template: Option<String>,
    pub response_contract_prompt: Option<String>,
    pub narration_prompt: Option<String>,
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
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
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
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
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
    pub custom_tabs: std::collections::HashMap<String, String>,
    pub system_prompt_template: String,
    pub response_contract_prompt: String,
    pub narration_prompt: String,
}
