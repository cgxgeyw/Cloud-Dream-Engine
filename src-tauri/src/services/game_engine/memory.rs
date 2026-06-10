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
const BUILTIN_LOCAL_EMBEDDING_DISPLAY_NAME: &str = "鍐呯疆 Embedding锛欱AAI/bge-small-zh-v1.5";
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
    ) -> Result<Vec<MemoryEntry>, String> {
        let Some(character_id) = character_id
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return Ok(Vec::new());
        };
        let repo = crate::db::repositories::memory_repo::MemoryRepository::new(conn);
        let candidate_limit = resolve_character_memory_candidate_limit(world).max(limit.max(1) * 4);
        let memories = repo.list(&MemoryQueryParams {
            world_id: Some(world_id.to_string()),
            session_id: Some(session_id.to_string()),
            character_id: Some(character_id.to_string()),
            layer: None,
            limit: Some(candidate_limit),
        })?;
        let ranked = self.rank_character_memories(
            conn,
            world,
            memories,
            session_id,
            query_text,
            location,
            scene_id,
            participants,
        );
        Ok(ranked.into_iter().take(limit.max(1) as usize).collect())
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
                for layer in ["working", "short_term", "archive"] {
                    memories.push(build_memory_entry(
                        world,
                        session,
                        turn_index,
                        character_id,
                        layer,
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
                for layer in ["working", "short_term", "archive"] {
                    memories.push(build_memory_entry(
                        world,
                        session,
                        turn_index,
                        character_id,
                        layer,
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

    fn build_semantic_score_map(
        &self,
        conn: &Connection,
        memories: &[MemoryEntry],
        model: &ModelConfig,
        query_text: &str,
        location: &str,
        scene_id: Option<&str>,
        participants: &[String],
    ) -> Result<HashMap<String, f64>, String> {
        if memories.is_empty() {
            return Ok(HashMap::new());
        }
        let query_inputs = vec![build_memory_query_text(
            query_text,
            location,
            scene_id,
            participants,
        )];
        let query_vector = self
            .embed_texts(model, &query_inputs)?
            .into_iter()
            .next()
            .unwrap_or_default();
        if query_vector.is_empty() {
            return Ok(HashMap::new());
        }

        let model_key = model.id.trim().to_string();
        let memory_ids = memories
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        let embedding_repo =
            crate::db::repositories::memory_embedding_repo::MemoryEmbeddingRepository::new(conn);
        let mut stored_vectors =
            embedding_repo.list_by_model_and_memory_ids(&model_key, &memory_ids)?;

        let missing = memories
            .iter()
            .filter(|entry| !stored_vectors.contains_key(&entry.id))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let inputs = missing
                .iter()
                .map(build_memory_embedding_input)
                .collect::<Vec<_>>();
            let vectors = self.embed_texts(model, &inputs)?;
            for (entry, vector) in missing.iter().zip(vectors.into_iter()) {
                if vector.is_empty() {
                    continue;
                }
                embedding_repo.upsert(&entry.id, &model_key, &vector)?;
                stored_vectors.insert(entry.id.clone(), vector);
            }
        }

        let mut scores = HashMap::new();
        for entry in memories {
            let Some(vector) = stored_vectors.get(&entry.id) else {
                continue;
            };
            let cosine = cosine_similarity(&query_vector, vector);
            scores.insert(entry.id.clone(), (cosine.clamp(-1.0, 1.0) + 1.0) / 2.0);
        }
        Ok(scores)
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

    fn rank_character_memories(
        &self,
        conn: &Connection,
        world: &WorldDefinition,
        mut memories: Vec<MemoryEntry>,
        session_id: &str,
        query_text: &str,
        location: &str,
        scene_id: Option<&str>,
        participants: &[String],
    ) -> Vec<MemoryEntry> {
        let normalized_query = normalize_memory_text(query_text);
        let query_terms = build_memory_search_terms(query_text);
        let participant_terms = participants
            .iter()
            .map(|item| normalize_memory_text(item))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        let location_term = normalize_memory_text(location);
        let newest_created_at = memories.iter().map(|entry| entry.created_at.clone()).max();
        let retrieval_mode = resolve_character_memory_retrieval_mode(world);
        let lexical_scores = memories
            .iter()
            .map(|entry| {
                (
                    entry.id.clone(),
                    score_memory_entry(
                        entry,
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
        let semantic_scores = if retrieval_mode == "lexical_only" {
            HashMap::new()
        } else {
            self.resolve_embedding_model(conn)
                .ok()
                .flatten()
                .and_then(|model| {
                    self.build_semantic_score_map(
                        conn,
                        &memories,
                        &model,
                        query_text,
                        location,
                        scene_id,
                        participants,
                    )
                    .ok()
                })
                .unwrap_or_default()
        };
        let semantic_weight = resolve_character_memory_semantic_weight(world);
        let lexical_weight = (1.0 - semantic_weight).clamp(0.0, 1.0);
        let lexical_normalized = normalize_rank_scores(&lexical_scores);

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
            ("archive", 2usize),
            ("canonical_event", 1usize),
        ] {
            let mut used = 0usize;
            for entry in &memories {
                if entry.layer != layer || selected_ids.contains(&entry.id) || used >= quota {
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
        "builtin-local" | "builtin_local" | "local" | "鍐呯疆鏈湴" => {
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

fn score_memory_entry(
    entry: &MemoryEntry,
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
    let mut score = entry.importance;
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
    score += match entry.layer.as_str() {
        "working" => 2.4,
        "short_term" => 1.6,
        "canonical_event" => 1.9,
        _ => 0.0,
    };
    score += recency_bonus(entry.created_at.as_str(), newest_created_at);
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
            )
            .expect("recall raw entries");

        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].layer, "archive");
        assert_eq!(recalled[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(recalled[0].role.as_deref(), Some("agent"));
        assert!(recalled[0].content.contains("4318"));
    }

    #[test]
    fn persist_turn_entries_writes_archive_layer() {
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
        assert!(
            written.iter().any(|entry| entry.layer == "archive"),
            "turn persistence should include archive layer for long-term memory"
        );

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
    fn recall_entries_for_character_keeps_archive_visible_despite_recent_noise() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        for index in 0..6 {
            repo.insert(&MemoryEntry {
                id: format!("m-work-{index}"),
                world_id: "world-1".to_string(),
                session_id: "sess-1".to_string(),
                character_id: "char-a".to_string(),
                layer: if index % 2 == 0 {
                    "working".to_string()
                } else {
                    "short_term".to_string()
                },
                content: format!("Ambient harbor chatter {index}"),
                source: "speaker_response".to_string(),
                importance: 0.2,
                created_at: Utc::now().to_rfc3339(),
                turn_index: 10 + index,
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
        repo.insert(&MemoryEntry {
            id: "m-archive-secret".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "archive".to_string(),
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
            )
            .expect("recall with archive");

        assert!(
            recalled
                .iter()
                .any(|entry| entry.layer == "archive" && entry.content.contains("moon glass")),
            "archive memory should remain recallable even when recent working memories exist"
        );
    }
}
