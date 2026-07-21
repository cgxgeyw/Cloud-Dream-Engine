use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(feature = "local-embedding")]
use candle_core::{DType, Device, Tensor};
#[cfg(feature = "local-embedding")]
use candle_nn::VarBuilder;
#[cfg(feature = "local-embedding")]
use candle_transformers::models::bert::{BertModel as CandleBertModel, Config as CandleBertConfig};
use chrono::Utc;
use rusqlite::Connection;
#[cfg(feature = "local-embedding")]
use tokenizers::Tokenizer;

use crate::models::character::CharacterDefinition;
use crate::models::memory::{MemoryEntry, MemoryQueryParams};
use crate::models::model_config::{EmbeddingModelFileStatus, EmbeddingModelStatus, ModelConfig};
use crate::models::session::{ChatMessage, SessionSnapshot};
use crate::models::world::WorldDefinition;

const BUILTIN_LOCAL_EMBEDDING_PROVIDER: &str = "builtin-local";
const BUILTIN_LOCAL_EMBEDDING_MODEL_ID: &str = "BAAI/bge-small-zh-v1.5";
const BUILTIN_LOCAL_EMBEDDING_DISPLAY_NAME: &str = "内置本地 Embedding / bge-small-zh-v1.5";
#[cfg(feature = "local-embedding")]
const BUILTIN_LOCAL_EMBEDDING_MAX_LENGTH: usize = 512;
const BUILTIN_LOCAL_EMBEDDING_DOWNLOAD_BASES: &[&str] = &[
    "https://hf-mirror.com/BAAI/bge-small-zh-v1.5/resolve/main",
    "https://huggingface.co/BAAI/bge-small-zh-v1.5/resolve/main",
];
const BUILTIN_LOCAL_EMBEDDING_FILES: &[(&str, &str)] = &[
    ("model.safetensors", "model.safetensors"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
];

#[cfg(feature = "local-embedding")]
struct LocalEmbeddingInstance {
    model_key: String,
    tokenizer: Tokenizer,
    model: CandleBertModel,
    device: Device,
    max_length: usize,
}

#[cfg(feature = "local-embedding")]
type LocalEmbeddingCache = Mutex<Option<LocalEmbeddingInstance>>;

#[cfg(not(feature = "local-embedding"))]
type LocalEmbeddingCache = Mutex<()>;

/// 角色记忆召回的分阶段计划。
///
/// 设计目的(C1/H9):嵌入 HTTP 调用绝不能在持有全局 DB 锁期间发起。
/// 召回因此被拆为三步:
/// 1. `prepare_character_recall`(同步,持锁):读取候选记忆、词法打分、已存向量,
///    并收集仍需嵌入的输入。
/// 2. `embed_recall_plan`(异步,锁外):做嵌入 HTTP 往返,填充查询向量与新向量。
/// 3. `finalize_character_recall`(同步,持锁):写回新向量缓存并完成排序。
pub struct CharacterRecallPlan {
    limit: i32,
    candidates: Vec<MemoryEntry>,
    lexical_scores: HashMap<String, f64>,
    retrieval_mode: String,
    semantic_weight: f64,
    model: Option<ModelConfig>,
    model_key: String,
    query_input: String,
    stored_vectors: HashMap<String, Vec<f32>>,
    pending_inputs: Vec<(String, String)>,
    query_vector: Vec<f32>,
    computed_vectors: Vec<(String, Vec<f32>)>,
    /// 每个候选记忆的"有效层"(按回合年龄推算,见 derive_effective_layer)。
    /// 排序层加成与分层配额都以它为准,存储层保持不变。
    effective_layers: HashMap<String, String>,
    /// archive 层配额(默认 2;角色策略 importance_bias 时提升到 3)。
    archive_quota: usize,
}

impl CharacterRecallPlan {
    fn empty() -> Self {
        Self {
            limit: 1,
            candidates: Vec::new(),
            lexical_scores: HashMap::new(),
            retrieval_mode: "hybrid".to_string(),
            semantic_weight: 0.65,
            model: None,
            model_key: String::new(),
            query_input: String::new(),
            stored_vectors: HashMap::new(),
            pending_inputs: Vec::new(),
            query_vector: Vec::new(),
            computed_vectors: Vec::new(),
            effective_layers: HashMap::new(),
            archive_quota: 2,
        }
    }
}

pub struct MemoryService {
    data_dir: PathBuf,
    http_client: reqwest::blocking::Client,
    #[allow(dead_code)]
    local_embedding: LocalEmbeddingCache,
}

impl MemoryService {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_data_dir(std::env::temp_dir().join("dream-narrative-engine"))
    }

    pub fn with_data_dir(data_dir: PathBuf) -> Self {
        let http_client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .unwrap_or_default();
        Self {
            data_dir,
            http_client,
            local_embedding: build_local_embedding_cache(),
        }
    }

    /// 同步召回(测试 / 非锁敏感场景使用)。生产说话人回合走分阶段路径
    /// (`prepare_character_recall` → `embed_recall_plan` → `finalize_character_recall`),
    /// 以避免持锁期间发起嵌入 HTTP(C1/H9)。此处复用同一套阶段以保证行为一致。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn recall_entries_for_character(
        &self,
        conn: &Connection,
        world: &WorldDefinition,
        world_id: &str,
        session_id: &str,
        character_id: Option<&str>,
        query_text: &str,
        location: &str,
        scene_id: Option<&str>,
        participants: &[String],
        limit: i32,
        memory_strategy: Option<&str>,
    ) -> Result<Vec<MemoryEntry>, String> {
        let mut plan = self.prepare_character_recall(
            conn,
            world,
            world_id,
            session_id,
            character_id,
            query_text,
            location,
            scene_id,
            participants,
            limit,
            memory_strategy,
        )?;
        self.embed_recall_plan(&mut plan);
        Ok(self.finalize_character_recall(conn, plan))
    }

    /// 阶段 1(同步,持锁):读取候选记忆 + 词法打分 + 已存向量,收集待嵌入输入。
    /// 不做任何 HTTP。返回的 plan 交给 `embed_recall_plan`(锁外)继续。
    pub fn prepare_character_recall(
        &self,
        conn: &Connection,
        world: &WorldDefinition,
        world_id: &str,
        session_id: &str,
        character_id: Option<&str>,
        query_text: &str,
        location: &str,
        scene_id: Option<&str>,
        participants: &[String],
        limit: i32,
        memory_strategy: Option<&str>,
    ) -> Result<CharacterRecallPlan, String> {
        let Some(character_id) = character_id
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return Ok(CharacterRecallPlan::empty());
        };
        let strategy = parse_memory_strategy(memory_strategy.unwrap_or_default());
        if strategy.disabled {
            return Ok(CharacterRecallPlan::empty());
        }
        let tuning = MemoryRecallTuning {
            recency_bias: strategy.recency_bias,
            importance_bias: strategy.importance_bias,
        };
        let repo = crate::db::repositories::memory_repo::MemoryRepository::new(conn);
        let candidate_limit = resolve_character_memory_candidate_limit(world).max(limit.max(1) * 4);
        let candidates = repo.list(&MemoryQueryParams {
            world_id: Some(world_id.to_string()),
            session_id: Some(session_id.to_string()),
            character_id: Some(character_id.to_string()),
            layer: None,
            limit: Some(candidate_limit),
        })?;

        let newest_turn = candidates
            .iter()
            .map(|entry| entry.turn_index)
            .max()
            .unwrap_or(0);
        let working_window = resolve_character_memory_working_window_turns(world);
        let short_term_window = resolve_character_memory_short_term_window_turns(world);
        let effective_layers = candidates
            .iter()
            .map(|entry| {
                (
                    entry.id.clone(),
                    derive_effective_layer(entry, newest_turn, working_window, short_term_window)
                        .to_string(),
                )
            })
            .collect::<HashMap<_, _>>();

        let normalized_query = normalize_memory_text(query_text);
        let query_terms = build_memory_search_terms(query_text);
        let participant_terms = participants
            .iter()
            .map(|item| normalize_memory_text(item))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        let location_term = normalize_memory_text(location);
        let newest_created_at = candidates.iter().map(|entry| entry.created_at.clone()).max();
        let lexical_scores = candidates
            .iter()
            .map(|entry| {
                (
                    entry.id.clone(),
                    score_memory_entry(
                        entry,
                        effective_layers
                            .get(&entry.id)
                            .map(String::as_str)
                            .unwrap_or(entry.layer.as_str()),
                        &tuning,
                        &normalized_query,
                        &query_terms,
                        &participant_terms,
                        &location_term,
                        scene_id,
                        session_id,
                        newest_created_at.as_deref(),
                    ),
                )
            })
            .collect::<HashMap<_, _>>();

        let retrieval_mode = strategy
            .retrieval_mode
            .clone()
            .unwrap_or_else(|| resolve_character_memory_retrieval_mode(world));
        let semantic_weight = resolve_character_memory_semantic_weight(world);
        let mut plan = CharacterRecallPlan {
            limit,
            candidates,
            lexical_scores,
            retrieval_mode: retrieval_mode.clone(),
            semantic_weight,
            effective_layers,
            archive_quota: if strategy.importance_bias { 3 } else { 2 },
            ..CharacterRecallPlan::empty()
        };

        if retrieval_mode == "lexical_only" {
            return Ok(plan);
        }
        let Some(model) = self.resolve_embedding_model(conn).ok().flatten() else {
            return Ok(plan);
        };

        let model_key = model.id.trim().to_string();
        let memory_ids = plan
            .candidates
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        let embedding_repo =
            crate::db::repositories::memory_embedding_repo::MemoryEmbeddingRepository::new(conn);
        let stored_vectors =
            embedding_repo.list_by_model_and_memory_ids(&model_key, &memory_ids)?;
        let pending_inputs = plan
            .candidates
            .iter()
            .filter(|entry| !stored_vectors.contains_key(&entry.id))
            .map(|entry| (entry.id.clone(), build_memory_embedding_input(entry)))
            .collect::<Vec<_>>();

        plan.query_input =
            build_memory_query_text(query_text, location, scene_id, participants);
        plan.model = Some(model);
        plan.model_key = model_key;
        plan.stored_vectors = stored_vectors;
        plan.pending_inputs = pending_inputs;
        Ok(plan)
    }

    /// 阶段 2(可在锁外的 spawn_blocking 中执行):做嵌入 HTTP 往返。
    /// 仅触网,不碰 DB。本地嵌入模型同样在此完成(纯 CPU,无锁)。
    pub fn embed_recall_plan(&self, plan: &mut CharacterRecallPlan) {
        let Some(model) = plan.model.clone() else {
            return;
        };
        match self.embed_texts(&model, std::slice::from_ref(&plan.query_input)) {
            Ok(vectors) => {
                plan.query_vector = vectors.into_iter().next().unwrap_or_default();
            }
            Err(_) => {
                // 嵌入失败时退回纯词法排序。
                plan.model = None;
                return;
            }
        }
        if plan.query_vector.is_empty() {
            plan.model = None;
            return;
        }
        if plan.pending_inputs.is_empty() {
            return;
        }
        let inputs = plan
            .pending_inputs
            .iter()
            .map(|(_, input)| input.clone())
            .collect::<Vec<_>>();
        if let Ok(vectors) = self.embed_texts(&model, &inputs) {
            for ((id, _), vector) in plan.pending_inputs.iter().zip(vectors.into_iter()) {
                if !vector.is_empty() {
                    plan.computed_vectors.push((id.clone(), vector));
                }
            }
        }
    }

    /// 阶段 3(同步,持锁):写回新算出的向量缓存,计算语义分并完成排序。
    pub fn finalize_character_recall(
        &self,
        conn: &Connection,
        mut plan: CharacterRecallPlan,
    ) -> Vec<MemoryEntry> {
        let limit = plan.limit.max(1) as usize;
        let semantic_scores = if plan.model.is_some() && !plan.query_vector.is_empty() {
            if !plan.computed_vectors.is_empty() {
                let embedding_repo =
                    crate::db::repositories::memory_embedding_repo::MemoryEmbeddingRepository::new(
                        conn,
                    );
                for (id, vector) in &plan.computed_vectors {
                    let _ = embedding_repo.upsert(id, &plan.model_key, vector);
                    plan.stored_vectors.insert(id.clone(), vector.clone());
                }
            }
            let mut scores = HashMap::new();
            for entry in &plan.candidates {
                if let Some(vector) = plan.stored_vectors.get(&entry.id) {
                    let cosine = cosine_similarity(&plan.query_vector, vector);
                    scores.insert(entry.id.clone(), (cosine.clamp(-1.0, 1.0) + 1.0) / 2.0);
                }
            }
            scores
        } else {
            HashMap::new()
        };
        let ranked = rank_memories_by_scores(
            plan.candidates,
            &plan.lexical_scores,
            &semantic_scores,
            &plan.retrieval_mode,
            plan.semantic_weight,
            &plan.effective_layers,
            plan.archive_quota,
        );
        ranked.into_iter().take(limit).collect()
    }

    pub fn build_turn_entries(
        &self,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        turn_index: i32,
        player_character_id: &str,
        player_character_name: &str,
        characters: &[CharacterDefinition],
        messages: &[ChatMessage],
        visible_characters: &[String],
    ) -> Vec<MemoryEntry> {
        let participant_names = {
            let mut names = visible_characters.to_vec();
            if !names.contains(&player_character_name.to_string()) {
                names.push(player_character_name.to_string());
            }
            for message in messages {
                if extract_turn_index(message) == Some(turn_index) {
                    if let Some(speaker) = message.speaker.as_ref() {
                        if !names.contains(speaker) {
                            names.push(speaker.clone());
                        }
                    }
                }
            }
            names
        };

        let participant_ids = participant_names
            .iter()
            .filter_map(|name| {
                if name == player_character_name {
                    Some(player_character_id.to_string())
                } else {
                    characters
                        .iter()
                        .find(|character| character.name == *name)
                        .map(|character| character.id.clone())
                }
            })
            .collect::<Vec<_>>();

        let mut memories = Vec::new();
        let current_turn_messages = messages
            .iter()
            .filter(|message| extract_turn_index(message) == Some(turn_index))
            .collect::<Vec<_>>();
        let player_message = current_turn_messages
            .iter()
            .find(|message| message.role == "player")
            .map(|message| message.content.clone())
            .unwrap_or_default();
        let assistant_messages = current_turn_messages
            .iter()
            .filter(|message| message.role == "agent")
            .collect::<Vec<_>>();

        if !player_message.as_str().trim().is_empty() {
            for character_id in &participant_ids {
                // 单层写入:对话记忆只存 working 一份,"有效层"在召回时按回合年龄推算
                // (derive_effective_layer),避免同一段内容以多个 layer 副本重复入库/重复进提示词。
                memories.push(build_memory_entry(
                    world,
                    session,
                    turn_index,
                    character_id,
                    "working",
                    player_message.as_str(),
                    "player_action",
                    0.65,
                    "dialogue",
                    Some(player_character_name),
                    Some("player"),
                    Some(session.location.as_str()),
                    Some(session.scene.scene_id.as_str()),
                    participant_names.clone(),
                ));
            }
        }

        for assistant_message in assistant_messages {
            let content = assistant_message.content.trim().to_string();
            if content.is_empty() {
                continue;
            }
            let speaker_name = assistant_message
                .speaker
                .as_ref()
                .map(|value| value.as_str())
                .unwrap_or(player_character_name);
            for character_id in &participant_ids {
                let importance = if speaker_name == player_character_name {
                    0.75
                } else {
                    0.68
                };
                memories.push(build_memory_entry(
                    world,
                    session,
                    turn_index,
                    character_id,
                    "working",
                    &content,
                    "speaker_response",
                    importance,
                    "dialogue",
                    Some(speaker_name),
                    Some("agent"),
                    Some(session.location.as_str()),
                    Some(session.scene.scene_id.as_str()),
                    participant_names.clone(),
                ));
            }
        }

        memories
    }

    pub fn persist_turn_entries(
        &self,
        conn: &Connection,
        world: &WorldDefinition,
        session: &SessionSnapshot,
        turn_index: i32,
        player_character_id: &str,
        player_character_name: &str,
        characters: &[CharacterDefinition],
        messages: &[ChatMessage],
        visible_characters: &[String],
        extra_entries: &[MemoryEntry],
    ) -> Result<Vec<MemoryEntry>, String> {
        let mut memory_entries = extra_entries.to_vec();
        memory_entries.extend(self.build_turn_entries(
            world,
            session,
            turn_index,
            player_character_id,
            player_character_name,
            characters,
            messages,
            visible_characters,
        ));
        let memory_repo = crate::db::repositories::memory_repo::MemoryRepository::new(conn);
        for entry in &memory_entries {
            memory_repo.insert(entry)?;
        }
        Ok(memory_entries)
    }

    pub fn commit_turn_memories(
        &self,
        conn: &Connection,
        recovery_journal: &[serde_json::Value],
        session_id: &str,
        turn_index: i32,
        runtime_application: &crate::services::game_engine::runtime_effects::DirectorRuntimeApplication,
        updated: &SessionSnapshot,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
    ) -> Result<(), String> {
        if !journal_has_completed_step(recovery_journal, "attributes_committed") {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "attributes_committed",
                "completed",
                serde_json::json!({
                    "session_attribute_update_count": runtime_application.session_attribute_updates.len(),
                    "character_attribute_update_count": runtime_application.character_attribute_updates.len(),
                    "session_attribute_updates": runtime_application.session_attribute_updates.iter().map(|item| {
                        serde_json::json!({
                            "schema_id": item.schema_id,
                            "value": item.value,
                        })
                    }).collect::<Vec<_>>(),
                    "character_attribute_updates": runtime_application.character_attribute_updates.iter().map(|item| {
                        serde_json::json!({
                            "character_id": item.character_id,
                            "schema_id": item.schema_id,
                            "value": item.value,
                        })
                    }).collect::<Vec<_>>(),
                }),
            )?;
        }
        if !journal_has_completed_step(recovery_journal, "memory_committed") {
            let memory_entries = self.persist_turn_entries(
                conn,
                world,
                updated,
                turn_index,
                &session.player_character_id,
                &session.player_character_name,
                characters,
                &updated.messages,
                &updated.visible_characters,
                &runtime_application.memory_entries,
            )?;
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "memory_committed",
                "completed",
                serde_json::json!({
                    "memory_count": memory_entries.len(),
                    "memory_entries": memory_entries.iter().map(|entry| {
                        serde_json::json!({
                            "id": entry.id,
                            "character_id": entry.character_id,
                            "layer": entry.layer,
                            "memory_type": entry.memory_type,
                            "source": entry.source,
                            "importance": entry.importance,
                            "content": entry.content,
                            "scene_id": entry.scene_id,
                            "event_id": entry.event_id,
                            "conversation_id": entry.conversation_id,
                            "item_id": entry.item_id,
                            "speaker": entry.speaker,
                            "role": entry.role,
                            "location": entry.location,
                            "participants": entry.participants,
                            "keywords": entry.keywords,
                        })
                    }).collect::<Vec<_>>()
                }),
            )?;
        }
        Ok(())
    }

    pub fn get_builtin_model_status(
        &self,
        model_id: Option<&str>,
    ) -> Result<EmbeddingModelStatus, String> {
        self.builtin_model_status(
            model_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(BUILTIN_LOCAL_EMBEDDING_MODEL_ID),
        )
    }

    pub fn download_builtin_model(
        &self,
        model_id: Option<&str>,
    ) -> Result<EmbeddingModelStatus, String> {
        let model_id = model_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(BUILTIN_LOCAL_EMBEDDING_MODEL_ID);
        if model_id != BUILTIN_LOCAL_EMBEDDING_MODEL_ID {
            return Err(format!("Unsupported builtin embedding model: {}", model_id));
        }

        let model_dir = self.builtin_model_dir(model_id);
        fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
        for (relative_path, remote_path) in BUILTIN_LOCAL_EMBEDDING_FILES {
            self.download_builtin_model_file(&model_dir, relative_path, remote_path)?;
        }
        self.builtin_model_status(model_id)
    }

    fn builtin_model_status(&self, model_id: &str) -> Result<EmbeddingModelStatus, String> {
        let model_dir = self.builtin_model_dir(model_id);
        let files = BUILTIN_LOCAL_EMBEDDING_FILES
            .iter()
            .map(|(relative_path, _)| {
                let path = model_dir.join(relative_path);
                let metadata = fs::metadata(&path).ok();
                EmbeddingModelFileStatus {
                    name: Path::new(relative_path)
                        .file_name()
                        .and_then(|item| item.to_str())
                        .unwrap_or(relative_path)
                        .to_string(),
                    relative_path: (*relative_path).to_string(),
                    exists: metadata.is_some(),
                    size_bytes: metadata.map(|item| item.len()).unwrap_or(0),
                }
            })
            .collect::<Vec<_>>();
        let installed = files.iter().all(|item| item.exists && item.size_bytes > 0);
        let total_size_bytes = files.iter().map(|item| item.size_bytes).sum::<u64>();
        let detail = if installed {
            "Built-in embedding model is ready.".to_string()
        } else {
            "Built-in embedding model is not fully downloaded; falling back to non-embedding retrieval.".to_string()
        };
        Ok(EmbeddingModelStatus {
            model_id: model_id.to_string(),
            display_name: BUILTIN_LOCAL_EMBEDDING_DISPLAY_NAME.to_string(),
            installed,
            detail,
            local_dir: model_dir.to_string_lossy().to_string(),
            total_size_bytes,
            files,
        })
    }
    fn builtin_model_dir(&self, model_id: &str) -> PathBuf {
        self.data_dir
            .join("embedding-models")
            .join(sanitize_embedding_model_id(model_id))
    }

    fn download_builtin_model_file(
        &self,
        model_dir: &Path,
        relative_path: &str,
        remote_path: &str,
    ) -> Result<(), String> {
        let target_path = model_dir.join(relative_path);
        if target_path.exists()
            && fs::metadata(&target_path)
                .map(|item| item.len())
                .unwrap_or(0)
                > 0
        {
            return Ok(());
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut last_error = String::new();
        for base_url in BUILTIN_LOCAL_EMBEDDING_DOWNLOAD_BASES {
            let url = format!("{}/{}", base_url.trim_end_matches('/'), remote_path);
            match self.download_file(&url, &target_path) {
                Ok(()) => return Ok(()),
                Err(err) => last_error = format!("{} -> {}", url, err),
            }
        }
        Err(format!(
            "Failed to download {}: {}",
            relative_path, last_error
        ))
    }

    fn download_file(&self, url: &str, target_path: &Path) -> Result<(), String> {
        let mut response = self
            .http_client
            .get(url)
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {} while downloading", response.status()));
        }
        let temp_path = target_path.with_extension("part");
        let mut file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = response.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
        fs::rename(&temp_path, target_path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn resolve_embedding_model(&self, conn: &Connection) -> Result<Option<ModelConfig>, String> {
        let mut stmt = conn
            .prepare("SELECT embedding_enabled, default_embedding_model FROM settings WHERE id = 1")
            .map_err(|e| e.to_string())?;
        let (enabled, default_embedding_model): (bool, String) = stmt
            .query_row([], |row| {
                Ok((row.get::<_, i64>(0)? != 0, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        if !enabled {
            return Ok(None);
        }

        let repo = crate::db::repositories::model_repo::ModelRepository::new(conn);
        let models = repo.list(Some("embedding"))?;
        if let Some(model) = models
            .iter()
            .find(|model| {
                model
                    .id
                    .eq_ignore_ascii_case(default_embedding_model.trim())
                    || model
                        .model_id
                        .eq_ignore_ascii_case(default_embedding_model.trim())
                    || model
                        .name
                        .eq_ignore_ascii_case(default_embedding_model.trim())
            })
            .cloned()
            .or_else(|| models.iter().find(|model| model.is_default).cloned())
            .or_else(|| models.into_iter().next())
        {
            return Ok(Some(model));
        }

        if default_embedding_model.trim() == BUILTIN_LOCAL_EMBEDDING_MODEL_ID {
            return Ok(Some(ModelConfig {
                id: "model-seed-bge-small-embedding".to_string(),
                name: BUILTIN_LOCAL_EMBEDDING_DISPLAY_NAME.to_string(),
                model_type: "embedding".to_string(),
                provider: BUILTIN_LOCAL_EMBEDDING_PROVIDER.to_string(),
                model_id: BUILTIN_LOCAL_EMBEDDING_MODEL_ID.to_string(),
                base_url: String::new(),
                api_key: String::new(),
                max_tokens: 512,
                streaming_enabled: false,
                is_default: true,
            }));
        }

        Ok(None)
    }

    fn embed_texts(&self, model: &ModelConfig, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if is_builtin_local_embedding_model(model) {
            return self.embed_texts_local(model, texts);
        }
        self.embed_texts_remote(model, texts)
    }

    #[cfg(feature = "local-embedding")]
    fn embed_texts_local(
        &self,
        model: &ModelConfig,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, String> {
        let model_id = model.model_id.trim();
        if model_id != BUILTIN_LOCAL_EMBEDDING_MODEL_ID {
            return Err(format!(
                "Unsupported builtin local embedding model: {}",
                model_id
            ));
        }

        let status = self.builtin_model_status(model_id)?;
        if !status.installed {
            return Err("Builtin local embedding model is not downloaded".to_string());
        }

        let mut guard = self
            .local_embedding
            .lock()
            .map_err(|_| "Local embedding lock poisoned".to_string())?;
        if guard
            .as_ref()
            .map(|item| item.model_key.as_str() != model.id.trim())
            .unwrap_or(true)
        {
            let model_dir = self.builtin_model_dir(model_id);
            let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
                .map_err(|e| e.to_string())?;
            let config = serde_json::from_slice::<CandleBertConfig>(
                &fs::read(model_dir.join("config.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let device = Device::Cpu;
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(
                    &[model_dir.join("model.safetensors")],
                    DType::F32,
                    &device,
                )
            }
            .map_err(|e| e.to_string())?;
            let bert_model = CandleBertModel::load(vb, &config).map_err(|e| e.to_string())?;
            *guard = Some(LocalEmbeddingInstance {
                model_key: model.id.trim().to_string(),
                tokenizer,
                model: bert_model,
                device,
                max_length: config
                    .max_position_embeddings
                    .min(BUILTIN_LOCAL_EMBEDDING_MAX_LENGTH),
            });
        }

        let instance = guard
            .as_mut()
            .ok_or_else(|| "Local embedding model not loaded".to_string())?;
        embed_texts_with_candle(instance, texts)
    }

    #[cfg(not(feature = "local-embedding"))]
    fn embed_texts_local(
        &self,
        _model: &ModelConfig,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>, String> {
        Err(
            "Builtin local embedding is unavailable in this build; enable the `local-embedding` feature."
                .to_string(),
        )
    }

    fn embed_texts_remote(
        &self,
        model: &ModelConfig,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, String> {
        if model.base_url.trim().is_empty() {
            return Err("Embedding model base_url is empty".to_string());
        }

        let url = format!("{}/embeddings", model.base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", model.api_key.trim()))
            .json(&serde_json::json!({
                "model": model.model_id.trim(),
                "input": texts,
            }))
            .send();

        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(_response) if normalize_embedding_provider(&model.provider) == "ollama" => {
                return self.embed_texts_remote_ollama(model, texts);
            }
            Ok(response) => {
                return Err(format!(
                    "Embedding API error {} at {}",
                    response.status(),
                    url
                ));
            }
            Err(err) if normalize_embedding_provider(&model.provider) == "ollama" => {
                let _ = err;
                return self.embed_texts_remote_ollama(model, texts);
            }
            Err(err) => return Err(err.to_string()),
        };

        let payload = response
            .json::<serde_json::Value>()
            .map_err(|e| e.to_string())?;
        let data = payload
            .get("data")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "Embedding response missing data array".to_string())?;
        let mut vectors = Vec::with_capacity(data.len());
        for item in data {
            let vector = item
                .get("embedding")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "Embedding item missing vector".to_string())?
                .iter()
                .filter_map(|value| value.as_f64().map(|item| item as f32))
                .collect::<Vec<_>>();
            vectors.push(vector);
        }
        Ok(vectors)
    }

    fn embed_texts_remote_ollama(
        &self,
        model: &ModelConfig,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, String> {
        let base_url = model.base_url.trim_end_matches('/');
        let root_base = base_url.strip_suffix("/v1").unwrap_or(base_url);
        let url = format!("{}/api/embed", root_base);
        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": model.model_id.trim(),
                "input": texts,
            }))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Ollama embedding API error {} at {}",
                response.status(),
                url
            ));
        }
        let payload = response
            .json::<serde_json::Value>()
            .map_err(|e| e.to_string())?;
        let embeddings = payload
            .get("embeddings")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "Ollama embedding response missing embeddings".to_string())?;
        Ok(embeddings
            .iter()
            .map(|item| {
                item.as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_f64().map(|item| item as f32))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect())
    }
}

/// 纯排序+分层配额选择:给定词法分与语义分,产出最终顺序。
/// 与任何 DB / 网络 IO 无关,可在锁内或锁外随意调用(C1/H9 分阶段共用)。
/// 层配额比较用 effective_layers(查询时按回合年龄推算),查不到时回退存储层。
fn rank_memories_by_scores(
    mut memories: Vec<MemoryEntry>,
    lexical_scores: &HashMap<String, f64>,
    semantic_scores: &HashMap<String, f64>,
    retrieval_mode: &str,
    semantic_weight: f64,
    effective_layers: &HashMap<String, String>,
    archive_quota: usize,
) -> Vec<MemoryEntry> {
    let lexical_weight = (1.0 - semantic_weight).clamp(0.0, 1.0);
    let lexical_normalized = normalize_rank_scores(lexical_scores);

    memories.sort_by(|left, right| {
        let left_semantic = semantic_scores.get(&left.id).copied().unwrap_or(0.0);
        let right_semantic = semantic_scores.get(&right.id).copied().unwrap_or(0.0);
        let left_lexical = lexical_normalized.get(&left.id).copied().unwrap_or(0.0);
        let right_lexical = lexical_normalized.get(&right.id).copied().unwrap_or(0.0);
        let left_final = if semantic_scores.is_empty() {
            left_lexical
        } else if retrieval_mode == "semantic_only" {
            left_semantic
        } else {
            left_lexical * lexical_weight + left_semantic * semantic_weight
        };
        let right_final = if semantic_scores.is_empty() {
            right_lexical
        } else if retrieval_mode == "semantic_only" {
            right_semantic
        } else {
            right_lexical * lexical_weight + right_semantic * semantic_weight
        };
        right_final
            .partial_cmp(&left_final)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                lexical_scores
                    .get(&right.id)
                    .copied()
                    .unwrap_or(0.0)
                    .partial_cmp(&lexical_scores.get(&left.id).copied().unwrap_or(0.0))
                    .unwrap_or(Ordering::Equal)
            })
    });

    let mut balanced = Vec::new();
    let mut selected_ids = Vec::new();
    for (layer, quota) in [
        ("working", 2usize),
        ("short_term", 2usize),
        ("archive", archive_quota),
        ("canonical_event", 1usize),
    ] {
        let mut used = 0usize;
        for entry in &memories {
            let effective_layer = effective_layers
                .get(&entry.id)
                .map(String::as_str)
                .unwrap_or(entry.layer.as_str());
            if effective_layer != layer || selected_ids.contains(&entry.id) || used >= quota {
                continue;
            }
            balanced.push(entry.clone());
            selected_ids.push(entry.id.clone());
            used += 1;
        }
    }
    for entry in memories {
        if selected_ids.contains(&entry.id) {
            continue;
        }
        balanced.push(entry.clone());
        selected_ids.push(entry.id.clone());
    }
    balanced
}

fn sanitize_embedding_model_id(model_id: &str) -> String {
    model_id
        .trim()
        .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
}

fn is_builtin_local_embedding_model(model: &ModelConfig) -> bool {
    let provider = normalize_embedding_provider(&model.provider);
    provider == BUILTIN_LOCAL_EMBEDDING_PROVIDER
        || (model.model_id.trim() == BUILTIN_LOCAL_EMBEDDING_MODEL_ID
            && model.base_url.trim().is_empty())
}

fn normalize_embedding_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "builtin-local" | "builtin_local" | "local" | "内置本地" => {
            BUILTIN_LOCAL_EMBEDDING_PROVIDER.to_string()
        }
        "openai-compatible" | "openai compatible" | "openai" => "openai".to_string(),
        "ollama" => "ollama".to_string(),
        "lm studio" | "lmstudio" => "lmstudio".to_string(),
        other => other.to_string(),
    }
}
fn resolve_character_memory_retrieval_mode(world: &WorldDefinition) -> String {
    world
        .director_config
        .get("character_memory_retrieval_mode")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| matches!(value.as_str(), "lexical_only" | "hybrid" | "semantic_only"))
        .unwrap_or_else(|| "hybrid".to_string())
}

