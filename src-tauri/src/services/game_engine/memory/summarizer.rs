use crate::models::character::CharacterDefinition;
use crate::models::generation_params::{GenerationParams, GENERATION_ROLE_UTILITY};
use crate::models::memory::{MemoryEntry, MemorySummaryRange};
use crate::models::model_config::ModelConfig;
use crate::models::session::{ChatMessage, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::game_engine::scene::SceneManager;
use crate::services::llm::client::{ChatMessage as LlmChatMessage, ChatRequest, LlmClient};
use std::collections::BTreeMap;

pub(crate) const MEMORY_TYPE_SUMMARY: &str = "summary";
const SUMMARY_MAX_TOKENS: i32 = 900;

#[derive(Debug, Clone)]
pub(crate) struct SummaryDraft {
    pub character_id: String,
    pub scene_id: String,
    pub scene_name: String,
    pub turn_start: i32,
    pub turn_end: i32,
    pub content: String,
    pub participants: Vec<String>,
}

/// 固定桶：1..=n、n+1..=2n；返回 (bucket_index, bucket_start, bucket_end)。
pub(crate) fn summary_bucket_bounds(turn_index: i32, interval: i32) -> (i32, i32, i32) {
    let interval = interval.max(1);
    let turn_index = turn_index.max(1);
    let bucket_index = (turn_index - 1).div_euclid(interval);
    let start = bucket_index * interval + 1;
    let end = start + interval - 1;
    (bucket_index, start, end)
}

pub(crate) fn resolve_memory_summary_interval(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("memory_summary_interval_rounds")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(1, 100) as i32)
        .unwrap_or(10)
}

/// 原文窗口按固定总结桶对齐。
///
/// 如果原文窗口的起点落在某个桶内部，就在“补回桶首”和“整桶让给总结”之间
/// 选更接近用户请求轮数的方案。这样不会出现“桶的一部分还在原文、另一部分
/// 已经该由总结覆盖”的空洞。只有该桶的总结已经存在时才对齐，否则保持用户
/// 请求的原文轮数，等总结生成后再收缩。
pub(crate) fn align_history_rounds_to_summary_bucket(
    conn: &rusqlite::Connection,
    session_id: &str,
    character_id: Option<&str>,
    current_turn: i32,
    requested_rounds: i32,
    interval: i32,
) -> i32 {
    let Some(character_id) = character_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return requested_rounds;
    };
    let requested_rounds = requested_rounds.max(0);
    let interval = interval.max(1);
    if requested_rounds == 0 || current_turn <= 0 {
        return requested_rounds;
    }
    let raw_start = current_turn - requested_rounds;
    if raw_start <= 1 {
        return requested_rounds;
    }
    let bucket_index = (raw_start - 1).div_euclid(interval);
    let bucket_start = bucket_index * interval + 1;
    let existing_buckets =
        crate::db::repositories::memory_summary_repo::MemorySummaryRepository::new(conn)
            .list_by_character(session_id, character_id)
            .unwrap_or_default()
            .into_iter()
            .map(|range| range.bucket_index)
            .collect::<std::collections::HashSet<_>>();
    if existing_buckets.is_empty() {
        // 老存档还没有任何总结：保持用户请求的原文轮数。角色路有旧对话兜底，
        // 主控路等第一次升级回填后再使用总结，不能把整个存档原文一次性塞回去。
        return requested_rounds;
    }
    // 任何更早的桶缺总结，都把原文补回那个桶的起点，避免“新总结有、旧桶没总结”
    // 导致中间断一段；但最多补到 requested + 2*n，避免老存档一次拉回全部历史。
    if let Some(missing_index) = (0..=bucket_index).find(|index| !existing_buckets.contains(index))
    {
        let missing_start = missing_index * interval + 1;
        let missing_rounds = (current_turn - missing_start).max(0);
        if missing_rounds <= requested_rounds + interval * 2 {
            return missing_rounds;
        }
        return requested_rounds;
    }
    if raw_start <= bucket_start {
        return requested_rounds;
    }
    let next_bucket_start = bucket_start + interval;
    let extend_rounds = (current_turn - bucket_start).max(0);
    let drop_rounds = (current_turn - next_bucket_start).max(0);
    // 两个候选：把桶补回原文（多带最多 n-1 回合），或整桶让给总结
    // （少带最多 n-1 回合）。选更接近用户请求轮数的那个，避免第 12 回合
    // 就把 1-10 整桶丢掉。
    let extend_diff = (extend_rounds - requested_rounds).abs();
    let drop_diff = (drop_rounds - requested_rounds).abs();
    if drop_diff < extend_diff {
        drop_rounds
    } else {
        extend_rounds
    }
}

