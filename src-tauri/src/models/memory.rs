use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub character_id: String,
    pub layer: String,
    pub content: String,
    pub source: String,
    pub importance: f64,
    pub created_at: String,
    pub turn_index: i32,
    pub conversation_id: Option<String>,
    pub event_id: Option<String>,
    pub item_id: Option<String>,
    pub scene_id: Option<String>,
    pub memory_type: String,
    pub speaker: Option<String>,
    pub role: Option<String>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryParams {
    pub world_id: Option<String>,
    pub session_id: Option<String>,
    pub character_id: Option<String>,
    pub layer: Option<String>,
    pub limit: Option<i32>,
}

/// 事实卡片:实体(人物/物品/地点/概念)。按 session 隔离,
/// name_normalized 是归一化后的匹配键(UNIQUE(session_id, name_normalized)),
/// aliases 记录出现过的其它写法(归一化后的形式)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntity {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub name: String,
    pub name_normalized: String,
    pub entity_type: String,
    pub aliases: Vec<String>,
    pub mention_count: i32,
    pub first_seen_turn: i32,
    pub last_seen_turn: i32,
    pub created_at: String,
}

/// 事实卡片:关系(主语—谓语—宾语)。带回合时效:
/// invalid_at_turn 为 NULL 表示事实当前有效;被新事实推翻时标记失效回合而非删除,
/// 这样"曾经怎样"和"现在怎样"都能回答。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRelation {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub subject_entity_id: String,
    pub predicate: String,
    pub object_entity_id: Option<String>,
    pub object_text: String,
    pub valid_from_turn: i32,
    pub invalid_at_turn: Option<i32>,
    pub source: String,
    pub confidence: f64,
    pub created_at: String,
}

/// LLM 从本回合对话中提取的一条事实更新(fact_extractions 数组的一项)。
/// action: "upsert"(默认)登记/更新事实;"invalidate"作废 subject+predicate 的现有事实。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactUpdate {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub action: String,
    pub subject_type: String,
    pub object_type: String,
    pub confidence: f64,
}
