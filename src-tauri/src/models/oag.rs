use serde::{Deserialize, Serialize};

pub const OAG_SCHEMA_VERSION: i64 = 1;
pub const OAG_RESERVED_TYPE_KEYS: [&str; 4] = ["scene", "event", "memory", "document"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagOntology {
    #[serde(default)]
    pub object_types: Vec<OagObjectTypeDefinition>,
    #[serde(default)]
    pub link_types: Vec<OagLinkTypeDefinition>,
    #[serde(default)]
    pub character_object_type: Option<String>,
    #[serde(default)]
    pub location_object_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagObjectTypeDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub actor: bool,
    #[serde(default)]
    pub platform_role: String,
    #[serde(default)]
    pub attributes: Vec<OagAttributeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagAttributeDefinition {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub enum_options: Vec<String>,
    #[serde(default)]
    pub functional: bool,
    #[serde(default = "default_true")]
    pub temporal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagLinkTypeDefinition {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub domain: Vec<String>,
    #[serde(default)]
    pub range: Vec<String>,
    #[serde(default)]
    pub functional: bool,
    #[serde(default)]
    pub symmetric: bool,
    #[serde(default)]
    pub transitive: bool,
    #[serde(default = "default_true")]
    pub temporal: bool,
    #[serde(default)]
    pub inverse: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagCanonObject {
    pub object_type: String,
    pub canonical_key: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagCanonLink {
    pub from: String,
    pub link_type: String,
    pub to: serde_json::Value,
    #[serde(default)]
    pub original_predicate: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OagCanonData {
    #[serde(default)]
    pub objects: Vec<OagCanonObject>,
    #[serde(default)]
    pub links: Vec<OagCanonLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OagCanonImportChangeKind {
    Added,
    Modified,
    Retired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagCanonImportChange {
    pub kind: OagCanonImportChangeKind,
    pub key: String,
    pub label: String,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagCanonImportPreview {
    pub world_id: Option<String>,
    pub is_upgrade: bool,
    pub compatibility: bool,
    pub ontology_change: Option<OagCanonImportChange>,
    pub object_type_changes: Vec<OagCanonImportChange>,
    pub link_type_changes: Vec<OagCanonImportChange>,
    pub object_changes: Vec<OagCanonImportChange>,
    pub link_changes: Vec<OagCanonImportChange>,
    pub added_count: usize,
    pub modified_count: usize,
    pub retired_count: usize,
    pub unchanged_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagObjectType {
    pub id: String,
    pub world_id: String,
    pub key: String,
    pub label: String,
    pub parent_key: Option<String>,
    pub actor: bool,
    pub platform_role: String,
    pub attributes: Vec<OagAttributeDefinition>,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagLinkType {
    pub id: String,
    pub world_id: String,
    pub key: String,
    pub label: String,
    pub domain: Vec<String>,
    pub range: Vec<String>,
    pub functional: bool,
    pub symmetric: bool,
    pub transitive: bool,
    pub temporal: bool,
    pub inverse: Option<String>,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagObject {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub object_type: String,
    pub canonical_key: String,
    pub name: String,
    pub name_normalized: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub source: String,
    pub visibility: String,
    pub asserted_by_id: Option<String>,
    pub source_event_id: Option<String>,
    pub truth_status: String,
    pub created_turn: i32,
    pub updated_turn: i32,
    pub retired_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagAttribute {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub object_id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub value_type: String,
    pub valid_from_turn: i32,
    pub valid_to_turn: Option<i32>,
    pub source: String,
    pub confidence: f64,
    pub asserted_by_id: Option<String>,
    pub source_event_id: Option<String>,
    pub visibility: String,
    pub truth_status: String,
    pub supersedes_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagLink {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub from_object_id: String,
    pub link_type: String,
    pub to_object_id: Option<String>,
    pub to_value: String,
    pub original_predicate: String,
    pub attributes: serde_json::Value,
    pub valid_from_turn: i32,
    pub valid_to_turn: Option<i32>,
    pub source: String,
    pub confidence: f64,
    pub asserted_by_id: Option<String>,
    pub source_event_id: Option<String>,
    pub visibility: String,
    pub truth_status: String,
    pub inferred: bool,
    pub supersedes_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagScene {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub runtime_scene_id: String,
    pub name: String,
    pub location_object_id: Option<String>,
    pub background_hint: String,
    pub valid_from_turn: i32,
    pub valid_to_turn: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagSceneParticipant {
    pub id: String,
    pub scene_id: String,
    pub object_id: String,
    pub role: String,
    pub presence_mode: String,
    pub from_turn: i32,
    pub to_turn: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagEvent {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub ordinal: i32,
    pub scene_id: Option<String>,
    pub event_type: String,
    pub title: String,
    pub summary: String,
    pub details: serde_json::Value,
    pub source_message_id: Option<String>,
    pub dedupe_key: String,
    pub visibility: String,
    pub importance: f64,
    pub truth_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagEventObject {
    pub event_id: String,
    pub object_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagTurnAudience {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub message_id: String,
    pub scene_id: Option<String>,
    pub actor_id: String,
    pub access_kind: String,
    pub presence_mode: String,
    pub learned_turn: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagKnowledge {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub knowledge_type: String,
    pub access_kind: String,
    pub source_event_id: Option<String>,
    pub source_actor_id: Option<String>,
    pub learned_turn: i32,
    pub confidence: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagMemoryObject {
    pub memory_id: String,
    pub object_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagLegacyIdMap {
    pub source_table: String,
    pub source_id: String,
    pub world_id: String,
    pub session_id: String,
    pub target_table: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagQueryTrace {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub actor_id: Option<String>,
    pub as_of_turn: i32,
    pub query: serde_json::Value,
    pub visible_nodes: i32,
    pub filtered_count: i32,
    pub result: serde_json::Value,
    pub latency_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagWriteTrace {
    pub id: String,
    pub world_id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub step: String,
    pub status: String,
    pub detail: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum OagActor {
    Object(String),
    Player(String),
    Director,
    Author,
}

impl OagActor {
    pub fn is_omniscient(&self) -> bool {
        matches!(self, Self::Director | Self::Author)
    }

    pub fn object_id(&self) -> Option<&str> {
        match self {
            Self::Object(id) | Self::Player(id) => Some(id),
            Self::Director | Self::Author => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagQueryContext {
    pub world_id: String,
    pub session_id: String,
    pub actor: OagActor,
    pub as_of_turn: i32,
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagEntityReference {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub object_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagAttributeCondition {
    pub key: String,
    #[serde(default = "default_eq")]
    pub op: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagLinkCondition {
    pub link_type: String,
    #[serde(default = "default_out")]
    pub direction: String,
    #[serde(default)]
    pub to: Option<OagEntityReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagTraverseStep {
    pub link_type: String,
    #[serde(default = "default_out")]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagKnowledgeTarget {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagQueryRequest {
    pub op: String,
    #[serde(default)]
    pub entity: Option<OagEntityReference>,
    #[serde(default)]
    pub object_type: Option<String>,
    #[serde(default)]
    pub attribute: Option<OagAttributeCondition>,
    #[serde(default)]
    pub link: Option<OagLinkCondition>,
    #[serde(default)]
    pub steps: Vec<OagTraverseStep>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub metric_attribute: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub group_attribute: Option<String>,
    #[serde(default)]
    pub knowledge_actor: Option<OagEntityReference>,
    #[serde(default)]
    pub target: Option<OagKnowledgeTarget>,
    #[serde(default)]
    pub mentioned: Option<String>,
    #[serde(default)]
    pub as_of_turn: Option<i32>,
    #[serde(default)]
    pub from_turn: Option<i32>,
    #[serde(default)]
    pub to_turn: Option<i32>,
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub participant: Option<OagEntityReference>,
    #[serde(default)]
    pub location: Option<OagEntityReference>,
    #[serde(default)]
    pub include_history: bool,
    #[serde(default)]
    pub include_retired: bool,
    #[serde(default = "default_query_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OagQueryResponse {
    pub ok: bool,
    pub op: String,
    pub count: usize,
    pub truncated: bool,
    pub results: Vec<serde_json::Value>,
    pub trace_id: String,
    pub hint: String,
}

fn default_true() -> bool {
    true
}

fn default_value_type() -> String {
    "string".to_string()
}

fn default_eq() -> String {
    "eq".to_string()
}

fn default_out() -> String {
    "out".to_string()
}

fn default_max_nodes() -> usize {
    200
}

fn default_query_limit() -> usize {
    20
}
