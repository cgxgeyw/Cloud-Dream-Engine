use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;

use crate::db::repositories::attribute_repo::AttributeRepository;
use crate::db::repositories::character_repo::CharacterRepository;
use crate::models::attribute::{
    normalize_attribute_value_type, AttributeSchemaCreateRequest, ATTRIBUTE_SCOPE_WORLD,
    ATTRIBUTE_VALUE_TYPE_BOOLEAN, ATTRIBUTE_VALUE_TYPE_LIST, ATTRIBUTE_VALUE_TYPE_NUMBER,
    ATTRIBUTE_VALUE_TYPE_TEXT,
};
use crate::models::character::CharacterCreateRequest;
use crate::models::generation_params::{GenerationParams, GENERATION_ROLE_UTILITY};
use crate::models::interaction::{
    INTERACTION_KIND_CHOICE, INTERACTION_KIND_CONFIRM, INTERACTION_KIND_FORM,
    INTERACTION_KIND_MULTI_CHOICE, INTERACTION_KIND_SLIDER,
};
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
    #[serde(default)]
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
    runtime_context_prompt: String,
    #[serde(default)]
    world_director_prompt: String,
    #[serde(default)]
    opening_message: String,
    /// 玩家扮演的角色名。空字符串 = 不指定玩家角色。
    #[serde(default)]
    player_character: String,
    /// 主控可下发的交互类型。空 = 由persist_world_draft 按模式兜底。
    #[serde(default)]
    director_interaction_kinds: Vec<String>,
    #[serde(default)]
    attribute_schemas: Vec<AiAttributeSchemaDraft>,
}

/// 世界级持久化属性的草稿。key 为空的一律丢弃。
#[derive(Debug, Deserialize)]
struct AiAttributeSchemaDraft {
    #[serde(default)]
    key: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    value_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    default_value: serde_json::Value,
    #[serde(default)]
    enum_options: Vec<String>,
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
        app_generation: &GenerationParams,
        on_progress: Option<&mut (dyn FnMut(usize) + Send)>,
    ) -> Result<AiWorldDraft, String> {
        let concept = request.concept.trim();
        if concept.is_empty() {
            return Err("请先填写世界概念".to_string());
        }
        generate_world_draft(
            llm,
            model,
            normalize_mode(&request.mode),
            concept,
            app_generation,
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
            return Err("请先填写世界概念".to_string());
        }
        persist_world_draft(conn, model, normalize_mode(&request.mode), concept, draft)
    }
}

