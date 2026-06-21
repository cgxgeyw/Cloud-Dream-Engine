use rusqlite::Connection;
use serde::Deserialize;

use crate::db::repositories::character_repo::CharacterRepository;
use crate::models::character::CharacterCreateRequest;
use crate::models::model_config::ModelConfig;
use crate::models::world::{
    AiWorldCreateRequest, AiWorldCreateResponse, WorldCreateRequest, WorldOpeningMessage,
    WorldUpdateRequest,
};
use crate::services::catalog::world_service::WorldService;
use crate::services::llm::client::{ChatMessage, ChatRequest, LlmClient};

#[derive(Debug, Deserialize)]
pub(crate) struct AiWorldDraft {
    world: AiWorldDraftWorld,
    characters: Vec<AiCharacterDraft>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AiWorldDraftWorld {
    name: String,
    genre: String,
    background_prompt: String,
    opening_scene: String,
    summary: String,
    time_system: String,
    #[serde(default)]
    map_nodes: serde_json::Value,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    runtime_context_prompt: String,
    #[serde(default)]
    world_director_prompt: String,
    #[serde(default)]
    opening_message: String,
}

#[derive(Debug, Deserialize)]
struct AiCharacterDraft {
    name: String,
    role: String,
    background_prompt: String,
    #[serde(default)]
    memory_strategy: String,
    #[serde(default)]
    recent_dialogue_rounds: i32,
    #[serde(default)]
    attributes: Vec<String>,
    #[serde(default)]
    system_prompt_template: String,
    #[serde(default)]
    response_contract_prompt: String,
    #[serde(default)]
    narration_prompt: String,
    #[serde(default)]
    runtime_system_prompt: String,
}

pub struct AiWorldBuilderService;

impl AiWorldBuilderService {
    pub async fn generate_draft(
        llm: &LlmClient,
        model: &ModelConfig,
        request: AiWorldCreateRequest,
        on_progress: Option<&mut (dyn FnMut(usize) + Send)>,
    ) -> Result<AiWorldDraft, String> {
        let concept = request.concept.trim();
        if concept.is_empty() {
            return Err("World concept is required".to_string());
        }
        generate_world_draft(
            llm,
            model,
            normalize_mode(&request.mode),
            concept,
            on_progress,
        )
        .await
    }

