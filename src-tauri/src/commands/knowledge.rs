use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: String,
    pub world_id: Option<String>,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub chunk_strategy: String,
    pub chunk_max_chars: i64,
    pub chunk_overlap_chars: i64,
    pub chunk_delimiter: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub knowledge_base_id: String,
    pub title: String,
    pub content: String,
    pub source_filename: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledKnowledgeBase {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_chunk_strategy")]
    pub chunk_strategy: String,
    #[serde(default = "default_chunk_max_chars")]
    pub chunk_max_chars: i64,
    #[serde(default = "default_chunk_overlap_chars")]
    pub chunk_overlap_chars: i64,
    #[serde(default)]
    pub chunk_delimiter: String,
    #[serde(default)]
    pub documents: Vec<BundledDocument>,
}
impl BundledKnowledgeBase {
    fn chunk_config(&self) -> ChunkConfig {
        ChunkConfig {
            strategy: self.chunk_strategy.clone(),
            max_chars: self.chunk_max_chars,
            overlap_chars: self.chunk_overlap_chars,
            delimiter: self.chunk_delimiter.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledDocument {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub source_filename: String,
}

pub fn tool_capability(world: &crate::models::world::WorldDefinition) -> Option<serde_json::Value> {
    (world.director_config["knowledge_access"] == true).then(|| serde_json::json!({
        "tool_name": "search_knowledge", "description": "Search this world's authorized knowledge documents. Results include source titles and text excerpts.",
        "arguments_schema": {"type":"object", "required":["query"], "properties":{"query":{"type":"string"}}}
    }))
}

const KNOWLEDGE_RESULT_LIMIT: usize = 6;
const KNOWLEDGE_SEMANTIC_WEIGHT: f64 = 0.6;
/// 完全没有关键词命中时，语义相似度至少要达到这个值才允许入选，
/// 避免把与问题无关的切片也塞进结果。
const KNOWLEDGE_SEMANTIC_FLOOR: f64 = 0.35;
/// 单次检索最多为多少个未缓存切片补算向量。知识库正文可能有成千上万个切片，
/// 一回合内全量嵌入会明显拖慢回合，因此按“词法相关度优先”分批预热。
const KNOWLEDGE_EMBED_BUDGET: usize = 64;
const KNOWLEDGE_EMBED_BATCH: usize = 16;

#[derive(Debug, Clone)]
pub struct KnowledgeChunkCandidate {
    document_id: String,
    title: String,
    knowledge_base: String,
    chunk_index: usize,
    content: String,
    lexical: f64,
    matched_terms: Vec<String>,
    vector: Option<Vec<f32>>,
}

/// 检索计划：阶段 1 持锁准备，阶段 2 锁外嵌入，阶段 3 持锁写回并排序。
/// 与角色记忆召回的分阶段设计保持一致，避免本地模型推理期间一直占着 DB 锁。
#[derive(Debug, Clone)]
pub struct KnowledgeSearchPlan {
    query: String,
    terms: Vec<String>,
    model: Option<crate::models::model_config::ModelConfig>,
    model_key: String,
    query_vector: Vec<f32>,
    candidates: Vec<KnowledgeChunkCandidate>,
    pending: Vec<(usize, String)>,
    computed: Vec<(usize, Vec<f32>)>,
}

/// 纯词法检索（无向量模型时的回退路径，测试也走这里）。作用域是「主控范围」。
pub fn search(
    conn: &rusqlite::Connection,
    world_id: &str,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let plan = prepare_search(conn, world_id, None, arguments)?;
    Ok(finalize_search(conn, plan))
}

/// 世界主控可查询的知识库范围：该世界勾选的库（含自带库与公共库）。
///
/// 世界自带的知识库永远不能被别的世界看到（`world_id` 必须等于当前世界）；
/// 世界自带的库同样要在世界主控编辑页勾选——一个世界可以带好几个库分给不同角色，
/// 世界层的勾选是这些库的「可用上限」，角色再在这个范围内各自收窄。
pub fn world_knowledge_scope(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<Vec<String>, String> {
    let selected = match read_world_selected_knowledge_ids(conn, world_id)? {
        Some(selected) => selected,
        // 老存档没有选择字段：世界自带的库全部可用，公共库沿用旧绑定表。
        None => {
            let legacy = read_legacy_world_bindings(conn, world_id)?;
            let mut ids = list_world_owned_base_ids(conn, world_id)?;
            for id in legacy {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            return Ok(ids);
        }
    };
    // 只保留真实属于「本世界自带」或「公共」的 id，防止脏数据越界。
    let allowed = allowed_knowledge_ids(conn, world_id)?;
    let mut ids = selected
        .into_iter()
        .filter(|id| allowed.contains(id))
        .collect::<Vec<_>>();
    // 输出按 id 排序，避免依赖勾选顺序（对检索无影响，但便于测试与调试）。
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 该世界的硬边界：本世界自带的 enabled 库 + 全部公共 enabled 库。
fn allowed_knowledge_ids(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id, world_id FROM knowledge_bases WHERE enabled = 1 AND (world_id = ?1 OR world_id IS NULL)")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|(_, owner)| matches!(owner, None) || owner.as_deref() == Some(world_id))
        .map(|(id, _)| id)
        .collect())
}

fn list_world_owned_base_ids(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM knowledge_bases WHERE world_id = ?1 AND enabled = 1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 角色可查询的知识库范围。
///
/// 先取世界范围（世界自带 + 该世界勾选的知识库）——这是硬边界，角色不能越出；
/// 再看角色自己勾选的集合：非空则取交集，为空表示沿用世界主控范围。
/// 一个世界可以带多个知识库分给不同角色，所以这里必须是交集而不是并集。
pub fn character_knowledge_scope(
    conn: &rusqlite::Connection,
    world_id: &str,
    character_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let world_scope = world_knowledge_scope(conn, world_id)?;
    let Some(character_id) = character_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(world_scope);
    };
    let raw: Option<String> = conn
        .query_row(
            "SELECT knowledge_base_ids_json FROM characters WHERE id = ?1 AND world_id = ?2",
            params![character_id, world_id],
            |row| row.get(0),
        )
        .ok();
    let selected = raw
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default();
    if selected.is_empty() {
        return Ok(world_scope);
    }
    Ok(world_scope
        .into_iter()
        .filter(|id| selected.iter().any(|value| value == id))
        .collect())
}

/// 读取世界主控编辑页勾选的知识库 id。
///
/// 新数据存在 `director_config.knowledge_base_ids`（世界自带库与公共库都在里面）。
/// 返回 `None` 表示老存档还没有这个字段，调用方需要走兼容分支。
fn read_world_selected_knowledge_ids(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<Option<Vec<String>>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT director_config_json FROM worlds WHERE id = ?1",
            params![world_id],
            |row| row.get(0),
        )
        .ok();
    let parsed = raw
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    let Some(ids) = parsed
        .as_ref()
        .and_then(|value| value.get("knowledge_base_ids"))
        .and_then(|value| value.as_array())
    else {
        return Ok(None);
    };
    Ok(Some(
        ids.iter()
            .filter_map(|value| value.as_str().map(str::trim).map(str::to_string))
            .filter(|value| !value.is_empty())
            .collect(),
    ))
}

/// 旧版绑定表里的公共库 id（仅老存档兼容用）。
fn read_legacy_world_bindings(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT knowledge_base_id FROM world_knowledge_bindings WHERE world_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn resolve_search_base_ids(
    conn: &rusqlite::Connection,
    world_id: &str,
    character_id: Option<&str>,
) -> Result<Vec<String>, String> {
    character_knowledge_scope(conn, world_id, character_id)
}

/// 阶段 1（同步，持锁）：解析查询、切分文档、读已缓存的切片向量、算词法分、
/// 收集待嵌入输入。不做任何嵌入推理。
///
/// `character_id` 非空时按该角色自己的知识库范围收窄（角色编辑页勾选的集合）；
/// 为空表示主控范围（世界自带 + 世界已启用的公共库）。
pub fn prepare_search(
    conn: &rusqlite::Connection,
    world_id: &str,
    character_id: Option<&str>,
    arguments: &serde_json::Value,
) -> Result<KnowledgeSearchPlan, String> {
    let query = arguments["query"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 2048)
        .ok_or("查询不能为空且最多 2048 字节")?
        .to_string();
    let terms = crate::services::game_engine::memory::build_memory_search_terms(&query);

    let rows = {
        let base_ids = resolve_search_base_ids(conn, world_id, character_id)?;
        if base_ids.is_empty() {
            return Ok(KnowledgeSearchPlan {
                query,
                terms,
                model: None,
                model_key: String::new(),
                query_vector: Vec::new(),
                candidates: Vec::new(),
                pending: Vec::new(),
                computed: Vec::new(),
            });
        }
        let placeholders = (0..base_ids.len())
            .map(|index| format!("?{}", index + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT d.id, d.title, d.content, b.name, b.chunk_strategy, b.chunk_max_chars, b.chunk_overlap_chars, b.chunk_delimiter FROM knowledge_documents d JOIN knowledge_bases b ON b.id = d.knowledge_base_id WHERE b.enabled = 1 AND d.knowledge_base_id IN ({placeholders}) ORDER BY d.id"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(base_ids.len());
        for id in &base_ids {
            values.push(id);
        }
        let mapped = stmt
            .query_map(values.as_slice(), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    ChunkConfig {
                        strategy: r.get::<_, String>(4)?,
                        max_chars: r.get::<_, i64>(5)?,
                        overlap_chars: r.get::<_, i64>(6)?,
                        delimiter: r.get::<_, String>(7)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    let mut plan = KnowledgeSearchPlan {
        query,
        terms,
        model: None,
        model_key: String::new(),
        query_vector: Vec::new(),
        candidates: Vec::new(),
        pending: Vec::new(),
        computed: Vec::new(),
    };
    if rows.is_empty() {
        return Ok(plan);
    }

    let model = crate::services::game_engine::memory::resolve_embedding_model_config(conn);
    let cached = match model.as_ref() {
        Some(model) => {
            plan.model_key = model.id.trim().to_string();
            let document_ids = rows.iter().map(|row| row.0.clone()).collect::<Vec<_>>();
            crate::db::repositories::knowledge_embedding_repo::KnowledgeEmbeddingRepository::new(
                conn,
            )
            .list_by_documents(&plan.model_key, &document_ids)
            .unwrap_or_default()
        }
        None => std::collections::HashMap::new(),
    };
    plan.model = model;

    for (id, title, content, base, config) in rows {
        for (chunk_index, excerpt) in split_knowledge_text(&content, &config)
            .into_iter()
            .enumerate()
        {
            let text = crate::services::game_engine::memory::normalize_memory_text(&format!(
                "{title} {excerpt}"
            ));
            let matched_terms = plan
                .terms
                .iter()
                .filter(|term| text.contains(term.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let lexical = matched_terms.len() as f64;
            let vector = cached
                .get(&(id.clone(), chunk_index))
                .filter(|vector| !vector.is_empty())
                .cloned();
            plan.candidates.push(KnowledgeChunkCandidate {
                document_id: id.clone(),
                title: title.clone(),
                knowledge_base: base.clone(),
                chunk_index,
                content: excerpt,
                lexical,
                matched_terms,
                vector,
            });
        }
    }

    if plan.model.is_some() {
        let mut missing = plan
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.vector.is_none())
            .map(|(index, candidate)| (index, candidate.lexical))
            .collect::<Vec<_>>();
        // 词法分高的切片优先补算，剩下的在后续回合继续按同一顺序预热。
        missing.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        plan.pending = missing
            .into_iter()
            .take(KNOWLEDGE_EMBED_BUDGET)
            .map(|(index, _)| {
                let candidate = &plan.candidates[index];
                (
                    index,
                    crate::services::game_engine::memory::normalize_memory_text(&format!(
                        "{} {}",
                        candidate.title, candidate.content
                    )),
                )
            })
            .collect();
    }

    Ok(plan)
}

/// 阶段 2（锁外）：嵌入查询和待补算的切片。必须在不持有 DB 锁时调用。
pub fn embed_search_plan(
    memory: &crate::services::game_engine::memory::MemoryService,
    plan: &mut KnowledgeSearchPlan,
) {
    let Some(model) = plan.model.clone() else {
        return;
    };
    match memory.embed_with_resolved_model(&model, std::slice::from_ref(&plan.query)) {
        Ok(vectors) => plan.query_vector = vectors.into_iter().next().unwrap_or_default(),
        Err(_) => {
            // 嵌入失败（模型未下载 / 构建未启用）时退回纯词法，不影响工具调用。
            plan.model = None;
            return;
        }
    }
    if plan.query_vector.is_empty() {
        plan.model = None;
        return;
    }
    if plan.pending.is_empty() {
        return;
    }

    let pending = std::mem::take(&mut plan.pending);
    let mut offset = 0;
    while offset < pending.len() {
        let end = (offset + KNOWLEDGE_EMBED_BATCH).min(pending.len());
        let inputs = pending[offset..end]
            .iter()
            .map(|(_, input)| input.clone())
            .collect::<Vec<_>>();
        match memory.embed_with_resolved_model(&model, &inputs) {
            Ok(vectors) => {
                for (position, vector) in vectors.into_iter().enumerate() {
                    if vector.is_empty() {
                        continue;
                    }
                    if let Some((index, _)) = pending.get(offset + position) {
                        plan.computed.push((*index, vector));
                    }
                }
            }
            // 某一批失败就停止继续请求；已缓存的向量仍可参与排序。
            Err(_) => break,
        }
        offset = end;
    }
}

/// 阶段 3（同步，持锁）：写回新算出的切片向量并完成混合排序。
pub fn finalize_search(
    conn: &rusqlite::Connection,
    mut plan: KnowledgeSearchPlan,
) -> serde_json::Value {
    if !plan.computed.is_empty() && !plan.model_key.is_empty() {
        let repo =
            crate::db::repositories::knowledge_embedding_repo::KnowledgeEmbeddingRepository::new(
                conn,
            );
        for (index, vector) in std::mem::take(&mut plan.computed) {
            let Some(candidate) = plan.candidates.get_mut(index) else {
                continue;
            };
            let _ = repo.upsert(
                &candidate.document_id,
                candidate.chunk_index,
                &plan.model_key,
                &vector,
            );
            candidate.vector = Some(vector);
        }
    }

    let query_vector = std::mem::take(&mut plan.query_vector);
    let semantic_ready = !query_vector.is_empty()
        && plan
            .candidates
            .iter()
            .any(|candidate| candidate.vector.is_some());
    let max_lexical = plan
        .candidates
        .iter()
        .map(|candidate| candidate.lexical)
        .fold(0.0_f64, f64::max);

    let mut scored = plan
        .candidates
        .into_iter()
        .filter_map(|candidate| {
            let lexical_score = if max_lexical > 0.0 {
                (candidate.lexical / max_lexical).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let semantic_score = if semantic_ready {
                candidate
                    .vector
                    .as_deref()
                    .map(|vector| cosine_similarity(&query_vector, vector).clamp(0.0, 1.0))
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            // 没有向量可用时必须命中关键词，避免退回“随机切片”；
            // 有向量时也丢到语义相关度太低的切片（正交/无关内容）。
            if candidate.lexical <= 0.0 {
                if !semantic_ready || semantic_score < KNOWLEDGE_SEMANTIC_FLOOR {
                    return None;
                }
            }
            let lexical_weight = if semantic_ready {
                1.0 - KNOWLEDGE_SEMANTIC_WEIGHT
            } else {
                1.0
            };
            let score = lexical_score * lexical_weight + semantic_score * KNOWLEDGE_SEMANTIC_WEIGHT;
            Some((score, lexical_score, candidate))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.2.chunk_index.cmp(&right.2.chunk_index))
    });

    let results = scored
        .into_iter()
        .take(KNOWLEDGE_RESULT_LIMIT)
        .map(|(score, _, candidate)| {
            serde_json::json!({
                "document_id": candidate.document_id,
                "title": candidate.title,
                "knowledge_base": candidate.knowledge_base,
                "chunk": candidate.chunk_index,
                "content": candidate.content,
                "matched_terms": candidate.matched_terms,
                "score": (score * 1000.0).round() / 1000.0,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "query": plan.query,
        "semantic": semantic_ready,
        "results": results,
    })
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += (*a as f64) * (*b as f64);
        left_norm += (*a as f64) * (*a as f64);
        right_norm += (*b as f64) * (*b as f64);
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

const CHUNK_STRATEGY_LENGTH: &str = "length";
const CHUNK_STRATEGY_DELIMITER: &str = "delimiter";
const CHUNK_MAX_CHARS_MIN: i64 = 100;
const CHUNK_MAX_CHARS_MAX: i64 = 8000;
const CHUNK_OVERLAP_MAX: i64 = 2000;
const CHUNK_SEPARATORS: [&str; 10] = ["\n\n", "\n", "。", "！", "？", "；", ". ", "! ", "? ", " "];

fn default_chunk_strategy() -> String {
    CHUNK_STRATEGY_LENGTH.to_string()
}
fn default_chunk_max_chars() -> i64 {
    800
}
fn default_chunk_overlap_chars() -> i64 {
    120
}

#[derive(Debug, Clone)]
pub struct ChunkConfig {
    pub strategy: String,
    pub max_chars: i64,
    pub overlap_chars: i64,
    pub delimiter: String,
}

impl ChunkConfig {
    fn validate(&self) -> Result<(), String> {
        if !(CHUNK_MAX_CHARS_MIN..=CHUNK_MAX_CHARS_MAX).contains(&self.max_chars) {
            return Err(format!(
                "单块字数上限需在 {CHUNK_MAX_CHARS_MIN} 到 {CHUNK_MAX_CHARS_MAX} 之间"
            ));
        }
        if self.overlap_chars < 0
            || self.overlap_chars > CHUNK_OVERLAP_MAX
            || self.overlap_chars >= self.max_chars
        {
            return Err("重叠字数需大于等于 0、小于单块上限，且不超过 2000".into());
        }
        match self.strategy.as_str() {
            CHUNK_STRATEGY_LENGTH => {}
            CHUNK_STRATEGY_DELIMITER => {
                if self.delimiter.is_empty() || self.delimiter.chars().count() > 16 {
                    return Err("分隔符不能为空，且最多 16 个字符".into());
                }
            }
            _ => return Err("分块方式只能是 length 或 delimiter".into()),
        }
        Ok(())
    }
}

fn split_knowledge_text(text: &str, config: &ChunkConfig) -> Vec<String> {
    let text = text.trim();
    let max = config.max_chars.max(1) as usize;
    let overlap = config.overlap_chars.max(0) as usize;
    if config.strategy == CHUNK_STRATEGY_DELIMITER {
        return text
            .split(config.delimiter.as_str())
            .flat_map(|piece| split_recursive(piece, &CHUNK_SEPARATORS, max, overlap))
            .collect();
    }
    split_recursive(text, &CHUNK_SEPARATORS, max, overlap)
}

fn split_recursive(text: &str, separators: &[&str], max: usize, overlap: usize) -> Vec<String> {
    if text.chars().count() <= max {
        let trimmed = text.trim();
        return if trimmed.is_empty() {
            Vec::new()
        } else {
            vec![trimmed.to_string()]
        };
    }
    let Some((index, separator)) = separators
        .iter()
        .enumerate()
        .find(|(_, s)| text.contains(**s))
    else {
        return hard_split(text, max, overlap);
    };
    let rest = &separators[index + 1..];
    let mut chunks = Vec::new();
    let mut parts: VecDeque<&str> = VecDeque::new();
    let mut len = 0;
    for piece in text.split_inclusive(*separator) {
        let piece_len = piece.chars().count();
        if piece_len > max {
            emit_parts(&mut chunks, &parts);
            parts.clear();
            len = 0;
            chunks.extend(split_recursive(piece, rest, max, overlap));
            continue;
        }
        if len + piece_len > max {
            emit_parts(&mut chunks, &parts);
            while !parts.is_empty() && (len > overlap || len + piece_len > max) {
                let removed = parts.pop_front().unwrap_or_default();
                len -= removed.chars().count();
            }
        }
        parts.push_back(piece);
        len += piece_len;
    }
    emit_parts(&mut chunks, &parts);
    chunks
}

fn emit_parts(chunks: &mut Vec<String>, parts: &VecDeque<&str>) {
    let text: String = parts.iter().copied().collect();
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }
}

fn hard_split(text: &str, max: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let step = max.saturating_sub(overlap).max(1);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + max).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start += step;
    }
    chunks
}

fn enable_world_knowledge(conn: &rusqlite::Connection, world_id: &str) -> Result<(), String> {
    conn.execute("UPDATE worlds SET director_config_json = json_set(director_config_json, '$.knowledge_access', json('true')) WHERE id = ?1", params![world_id]).map_err(|e| e.to_string()).map(|_| ())
}

/// 世界主控编辑页可勾选的知识库列表：该世界自带的库 + 全部公共库。
///
/// 一个世界可以自带多个知识库分给不同角色，所以自带的库也要能勾选；
/// 但范围只限当前世界，别的世界的专属库不会出现在这里。
/// `world_id` 为空（新建世界还没保存）时只列公共库。
#[tauri::command]
pub async fn list_world_knowledge_candidates(
    state: State<'_, AppState>,
    world_id: Option<String>,
) -> Result<Vec<KnowledgeBase>, String> {
    let db = state.db.lock().await;
    let world_id = world_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let collect =
        |sql: &str, values: &[&dyn rusqlite::types::ToSql]| -> Result<Vec<KnowledgeBase>, String> {
            let mut stmt = db.conn().prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(values, row_base)
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())
        };

    match world_id.as_deref() {
        Some(id) => collect(
            &format!(
                "SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE enabled = 1 AND (world_id = ?1 OR world_id IS NULL) ORDER BY world_id IS NOT NULL DESC, name"
            ),
            &[&id as &dyn rusqlite::types::ToSql],
        ),
        None => collect(
            &format!(
                "SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE enabled = 1 AND world_id IS NULL ORDER BY name"
            ),
            &[],
        ),
    }
}

/// 某世界当前可查询的知识库全集（世界自带 + 该世界已勾选的公共库）。
///
/// 角色编辑页用它渲染「可收窄的范围」；角色勾选的只能是这个集合的子集。
#[tauri::command]
pub async fn list_world_knowledge_scope(
    state: State<'_, AppState>,
    world_id: String,
) -> Result<Vec<KnowledgeBase>, String> {
    let db = state.db.lock().await;
    let ids = world_knowledge_scope(db.conn(), &world_id)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE id IN ({}) ORDER BY name",
        ids.iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", index + 1))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stmt = db.conn().prepare(&sql).map_err(|e| e.to_string())?;
    let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(ids.len());
    for id in &ids {
        values.push(id);
    }
    let rows = stmt
        .query_map(values.as_slice(), row_base)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn export_bundled(
    conn: &rusqlite::Connection,
    world_id: &str,
) -> Result<Vec<BundledKnowledgeBase>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, chunk_strategy, chunk_max_chars, chunk_overlap_chars, chunk_delimiter FROM knowledge_bases WHERE world_id = ?1 ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            name,
            description,
            chunk_strategy,
            chunk_max_chars,
            chunk_overlap_chars,
            chunk_delimiter,
        ) = row.map_err(|e| e.to_string())?;
        let mut docs = conn.prepare("SELECT title, content, source_filename FROM knowledge_documents WHERE knowledge_base_id = ?1 ORDER BY id").map_err(|e| e.to_string())?;
        let documents = docs
            .query_map(params![id], |r| {
                Ok(BundledDocument {
                    title: r.get(0)?,
                    content: r.get(1)?,
                    source_filename: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        result.push(BundledKnowledgeBase {
            name,
            description,
            chunk_strategy,
            chunk_max_chars,
            chunk_overlap_chars,
            chunk_delimiter,
            documents,
        });
    }
    Ok(result)
}

pub fn import_bundled(
    conn: &rusqlite::Connection,
    world_id: &str,
    bases: &[BundledKnowledgeBase],
) -> Result<(), String> {
    // World-package upgrades replace only that world's bundled resources.
    conn.execute(
        "DELETE FROM knowledge_bases WHERE world_id = ?1",
        params![world_id],
    )
    .map_err(|e| e.to_string())?;
    for base in bases {
        if base.name.trim().is_empty() {
            return Err("知识库名称不能为空".into());
        }
        let config = base.chunk_config();
        config.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO knowledge_bases (id, world_id, name, description, chunk_strategy, chunk_max_chars, chunk_overlap_chars, chunk_delimiter) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, world_id, base.name, base.description, config.strategy, config.max_chars, config.overlap_chars, config.delimiter],
        )
        .map_err(|e| e.to_string())?;
        for doc in &base.documents {
            if doc.content.len() > 20 * 1024 * 1024 {
                return Err("知识库正文最大 20MB".into());
            }
            conn.execute("INSERT INTO knowledge_documents (id, knowledge_base_id, title, content, source_filename, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid::Uuid::new_v4().to_string(), id, doc.title, doc.content, doc.source_filename, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

const BASE_COLUMNS: &str = "id, world_id, name, description, enabled, chunk_strategy, chunk_max_chars, chunk_overlap_chars, chunk_delimiter";

fn row_base(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeBase> {
    Ok(KnowledgeBase {
        id: row.get(0)?,
        world_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        chunk_strategy: row.get(5)?,
        chunk_max_chars: row.get(6)?,
        chunk_overlap_chars: row.get(7)?,
        chunk_delimiter: row.get(8)?,
    })
}

#[tauri::command]
pub async fn list_knowledge_bases(
    state: State<'_, AppState>,
    world_id: Option<String>,
) -> Result<Vec<KnowledgeBase>, String> {
    let db = state.db.lock().await;
    let sql = format!("SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE (?1 IS NULL OR world_id = ?1 OR world_id IS NULL) ORDER BY world_id IS NOT NULL, name");
    let mut stmt = db.conn().prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], row_base)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_knowledge_base(
    state: State<'_, AppState>,
    name: String,
    description: String,
    world_id: Option<String>,
) -> Result<KnowledgeBase, String> {
    let id = uuid::Uuid::new_v4().to_string();
    if name.trim().is_empty() {
        return Err("知识库名称不能为空".into());
    }
    let db = state.db.lock().await;
    let default = ChunkConfig {
        strategy: default_chunk_strategy(),
        max_chars: default_chunk_max_chars(),
        overlap_chars: default_chunk_overlap_chars(),
        delimiter: String::new(),
    };
    db.conn()
        .execute(
            "INSERT INTO knowledge_bases (id, world_id, name, description, chunk_strategy, chunk_max_chars, chunk_overlap_chars, chunk_delimiter) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, world_id, name.trim(), description, default.strategy, default.max_chars, default.overlap_chars, default.delimiter],
        )
        .map_err(|e| e.to_string())?;
    if let Some(owner) = &world_id {
        enable_world_knowledge(db.conn(), owner)?;
    }
    db.conn()
        .query_row(
            &format!("SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE id = ?1"),
            params![id],
            row_base,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_knowledge_chunking(
    state: State<'_, AppState>,
    id: String,
    strategy: String,
    max_chars: i64,
    overlap_chars: i64,
    delimiter: String,
) -> Result<KnowledgeBase, String> {
    let config = ChunkConfig {
        strategy,
        max_chars,
        overlap_chars,
        delimiter,
    };
    config.validate()?;
    let db = state.db.lock().await;
    let changed = db
        .conn()
        .execute(
            "UPDATE knowledge_bases SET chunk_strategy = ?2, chunk_max_chars = ?3, chunk_overlap_chars = ?4, chunk_delimiter = ?5 WHERE id = ?1",
            params![id, config.strategy, config.max_chars, config.overlap_chars, config.delimiter],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("知识库不存在".into());
    }
    // 分块方式变了，切片索引整体位移，旧向量必须作废。
    let _ = crate::db::repositories::knowledge_embedding_repo::KnowledgeEmbeddingRepository::new(
        db.conn(),
    )
    .clear_base(&id);
    db.conn()
        .query_row(
            &format!("SELECT {BASE_COLUMNS} FROM knowledge_bases WHERE id = ?1"),
            params![id],
            row_base,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_knowledge_base(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().await;
    db.conn()
        .execute("DELETE FROM knowledge_bases WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())
        .map(|_| ())
}

#[tauri::command]
pub async fn list_knowledge_documents(
    state: State<'_, AppState>,
    knowledge_base_id: String,
) -> Result<Vec<KnowledgeDocument>, String> {
    let db = state.db.lock().await;
    let mut stmt = db.conn().prepare("SELECT id, knowledge_base_id, title, content, source_filename, updated_at FROM knowledge_documents WHERE knowledge_base_id = ?1 ORDER BY title").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![knowledge_base_id], |row| {
            Ok(KnowledgeDocument {
                id: row.get(0)?,
                knowledge_base_id: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                source_filename: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn extract_text(filename: &str, data: &[u8]) -> Result<String, String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" | "jsonl" | "yaml" | "yml" | "xml" => {
            String::from_utf8(data.to_vec()).map_err(|_| "文件不是有效的 UTF-8 文本".to_string())
        }
        "docx" => {
            let cursor = std::io::Cursor::new(data);
            let mut archive =
                zip::ZipArchive::new(cursor).map_err(|e| format!("无法读取 DOCX：{e}"))?;
            let file = archive
                .by_name("word/document.xml")
                .map_err(|_| "DOCX 中没有正文".to_string())?;
            let mut xml = String::new();
            use std::io::Read;
            file.take(20 * 1024 * 1024 + 1)
                .read_to_string(&mut xml)
                .map_err(|e| e.to_string())?;
            if xml.len() > 20 * 1024 * 1024 {
                return Err("DOCX 解压正文过大".into());
            }
            let mut reader = quick_xml::Reader::from_str(&xml);
            let mut text = String::new();
            let mut in_text = false;
            loop {
                match reader.read_event().map_err(|e| e.to_string())? {
                    quick_xml::events::Event::Start(e) if e.local_name().as_ref() == b"t" => {
                        in_text = true
                    }
                    quick_xml::events::Event::Text(e) if in_text => {
                        text.push_str(&e.unescape().map_err(|e| e.to_string())?)
                    }
                    quick_xml::events::Event::End(e) => {
                        if e.local_name().as_ref() == b"t" {
                            in_text = false;
                        }
                        if e.local_name().as_ref() == b"p" {
                            text.push('\n');
                        }
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => (),
                }
            }
            Ok(text)
        }
        "pdf" => {
            pdf_extract::extract_text_from_mem(data).map_err(|e| format!("PDF 文本提取失败：{e}"))
        }
        _ => Err("支持 TXT、Markdown、CSV、JSON、YAML、XML、DOCX 和 PDF 文件".to_string()),
    }
}

#[tauri::command]
pub async fn update_knowledge_document(
    state: State<'_, AppState>,
    id: String,
    title: String,
    content: String,
) -> Result<(), String> {
    if title.trim().is_empty() || content.trim().is_empty() || content.len() > 20 * 1024 * 1024 {
        return Err("标题和正文不能为空，正文最大 20MB".into());
    }
    let db = state.db.lock().await;
    db.conn().execute("UPDATE knowledge_documents SET title = ?2, content = ?3, updated_at = ?4 WHERE id = ?1", params![id, title, content, chrono::Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
    // 正文或标题变了，旧切片向量随之失效；下次检索重新补算。
    let _ = crate::db::repositories::knowledge_embedding_repo::KnowledgeEmbeddingRepository::new(
        db.conn(),
    )
    .clear_document(&id);
    Ok(())
}

#[tauri::command]
pub async fn add_knowledge_document(
    state: State<'_, AppState>,
    knowledge_base_id: String,
    filename: String,
    data: Vec<u8>,
) -> Result<KnowledgeDocument, String> {
    if data.len() > 20 * 1024 * 1024 {
        return Err("知识库文件最大 20MB".to_string());
    }
    let extract_filename = filename.clone();
    let content =
        tauri::async_runtime::spawn_blocking(move || extract_text(&extract_filename, &data))
            .await
            .map_err(|e| e.to_string())??;
    if content.len() > 20 * 1024 * 1024 {
        return Err("提取正文最大 20MB".into());
    }
    if content.trim().is_empty() {
        return Err("文件没有可用文字".to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&filename)
        .to_string();
    let db = state.db.lock().await;
    db.conn().execute("INSERT INTO knowledge_documents (id, knowledge_base_id, title, content, source_filename, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![id, knowledge_base_id, title, content, filename, now]).map_err(|e| e.to_string())?;
    Ok(KnowledgeDocument {
        id,
        knowledge_base_id,
        title,
        content,
        source_filename: filename,
        updated_at: now,
    })
}

#[tauri::command]
pub async fn delete_knowledge_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    db.conn()
        .execute("DELETE FROM knowledge_documents WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE worlds(id TEXT PRIMARY KEY, director_config_json TEXT DEFAULT '{}'); CREATE TABLE knowledge_bases(id TEXT PRIMARY KEY, world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE, name TEXT, description TEXT, enabled INTEGER DEFAULT 1, chunk_strategy TEXT NOT NULL DEFAULT 'length', chunk_max_chars INTEGER NOT NULL DEFAULT 800, chunk_overlap_chars INTEGER NOT NULL DEFAULT 120, chunk_delimiter TEXT NOT NULL DEFAULT ''); CREATE TABLE knowledge_documents(id TEXT PRIMARY KEY, knowledge_base_id TEXT REFERENCES knowledge_bases(id) ON DELETE CASCADE, title TEXT, content TEXT, source_filename TEXT, updated_at TEXT); CREATE TABLE knowledge_chunk_embeddings(document_id TEXT NOT NULL, chunk_index INTEGER NOT NULL, model_key TEXT NOT NULL, vector_json TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(document_id, chunk_index, model_key)); CREATE TABLE world_knowledge_bindings(world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE, knowledge_base_id TEXT REFERENCES knowledge_bases(id) ON DELETE CASCADE, PRIMARY KEY(world_id,knowledge_base_id)); INSERT INTO worlds(id) VALUES('one'),('two');").unwrap();
        conn
    }
    #[test]
    fn bundled_roundtrip_upgrade_and_cascade() {
        let conn = database();
        let payload = vec![BundledKnowledgeBase {
            name: "规则".into(),
            description: "规则说明".into(),
            chunk_strategy: default_chunk_strategy(),
            chunk_max_chars: default_chunk_max_chars(),
            chunk_overlap_chars: default_chunk_overlap_chars(),
            chunk_delimiter: String::new(),
            documents: vec![BundledDocument {
                title: "毒酒".into(),
                content: "毒酒扣血".into(),
                source_filename: "rules.md".into(),
            }],
        }];
        import_bundled(&conn, "one", &payload).unwrap();
        assert_eq!(
            export_bundled(&conn, "one").unwrap()[0].documents[0].content,
            "毒酒扣血"
        );
        import_bundled(&conn, "one", &payload).unwrap();
        assert_eq!(export_bundled(&conn, "one").unwrap().len(), 1);
        assert!(
            search(&conn, "two", &serde_json::json!({"query":"毒酒"})).unwrap()["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            search(&conn, "one", &serde_json::json!({"query":"毒酒"})).unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        conn.execute("DELETE FROM worlds WHERE id='one'", [])
            .unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM knowledge_documents", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn public_access_requires_world_selection() {
        let conn = database();
        conn.execute_batch("INSERT INTO knowledge_bases (id, world_id, name, description, enabled) VALUES('public',NULL,'公共','',1); INSERT INTO knowledge_documents VALUES('doc','public','毒酒','毒酒扣血','rules.md','now');").unwrap();
        let query = serde_json::json!({"query":"毒酒"});
        assert!(search(&conn, "one", &query).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty());
        conn.execute_batch(
            "UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"public\"]}' WHERE id='one';",
        )
        .unwrap();
        assert_eq!(
            search(&conn, "one", &query).unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(search(&conn, "two", &query).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn legacy_bindings_still_grant_access() {
        let conn = database();
        conn.execute_batch("INSERT INTO knowledge_bases (id, world_id, name, description, enabled) VALUES('public',NULL,'公共','',1); INSERT INTO knowledge_documents VALUES('doc','public','毒酒','毒酒扣血','rules.md','now'); INSERT INTO world_knowledge_bindings VALUES('one','public');").unwrap();
        assert_eq!(
            search(&conn, "one", &serde_json::json!({"query":"毒酒"})).unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn world_owned_base_requires_world_selection() {
        let conn = database();
        conn.execute_batch("INSERT INTO knowledge_bases (id, world_id, name, description, enabled) VALUES('owned','one','专属','',1); INSERT INTO knowledge_documents VALUES('doc','owned','毒酒','毒酒扣血','rules.md','now');").unwrap();
        let query = serde_json::json!({"query":"毒酒"});

        // 世界已有选择字段但没勾这个自带库：不参与检索。
        conn.execute_batch("UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[]}' WHERE id='one';")
            .unwrap();
        assert!(search(&conn, "one", &query).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty());

        // 勾选后即可查询。
        conn.execute_batch("UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"owned\"]}' WHERE id='one';")
            .unwrap();
        assert_eq!(
            search(&conn, "one", &query).unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn world_owned_base_never_leaks_to_other_worlds() {
        let conn = database();
        conn.execute_batch("INSERT INTO knowledge_bases (id, world_id, name, description, enabled) VALUES('owned','one','专属','',1); INSERT INTO knowledge_documents VALUES('doc','owned','毒酒','毒酒扣血','rules.md','now');").unwrap();
        let query = serde_json::json!({"query":"毒酒"});
        conn.execute_batch(
            "UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"owned\"]}' WHERE id='one';",
        )
        .unwrap();
        assert_eq!(
            search(&conn, "one", &query).unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // 即使把专属库 id 硬塞进另一个世界的选择里，也不能生效。
        conn.execute_batch(
            "UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"owned\"]}' WHERE id='two';",
        )
        .unwrap();
        assert!(search(&conn, "two", &query).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(character_knowledge_scope(&conn, "two", None)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn character_scope_is_intersection_with_world_scope() {
        let conn = database();
        conn.execute_batch(
            "CREATE TABLE characters(id TEXT PRIMARY KEY, world_id TEXT, knowledge_base_ids_json TEXT NOT NULL DEFAULT '[]');\
             INSERT INTO knowledge_bases (id, world_id, name, description, enabled) VALUES('owned','one','专属','',1),('public',NULL,'公共','',1),('other',NULL,'未勾选','',1);\
             INSERT INTO characters (id, world_id, knowledge_base_ids_json) VALUES('c1','one','[\"owned\",\"other\"]');",
        )
        .unwrap();
        // 世界只勾了 public；角色勾的 owned / other 都不在世界范围 → 交集为空。
        conn.execute_batch(
            "UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"public\"]}' WHERE id='one';",
        )
        .unwrap();
        let scope = character_knowledge_scope(&conn, "one", Some("c1")).unwrap();
        assert!(scope.is_empty());

        // 世界把自带库也勾上后，角色选的自带库才生效。
        conn.execute_batch(
            "UPDATE worlds SET director_config_json = '{\"knowledge_base_ids\":[\"public\",\"owned\"]}' WHERE id='one';",
        )
        .unwrap();
        let scope = character_knowledge_scope(&conn, "one", Some("c1")).unwrap();
        assert_eq!(scope, vec!["owned".to_string()]);

        // 角色一个都不勾时沿用世界范围。
        conn.execute_batch("UPDATE characters SET knowledge_base_ids_json='[]' WHERE id='c1';")
            .unwrap();
        let scope = character_knowledge_scope(&conn, "one", Some("c1")).unwrap();
        assert_eq!(scope, vec!["owned".to_string(), "public".to_string()]);
    }
    fn length_config(max_chars: i64, overlap_chars: i64) -> ChunkConfig {
        ChunkConfig {
            strategy: CHUNK_STRATEGY_LENGTH.into(),
            max_chars,
            overlap_chars,
            delimiter: String::new(),
        }
    }
    #[test]
    fn splitter_keeps_short_text_whole_and_prefers_paragraphs() {
        let config = length_config(800, 120);
        assert_eq!(
            split_knowledge_text("  短文本  ", &config),
            vec!["短文本".to_string()]
        );
        assert!(split_knowledge_text("  \n\n ", &config).is_empty());
        let para_a = "甲".repeat(500);
        let para_b = "乙".repeat(500);
        let chunks = split_knowledge_text(&format!("{para_a}\n\n{para_b}"), &config);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], para_a);
        assert_eq!(chunks[1], para_b);
    }
    #[test]
    fn splitter_respects_custom_limit_and_overlaps_sentences() {
        let text: String = (0..200).map(|i| format!("第{i}条规则说明内容。")).collect();
        let config = length_config(300, 40);
        let chunks = split_knowledge_text(&text, &config);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= 300));
        assert!(chunks.iter().all(|c| c.starts_with('第')));
        assert!(chunks.windows(2).any(|pair| {
            let first_next = pair[1].split('。').next().unwrap_or_default();
            pair[0].contains(first_next)
        }));
    }
    #[test]
    fn splitter_hard_splits_text_without_separators() {
        let text = "x".repeat(900);
        let chunks = split_knowledge_text(&text, &length_config(300, 50));
        assert!(chunks.len() >= 3);
        assert!(chunks.iter().all(|c| c.chars().count() <= 300));
    }
    #[test]
    fn delimiter_strategy_splits_on_custom_symbol() {
        let config = ChunkConfig {
            strategy: CHUNK_STRATEGY_DELIMITER.into(),
            max_chars: 800,
            overlap_chars: 0,
            delimiter: "###".into(),
        };
        let chunks = split_knowledge_text("第一节\n###\n第二节\n### \n\n### 第三节", &config);
        assert_eq!(chunks, vec!["第一节", "第二节", "第三节"]);
    }
    #[test]
    fn delimiter_strategy_caps_oversized_sections_by_length() {
        let config = ChunkConfig {
            strategy: CHUNK_STRATEGY_DELIMITER.into(),
            max_chars: 300,
            overlap_chars: 0,
            delimiter: "###".into(),
        };
        let long_section = "长段落。".repeat(200);
        let chunks = split_knowledge_text(&format!("短段\n###\n{long_section}"), &config);
        assert_eq!(chunks[0], "短段");
        assert!(chunks.len() > 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 300));
    }
    #[test]
    fn hybrid_ranking_uses_vectors_even_without_lexical_hit() {
        let conn = database();
        let plan = KnowledgeSearchPlan {
            query: "城门守军口令".into(),
            terms: vec!["城门".into()],
            model: None,
            model_key: "model-test".into(),
            query_vector: vec![1.0, 0.0],
            candidates: vec![
                KnowledgeChunkCandidate {
                    document_id: "doc-a".into(),
                    title: "旧城墙".into(),
                    knowledge_base: "公共".into(),
                    chunk_index: 0,
                    content: "城墙的砖石来自西郊采石场，城门由两队轮值。".into(),
                    lexical: 1.0,
                    matched_terms: vec!["城门".into()],
                    vector: Some(vec![1.0, 0.0]),
                },
                KnowledgeChunkCandidate {
                    document_id: "doc-a".into(),
                    title: "旧城墙".into(),
                    knowledge_base: "公共".into(),
                    chunk_index: 1,
                    content: "每晚换防，口令由当值队长口头传达。".into(),
                    lexical: 0.0,
                    matched_terms: vec![],
                    vector: Some(vec![0.99, 0.01]),
                },
                KnowledgeChunkCandidate {
                    document_id: "doc-b".into(),
                    title: "草药".into(),
                    knowledge_base: "公共".into(),
                    chunk_index: 0,
                    content: "止血草生长在北坡。".into(),
                    lexical: 0.0,
                    matched_terms: vec![],
                    vector: Some(vec![0.0, 1.0]),
                },
            ],
            pending: vec![],
            computed: vec![],
        };
        let value = finalize_search(&conn, plan);
        assert_eq!(value["semantic"], serde_json::json!(true));
        let results = value["results"].as_array().unwrap();
        assert!(!results.is_empty());
        // 与查询向量同向的切片即使没有词法命中也能进入结果。
        assert!(results
            .iter()
            .any(|item| item["content"].as_str().unwrap().contains("口令")));
        assert!(results
            .iter()
            .all(|item| !item["content"].as_str().unwrap().contains("止血草")));
    }
    #[test]
    fn keyword_only_fallback_drops_unmatched_chunks() {
        let conn = database();
        let plan = KnowledgeSearchPlan {
            query: "毒酒".into(),
            terms: vec!["毒酒".into()],
            model: None,
            model_key: String::new(),
            query_vector: vec![],
            candidates: vec![
                KnowledgeChunkCandidate {
                    document_id: "doc-a".into(),
                    title: "规则".into(),
                    knowledge_base: "公共".into(),
                    chunk_index: 0,
                    content: "毒酒每次扣十点生命。".into(),
                    lexical: 1.0,
                    matched_terms: vec!["毒酒".into()],
                    vector: None,
                },
                KnowledgeChunkCandidate {
                    document_id: "doc-a".into(),
                    title: "规则".into(),
                    knowledge_base: "公共".into(),
                    chunk_index: 1,
                    content: "火把照亮整条走廊。".into(),
                    lexical: 0.0,
                    matched_terms: vec![],
                    vector: None,
                },
            ],
            pending: vec![],
            computed: vec![],
        };
        let value = finalize_search(&conn, plan);
        assert_eq!(value["semantic"], serde_json::json!(false));
        let results = value["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0]["content"].as_str().unwrap().contains("毒酒"));
    }
    #[test]
    fn chunk_config_rejects_invalid_values() {
        assert!(length_config(50, 0).validate().is_err());
        assert!(length_config(800, 800).validate().is_err());
        assert!(length_config(800, -1).validate().is_err());
        assert!(length_config(800, 120).validate().is_ok());
        let empty_delimiter = ChunkConfig {
            strategy: CHUNK_STRATEGY_DELIMITER.into(),
            max_chars: 800,
            overlap_chars: 0,
            delimiter: String::new(),
        };
        assert!(empty_delimiter.validate().is_err());
    }
    #[test]
    fn docx_extracts_only_text_and_unescapes_xml() {
        use std::io::Write;
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all("<w:document xmlns:w='test'><w:p><w:r><w:rPr/><w:t>规则 &amp; 毒酒</w:t></w:r></w:p></w:document>".as_bytes()).unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert_eq!(extract_text("test.docx", &bytes).unwrap(), "规则 & 毒酒\n");
        assert!(extract_text("bad.txt", &[0xff]).is_err());
        assert!(extract_text("bad.exe", b"text").is_err());
    }
}
