use rusqlite::Connection;

pub const OAG_TABLE_COUNT: usize = 16;
pub const OAG_INDEX_COUNT: usize = 14;

const TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS oag_schema_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  schema_version INTEGER NOT NULL,
  applied_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS oag_object_types (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  key TEXT NOT NULL,
  label TEXT NOT NULL,
  parent_key TEXT,
  actor INTEGER NOT NULL DEFAULT 0,
  platform_role TEXT NOT NULL DEFAULT '',
  attributes_json TEXT NOT NULL DEFAULT '[]',
  builtin INTEGER NOT NULL DEFAULT 0,
  UNIQUE(world_id, key)
);

CREATE TABLE IF NOT EXISTS oag_link_types (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  key TEXT NOT NULL,
  label TEXT NOT NULL,
  domain_json TEXT NOT NULL DEFAULT '[]',
  range_json TEXT NOT NULL DEFAULT '[]',
  functional INTEGER NOT NULL DEFAULT 0,
  symmetric INTEGER NOT NULL DEFAULT 0,
  transitive INTEGER NOT NULL DEFAULT 0,
  temporal INTEGER NOT NULL DEFAULT 1,
  inverse TEXT,
  builtin INTEGER NOT NULL DEFAULT 0,
  UNIQUE(world_id, key)
);