fn resolve_character_memory_candidate_limit(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("character_memory_candidate_limit")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(20, 600) as i32)
        .unwrap_or(200)
}

fn resolve_character_memory_semantic_weight(world: &WorldDefinition) -> f64 {
    world
        .director_config
        .get("character_memory_semantic_weight")
        .and_then(|value| value.as_f64())
        .map(|value| value.clamp(0.0, 1.0))
        .unwrap_or(0.65)
}

fn resolve_character_memory_working_window_turns(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("character_memory_working_window_turns")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(1, 10) as i32)
        .unwrap_or(3)
}

fn resolve_character_memory_short_term_window_turns(world: &WorldDefinition) -> i32 {
    world
        .director_config
        .get("character_memory_short_term_window_turns")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(5, 60) as i32)
        .unwrap_or(15)
}

/// 召回时按回合年龄推算记忆的"有效层"。
///
/// 对话记忆(player_action / speaker_response)自 M6 起只按 working 单层写入,
/// 这里的推算替代了过去"同一内容写三个 layer 副本"的做法:
/// 距最新回合 ≤ working_window → working,≤ short_term_window → short_term,更老 → archive。
/// 非对话记忆(trigger/rule/LLM 写入)与 canonical_event 保持存储层不变;
/// turn_index 缺失(≤0)时无法推算年龄,同样回退存储层。
fn derive_effective_layer<'a>(
    entry: &'a MemoryEntry,
    newest_turn: i32,
    working_window: i32,
    short_term_window: i32,
) -> &'a str {
    if entry.layer == "canonical_event" {
        return entry.layer.as_str();
    }
    let is_dialogue = matches!(entry.source.as_str(), "player_action" | "speaker_response");
    if is_dialogue
        && matches!(entry.layer.as_str(), "working" | "short_term" | "archive")
        && newest_turn > 0
        && entry.turn_index > 0
    {
        let age = (newest_turn - entry.turn_index).max(0);
        if age <= working_window {
            "working"
        } else if age <= short_term_window {
            "short_term"
        } else {
            "archive"
        }
    } else {
        entry.layer.as_str()
    }
}