    pub fn persist_world(
        conn: &Connection,
        model: &ModelConfig,
        request: &AiWorldCreateRequest,
        draft: AiWorldDraft,
    ) -> Result<AiWorldCreateResponse, String> {
        let concept = request.concept.trim();
        if concept.is_empty() {
            return Err("World concept is required".to_string());
        }
        persist_world_draft(conn, model, normalize_mode(&request.mode), concept, draft)
    }
}

async fn generate_world_draft(
    llm: &LlmClient,
    model: &ModelConfig,
    mode: &str,
    concept: &str,
    mut on_progress: Option<&mut (dyn FnMut(usize) + Send)>,
) -> Result<AiWorldDraft, String> {
    let target = if mode == "single_agent" {
        "single-agent service/chat world with exactly one assistant character"
    } else {
        "multi-agent world simulation with three to five playable or NPC characters"
    };
    let system_prompt = r#"You are a world package architect for Cloud Dream Engine.
Return only valid JSON. Do not wrap in Markdown.
Create coherent world and character data from the user's concept.
Keep prompts directly editable by creators. Avoid meta text like "the system will".
For single-agent mode, create exactly one assistant/agent character.
For multi-agent mode, create 3 to 5 distinct characters with useful roles.
All user-facing content should be Simplified Chinese unless the concept strongly asks otherwise.

JSON shape:
{
  "world": {
    "name": "short world name",
    "genre": "category tags",
    "background_prompt": "objective setting and shared world facts",
    "opening_scene": "starting scene name",
    "summary": "one sentence premise",
    "time_system": "time rules",
    "map_nodes": {"version":1,"root":{"id":"root","label":"...","children":[{"id":"...","label":"..."}]},"edges":[]},
    "triggers": ["keyword"],
    "runtime_context_prompt": "runtime context available every turn",
    "world_director_prompt": "world director behavior and orchestration guidance",
    "opening_message": "first message shown to the player"
  },
  "characters": [
    {
      "name": "character name",
      "role": "role title",
      "background_prompt": "character facts, personality, goals, boundaries",
      "memory_strategy": "short memory guidance",
      "recent_dialogue_rounds": 8,
      "attributes": ["attribute or tag"],
      "system_prompt_template": "editable character system prompt",
      "response_contract_prompt": "format and response requirements",
      "narration_prompt": "narration style guidance",
      "runtime_system_prompt": "per-turn runtime guidance"
    }
  ],
  "notes": ["short creation note"]
}"#;
    let user_prompt = format!("Mode: {target}\nConcept:\n{concept}");
    // Multi-agent drafts contain 3-5 fully-specified characters, so the JSON is
    // far larger than single-agent. Cap the output budget by mode to avoid the
    // response being truncated mid-JSON (which surfaces as a parse failure).
    let output_token_floor = if mode == "single_agent" { 1200 } else { 4000 };
    let output_token_ceiling = if mode == "single_agent" { 6000 } else { 16000 };
    let max_output_tokens = model
        .max_tokens
        .max(output_token_floor)
        .min(output_token_ceiling);
    let chat_request = ChatRequest {
        model: model.model_id.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: serde_json::Value::String(system_prompt.to_string()),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String(user_prompt),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            },
        ],
        temperature: Some(0.7),
        max_tokens: Some(max_output_tokens),
        stream: Some(model.streaming_enabled),
        json_mode: Some(true),
        response_schema: None,
        tools: None,
        tool_choice: None,
        native_tool_calling: None,
    };

    let response = if model.streaming_enabled {
        // Stream so the UI can show live progress (accumulated characters).
        let mut received = 0usize;
        let streamed = llm
            .chat_completion_stream(
                &model.provider,
                &model.base_url,
                &model.api_key,
                &chat_request,
                |chunk| {
                    received += chunk.delta.chars().count();
                    if let Some(cb) = on_progress.as_deref_mut() {
                        cb(received);
                    }
                },
            )
            .await;
        match streamed {
            Ok(value) => value,
            // Fall back to non-streaming if the stream path fails outright.
            Err(_) => {
                llm.chat_completion(
                    &model.provider,
                    &model.base_url,
                    &model.api_key,
                    &chat_request,
                )
                .await?
            }
        }
    } else {
        llm.chat_completion(
            &model.provider,
            &model.base_url,
            &model.api_key,
            &chat_request,
        )
        .await?
    };
    parse_draft_json(&response.content)
}