CREATE TABLE IF NOT EXISTS oag_objects (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  object_type TEXT NOT NULL,
  canonical_key TEXT NOT NULL,
  name TEXT NOT NULL,
  name_normalized TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  description TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'system',
  visibility TEXT NOT NULL DEFAULT 'scene',
  asserted_by_id TEXT,
  source_event_id TEXT,
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  created_turn INTEGER NOT NULL DEFAULT 0,
  updated_turn INTEGER NOT NULL DEFAULT 0,
  retired_at TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT '',
  UNIQUE(world_id, session_id, canonical_key),
  FOREIGN KEY(asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY(source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS oag_attributes (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  object_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL DEFAULT 'null',
  value_type TEXT NOT NULL DEFAULT 'string',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'system',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by_id TEXT,
  source_event_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  supersedes_id TEXT,
  created_at TEXT NOT NULL DEFAULT '',
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY (source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL,
  FOREIGN KEY (supersedes_id) REFERENCES oag_attributes(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS oag_links (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  from_object_id TEXT NOT NULL,
  link_type TEXT NOT NULL,
  to_object_id TEXT,
  to_value TEXT NOT NULL DEFAULT '',
  original_predicate TEXT NOT NULL DEFAULT '',
  attributes_json TEXT NOT NULL DEFAULT '{}',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'system',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by_id TEXT,
  source_event_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  inferred INTEGER NOT NULL DEFAULT 0,
  supersedes_id TEXT,
  created_at TEXT NOT NULL DEFAULT '',
  CHECK (
    (to_object_id IS NOT NULL AND to_value = '')
    OR (to_object_id IS NULL AND to_value <> '')
  ),
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (from_object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (to_object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY (source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL,
  FOREIGN KEY (supersedes_id) REFERENCES oag_links(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS oag_scenes (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  runtime_scene_id TEXT NOT NULL,
  name TEXT NOT NULL,
  location_object_id TEXT,
  background_hint TEXT NOT NULL DEFAULT '',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, runtime_scene_id),
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (location_object_id) REFERENCES oag_objects(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS oag_scene_participants (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'participant',
  presence_mode TEXT NOT NULL DEFAULT 'present',
  from_turn INTEGER NOT NULL DEFAULT 0,
  to_turn INTEGER,
  UNIQUE(scene_id, object_id, from_turn, presence_mode),
  CHECK (to_turn IS NULL OR to_turn > from_turn),
  FOREIGN KEY (scene_id) REFERENCES oag_scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oag_events (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  ordinal INTEGER NOT NULL DEFAULT 0,
  scene_id TEXT,
  event_type TEXT NOT NULL DEFAULT 'turn',
  title TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  details_json TEXT NOT NULL DEFAULT '{}',
  source_message_id TEXT,
  dedupe_key TEXT NOT NULL,
  visibility TEXT NOT NULL DEFAULT 'scene',
  importance REAL NOT NULL DEFAULT 0.5,
  truth_status TEXT NOT NULL DEFAULT 'confirmed',
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, dedupe_key),
  FOREIGN KEY (scene_id) REFERENCES oag_scenes(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS oag_event_objects (
  event_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'participant',
  PRIMARY KEY(event_id, object_id, role),
  FOREIGN KEY(event_id) REFERENCES oag_events(id) ON DELETE CASCADE,
  FOREIGN KEY(object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oag_turn_audience (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  message_id TEXT NOT NULL,
  scene_id TEXT,
  actor_id TEXT NOT NULL,
  access_kind TEXT NOT NULL DEFAULT 'content',
  presence_mode TEXT NOT NULL DEFAULT 'present',
  learned_turn INTEGER NOT NULL,
  UNIQUE(session_id, message_id, actor_id, access_kind),
  FOREIGN KEY(scene_id) REFERENCES oag_scenes(id) ON DELETE SET NULL,
  FOREIGN KEY(actor_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oag_knowledge (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  knowledge_type TEXT NOT NULL DEFAULT 'witnessed',
  access_kind TEXT NOT NULL DEFAULT 'content',
  source_event_id TEXT,
  source_actor_id TEXT,
  learned_turn INTEGER NOT NULL,
  confidence REAL NOT NULL DEFAULT 1.0,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, actor_id, target_kind, target_id, source_event_id, access_kind),
  FOREIGN KEY(actor_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY(source_actor_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY(source_event_id) REFERENCES oag_events(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oag_memory_objects (
  memory_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'mentioned',
  PRIMARY KEY(memory_id, object_id, role),
  FOREIGN KEY(memory_id) REFERENCES memories(id) ON DELETE CASCADE,
  FOREIGN KEY(object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oag_legacy_id_map (
  source_table TEXT NOT NULL,
  source_id TEXT NOT NULL,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  target_table TEXT NOT NULL,
  target_id TEXT NOT NULL,
  PRIMARY KEY(source_table, source_id)
);

CREATE TABLE IF NOT EXISTS oag_query_traces (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  actor_id TEXT,
  as_of_turn INTEGER NOT NULL,
  query_json TEXT NOT NULL DEFAULT '{}',
  visible_nodes INTEGER NOT NULL DEFAULT 0,
  filtered_count INTEGER NOT NULL DEFAULT 0,
  result_json TEXT NOT NULL DEFAULT '{}',
  latency_ms INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS oag_write_traces (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  step TEXT NOT NULL,
  status TEXT NOT NULL,
  detail_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT ''
);
"#;

const INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_oag_objects_scope_name
  ON oag_objects(world_id, session_id, object_type, name_normalized);
CREATE INDEX IF NOT EXISTS idx_oag_objects_canonical_key
  ON oag_objects(world_id, session_id, canonical_key);
CREATE INDEX IF NOT EXISTS idx_oag_attributes_object
  ON oag_attributes(object_id, key, valid_from_turn, valid_to_turn);
CREATE INDEX IF NOT EXISTS idx_oag_links_from
  ON oag_links(from_object_id, link_type, valid_from_turn, valid_to_turn);
CREATE INDEX IF NOT EXISTS idx_oag_links_to
  ON oag_links(to_object_id, link_type, valid_from_turn, valid_to_turn);
CREATE INDEX IF NOT EXISTS idx_oag_scenes_session
  ON oag_scenes(session_id, valid_from_turn, valid_to_turn);
CREATE INDEX IF NOT EXISTS idx_oag_scene_participants_object
  ON oag_scene_participants(object_id, from_turn, to_turn);
CREATE INDEX IF NOT EXISTS idx_oag_events_turn
  ON oag_events(session_id, turn_index, ordinal);
CREATE INDEX IF NOT EXISTS idx_oag_event_objects_object
  ON oag_event_objects(object_id, event_id);
CREATE INDEX IF NOT EXISTS idx_oag_turn_audience_turn
  ON oag_turn_audience(session_id, turn_index, actor_id);
CREATE INDEX IF NOT EXISTS idx_oag_knowledge_actor_target
  ON oag_knowledge(session_id, actor_id, target_kind, target_id, learned_turn);
CREATE INDEX IF NOT EXISTS idx_oag_knowledge_target_actor
  ON oag_knowledge(session_id, target_kind, target_id, actor_id, learned_turn);
CREATE INDEX IF NOT EXISTS idx_oag_memory_objects_object
  ON oag_memory_objects(object_id, memory_id);
CREATE INDEX IF NOT EXISTS idx_oag_query_traces_turn
  ON oag_query_traces(session_id, turn_index, actor_id);
"#;

pub fn create(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(TABLES)?;
    conn.execute_batch(INDEXES)
}