/// 角色 memory_strategy 自由文本解析出的召回行为覆盖项。
/// 识别不到任何关键词时全部为默认,行为与之前完全一致(向后兼容)。
#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryStrategyOverrides {
    /// off/none/不记/无记忆:跳过召回。
    pub disabled: bool,
    /// semantic/语义 → semantic_only;lexical/词法 → lexical_only。
    pub retrieval_mode: Option<String>,
    /// recent/近期/最近:时效分加权。
    pub recency_bias: bool,
    /// important/archive/重要:重要度加权 + archive 配额提升。
    pub importance_bias: bool,
}

pub(crate) fn parse_memory_strategy(strategy: &str) -> MemoryStrategyOverrides {
    let text = strategy.trim().to_lowercase();
    let mut overrides = MemoryStrategyOverrides::default();
    if text.is_empty() {
        return overrides;
    }
    let contains_any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    // 短英文词整词匹配,避免 "offline"/"often" 这类误伤;中文按子串匹配。
    let has_word = |word: &str| {
        text.split(|ch: char| !ch.is_alphanumeric())
            .any(|token| token == word)
    };
    if has_word("off") || has_word("none") || contains_any(&["不记", "无记忆", "不用记"]) {
        overrides.disabled = true;
        return overrides;
    }
    if has_word("semantic") || text.contains("语义") {
        overrides.retrieval_mode = Some("semantic_only".to_string());
    } else if has_word("lexical") || text.contains("词法") {
        overrides.retrieval_mode = Some("lexical_only".to_string());
    }
    if has_word("recent") || contains_any(&["近期", "最近"]) {
        overrides.recency_bias = true;
    }
    if has_word("important") || has_word("archive") || text.contains("重要") {
        overrides.importance_bias = true;
    }
    overrides
}