async fn generate_world_draft(
    llm: &LlmClient,
    model: &ModelConfig,
    mode: &str,
    concept: &str,
    app_generation: &GenerationParams,
    mut on_progress: Option<&mut (dyn FnMut(usize) + Send)>,
) -> Result<AiWorldDraft, String> {
    let target = if mode == "single_agent" {
        "single-agent service/chat world with exactly one assistant character"
    } else {
        "multi-agent world simulation with three to five playable or NPC characters"
    };
    let system_prompt = r#"You are a world package architect for Cloud Dream Engine.
Return only valid JSON. A single ```json code fence around it is acceptable.
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
    "runtime_context_prompt": "runtime context available every turn",
    "world_director_prompt": "world director behavior and orchestration guidance",
    "opening_message": "first message shown to the player",
    "player_character": "name of one character the player plays, or empty string if the player has no avatar",
    "director_interaction_kinds": ["choice"],
    "attribute_schemas": [
      {"key":"health","label":"体力","value_type":"number","description":"0-100","default_value":100,"enum_options":[]}
    ]
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
}

Field rules:
- "player_character" must exactly match one entry of "characters"[].name, or be empty. Never guess a name that is not in the list.
- "director_interaction_kinds" may only contain "choice", "multi_choice", "form", "confirm", "slider". Use ["choice"] unless the concept really needs forms or sliders. Single-agent service worlds should use [].
- "attribute_schemas" describes the world's own persistent state (health, money, reputation...). Use 2-5 entries.
  - "key": stable lowercase identifier, no spaces.
  - "value_type": one of "text", "number", "boolean", "list", "json".
  - "default_value": must match the type. Omit it (or use null) to let the app pick the zero value.
  - "enum_options": only for "list"; otherwise use [].
  - Do not invent numeric ranges: put constraints in "description" as text."#;
    let user_prompt = format!("Mode: {target}\nConcept:\n{concept}");
    // Multi-agent drafts contain 3-5 fully-specified characters, so the JSON is
    // far larger than single-agent. Cap the output budget so a generous model
    // setting cannot balloon the request — but never *raise* it: the user's
    // max_tokens in Settings is an explicit choice, and silently exceeding it
    // can push the request past a hard limit the provider enforces.
    let output_token_ceiling = if mode == "single_agent" { 6000 } else { 16000 };
    let max_output_tokens = model.max_tokens.min(output_token_ceiling);
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
        // 世界生成器吃应用级生成参数（第 8 项：宿主辅助调用没有世界/会话上下文），
        // 但 max_tokens 由上面按模式算出的输出预算说话——配小了会把 JSON 截断成解析失败。
        generation: GenerationParams {
            max_tokens: Some(max_output_tokens),
            ..GenerationParams::builtin_default_for_role(GENERATION_ROLE_UTILITY)
                .merge(app_generation)
        },
        stream: Some(model.streaming_enabled),
        // 遵从模型级的「关闭 JSON 模式」开关：部分模型在 json_object 下会把键名写坏，
        // parse_draft_json 只兜整体截断，字段坏掉就直接解析失败。
        json_mode: Some(!model.json_mode_disabled),
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
    let attribute_schema_drafts = draft.world.attribute_schemas;
    let player_character_name = clean(&draft.world.player_character);
    let interaction_kinds = normalize_interaction_kinds(mode, &draft.world.director_interaction_kinds);
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
            "director_interaction_kinds": interaction_kinds,
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
            "director_interaction_kinds": interaction_kinds,
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
            // triggers 运行时从不执行（自动逻辑走 logic.events），不再让模型产出无用的关键词。
            triggers: Vec::new(),
            time_config: serde_json::json!({ "mode": "realtime", "label": "\u{5b9e}\u{65f6}" }),
            director_config,
            // 空对象会被 normalize_world_ui_theme_config 判成 runtime_version 2。
            // 显式声明 3，才能拿到默认桌面/移动双入口文档（与编辑器新建世界一致）。
            ui_theme_config: serde_json::json!({
                "runtime_version": 3,
                "capabilities": ["supports_file_picker", "supports_mic"],
            }),
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
    // 玩家角色由模型显式指定；没指定或对不上任何角色就留空，
    // 绝不退化成「第一个角色当玩家」——那会把 NPC 错标成主角。
    let player_character_id = created_characters
        .iter()
        .find(|item| item.name.trim() == player_character_name)
        .map(|item| item.id.clone());
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
    persist_attribute_schemas(conn, &world.id, &attribute_schema_drafts, &mut notes);

    Ok(AiWorldCreateResponse {
        world,
        characters: created_characters,
        notes,
    })
}

/// 把模型产出的属性草稿落成 world 作用域的属性定义。
///
/// 两个必须注意的点：
/// - world 作用域的属性是**全局表**里的 `scope = "world"`，不加限制会串到别的世界。
///   所以 display_policy.applicable_world_ids 必须钉死到新世界 id，与属性面板手建时一致。
/// - 任何一条不合法（key 重复、类型不支持、默认值与类型不符）都跳过并记进 notes，
///   绝不让一条坏数据把整个建世界流程带崩。
fn persist_attribute_schemas(
    conn: &Connection,
    world_id: &str,
    drafts: &[AiAttributeSchemaDraft],
    notes: &mut Vec<String>,
) -> usize {
    if drafts.is_empty() {
        return 0;
    }
    let repo = AttributeRepository::new(conn);
    let mut existing_keys: Vec<String> = repo
        .list_schemas(Some(ATTRIBUTE_SCOPE_WORLD))
        .unwrap_or_default()
        .into_iter()
        .map(|schema| schema.key)
        .collect();
    let mut created = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for draft in drafts {
        let key = draft.key.trim().to_string();
        if key.is_empty() {
            continue;
        }
        let Some(value_type) = normalize_attribute_value_type(&draft.value_type) else {
            skipped.push(format!("{key}（类型 {} 不受支持）", draft.value_type.trim()));
            continue;
        };
        if existing_keys.iter().any(|item| item == &key) {
            skipped.push(format!("{key}（属性 key 已存在）"));
            continue;
        }
        let label = {
            let candidate = draft.label.trim().to_string();
            if candidate.is_empty() {
                key.clone()
            } else {
                candidate
            }
        };
        let enum_options = if value_type == ATTRIBUTE_VALUE_TYPE_LIST {
            normalize_list(draft.enum_options.clone())
        } else {
            Vec::new()
        };
        let request = AttributeSchemaCreateRequest {
            scope: ATTRIBUTE_SCOPE_WORLD.to_string(),
            key: key.clone(),
            label,
            value_type: value_type.clone(),
            description: draft.description.trim().to_string(),
            default_value: coerce_default_value(&value_type, &draft.default_value),
            enum_options,
            display_policy: world_scoped_display_policy(world_id),
            access_policy: policy(&[
                ("creator_read", serde_json::json!(true)),
                ("player_read", serde_json::json!(false)),
                ("agent_self_read", serde_json::json!(false)),
                ("agent_other_read", serde_json::json!(false)),
                ("director_read", serde_json::json!(true)),
                ("plugin_read", serde_json::json!(true)),
            ]),
            mutation_policy: policy(&[
                ("creator_write", serde_json::json!(true)),
                ("allowed_ops", serde_json::json!(["set"])),
            ]),
            influence_policy: policy(&[
                ("prompt.director", serde_json::json!({ "enabled": true, "mode": "raw" })),
                (
                    "ui.status_panel",
                    serde_json::json!({ "enabled": true, "mode": "text" }),
                ),
            ]),
            projection_policy: policy(&[
                ("inherit_to_session", serde_json::json!(true)),
                ("session_owner_type", serde_json::json!("session")),
                ("mutable_in_session", serde_json::json!(true)),
            ]),
        };
        match repo.create_schema(&request) {
            Ok(_) => {
                existing_keys.push(key);
                created += 1;
            }
            Err(error) => skipped.push(format!("{key}（{error}）")),
        }
    }

    if !skipped.is_empty() {
        notes.push(format!("已跳过无法创建的属性：{}。", skipped.join("；")));
    }
    created
}

fn world_scoped_display_policy(world_id: &str) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("editor_visible".to_string(), serde_json::json!(true)),
        ("game_visible".to_string(), serde_json::json!(false)),
        ("debug_visible".to_string(), serde_json::json!(true)),
        // world 作用域的属性存在全局表里，不钉世界id 就会显示到所有世界的面板上。
        (
            "applicable_world_ids".to_string(),
            serde_json::json!([world_id]),
        ),
    ])
}

/// 键值对形式的策略对象，语义与属性面板手建时一致。
fn policy(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

/// 模型给的默认值经常缺字段或类型不符；这里按声明类型收敛成一个一定合法的值。
fn coerce_default_value(value_type: &str, provided: &serde_json::Value) -> serde_json::Value {
    let usable = match value_type {
        ATTRIBUTE_VALUE_TYPE_TEXT => provided.as_str().is_some(),
        ATTRIBUTE_VALUE_TYPE_NUMBER => provided.as_f64().is_some(),
        ATTRIBUTE_VALUE_TYPE_BOOLEAN => provided.is_boolean(),
        ATTRIBUTE_VALUE_TYPE_LIST => provided.is_array(),
        _ => !provided.is_null(),
    };
    if usable {
        return provided.clone();
    }
    match value_type {
        ATTRIBUTE_VALUE_TYPE_TEXT => serde_json::json!(""),
        ATTRIBUTE_VALUE_TYPE_NUMBER => serde_json::json!(0),
        ATTRIBUTE_VALUE_TYPE_BOOLEAN => serde_json::json!(false),
        ATTRIBUTE_VALUE_TYPE_LIST => serde_json::json!([]),
        _ => serde_json::json!({}),
    }
}

/// 主控可下发的交互类型。模型没给或给错时按模式兜底：
/// world_sim 默认给 choice（否则世界永远发不出选项），agent_chat 本身不支持交互，保持空。
fn normalize_interaction_kinds(mode: &str, values: &[String]) -> Vec<String> {
    let is_supported = |value: &str| {
        matches!(
            value,
            INTERACTION_KIND_CHOICE
                | INTERACTION_KIND_MULTI_CHOICE
                | INTERACTION_KIND_FORM
                | INTERACTION_KIND_CONFIRM
                | INTERACTION_KIND_SLIDER
        )
    };
    let mut kinds: Vec<String> = values
        .iter()
        .map(|value| clean(value))
        .filter(|value| !value.is_empty() && is_supported(value))
        .collect();
    kinds.dedup();
    if kinds.is_empty() && mode == "multi_agent" {
        kinds.push(INTERACTION_KIND_CHOICE.to_string());
    }
    kinds
}

fn parse_draft_json(raw: &str) -> Result<AiWorldDraft, String> {
    let trimmed = raw.trim();
    if let Ok(value) = serde_json::from_str::<AiWorldDraft>(trimmed) {
        return Ok(value);
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| "AI 响应中没有 JSON".to_string())?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| "AI 响应中的 JSON 不完整".to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_from(json: &str) -> AiWorldDraft {
        parse_draft_json(json).expect("draft should parse")
    }

    #[test]
    fn interaction_kinds_default_to_choice_for_world_sim() {
        assert_eq!(
            normalize_interaction_kinds("multi_agent", &[]),
            vec![INTERACTION_KIND_CHOICE.to_string()]
        );
    }

    #[test]
    fn interaction_kinds_stay_empty_for_agent_chat() {
        // agent_chat 走角色回复链路，根本不解析主控 interaction，留空才是对的。
        assert!(normalize_interaction_kinds("single_agent", &[]).is_empty());
    }

    #[test]
    fn interaction_kinds_drop_unknown_entries_and_dedupe() {
        let kinds = normalize_interaction_kinds(
            "multi_agent",
            &[
                "form".to_string(),
                "form".to_string(),
                "teleport".to_string(),
                "  ".to_string(),
            ],
        );
        assert_eq!(kinds, vec!["form".to_string()]);
    }

    #[test]
    fn output_budget_never_exceeds_model_setting() {
        // 用户在设置里把 max_tokens 调到 500，生成器不许偷偷抬到下限 1200。
        let ceiling = 6000i32;
        assert_eq!(500i32.min(ceiling), 500);
        // 上限仍然生效：给一个极大的值也不该原样发出去。
        assert_eq!(64_000i32.min(16_000), 16_000);
    }

    #[test]
    fn default_value_is_coerced_to_the_declared_type() {
        // 模型经常给错类型（text 字段填数字、number 字段留空）。
        assert_eq!(
            coerce_default_value(ATTRIBUTE_VALUE_TYPE_TEXT, &serde_json::json!(42)),
            serde_json::json!("")
        );
        assert_eq!(
            coerce_default_value(ATTRIBUTE_VALUE_TYPE_NUMBER, &serde_json::Value::Null),
            serde_json::json!(0)
        );
        assert_eq!(
            coerce_default_value(ATTRIBUTE_VALUE_TYPE_BOOLEAN, &serde_json::json!("true")),
            serde_json::json!(false)
        );
        // 类型对得上就原样保留。
        assert_eq!(
            coerce_default_value(ATTRIBUTE_VALUE_TYPE_NUMBER, &serde_json::json!(7)),
            serde_json::json!(7)
        );
    }

    #[test]
    fn draft_tolerates_missing_optional_blocks() {
        // 只给必填字段：可选块全部缺省，且 characters 缺失也不该让整个解析失败。
        let draft = draft_from(
            r#"{"world":{"name":"n","genre":"g","background_prompt":"b","opening_scene":"s","summary":"m","time_system":"t"}}"#,
        );
        assert!(draft.characters.is_empty());
        assert!(draft.world.attribute_schemas.is_empty());
        assert!(draft.world.director_interaction_kinds.is_empty());
    }

    #[test]
    fn draft_reads_player_and_attribute_schemas() {
        let draft = draft_from(
            r#"{
                "world":{
                    "name":"n","genre":"g","background_prompt":"b","opening_scene":"s",
                    "summary":"m","time_system":"t",
                    "player_character":"阿宁",
                    "director_interaction_kinds":["choice"],
                    "attribute_schemas":[
                        {"key":"health","label":"体力","value_type":"number","default_value":100}
                    ]
                },
                "characters":[{"name":"阿宁","role":"主角","background_prompt":"x"}]
            }"#,
        );
        assert_eq!(draft.world.player_character, "阿宁");
        assert_eq!(
            normalize_interaction_kinds("multi_agent", &draft.world.director_interaction_kinds),
            vec![INTERACTION_KIND_CHOICE.to_string()]
        );
        assert_eq!(draft.world.attribute_schemas.len(), 1);
        assert_eq!(draft.world.attribute_schemas[0].key, "health");
    }

    #[test]
    fn fenced_json_is_recovered() {
        // json_mode 关闭后模型常带```json 围栏，解析必须仍然成功。
        let draft = draft_from(
            "```json\n{\"world\":{\"name\":\"n\",\"genre\":\"g\",\"background_prompt\":\"b\",\"opening_scene\":\"s\",\"summary\":\"m\",\"time_system\":\"t\"}}\n```",
        );
        assert_eq!(draft.world.name, "n");
    }
}