/// 生成一个固定桶内、按场景拆分的总结。只使用该角色 witnesses 可见的消息。
pub(crate) async fn generate_bucket_summaries(
    llm: &LlmClient,
    model: &ModelConfig,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
    _bucket_index: i32,
    bucket_start: i32,
    bucket_end: i32,
    prompt_template: &str,
) -> Vec<SummaryDraft> {
    let mut drafts = Vec::new();
    for character in characters {
        if character.name == session.player_character_name {
            continue;
        }
        if super::parse_memory_strategy(&character.memory_strategy).disabled {
            continue;
        }
        let visible = messages_for_character_in_bucket(
            &session.messages,
            &character.name,
            bucket_start,
            bucket_end,
        );
        if visible.is_empty() {
            continue;
        }
        let mut by_scene: BTreeMap<String, Vec<&ChatMessage>> = BTreeMap::new();
        for message in visible {
            by_scene
                .entry(message_scene_key(message))
                .or_default()
                .push(message);
        }
        for (scene_key, messages) in by_scene {
            let Some((turn_start, turn_end)) = message_turn_bounds(&messages) else {
                continue;
            };
            let scene_id = resolve_scene_id(&messages).unwrap_or_else(|| scene_key.clone());
            let scene_name = resolve_scene_name(&scene_key, &messages, session);
            let participants = collect_participants(&messages, &session.player_character_name);
            let transcript = build_transcript(&messages, &session.player_character_name);
            if transcript.trim().is_empty() {
                continue;
            }
            let prompt = build_summary_prompt(
                prompt_template,
                &character.name,
                &scene_name,
                bucket_start,
                bucket_end,
                turn_start,
                turn_end,
                &transcript,
            );
            let content = match request_summary(llm, model, &prompt).await {
                Ok(content) if !content.trim().is_empty() => content,
                _ => continue,
            };
            drafts.push(SummaryDraft {
                character_id: character.id.clone(),
                scene_id,
                scene_name,
                turn_start,
                turn_end,
                content,
                participants,
            });
        }
    }
    drafts
}

/// 只取该角色见证过的 player/agent 消息；system、交易卡片等不进总结。
fn messages_for_character_in_bucket<'a>(
    messages: &'a [ChatMessage],
    character_name: &str,
    bucket_start: i32,
    bucket_end: i32,
) -> Vec<&'a ChatMessage> {
    messages
        .iter()
        .filter(|message| {
            let turn = message_turn_index(message);
            matches!(turn, Some(turn) if turn >= bucket_start && turn <= bucket_end)
                && (message.role == "player" || message.role == "agent")
                && message_has_summary_text(message)
                && message_has_witnesses(message)
                && super::super::orchestrator::turn_context::message_visible_to(
                    message,
                    Some(character_name),
                )
        })
        .collect()
}

/// 总结必须有明确的观众名单。旧存档的消息没有 witnesses，无法判断谁在场，
/// 宁可不生成总结（继续用原文兜底），也不把所有人的经历写进每个角色记忆。
fn message_has_witnesses(message: &ChatMessage) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("witnesses"))
        .and_then(|value| value.as_array())
        .is_some()
}

pub(crate) fn character_has_visible_messages(
    session: &SessionSnapshot,
    character_name: &str,
    bucket_start: i32,
    bucket_end: i32,
) -> bool {
    !messages_for_character_in_bucket(&session.messages, character_name, bucket_start, bucket_end)
        .is_empty()
}

pub(crate) fn bucket_has_any_messages(
    session: &SessionSnapshot,
    bucket_start: i32,
    bucket_end: i32,
) -> bool {
    session.messages.iter().any(|message| {
        let turn = message_turn_index(message);
        matches!(turn, Some(turn) if turn >= bucket_start && turn <= bucket_end)
            && (message.role == "player" || message.role == "agent")
            && message_has_summary_text(message)
    })
}