fn build_memory_query_text(
    query_text: &str,
    location: &str,
    scene_id: Option<&str>,
    participants: &[String],
) -> String {
    let participants_line = participants
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    [
        format!("player_input: {}", query_text.trim()),
        format!("location: {}", location.trim()),
        format!("scene: {}", scene_id.unwrap_or_default().trim()),
        format!("participants: {}", participants_line),
    ]
    .into_iter()
    .filter(|line| !line.ends_with(':'))
    .collect::<Vec<_>>()
    .join("\n")
}
fn build_memory_embedding_input(memory: &MemoryEntry) -> String {
    let participants = memory
        .participants
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let keywords = memory
        .keywords
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    [
        format!("memory_type: {}", memory.memory_type.trim()),
        format!("source: {}", memory.source.trim()),
        format!("turn: {}", memory.turn_index),
        format!(
            "location: {}",
            memory.location.as_deref().unwrap_or_default().trim()
        ),
        format!(
            "scene: {}",
            memory.scene_id.as_deref().unwrap_or_default().trim()
        ),
        format!(
            "speaker: {}",
            memory.speaker.as_deref().unwrap_or_default().trim()
        ),
        format!("participants: {}", participants),
        format!("keywords: {}", keywords),
        format!("content: {}", memory.content.trim()),
    ]
    .into_iter()
    .filter(|line| !line.ends_with(':'))
    .collect::<Vec<_>>()
    .join("\n")
}
fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_value = *left_value as f64;
        let right_value = *right_value as f64;
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn normalize_rank_scores(scores: &HashMap<String, f64>) -> HashMap<String, f64> {
    if scores.is_empty() {
        return HashMap::new();
    }
    let min_score = scores.values().copied().fold(f64::INFINITY, f64::min);
    let max_score = scores.values().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (max_score - min_score).abs();
    scores
        .iter()
        .map(|(key, value)| {
            let normalized = if span <= f64::EPSILON {
                1.0
            } else {
                (value - min_score) / span
            };
            (key.clone(), normalized.clamp(0.0, 1.0))
        })
        .collect()
}