fn persist_world_draft(
    conn: &Connection,
    model: &ModelConfig,
    mode: &str,
    concept: &str,
    draft: AiWorldDraft,
) -> Result<AiWorldCreateResponse, String> {
    let mut notes = normalize_list(draft.notes);
    let characters = normalize_characters(mode, draft.characters, concept);
    let default_agent_id_placeholder = "__DEFAULT_AGENT__";
    let director_config = if mode == "single_agent" {
        serde_json::json!({
            "service_mode": "agent_chat",
            "default_agent_id": default_agent_id_placeholder,
            "allow_scene_transition": false,
            "allow_npc_spawn": false,
            "history_dialogue_rounds": 8,
            "director_tool_loop_limit": 4,
            "runtime_context_prompt": clean(&draft.world.runtime_context_prompt),
            "world_director_prompt": clean(&draft.world.world_director_prompt),
            "prompt_presets": [],
            "return_processing_rules": [],
            "runtime_policy": { "memory_write_mode": "session" },
            "allowed_mcp_tool_ids": []
        })
    } else {
        serde_json::json!({
            "service_mode": "world_sim",
            "allow_scene_transition": true,
            "allow_npc_spawn": true,
            "history_dialogue_rounds": 8,
            "director_tool_loop_limit": 4,
            "runtime_context_prompt": clean(&draft.world.runtime_context_prompt),
            "world_director_prompt": clean(&draft.world.world_director_prompt),
            "prompt_presets": [],
            "return_processing_rules": [],
            "runtime_policy": { "memory_write_mode": "session" },
            "allowed_mcp_tool_ids": []
        })
    };

    let world_service = WorldService::new();
    let opening_message = clean(&draft.world.opening_message);
    let world = world_service.create_world(
        conn,
        WorldCreateRequest {
            name: fallback(
                clean(&draft.world.name),
                "\u{0041}\u{0049}\u{0020}\u{4e16}\u{754c}",
            ),
            genre: fallback(
                clean(&draft.world.genre),
                if mode == "single_agent" {
                    "\u{5355}\u{667a}\u{80fd}\u{4f53}"
                } else {
                    "\u{591a}\u{667a}\u{80fd}\u{4f53}"
                },
            ),
            background_prompt: fallback(clean(&draft.world.background_prompt), concept),
            opening_scene: fallback(clean(&draft.world.opening_scene), "\u{5f00}\u{573a}"),
            summary: fallback(clean(&draft.world.summary), concept),
            time_system: fallback(
                clean(&draft.world.time_system),
                "\u{65f6}\u{95f4}\u{968f}\u{5bf9}\u{8bdd}\u{63a8}\u{8fdb}",
            ),
            map_nodes: normalize_map_nodes(&draft.world.map_nodes, &draft.world.opening_scene),
            triggers: normalize_triggers(draft.world.triggers, concept),
            time_config: serde_json::json!({ "mode": "realtime", "label": "\u{5b9e}\u{65f6}" }),
            director_config,
            ui_theme_config: serde_json::json!({}),
            opening_messages: vec![WorldOpeningMessage {
                role: "system".to_string(),
                content: fallback(opening_message, &clean(&draft.world.summary)),
                speaker: None,
            }],
            opening_character_ids: Vec::new(),
            player_character_id: None,
        },
    )?;

    let char_repo = CharacterRepository::new(conn);
    let mut created_characters = Vec::new();
    for character in characters {
        created_characters.push(char_repo.create(
            &world.id,
            &CharacterCreateRequest {
                name: fallback(clean(&character.name), "\u{89d2}\u{8272}"),
                role: fallback(clean(&character.role), "\u{4e16}\u{754c}\u{89d2}\u{8272}"),
                background_prompt: fallback(clean(&character.background_prompt), concept),
                model: model.id.clone(),
                memory_strategy: fallback(
                    clean(&character.memory_strategy),
                    "\u{8bb0}\u{4f4f}\u{73a9}\u{5bb6}\u{504f}\u{597d}\u{3001}\u{627f}\u{8bfa}\u{3001}\u{5173}\u{7cfb}\u{53d8}\u{5316}\u{548c}\u{672a}\u{5b8c}\u{6210}\u{4e8b}\u{9879}\u{3002}",
                ),
                recent_dialogue_rounds: character.recent_dialogue_rounds.clamp(4, 20),
                attributes: normalize_list(character.attributes),
                portrait_assets: Vec::new(),
                avatar_asset: String::new(),
                system_prompt_template: clean(&character.system_prompt_template),
                response_contract_prompt: clean(&character.response_contract_prompt),
                narration_prompt: clean(&character.narration_prompt),
                runtime_system_prompt: clean(&character.runtime_system_prompt),
            },
        )?);
    }

    let opening_character_ids: Vec<String> = created_characters
        .iter()
        .map(|item| item.id.clone())
        .collect();
    let player_character_id = if mode == "multi_agent" {
        opening_character_ids.first().cloned()
    } else {
        None
    };
    let director_config = if mode == "single_agent" {
        replace_default_agent_id(
            world.director_config.clone(),
            created_characters.first().map(|item| item.id.as_str()),
        )
    } else {
        world.director_config.clone()
    };
    let world = world_service.update_world(
        conn,
        &world.id,
        WorldUpdateRequest {
            name: None,
            genre: None,
            background_prompt: None,
            opening_scene: None,
            summary: None,
            time_system: None,
            map_nodes: None,
            triggers: None,
            time_config: None,
            director_config: Some(director_config),
            ui_theme_config: None,
            opening_messages: None,
            opening_character_ids: Some(opening_character_ids),
            player_character_id: Some(player_character_id),
        },
    )?;
    notes.push(if mode == "single_agent" {
        "Created a single-agent world and set its only character as the default agent.".to_string()
    } else {
        "Created a multi-agent world with opening characters.".to_string()
    });

    Ok(AiWorldCreateResponse {
        world,
        characters: created_characters,
        notes,
    })
}