/// 世界主控的全局剧情总结：不按角色隔离，桶内所有 player/agent 消息都可见。
pub(crate) async fn generate_director_bucket_summary(
    llm: &LlmClient,
    model: &ModelConfig,
    session: &SessionSnapshot,
    bucket_start: i32,
    bucket_end: i32,
    prompt_template: &str,
) -> Option<SummaryDraft> {
    let messages = session
        .messages
        .iter()
        .filter(|message| {
            let turn = message_turn_index(message);
            matches!(turn, Some(turn) if turn >= bucket_start && turn <= bucket_end)
                && (message.role == "player" || message.role == "agent")
                && message_has_summary_text(message)
        })
        .collect::<Vec<_>>();
    if messages.is_empty() {
        return None;
    }
    let (turn_start, turn_end) = message_turn_bounds(&messages)?;
    let transcript = build_transcript(&messages, &session.player_character_name);
    if transcript.trim().is_empty() {
        return None;
    }
    let participants = collect_participants(&messages, &session.player_character_name);
    let prompt = build_summary_prompt(
        prompt_template,
        "世界主控",
        "世界",
        bucket_start,
        bucket_end,
        turn_start,
        turn_end,
        &transcript,
    );
    let content = request_summary(llm, model, &prompt).await.ok()?;
    if content.trim().is_empty() {
        return None;
    }
    Some(SummaryDraft {
        character_id: "director".to_string(),
        scene_id: String::new(),
        scene_name: "世界".to_string(),
        turn_start,
        turn_end,
        content,
        participants,
    })
}

fn message_has_summary_text(message: &ChatMessage) -> bool {
    if !message.content.as_prompt_text().trim().is_empty() {
        return true;
    }
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("narration"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn message_turn_index(message: &ChatMessage) -> Option<i32> {
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("turn_index"))
        .and_then(|value| value.as_i64())
        .map(|value| value as i32)
}

fn message_scene_key(message: &ChatMessage) -> String {
    let metadata = message.metadata.as_ref();
    let scene_name = metadata
        .and_then(|meta| meta.get("scene_name"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(scene_name) = scene_name {
        return scene_name.to_string();
    }
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("scene_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(slugify_scene_id)
        .unwrap_or_default()
}

fn slugify_scene_id(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        SceneManager::new().slugify(value)
    }
}

fn resolve_scene_id(messages: &[&ChatMessage]) -> Option<String> {
    messages.iter().find_map(|message| {
        message
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("scene_id"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(slugify_scene_id)
    })
}

fn message_turn_bounds(messages: &[&ChatMessage]) -> Option<(i32, i32)> {
    let mut start = i32::MAX;
    let mut end = i32::MIN;
    for message in messages {
        if let Some(turn) = message_turn_index(message) {
            start = start.min(turn);
            end = end.max(turn);
        }
    }
    (start != i32::MAX && end != i32::MIN).then_some((start, end))
}

fn resolve_scene_name(
    scene_id: &str,
    messages: &[&ChatMessage],
    session: &SessionSnapshot,
) -> String {
    for message in messages {
        if let Some(name) = message
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("scene_name"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return name.to_string();
        }
    }
    if !scene_id.is_empty() && scene_id == session.scene.scene_id {
        return session.scene.name.clone();
    }
    scene_id.to_string()
}

fn collect_participants(messages: &[&ChatMessage], player_name: &str) -> Vec<String> {
    let mut participants = Vec::new();
    let mut push = |name: &str| {
        let name = name.trim();
        if !name.is_empty() && !participants.iter().any(|item| item == name) {
            participants.push(name.to_string());
        }
    };
    push(player_name);
    for message in messages {
        if let Some(speaker) = message.speaker.as_deref() {
            push(speaker);
        }
    }
    participants
}

fn build_transcript(messages: &[&ChatMessage], player_name: &str) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let turn = message_turn_index(message).unwrap_or(0);
        let speaker = message
            .speaker
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                if message.role == "player" {
                    player_name
                } else {
                    "NPC"
                }
            });
        let content = message.content.as_prompt_text().trim().to_string();
        if !content.is_empty() {
            lines.push(format!("[第{turn}回合][{speaker}] {content}"));
        }
        if let Some(narration) = message
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("narration"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("[第{turn}回合][旁白] {narration}"));
        }
    }
    lines.join("\n")
}