#[cfg(feature = "local-embedding")]
fn embed_texts_with_candle(
    instance: &mut LocalEmbeddingInstance,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let encodings = instance
        .tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| e.to_string())?;
    if encodings.is_empty() {
        return Ok(Vec::new());
    }

    let max_len = encodings
        .iter()
        .map(|encoding| encoding.len())
        .max()
        .unwrap_or(1)
        .min(instance.max_length.max(1));
    let batch_size = encodings.len();

    let mut input_ids = Vec::with_capacity(batch_size * max_len);
    let mut token_type_ids = Vec::with_capacity(batch_size * max_len);
    let mut attention_mask = Vec::with_capacity(batch_size * max_len);

    for encoding in encodings {
        let mut ids = encoding
            .get_ids()
            .iter()
            .copied()
            .take(max_len)
            .collect::<Vec<_>>();
        let mut type_ids = encoding
            .get_type_ids()
            .iter()
            .copied()
            .take(max_len)
            .collect::<Vec<_>>();
        let mut mask = encoding
            .get_attention_mask()
            .iter()
            .copied()
            .take(max_len)
            .collect::<Vec<_>>();
        while ids.len() < max_len {
            ids.push(0);
            type_ids.push(0);
            mask.push(0);
        }
        input_ids.extend(ids);
        token_type_ids.extend(type_ids);
        attention_mask.extend(mask);
    }

    let input_ids = Tensor::from_vec(input_ids, (batch_size, max_len), &instance.device)
        .map_err(|e| e.to_string())?;
    let token_type_ids = Tensor::from_vec(token_type_ids, (batch_size, max_len), &instance.device)
        .map_err(|e| e.to_string())?;
    let attention_mask = Tensor::from_vec(attention_mask, (batch_size, max_len), &instance.device)
        .map_err(|e| e.to_string())?;

    let sequence_output = instance
        .model
        .forward(&input_ids, &token_type_ids, Some(&attention_mask))
        .map_err(|e| e.to_string())?;
    let sequence_output = sequence_output
        .to_dtype(DType::F32)
        .map_err(|e| e.to_string())?;
    let tokens = sequence_output
        .to_vec3::<f32>()
        .map_err(|e| e.to_string())?;

    Ok(tokens
        .into_iter()
        .map(|batch_item| {
            let mut cls = batch_item.into_iter().next().unwrap_or_default();
            normalize_vector_in_place(&mut cls);
            cls
        })
        .collect())
}