fn parse_draft_json(raw: &str) -> Result<AiWorldDraft, String> {
    let trimmed = raw.trim();
    if let Ok(value) = serde_json::from_str::<AiWorldDraft>(trimmed) {
        return Ok(value);
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| "AI response did not contain JSON".to_string())?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| "AI response did not contain complete JSON".to_string())?;
    serde_json::from_str(&trimmed[start..=end]).map_err(|error| {
        // A missing closing brace / unterminated string almost always means the
        // model hit the output token limit and the JSON was cut off. Multi-agent
        // drafts are the usual trigger because they are much larger.
        let likely_truncated = error.to_string().contains("EOF")
            || !trimmed.trim_end().ends_with('}');
        if likely_truncated {
            format!(
                "AI response was cut off before the JSON finished (likely the model's output token limit). Try a shorter concept, raise the model's max tokens in Settings, or use single-agent mode. ({error})"
            )
        } else {
            format!("Failed to parse AI world JSON: {error}")
        }
    })
}

fn normalize_mode(mode: &str) -> &'static str {
    match mode.trim() {
        "multi_agent" | "multi" | "multi-agent" => "multi_agent",
        _ => "single_agent",
    }
}

fn normalize_characters(
    mode: &str,
    mut characters: Vec<AiCharacterDraft>,
    concept: &str,
) -> Vec<AiCharacterDraft> {
    characters.retain(|item| !clean(&item.name).is_empty());
    if mode == "single_agent" {
        characters.truncate(1);
    } else if characters.len() > 5 {
        characters.truncate(5);
    }
    if characters.is_empty() {
        characters.push(AiCharacterDraft {
            name: if mode == "single_agent" {
                "\u{4e16}\u{754c}\u{52a9}\u{624b}"
            } else {
                "\u{5f15}\u{8def}\u{4eba}"
            }
            .to_string(),
            role: if mode == "single_agent" {
                "\u{5355}\u{667a}\u{80fd}\u{4f53}\u{52a9}\u{624b}"
            } else {
                "\u{5f00}\u{573a}\u{5f15}\u{5bfc}\u{89d2}\u{8272}"
            }
            .to_string(),
            background_prompt: concept.to_string(),
            memory_strategy: String::new(),
            recent_dialogue_rounds: 8,
            attributes: Vec::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
        });
    }
    characters
}

fn replace_default_agent_id(
    mut config: serde_json::Value,
    character_id: Option<&str>,
) -> serde_json::Value {
    if let Some(id) = character_id {
        if let Some(object) = config.as_object_mut() {
            object.insert(
                "default_agent_id".to_string(),
                serde_json::Value::String(id.to_string()),
            );
        }
    }
    config
}

fn normalize_map_nodes(value: &serde_json::Value, opening_scene: &str) -> serde_json::Value {
    if value.is_object() {
        return value.clone();
    }
    let label = fallback(clean(opening_scene), "\u{5f00}\u{573a}");
    serde_json::json!({
        "version": 1,
        "root": {
            "id": "root",
            "label": label,
            "children": []
        },
        "edges": []
    })
}

fn normalize_triggers(values: Vec<String>, concept: &str) -> Vec<String> {
    let mut output = normalize_list(values);
    if output.is_empty() {
        output.extend(
            concept
                .split(|c: char| c.is_whitespace() || ",.;:!?，。；、！？".contains(c))
                .map(str::trim)
                .filter(|item| item.chars().count() >= 2)
                .take(5)
                .map(str::to_string),
        );
    }
    if output.is_empty() {
        output.push("\u{5f00}\u{59cb}".to_string());
    }
    output
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        let value = clean(&value);
        if !value.is_empty() && !output.iter().any(|item| item == &value) {
            output.push(value);
        }
    }
    output
}

fn clean(value: &str) -> String {
    value.trim().to_string()
}

fn fallback(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}