const DEFAULT_CHARACTER_SUMMARY_PROMPT: &str = "\
你在整理角色「{{character_name}}」的个人记忆。
只根据下面这段该角色亲历/亲耳听到的对话，写一段第一人称记忆总结。
要求：
1. 只写这段材料里确实发生的事，不编造、不补充材料外信息。
2. 保留：发生了什么、谁做了什么决定、承诺、发现、关系变化、知道的秘密、未解决的线索、地点/时间变化。
3. 不要逐句复述对话，不要写“第几句说了什么”。
4. 如果角色不在场或材料不足，就写“这段记忆很模糊”或直接省略。
5. 输出纯文本，不要 Markdown，不要 JSON，200-400 字。

固定回合桶：{{bucket_start}}-{{bucket_end}}
实际场景段：{{turn_start}}-{{turn_end}}
场景：{{scene_name}}
对话材料：
{{transcript}}";

const DEFAULT_DIRECTOR_SUMMARY_PROMPT: &str = "\
你在整理这个世界的全局剧情总结，供世界主控后续调度使用。
只根据下面这段实际发生的对话，写一段客观、紧凑的剧情总结。
要求：
1. 只写材料里确实发生的事，不编造、不补充材料外信息。
2. 保留：主要事件、玩家和 NPC 的关键行动/决定、地点和时间变化、重要物品、秘密、未解决线索。
3. 不要逐句复述，不要写分析过程。
4. 输出纯文本，不要 Markdown，不要 JSON，200-500 字。

固定回合桶：{{bucket_start}}-{{bucket_end}}
实际场景段：{{turn_start}}-{{turn_end}}
对话材料：
{{transcript}}";

pub(crate) fn resolve_character_memory_summary_prompt(world: &WorldDefinition) -> String {
    world
        .director_config
        .get("character_memory_summary_prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CHARACTER_SUMMARY_PROMPT)
        .to_string()
}

pub(crate) fn resolve_director_memory_summary_prompt(world: &WorldDefinition) -> String {
    world
        .director_config
        .get("director_memory_summary_prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_DIRECTOR_SUMMARY_PROMPT)
        .to_string()
}

pub(crate) fn default_character_memory_summary_prompt() -> &'static str {
    DEFAULT_CHARACTER_SUMMARY_PROMPT
}

pub(crate) fn default_director_memory_summary_prompt() -> &'static str {
    DEFAULT_DIRECTOR_SUMMARY_PROMPT
}

fn build_summary_prompt(
    template: &str,
    character_name: &str,
    scene_name: &str,
    bucket_start: i32,
    bucket_end: i32,
    turn_start: i32,
    turn_end: i32,
    transcript: &str,
) -> String {
    let template = if template.trim().is_empty() {
        DEFAULT_CHARACTER_SUMMARY_PROMPT
    } else {
        template
    };
    let mut rendered = template
        .replace("{{character_name}}", character_name)
        .replace("{{scene_name}}", scene_name)
        .replace("{{bucket_start}}", &bucket_start.to_string())
        .replace("{{bucket_end}}", &bucket_end.to_string())
        .replace("{{turn_start}}", &turn_start.to_string())
        .replace("{{turn_end}}", &turn_end.to_string())
        .replace("{{transcript}}", transcript);
    // 自定义模板如果忘了写 {{transcript}}，仍然把材料附在末尾，避免模型只看到指令。
    if !template.contains("{{transcript}}") {
        rendered.push_str("\n\n对话材料：\n");
        rendered.push_str(transcript);
    }
    rendered
}