#[cfg(feature = "local-embedding")]
fn build_local_embedding_cache() -> LocalEmbeddingCache {
    Mutex::new(None)
}

#[cfg(not(feature = "local-embedding"))]
fn build_local_embedding_cache() -> LocalEmbeddingCache {
    Mutex::new(())
}

#[cfg(feature = "local-embedding")]
fn normalize_vector_in_place(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|value| (*value as f64) * (*value as f64))
        .sum::<f64>()
        .sqrt();
    if norm <= f64::EPSILON {
        return;
    }
    for value in vector {
        *value = (*value as f64 / norm) as f32;
    }
}

fn append_turn_journal(
    conn: &Connection,
    session_id: &str,
    turn_index: i32,
    step: &str,
    status: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO turn_journal (id, session_id, turn_index, step, status, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            session_id,
            turn_index,
            step,
            status,
            serde_json::to_string(&payload).map_err(|e| e.to_string())?,
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn journal_has_completed_step(journal: &[serde_json::Value], step: &str) -> bool {
    journal.iter().any(|entry| {
        entry.get("step").and_then(|value| value.as_str()) == Some(step)
            && entry.get("status").and_then(|value| value.as_str()) == Some("completed")
    })
}

fn build_memory_search_terms(text: &str) -> Vec<String> {
    let normalized = normalize_memory_text(text);
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut terms = Vec::new();
    for token in normalized
        .split(|ch: char| !ch.is_alphanumeric() && !('\u{4e00}'..='\u{9fff}').contains(&ch))
        .filter(|token| token.len() >= 3)
    {
        let token = token.to_string();
        if !terms.contains(&token) {
            terms.push(token);
        }
    }
    let max_len = chars.len().min(4);
    for size in 2..=max_len {
        for start in 0..chars.len().saturating_sub(size) + 1 {
            let term = chars[start..start + size].iter().collect::<String>();
            if term.len() >= 2 && !terms.contains(&term) {
                terms.push(term);
            }
        }
    }
    if !normalized.is_empty() && normalized.len() <= 24 && !terms.contains(&normalized) {
        terms.push(normalized);
    }
    terms
}

/// 词法打分的可调项,来自角色 memory_strategy 的关键词解析(parse_memory_strategy)。
#[derive(Debug, Clone, Default)]
struct MemoryRecallTuning {
    /// true 时 recency_bonus ×2.0(偏好近期记忆)。
    recency_bias: bool,
    /// true 时 importance 起始分 ×2.0(偏好高重要度记忆)。
    importance_bias: bool,
}

fn score_memory_entry(
    entry: &MemoryEntry,
    effective_layer: &str,
    tuning: &MemoryRecallTuning,
    normalized_query: &str,
    query_terms: &[String],
    participant_terms: &[String],
    location_term: &str,
    scene_id: Option<&str>,
    session_id: &str,
    newest_created_at: Option<&str>,
) -> f64 {
    let mut searchable_parts = vec![entry.content.clone()];
    if let Some(speaker) = &entry.speaker {
        searchable_parts.push(speaker.clone());
    }
    if let Some(location) = &entry.location {
        searchable_parts.push(location.clone());
    }
    searchable_parts.extend(entry.participants.clone());
    searchable_parts.extend(entry.keywords.clone());
    let searchable = normalize_memory_text(&searchable_parts.join(" "));
    let mut score = if tuning.importance_bias {
        entry.importance * 2.0
    } else {
        entry.importance
    };
    if !normalized_query.is_empty() && searchable.contains(normalized_query) {
        score += 8.0;
    }
    for term in query_terms {
        if !term.is_empty() && searchable.contains(term) {
            score += 2.0 + (term.len().min(6) as f64) * 0.2;
        }
    }
    let participant_hits = participant_terms
        .iter()
        .filter(|term| !term.is_empty() && searchable.contains(term.as_str()))
        .count();
    score += participant_hits as f64 * 2.5;
    if !location_term.is_empty()
        && location_term == normalize_memory_text(entry.location.as_deref().unwrap_or_default())
    {
        score += 2.0;
    }
    if let Some(scene_id) = scene_id {
        if entry
            .scene_id
            .as_deref()
            .map(|value| value == scene_id)
            .unwrap_or(false)
        {
            score += 2.2;
        }
    }
    if entry
        .conversation_id
        .as_deref()
        .map(|value| value == session_id)
        .unwrap_or(false)
    {
        score += 1.8;
        if entry.memory_type == "dialogue" {
            score += 0.9;
        }
    }
    if entry.memory_type == "dialogue" {
        score += 0.4;
    }
    score += match effective_layer {
        "working" => 2.4,
        "short_term" => 1.6,
        "canonical_event" => 1.9,
        _ => 0.0,
    };
    let recency = recency_bonus(entry.created_at.as_str(), newest_created_at);
    score += if tuning.recency_bias {
        recency * 2.0
    } else {
        recency
    };
    score
}

fn recency_bonus(created_at: &str, newest_created_at: Option<&str>) -> f64 {
    let parse = |value: &str| chrono::DateTime::parse_from_rfc3339(value).ok();
    let Some(newest_created_at) = newest_created_at.and_then(parse) else {
        return 0.0;
    };
    let Some(created_at) = parse(created_at) else {
        return 0.0;
    };
    let age_seconds = (newest_created_at - created_at).num_seconds().max(0) as f64;
    let age_days = age_seconds / 86400.0;
    if age_days <= 1.0 {
        1.6
    } else if age_days <= 3.0 {
        1.1
    } else if age_days <= 7.0 {
        0.6
    } else if age_days <= 14.0 {
        0.2
    } else {
        0.0
    }
}

fn normalize_memory_text(text: &str) -> String {
    let compact = text.trim().to_lowercase().replace(char::is_whitespace, "");
    compact
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .collect()
}

fn extract_turn_index(message: &ChatMessage) -> Option<i32> {
    message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("turn_index"))
        .and_then(|value| value.as_i64())
        .map(|value| value as i32)
}

fn build_memory_entry(
    world: &WorldDefinition,
    session: &SessionSnapshot,
    turn_index: i32,
    character_id: &str,
    layer: &str,
    content: &str,
    source: &str,
    importance: f64,
    memory_type: &str,
    speaker: Option<&str>,
    role: Option<&str>,
    location: Option<&str>,
    scene_id: Option<&str>,
    participants: Vec<String>,
) -> MemoryEntry {
    MemoryEntry {
        id: format!("mem-{}", uuid::Uuid::new_v4().simple()),
        world_id: world.id.clone(),
        session_id: session.id.clone(),
        character_id: character_id.to_string(),
        layer: layer.trim().to_string(),
        content: content.trim().to_string(),
        source: source.trim().to_string(),
        importance,
        created_at: Utc::now().to_rfc3339(),
        turn_index,
        conversation_id: Some(session.id.clone()),
        event_id: None,
        item_id: None,
        scene_id: scene_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        memory_type: memory_type.trim().to_string(),
        speaker: speaker
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        role: role
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        location: location
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        participants: participants
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
        keywords: extract_memory_keywords(content),
    }
}

fn extract_memory_keywords(content: &str) -> Vec<String> {
    let normalized = normalize_memory_text(content);
    let mut keywords = Vec::new();
    for token in normalized
        .split(|ch: char| !ch.is_alphanumeric() && !('\u{4e00}'..='\u{9fff}').contains(&ch))
        .filter(|value| value.len() >= 2)
    {
        let token = token.to_string();
        if !keywords.contains(&token) {
            keywords.push(token);
        }
    }
    keywords
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::memory_repo::MemoryRepository;
    use crate::db::schema;
    use crate::models::session::{AssetSelection, MessageContent, SceneRuntime, SessionState};
    use rusqlite::Connection;

    fn sample_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: "".to_string(),
            background_prompt: "".to_string(),
            opening_scene: "Harbor".to_string(),
            summary: "".to_string(),
            time_system: "".to_string(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: "".to_string(),
            director_runtime_system_prompt: "".to_string(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: Some("char-player".to_string()),
        }
    }

    fn sample_session() -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "Harbor".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "".to_string(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string(), "Bob".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            inventory_items: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "harbor-scene".to_string(),
                name: "Harbor".to_string(),
                background_hint: "".to_string(),
                temporary_tags: vec![],
                present_characters: vec![
                    "Player".to_string(),
                    "Alice".to_string(),
                    "Bob".to_string(),
                ],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
        }
    }

    fn sample_characters() -> Vec<CharacterDefinition> {
        vec![
            CharacterDefinition {
                id: "char-a".to_string(),
                name: "Alice".to_string(),
                world_id: "world-1".to_string(),
                role: "".to_string(),
                background_prompt: "".to_string(),
                model: "".to_string(),
                memory_strategy: "".to_string(),
                recent_dialogue_rounds: 8,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: "".to_string(),
                response_contract_prompt: "".to_string(),
                narration_prompt: "".to_string(),
                runtime_system_prompt: "".to_string(),
            },
            CharacterDefinition {
                id: "char-b".to_string(),
                name: "Bob".to_string(),
                world_id: "world-1".to_string(),
                role: "".to_string(),
                background_prompt: "".to_string(),
                model: "".to_string(),
                memory_strategy: "".to_string(),
                recent_dialogue_rounds: 8,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: "".to_string(),
                response_contract_prompt: "".to_string(),
                narration_prompt: "".to_string(),
                runtime_system_prompt: "".to_string(),
            },
        ]
    }

    #[test]
    fn recall_entries_for_character_are_isolated_per_character() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        let alice_entry = MemoryEntry {
            id: "m-a".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "Alice secretly hid the silver key in locker 12.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.9,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 3,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec!["silver".to_string(), "key".to_string()],
        };
        let bob_entry = MemoryEntry {
            id: "m-b".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-b".to_string(),
            layer: "working".to_string(),
            content: "Bob believes the key was burned already.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.8,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 3,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Bob".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Bob".to_string()],
            keywords: vec!["key".to_string()],
        };
        repo.insert(&alice_entry).expect("insert alice memory");
        repo.insert(&bob_entry).expect("insert bob memory");

        let recalled_for_alice = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "Where is the silver key?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                10,
                None,
            )
            .expect("recall for alice");
        assert!(
            recalled_for_alice
                .iter()
                .any(|entry| entry.content.contains("silver key in locker 12")),
            "alice should recall her own memory"
        );
        assert!(
            recalled_for_alice
                .iter()
                .all(|entry| entry.character_id == "char-a"
                    && !entry.content.contains("burned already")),
            "alice recall must not leak bob memory"
        );
    }

    #[test]
    fn recall_entries_for_character_preserves_raw_dialogue_fields() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        let archive_entry = MemoryEntry {
            id: "m-archive-a".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "archive".to_string(),
            content: "Alice learned the vault code is 4318.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.95,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 1,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec!["vault".to_string(), "4318".to_string()],
        };
        repo.insert(&archive_entry).expect("insert archive memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "What is the vault code?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                5,
                None,
            )
            .expect("recall raw entries");

        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].layer, "archive");
        assert_eq!(recalled[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(recalled[0].role.as_deref(), Some("agent"));
        assert!(recalled[0].content.contains("4318"));
    }

    #[test]
    fn persist_turn_entries_writes_single_working_layer_without_duplicates() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let session = sample_session();
        let characters = sample_characters();
        let turn_index = 5;
        let messages = vec![
            ChatMessage {
                role: "player".to_string(),
                content: MessageContent::Text("Take the brass compass and hide it.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
            ChatMessage {
                role: "agent".to_string(),
                content: MessageContent::Text("Alice pockets the brass compass and nods.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
        ];

        let written = service
            .persist_turn_entries(
                &conn,
                &world,
                &session,
                turn_index,
                "char-player",
                "Player",
                &characters,
                &messages,
                &["Alice".to_string(), "Bob".to_string()],
                &[],
            )
            .expect("persist turn entries");
        // 单层写入:每条消息 × 每个在场角色(Player/Alice/Bob)恰好 1 条 working。
        assert_eq!(written.len(), 2 * 3);
        assert!(
            written.iter().all(|entry| entry.layer == "working"),
            "dialogue memories should be written once with the working layer"
        );
        let mut unique_pairs = std::collections::HashSet::new();
        for entry in &written {
            assert!(
                unique_pairs.insert((entry.character_id.clone(), entry.content.clone())),
                "same content must not be stored twice for one character"
            );
        }

        let repo = MemoryRepository::new(&conn);
        let all_for_alice = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-a".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list memories");
        assert!(
            all_for_alice
                .iter()
                .any(|entry| entry.content.contains("brass compass")),
            "persisted memory should be queryable from database"
        );
    }

    #[test]
    fn persist_turn_entries_isolates_private_dialogue_by_present_participants() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let session = sample_session();
        let characters = sample_characters();
        let turn_index = 7;
        let messages = vec![
            ChatMessage {
                role: "player".to_string(),
                content: MessageContent::Text("Tell Alice the passphrase is moon glass.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
            ChatMessage {
                role: "agent".to_string(),
                content: MessageContent::Text("Alice memorizes the passphrase: moon glass.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
        ];

        service
            .persist_turn_entries(
                &conn,
                &world,
                &session,
                turn_index,
                "char-player",
                "Player",
                &characters,
                &messages,
                &["Alice".to_string()],
                &[],
            )
            .expect("persist isolated turn");

        let repo = MemoryRepository::new(&conn);
        let alice_memories = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-a".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list alice memories");
        let bob_memories = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-b".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list bob memories");

        assert!(
            alice_memories
                .iter()
                .any(|entry| entry.content.contains("moon glass")),
            "alice should receive the private passphrase memory"
        );
        assert!(
            bob_memories
                .iter()
                .all(|entry| !entry.content.contains("moon glass")),
            "bob should not receive memories from dialogue he was not present for"
        );
    }

    #[test]
    fn recall_entries_for_character_keeps_old_memory_visible_despite_recent_noise() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        // 近期噪音:turn 20-25,全部 stored working。
        for index in 0..6 {
            repo.insert(&MemoryEntry {
                id: format!("m-work-{index}"),
                world_id: "world-1".to_string(),
                session_id: "sess-1".to_string(),
                character_id: "char-a".to_string(),
                layer: "working".to_string(),
                content: format!("Ambient harbor chatter {index}"),
                source: "speaker_response".to_string(),
                importance: 0.2,
                created_at: Utc::now().to_rfc3339(),
                turn_index: 20 + index,
                conversation_id: Some("sess-1".to_string()),
                event_id: None,
                item_id: None,
                scene_id: Some("harbor-scene".to_string()),
                memory_type: "dialogue".to_string(),
                speaker: Some("Alice".to_string()),
                role: Some("agent".to_string()),
                location: Some("Harbor".to_string()),
                participants: vec!["Player".to_string(), "Alice".to_string()],
                keywords: vec!["harbor".to_string()],
            })
            .expect("insert noise memory");
        }
        // 老记忆:turn 1,距最新 24 轮 > short_term 窗口(15),有效层推算为 archive,
        // 应被 archive 配额保住;存储层保持 working 不变。
        repo.insert(&MemoryEntry {
            id: "m-archive-secret".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "Alice archived that the eclipse gate opens with moon glass.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.98,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 1,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec![
                "eclipse".to_string(),
                "moon".to_string(),
                "glass".to_string(),
            ],
        })
        .expect("insert archive memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "How does the eclipse gate open?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                5,
                None,
            )
            .expect("recall with archive");

        assert!(
            recalled
                .iter()
                .any(|entry| entry.id == "m-archive-secret"
                    && entry.layer == "working"
                    && entry.content.contains("moon glass")),
            "old memory should remain recallable via the derived archive quota, stored layer unchanged"
        );
    }

    fn bare_entry(id: &str, layer: &str, source: &str, turn_index: i32) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: layer.to_string(),
            content: format!("content-{id}"),
            source: source.to_string(),
            importance: 0.5,
            created_at: Utc::now().to_rfc3339(),
            turn_index,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: None,
            memory_type: "dialogue".to_string(),
            speaker: None,
            role: None,
            location: None,
            participants: vec![],
            keywords: vec![],
        }
    }

    #[test]
    fn derive_effective_layer_ages_dialogue_memories_by_turn_distance() {
        let cases = [
            // (turn_index, 期望有效层): newest=20, working≤3, short_term≤15
            (20, "working"),
            (17, "working"),  // age 3,边界
            (16, "short_term"), // age 4
            (5, "short_term"),  // age 15,边界
            (4, "archive"),   // age 16
            (1, "archive"),
        ];
        for (turn_index, expected) in cases {
            let entry = bare_entry("m", "working", "speaker_response", turn_index);
            assert_eq!(
                derive_effective_layer(&entry, 20, 3, 15),
                expected,
                "turn {turn_index} should derive to {expected}"
            );
        }
    }

    #[test]
    fn derive_effective_layer_passes_through_non_dialogue_and_canonical() {
        // canonical_event 直通
        let canonical = bare_entry("m", "canonical_event", "speaker_response", 1);
        assert_eq!(
            derive_effective_layer(&canonical, 20, 3, 15),
            "canonical_event"
        );
        // 非对话 source(trigger/rule/LLM 写入)保持存储层
        let rule_memory = bare_entry("m", "short_term", "rule", 1);
        assert_eq!(
            derive_effective_layer(&rule_memory, 20, 3, 15),
            "short_term"
        );
        // turn_index 缺失(≤0)无法推算年龄,回退存储层
        let no_turn = bare_entry("m", "working", "player_action", 0);
        assert_eq!(derive_effective_layer(&no_turn, 20, 3, 15), "working");
        // newest_turn 未知(空候选池)同样回退
        let entry = bare_entry("m", "working", "player_action", 3);
        assert_eq!(derive_effective_layer(&entry, 0, 3, 15), "working");
    }

    #[test]
    fn parse_memory_strategy_recognizes_keywords() {
        let default = parse_memory_strategy("");
        assert!(!default.disabled && !default.recency_bias && !default.importance_bias);
        assert!(default.retrieval_mode.is_none());

        assert!(parse_memory_strategy("off").disabled);
        assert!(parse_memory_strategy("无记忆").disabled);
        assert!(parse_memory_strategy("不用记住玩家说的话").disabled);
        // 整词匹配:"offline"/"often" 不应触发 disabled
        assert!(!parse_memory_strategy("offline archive").disabled);
        assert!(!parse_memory_strategy("often recalls").disabled);
        // disabled 优先级最高
        let disabled = parse_memory_strategy("off recent important");
        assert!(disabled.disabled && !disabled.recency_bias && !disabled.importance_bias);

        assert_eq!(
            parse_memory_strategy("semantic").retrieval_mode.as_deref(),
            Some("semantic_only")
        );
        assert_eq!(
            parse_memory_strategy("用语义检索").retrieval_mode.as_deref(),
            Some("semantic_only")
        );
        assert_eq!(
            parse_memory_strategy("lexical").retrieval_mode.as_deref(),
            Some("lexical_only")
        );

        assert!(parse_memory_strategy("recent").recency_bias);
        assert!(parse_memory_strategy("多关注最近发生的事").recency_bias);
        assert!(parse_memory_strategy("important").importance_bias);
        assert!(parse_memory_strategy("重要的事要牢记").importance_bias);

        // 种子里的自然语言描述不含任何关键词 → 全默认,行为不变
        for seed_text in [
            "记住宴会中的人际变化与诗句往来。",
            "default",
            "short memory guidance",
        ] {
            let parsed = parse_memory_strategy(seed_text);
            assert!(
                !parsed.disabled
                    && !parsed.recency_bias
                    && !parsed.importance_bias
                    && parsed.retrieval_mode.is_none(),
                "seed text {:?} should parse to defaults",
                seed_text
            );
        }
    }

    #[test]
    fn recall_with_disabled_strategy_returns_nothing() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();
        repo.insert(&bare_entry("m-1", "working", "speaker_response", 3))
            .expect("insert memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("不记"),
            )
            .expect("recall with disabled strategy");
        assert!(recalled.is_empty(), "disabled strategy should skip recall");
    }

    #[test]
    fn recall_strategy_overrides_retrieval_mode() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();
        repo.insert(&bare_entry("m-1", "working", "speaker_response", 3))
            .expect("insert memory");

        let plan = service
            .prepare_character_recall(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("semantic"),
            )
            .expect("prepare recall");
        assert_eq!(plan.retrieval_mode, "semantic_only");

        let plan = service
            .prepare_character_recall(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("important"),
            )
            .expect("prepare recall");
        assert_eq!(plan.retrieval_mode, "hybrid", "mode 未指定时保持 world 默认");
        assert_eq!(plan.archive_quota, 3, "importance_bias 应提升 archive 配额");
    }

    #[test]
    fn rank_memories_uses_effective_layers_for_quotas() {
        // B 词法分高于 C,但 B 的有效层是 archive、C 是 working:
        // working 配额先选 A、C,archive 配额再选 B。
        let memories = vec![
            bare_entry("a", "working", "speaker_response", 20),
            bare_entry("b", "working", "speaker_response", 1),
            bare_entry("c", "working", "speaker_response", 19),
        ];
        let lexical_scores: HashMap<String, f64> = [
            ("a".to_string(), 10.0),
            ("b".to_string(), 8.0),
            ("c".to_string(), 6.0),
        ]
        .into_iter()
        .collect();
        let effective_layers: HashMap<String, String> = [
            ("a".to_string(), "working".to_string()),
            ("b".to_string(), "archive".to_string()),
            ("c".to_string(), "working".to_string()),
        ]
        .into_iter()
        .collect();

        let ranked = rank_memories_by_scores(
            memories,
            &lexical_scores,
            &HashMap::new(),
            "hybrid",
            0.65,
            &effective_layers,
            2,
        );
        let order: Vec<&str> = ranked.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(order, vec!["a", "c", "b"]);
    }

    #[test]
    fn normalize_embedding_provider_accepts_chinese_builtin_local() {
        assert_eq!(normalize_embedding_provider("内置本地"), "builtin-local");
        assert_eq!(normalize_embedding_provider("builtin-local"), "builtin-local");
    }
}