/// 主控 prompt 用的全局总结：只取已经完全离开主控原文窗口的桶。
pub(crate) fn load_director_summary_text(
    conn: &rusqlite::Connection,
    session_id: &str,
    current_turn: i32,
    raw_rounds: i32,
) -> Result<String, String> {
    let ranges = crate::db::repositories::memory_summary_repo::MemorySummaryRepository::new(conn)
        .list_by_character(session_id, "director")?;
    let cutoff = current_turn - raw_rounds.max(0);
    let mut parts = Vec::new();
    for range in ranges {
        if range.bucket_end >= cutoff {
            continue;
        }
        let content: Option<String> = conn
            .query_row(
                "SELECT content FROM memories WHERE id = ?1",
                rusqlite::params![range.memory_id],
                |row| row.get(0),
            )
            .ok();
        let Some(content) = content
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let label = if range.turn_start == range.turn_end {
            format!("第{}回合", range.turn_start)
        } else {
            format!("第{}-{}回合", range.turn_start, range.turn_end)
        };
        parts.push(format!("【{label}总结】\n{content}"));
    }
    Ok(parts.join("\n\n"))
}

async fn request_summary(
    llm: &LlmClient,
    model: &ModelConfig,
    prompt: &str,
) -> Result<String, String> {
    let request = ChatRequest {
        model: model.model_id.clone(),
        messages: vec![
            LlmChatMessage {
                role: "system".to_string(),
                content: serde_json::Value::String(
                    "你是叙事游戏的记忆整理器。只输出一段可靠的角色记忆，不输出分析过程。"
                        .to_string(),
                ),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            },
            LlmChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String(prompt.to_string()),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            },
        ],
        generation: GenerationParams {
            max_tokens: Some(if model.max_tokens > 0 {
                model.max_tokens.min(SUMMARY_MAX_TOKENS)
            } else {
                SUMMARY_MAX_TOKENS
            }),
            ..GenerationParams::builtin_default_for_role(GENERATION_ROLE_UTILITY)
        },
        stream: Some(false),
        json_mode: Some(false),
        response_schema: None,
        tools: None,
        tool_choice: None,
        native_tool_calling: None,
    };
    let response = llm
        .chat_completion(&model.provider, &model.base_url, &model.api_key, &request)
        .await?;
    if response.is_truncated_by_token_limit() {
        return Err("summary response truncated by max_tokens".to_string());
    }
    Ok(strip_code_fence(&response.content))
}

fn strip_code_fence(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest
            .strip_prefix("text")
            .or_else(|| rest.strip_prefix("markdown"))
            .or_else(|| rest.strip_prefix("json"))
            .unwrap_or(rest);
        return rest
            .trim_start_matches(|ch| ch == '\r' || ch == '\n')
            .trim_end_matches("```")
            .trim()
            .to_string();
    }
    trimmed.to_string()
}

/// 把总结正文写进 memories，并把桶/场景范围写进 memory_summary_ranges。
pub(crate) fn persist_summary(
    conn: &rusqlite::Connection,
    world: &WorldDefinition,
    session: &SessionSnapshot,
    draft: &SummaryDraft,
    bucket_index: i32,
    bucket_start: i32,
    bucket_end: i32,
) -> Result<bool, String> {
    let Some(current_session) =
        crate::db::repositories::session_repo::SessionRepository::new(conn).get(&session.id)?
    else {
        return Ok(false);
    };
    let current_messages = serde_json::to_value(current_session.messages).unwrap_or_default();
    let snapshot_messages = serde_json::to_value(&session.messages).unwrap_or_default();
    // 回滚或重试会改写旧消息；只允许在捕获快照仍为当前历史前缀时落库。
    if !matches!(
        (&current_messages, &snapshot_messages),
        (serde_json::Value::Array(current), serde_json::Value::Array(snapshot))
            if current.starts_with(snapshot)
    ) {
        return Ok(false);
    }

    let mut keywords = super::extract_memory_keywords(&draft.content);
    for participant in &draft.participants {
        let participant = participant.trim();
        if !participant.is_empty() && !keywords.iter().any(|item| item == participant) {
            keywords.push(participant.to_string());
        }
    }
    let entry = MemoryEntry {
        id: format!("mem-summary-{}", uuid::Uuid::new_v4().simple()),
        world_id: world.id.clone(),
        session_id: session.id.clone(),
        character_id: draft.character_id.clone(),
        layer: "archive".to_string(),
        content: draft.content.trim().to_string(),
        source: "summary".to_string(),
        importance: 0.8,
        created_at: chrono::Utc::now().to_rfc3339(),
        // 用桶结束回合做 cutoff：确保整个桶离开原文窗口后才会被召回。
        turn_index: bucket_end,
        conversation_id: Some(session.id.clone()),
        event_id: None,
        item_id: None,
        scene_id: Some(draft.scene_id.clone()).filter(|value| !value.is_empty()),
        memory_type: MEMORY_TYPE_SUMMARY.to_string(),
        speaker: None,
        role: Some("memory".to_string()),
        location: Some(draft.scene_name.clone()).filter(|value| !value.is_empty()),
        participants: draft.participants.clone(),
        keywords,
    };
    let memory_repo = crate::db::repositories::memory_repo::MemoryRepository::new(conn);
    memory_repo.insert(&entry)?;
    let range = MemorySummaryRange {
        memory_id: entry.id.clone(),
        world_id: world.id.clone(),
        session_id: session.id.clone(),
        character_id: draft.character_id.clone(),
        bucket_index,
        bucket_start,
        bucket_end,
        scene_id: draft.scene_id.clone(),
        scene_name: draft.scene_name.clone(),
        turn_start: draft.turn_start,
        turn_end: draft.turn_end,
        created_at: entry.created_at.clone(),
    };
    let range_repo =
        crate::db::repositories::memory_summary_repo::MemorySummaryRepository::new(conn);
    match range_repo.insert(&range) {
        Ok(true) => Ok(true),
        Ok(false) => {
            // 唯一键冲突说明另一个任务已经写过这个桶/场景；删掉刚插入的正文避免孤儿。
            let _ = conn.execute(
                "DELETE FROM memories WHERE id = ?1",
                rusqlite::params![entry.id],
            );
            Ok(false)
        }
        Err(error) => {
            let _ = conn.execute(
                "DELETE FROM memories WHERE id = ?1",
                rusqlite::params![entry.id],
            );
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::memory_repo::MemoryRepository;
    use crate::db::repositories::memory_summary_repo::MemorySummaryRepository;
    use crate::db::repositories::session_repo::SessionRepository;
    use crate::db::schema;
    use crate::models::memory::{MemoryEntry, MemorySummaryRange};
    use crate::models::session::{AssetSelection, MessageContent, SceneRuntime, SessionState};
    use rusqlite::Connection;

    #[test]
    fn summary_buckets_are_fixed_not_cumulative() {
        assert_eq!(summary_bucket_bounds(1, 10), (0, 1, 10));
        assert_eq!(summary_bucket_bounds(10, 10), (0, 1, 10));
        assert_eq!(summary_bucket_bounds(11, 10), (1, 11, 20));
        assert_eq!(summary_bucket_bounds(20, 10), (1, 11, 20));
        assert_eq!(summary_bucket_bounds(21, 10), (2, 21, 30));
    }

    #[test]
    fn custom_summary_template_keeps_placeholders_and_appends_transcript() {
        let prompt = build_summary_prompt(
            "角色 {{character_name}} 在 {{scene_name}}，桶 {{bucket_start}}-{{bucket_end}}，段 {{turn_start}}-{{turn_end}}。",
            "韩立",
            "码头",
            1,
            10,
            1,
            6,
            "[第1回合][韩立] 拿到银钥匙",
        );
        assert!(prompt.contains("角色 韩立"));
        assert!(prompt.contains("桶 1-10"));
        assert!(prompt.contains("段 1-6"));
        assert!(prompt.contains("对话材料："));
        assert!(prompt.contains("拿到银钥匙"));
    }

    fn summary_memory(id: &str, turn_index: i32) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "archive".to_string(),
            content: "第一桶记忆".to_string(),
            source: "summary".to_string(),
            importance: 0.8,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            turn_index,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("scene-1".to_string()),
            memory_type: "summary".to_string(),
            speaker: None,
            role: Some("memory".to_string()),
            location: Some("码头".to_string()),
            participants: vec!["玩家".to_string(), "韩立".to_string()],
            keywords: vec!["码头".to_string()],
        }
    }

    fn summary_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: "Harbor".to_string(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({}),
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: Some("char-player".to_string()),
        }
    }

    fn summary_session(messages: Vec<ChatMessage>) -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "Harbor".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: String::new(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string()],
            messages,
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "harbor".to_string(),
                name: "Harbor".to_string(),
                background_hint: String::new(),
                temporary_tags: vec![],
                present_characters: vec!["Player".to_string(), "Alice".to_string()],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: GenerationParams::default(),
        }
    }

    fn summary_message(id: &str, turn_index: i32, content: &str) -> ChatMessage {
        ChatMessage {
            message_id: id.to_string(),
            role: "agent".to_string(),
            content: MessageContent::Text(content.to_string()),
            speaker: Some("Alice".to_string()),
            metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            parent_message_id: None,
        }
    }

    #[test]
    fn summary_persistence_rejects_rolled_back_or_rewritten_history() {
        let snapshot_messages = vec![
            summary_message("message-1", 1, "公开信息"),
            summary_message("message-2", 2, "已回滚的秘密"),
        ];
        let cases = vec![
            ("same", Some(snapshot_messages.clone()), true),
            (
                "appended",
                Some(vec![
                    snapshot_messages[0].clone(),
                    snapshot_messages[1].clone(),
                    summary_message("message-3", 3, "新回合"),
                ]),
                true,
            ),
            (
                "rolled_back",
                Some(vec![snapshot_messages[0].clone()]),
                false,
            ),
            (
                "rewritten",
                Some(vec![
                    snapshot_messages[0].clone(),
                    summary_message("replacement-2", 2, "重试后的内容"),
                ]),
                false,
            ),
            ("deleted", None, false),
        ];

        for (name, current_messages, expected) in cases {
            let conn = Connection::open_in_memory().expect("open sqlite");
            schema::create_tables(&conn).expect("create tables");
            if let Some(messages) = current_messages {
                SessionRepository::new(&conn)
                    .upsert(&summary_session(messages))
                    .expect("save current session");
            }
            let persisted = persist_summary(
                &conn,
                &summary_world(),
                &summary_session(snapshot_messages.clone()),
                &SummaryDraft {
                    character_id: "char-a".to_string(),
                    scene_id: "harbor".to_string(),
                    scene_name: "Harbor".to_string(),
                    turn_start: 1,
                    turn_end: 2,
                    content: "旧快照总结".to_string(),
                    participants: vec!["Player".to_string(), "Alice".to_string()],
                },
                0,
                1,
                10,
            )
            .expect("persist summary");
            assert_eq!(persisted, expected, "{name}");
            let memory_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
                .expect("count memories");
            let range_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM memory_summary_ranges", [], |row| {
                    row.get(0)
                })
                .expect("count summary ranges");
            assert_eq!(memory_count, i64::from(expected), "{name}");
            assert_eq!(range_count, i64::from(expected), "{name}");
        }
    }

    #[test]
    fn history_alignment_waits_for_the_summary_bucket() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");

        // 完全没有总结时保持用户请求的原文轮数；老存档不能一次拉回全部历史。
        assert_eq!(
            align_history_rounds_to_summary_bucket(&conn, "sess-1", Some("char-a"), 35, 30, 10),
            30
        );

        let memory = summary_memory("summary-1", 10);
        MemoryRepository::new(&conn)
            .insert(&memory)
            .expect("insert summary memory");
        MemorySummaryRepository::new(&conn)
            .insert(&MemorySummaryRange {
                memory_id: memory.id.clone(),
                world_id: "world-1".to_string(),
                session_id: "sess-1".to_string(),
                character_id: "char-a".to_string(),
                bucket_index: 0,
                bucket_start: 1,
                bucket_end: 10,
                scene_id: "scene-1".to_string(),
                scene_name: "码头".to_string(),
                turn_start: 1,
                turn_end: 10,
                created_at: memory.created_at.clone(),
            })
            .expect("insert summary range");

        // 第 35 回合：补回桶首是 34 回合，丢掉桶是 24 回合；
        // 34 更接近请求的 30，所以原文补到 1 号回合，避免过早压缩。
        assert_eq!(
            align_history_rounds_to_summary_bucket(&conn, "sess-1", Some("char-a"), 35, 30, 10),
            34
        );
    }
}
