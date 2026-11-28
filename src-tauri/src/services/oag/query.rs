use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection};

use crate::db::repositories::oag_repo::OagRepository;
use crate::models::oag::{
    OagActor, OagAttribute, OagAttributeCondition, OagEntityReference, OagEvent, OagEventObject,
    OagKnowledge, OagLink, OagObject, OagQueryContext, OagQueryRequest, OagQueryResponse, OagScene,
    OagSceneParticipant,
};

use super::canon::normalize_name;
use super::ontology::{self, ResolvedOntology};
use super::resolver;

#[derive(Debug, Clone)]
pub struct VisibleMemory {
    pub memory: serde_json::Value,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VisibleGraph {
    pub objects: Vec<OagObject>,
    pub attributes: Vec<OagAttribute>,
    pub links: Vec<OagLink>,
    pub events: Vec<OagEvent>,
    pub event_objects: Vec<OagEventObject>,
    pub scenes: Vec<OagScene>,
    pub scene_participants: Vec<OagSceneParticipant>,
    pub memories: Vec<VisibleMemory>,
    pub knowledge: Vec<OagKnowledge>,
    pub event_access: BTreeMap<String, String>,
    pub filtered_count: usize,
    pub truncated: bool,
    ontology: ResolvedOntology,
    context: OagQueryContext,
    legacy_object_ids: HashMap<String, String>,
}

impl VisibleGraph {
    pub fn load(
        conn: &Connection,
        mut context: OagQueryContext,
        ontology: ResolvedOntology,
        include_retired: bool,
        include_history: bool,
    ) -> Result<Self, String> {
        validate_context(&context)?;
        if include_retired && !context.actor.is_omniscient() {
            return Err("include_retired 仅允许导演或作者视角".to_string());
        }
        normalize_actor(conn, &mut context, &ontology)?;
        let repo = OagRepository::new(conn);
        let actor_id = context.actor.object_id();
        let omniscient = context.actor.is_omniscient();
        let (content_knowledge, envelope_knowledge) =
            load_knowledge(conn, &context, actor_id, omniscient)?;
        let visible = |kind: &str,
                       id: &str,
                       visibility: &str,
                       asserted_by: Option<&str>,
                       self_visible: bool| {
            omniscient
                || visibility == "public"
                || (self_visible && actor_id == Some(id))
                || (visibility == "private" && actor_id.is_some() && asserted_by == actor_id)
                || (visibility == "scene"
                    && content_knowledge
                        .get(&(kind.to_string(), id.to_string()))
                        .is_some_and(|learned| *learned <= context.as_of_turn))
        };

        let mut filtered_count = 0;
        let (objects, rejected) = retain_filtered(
            repo.list_objects_scope(&context.world_id, &context.session_id)?,
            |object| {
                object.created_turn <= context.as_of_turn
                    && (include_retired || object.retired_at.is_empty())
                    && visible(
                        "object",
                        &object.id,
                        &object.visibility,
                        object.asserted_by_id.as_deref(),
                        true,
                    )
            },
        );
        filtered_count += rejected;
        let object_ids = objects
            .iter()
            .map(|object| object.id.clone())
            .collect::<HashSet<_>>();
        let legacy_object_ids = load_legacy_object_ids(conn, &context, &object_ids)?;

        let (attributes, rejected) = retain_filtered(
            repo.list_attributes_scope(&context.world_id, &context.session_id)?,
            |attribute| {
                attribute.valid_from_turn <= context.as_of_turn
                    && (include_history
                        || attribute
                            .valid_to_turn
                            .is_none_or(|end| end > context.as_of_turn))
                    && object_ids.contains(&attribute.object_id)
                    && visible(
                        "attribute",
                        &attribute.id,
                        &attribute.visibility,
                        attribute.asserted_by_id.as_deref(),
                        false,
                    )
            },
        );
        filtered_count += rejected;

        let (links, rejected) = retain_filtered(
            repo.list_links_scope(&context.world_id, &context.session_id)?,
            |link| {
                link.valid_from_turn <= context.as_of_turn
                    && (include_history
                        || link
                            .valid_to_turn
                            .is_none_or(|end| end > context.as_of_turn))
                    && object_ids.contains(&link.from_object_id)
                    && link
                        .to_object_id
                        .as_ref()
                        .is_none_or(|id| object_ids.contains(id))
                    && visible(
                        "link",
                        &link.id,
                        &link.visibility,
                        link.asserted_by_id.as_deref(),
                        false,
                    )
            },
        );
        filtered_count += rejected;

        let (events, rejected) = retain_filtered(
            repo.list_events_scope(&context.world_id, &context.session_id)?,
            |event| {
                if event.turn_index > context.as_of_turn {
                    return false;
                }
                let content = omniscient
                    || event.visibility == "public"
                    || (event.visibility == "scene"
                        && content_knowledge
                            .get(&("event".to_string(), event.id.clone()))
                            .is_some_and(|learned| *learned <= context.as_of_turn));
                let envelope = event.visibility == "scene"
                    && envelope_knowledge
                        .get(&event.id)
                        .is_some_and(|learned| *learned <= context.as_of_turn);
                if content {
                    true
                } else if envelope {
                    true
                } else {
                    false
                }
            },
        );
        filtered_count += rejected;
        let mut event_access = BTreeMap::new();
        let mut content_event_ids = HashSet::new();
        let events = events
            .into_iter()
            .map(|event| {
                let content = omniscient
                    || event.visibility == "public"
                    || (event.visibility == "scene"
                        && content_knowledge
                            .get(&("event".to_string(), event.id.clone()))
                            .is_some_and(|learned| *learned <= context.as_of_turn));
                if content {
                    content_event_ids.insert(event.id.clone());
                    event_access.insert(event.id.clone(), "content".to_string());
                    event
                } else {
                    event_access.insert(event.id.clone(), "envelope".to_string());
                    OagEvent {
                        title: "获悉一条事件记录".to_string(),
                        summary: String::new(),
                        details: serde_json::json!({ "access_kind": "envelope" }),
                        source_message_id: None,
                        scene_id: None,
                        ..event
                    }
                }
            })
            .collect::<Vec<_>>();
        let event_objects = repo
            .list_event_objects_scope(&context.world_id, &context.session_id)?
            .into_iter()
            .filter(|item| {
                content_event_ids.contains(&item.event_id) && object_ids.contains(&item.object_id)
            })
            .collect();
        let scenes = repo
            .list_scenes_scope(&context.world_id, &context.session_id)?
            .into_iter()
            .filter(|scene| {
                scene.valid_from_turn <= context.as_of_turn
                    && events.iter().any(|event| {
                        content_event_ids.contains(&event.id)
                            && event.scene_id.as_deref() == Some(scene.id.as_str())
                    })
            })
            .collect::<Vec<_>>();
        let scene_ids = scenes
            .iter()
            .map(|scene| scene.id.as_str())
            .collect::<HashSet<_>>();
        let scene_participants = repo
            .list_scene_participants_scope(&context.world_id, &context.session_id)?
            .into_iter()
            .filter(|participant| {
                participant.from_turn <= context.as_of_turn
                    && participant
                        .to_turn
                        .is_none_or(|end| end > context.as_of_turn)
                    && scene_ids.contains(participant.scene_id.as_str())
                    && object_ids.contains(&participant.object_id)
            })
            .collect();
        let (mut memories, memories_truncated) =
            load_memories(conn, &context, actor_id, omniscient, context.max_nodes)?;
        for memory in &mut memories {
            memory.object_ids.retain(|id| object_ids.contains(id));
        }
        let attribute_ids = attributes
            .iter()
            .map(|attribute| attribute.id.clone())
            .collect::<HashSet<_>>();
        let link_ids = links
            .iter()
            .map(|link| link.id.clone())
            .collect::<HashSet<_>>();
        let event_ids = events
            .iter()
            .map(|event| event.id.clone())
            .collect::<HashSet<_>>();
        let memory_ids = memories
            .iter()
            .filter_map(|memory| memory.memory["id"].as_str().map(str::to_string))
            .collect::<HashSet<_>>();
        let knowledge = repo
            .list_knowledge_scope(&context.world_id, &context.session_id)?
            .into_iter()
            .filter(|item| {
                item.learned_turn <= context.as_of_turn
                    && (omniscient || actor_id == Some(item.actor_id.as_str()))
                    && object_ids.contains(&item.actor_id)
                    && match item.target_kind.as_str() {
                        "object" => object_ids.contains(&item.target_id),
                        "attribute" => attribute_ids.contains(&item.target_id),
                        "link" => link_ids.contains(&item.target_id),
                        "event" => event_ids.contains(&item.target_id),
                        "memory" => memory_ids.contains(&item.target_id),
                        _ => false,
                    }
                    && item
                        .source_actor_id
                        .as_ref()
                        .is_none_or(|id| object_ids.contains(id))
            })
            .collect();

        Ok(Self {
            objects,
            attributes,
            links,
            events,
            event_objects,
            scenes,
            scene_participants,
            memories,
            knowledge,
            event_access,
            filtered_count,
            truncated: memories_truncated,
            ontology,
            context,
            legacy_object_ids,
        })
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered_count
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn node_count(&self) -> usize {
        self.objects.len()
            + self.attributes.len()
            + self.links.len()
            + self.events.len()
            + self.memories.len()
    }

    pub fn as_of_turn(&self) -> i32 {
        self.context.as_of_turn
    }

    pub fn max_nodes(&self) -> usize {
        self.context.max_nodes
    }

    pub fn actor_id(&self) -> Option<&str> {
        self.context.actor.object_id()
    }

    pub fn is_omniscient(&self) -> bool {
        self.context.actor.is_omniscient()
    }

    pub(crate) fn ontology(&self) -> &ResolvedOntology {
        &self.ontology
    }

    pub fn object(&self, id: &str) -> Option<&OagObject> {
        self.objects.iter().find(|object| object.id == id)
    }

    pub fn resolve(&self, reference: &OagEntityReference) -> Result<Option<OagObject>, String> {
        if let Some(id) = reference
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            let resolved_id = self
                .legacy_object_ids
                .get(id)
                .map(String::as_str)
                .unwrap_or(id);
            return Ok(self
                .object(resolved_id)
                .filter(|object| {
                    reference.object_type.as_deref().is_none_or(|key| {
                        ontology::type_matches(
                            &self.ontology.ontology,
                            &object.object_type,
                            &[key.to_string()],
                        )
                    })
                })
                .cloned());
        }
        let name = reference
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "实体引用缺少稳定 ID 或名称".to_string())?;
        let candidates = self
            .objects
            .iter()
            .filter(|object| {
                reference.object_type.as_deref().is_none_or(|key| {
                    ontology::type_matches(
                        &self.ontology.ontology,
                        &object.object_type,
                        &[key.to_string()],
                    )
                })
            })
            .collect::<Vec<_>>();
        for matches in [
            candidates
                .iter()
                .filter(|object| object.name == name)
                .collect::<Vec<_>>(),
            candidates
                .iter()
                .filter(|object| object.aliases.iter().any(|alias| alias == name))
                .collect::<Vec<_>>(),
        ] {
            match matches.len() {
                0 => {}
                1 => return Ok(Some((*matches[0]).clone())),
                _ => return Err(format!("实体名称歧义，拒绝自动选择：{name}")),
            }
        }
        let normalized = normalize_name(name);
        let matches = candidates
            .iter()
            .filter(|object| {
                object.name_normalized == normalized
                    || object
                        .aliases
                        .iter()
                        .any(|alias| normalize_name(alias) == normalized)
            })
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some((*matches[0]).clone())),
            _ => Err(format!("实体名称歧义，拒绝自动选择：{name}")),
        }
    }

    fn attributes_for(&self, object_id: &str) -> Vec<&OagAttribute> {
        self.attributes
            .iter()
            .filter(|attribute| attribute.object_id == object_id)
            .collect()
    }

    fn links_for(&self, object_id: &str) -> Vec<serde_json::Value> {
        let mut result = Vec::new();
        for link in &self.links {
            if link.from_object_id == object_id {
                result.push(self.link_json(link, "out", false));
            } else if link.to_object_id.as_deref() == Some(object_id) {
                let symmetric = self.link_is_symmetric(link);
                result.push(self.link_json(link, if symmetric { "out" } else { "in" }, symmetric));
            }
        }
        result
    }

    fn events_for(&self, object_id: &str) -> Vec<&OagEvent> {
        let ids = self
            .event_objects
            .iter()
            .filter(|item| item.object_id == object_id)
            .map(|item| item.event_id.as_str())
            .collect::<HashSet<_>>();
        self.events
            .iter()
            .filter(|event| ids.contains(event.id.as_str()))
            .collect()
    }

    fn link_is_symmetric(&self, link: &OagLink) -> bool {
        self.ontology
            .ontology
            .link_types
            .iter()
            .find(|definition| definition.key == link.link_type)
            .is_some_and(|definition| definition.symmetric)
    }

    fn object_json(&self, object: &OagObject) -> serde_json::Value {
        let mut value = serde_json::to_value(object).unwrap_or_default();
        value["asserted_by_id"] = object
            .asserted_by_id
            .as_deref()
            .filter(|id| self.object(id).is_some())
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["source_event_id"] = object
            .source_event_id
            .as_deref()
            .filter(|id| self.event_access.get(*id) == Some(&"content".to_string()))
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        add_provenance(&mut value, object.source.as_str(), object.created_turn, 1.0);
        value
    }

    fn attribute_json(&self, attribute: &OagAttribute) -> serde_json::Value {
        let mut value = serde_json::to_value(attribute).unwrap_or_default();
        value["asserted_by_id"] = attribute
            .asserted_by_id
            .as_deref()
            .filter(|id| self.object(id).is_some())
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["source_event_id"] = attribute
            .source_event_id
            .as_deref()
            .filter(|id| self.event_access.get(*id) == Some(&"content".to_string()))
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["supersedes_id"] = attribute
            .supersedes_id
            .as_deref()
            .filter(|id| self.attributes.iter().any(|item| &item.id == id))
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        add_provenance(
            &mut value,
            attribute.source.as_str(),
            attribute.valid_from_turn,
            attribute.confidence,
        );
        if attribute
            .valid_to_turn
            .is_some_and(|end| end <= self.context.as_of_turn)
        {
            value["invalid_at_turn"] = serde_json::json!(attribute.valid_to_turn);
        }
        value
    }

    fn link_json(&self, link: &OagLink, direction: &str, symmetric: bool) -> serde_json::Value {
        let mut value = serde_json::to_value(link).unwrap_or_default();
        value["asserted_by_id"] = link
            .asserted_by_id
            .as_deref()
            .filter(|id| self.object(id).is_some())
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["source_event_id"] = link
            .source_event_id
            .as_deref()
            .filter(|id| self.event_access.get(*id) == Some(&"content".to_string()))
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["supersedes_id"] = link
            .supersedes_id
            .as_deref()
            .filter(|id| self.links.iter().any(|item| &item.id == id))
            .map(|id| serde_json::Value::String(id.to_string()))
            .unwrap_or(serde_json::Value::Null);
        value["direction"] = serde_json::json!(direction);
        value["symmetric"] = serde_json::json!(symmetric);
        value["from_object"] = self
            .object(&link.from_object_id)
            .map(|object| self.object_json(object))
            .unwrap_or(serde_json::Value::Null);
        value["to_object"] = link
            .to_object_id
            .as_deref()
            .and_then(|id| self.object(id))
            .map(|object| self.object_json(object))
            .unwrap_or(serde_json::Value::Null);
        if link.to_object_id.is_none() {
            value["literal"] = serde_json::from_str::<serde_json::Value>(&link.to_value)
                .unwrap_or_else(|_| serde_json::Value::String(link.to_value.clone()));
        }
        add_provenance(
            &mut value,
            link.source.as_str(),
            link.valid_from_turn,
            link.confidence,
        );
        if link
            .valid_to_turn
            .is_some_and(|end| end <= self.context.as_of_turn)
        {
            value["invalid_at_turn"] = serde_json::json!(link.valid_to_turn);
        }
        value
    }
}

fn validate_context(context: &OagQueryContext) -> Result<(), String> {
    if context.as_of_turn < 0 {
        return Err("查询回合不能为负数".to_string());
    }
    if context.max_nodes == 0 || context.max_nodes > 200 {
        return Err("max_nodes 必须在 1 到 200 之间".to_string());
    }
    Ok(())
}

fn normalize_actor(
    conn: &Connection,
    context: &mut OagQueryContext,
    ontology: &ResolvedOntology,
) -> Result<(), String> {
    if context.actor.is_omniscient() {
        return Ok(());
    }
    let id = context
        .actor
        .object_id()
        .ok_or_else(|| "查询视角缺少对象 ID".to_string())?;
    let object = resolver::resolve_id(
        conn,
        &context.world_id,
        &context.session_id,
        id,
        context.as_of_turn,
    )?;
    let is_actor = ontology
        .ontology
        .object_types
        .iter()
        .find(|definition| definition.key == object.object_type)
        .is_some_and(|definition| definition.actor);
    if !is_actor {
        return Err("查询视角对应的对象类型未标记为 actor".to_string());
    }
    if matches!(context.actor, OagActor::Object(_)) {
        context.actor = OagActor::Object(object.id);
    } else {
        context.actor = OagActor::Player(object.id);
    }
    Ok(())
}

fn retain_filtered<T>(mut items: Vec<T>, mut predicate: impl FnMut(&T) -> bool) -> (Vec<T>, usize) {
    let before = items.len();
    items.retain(|item| predicate(item));
    let rejected = before - items.len();
    (items, rejected)
}

fn load_legacy_object_ids(
    conn: &Connection,
    context: &OagQueryContext,
    object_ids: &HashSet<String>,
) -> Result<HashMap<String, String>, String> {
    let prefix = format!("{}:canon:characters:", context.world_id);
    let mut stmt = conn
        .prepare(
            "SELECT source_id, target_id FROM oag_legacy_id_map
             WHERE world_id = ?1 AND source_table = 'characters'",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![context.world_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut result = HashMap::new();
    for row in rows {
        let (source_id, target_id) = row.map_err(|error| error.to_string())?;
        if let Some(id) = source_id.strip_prefix(&prefix) {
            if object_ids.contains(&target_id) {
                result.insert(id.to_string(), target_id);
            }
        }
    }
    Ok(result)
}

fn load_knowledge(
    conn: &Connection,
    context: &OagQueryContext,
    actor_id: Option<&str>,
    omniscient: bool,
) -> Result<(HashMap<(String, String), i32>, HashMap<String, i32>), String> {
    let mut content = HashMap::new();
    let mut envelope = HashMap::new();
    if omniscient {
        return Ok((content, envelope));
    }
    let Some(actor_id) = actor_id else {
        return Ok((content, envelope));
    };
    let mut stmt = conn
        .prepare(
            "SELECT target_kind, target_id, access_kind, learned_turn
             FROM oag_knowledge
             WHERE world_id = ?1 AND session_id = ?2 AND actor_id = ?3
               AND learned_turn <= ?4",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(
            params![
                context.world_id,
                context.session_id,
                actor_id,
                context.as_of_turn
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (kind, id, access, learned) = row.map_err(|error| error.to_string())?;
        if access == "envelope" && kind == "event" {
            envelope
                .entry(id)
                .and_modify(|current: &mut i32| *current = (*current).min(learned))
                .or_insert(learned);
        } else if access == "content" {
            content
                .entry((kind, id))
                .and_modify(|current: &mut i32| *current = (*current).min(learned))
                .or_insert(learned);
        }
    }
    Ok((content, envelope))
}

fn load_memories(
    conn: &Connection,
    context: &OagQueryContext,
    actor_id: Option<&str>,
    omniscient: bool,
    max_nodes: usize,
) -> Result<(Vec<VisibleMemory>, bool), String> {
    if !omniscient && actor_id.is_none() {
        return Ok((Vec::new(), false));
    }
    let character_ids = if omniscient {
        Vec::new()
    } else {
        memory_character_ids(conn, context, actor_id.unwrap())?
    };
    let owner_filter = if omniscient {
        "1 = 1".to_string()
    } else if character_ids.is_empty() {
        return Ok((Vec::new(), false));
    } else {
        format!(
            "m.character_id IN ({})",
            std::iter::repeat("?")
                .take(character_ids.len())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let sql = format!(
        "SELECT m.id, m.world_id, m.session_id, m.character_id, m.layer, m.content,
                m.source, m.importance, m.created_at, m.turn_index, m.memory_type,
                m.speaker, m.role, m.location, mo.object_id, m.conversation_id,
                m.event_id, m.item_id, m.scene_id, m.participants_json, m.keywords_json
         FROM memories m
         LEFT JOIN oag_memory_objects mo ON mo.memory_id = m.id
         WHERE m.world_id = ?1 AND m.session_id = ?2 AND m.turn_index <= ?3
           AND ({owner_filter})
         ORDER BY m.turn_index DESC, m.id DESC, mo.object_id, mo.role
         LIMIT ?{}",
        character_ids.len() + 4
    );
    let mut values = Vec::<rusqlite::types::Value>::from([
        rusqlite::types::Value::Text(context.world_id.clone()),
        rusqlite::types::Value::Text(context.session_id.clone()),
        rusqlite::types::Value::Integer(context.as_of_turn.into()),
    ]);
    values.extend(
        character_ids
            .iter()
            .cloned()
            .map(rusqlite::types::Value::Text),
    );
    values.push(rusqlite::types::Value::Integer((max_nodes + 1) as i64));
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(values), |row| {
            let memory = serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "world_id": row.get::<_, String>(1)?,
                "session_id": row.get::<_, String>(2)?,
                "character_id": row.get::<_, String>(3)?,
                "layer": row.get::<_, String>(4)?,
                "content": row.get::<_, String>(5)?,
                "source": row.get::<_, String>(6)?,
                "importance": row.get::<_, f64>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "turn_index": row.get::<_, i32>(9)?,
                "memory_type": row.get::<_, String>(10)?,
                "speaker": row.get::<_, Option<String>>(11)?,
                "role": row.get::<_, Option<String>>(12)?,
                "location": row.get::<_, Option<String>>(13)?,
                "conversation_id": row.get::<_, Option<String>>(15)?,
                "event_id": row.get::<_, Option<String>>(16)?,
                "item_id": row.get::<_, Option<String>>(17)?,
                "scene_id": row.get::<_, Option<String>>(18)?,
                "participants": serde_json::from_str::<Vec<String>>(
                    &row.get::<_, String>(19)?
                ).unwrap_or_default(),
                "keywords": serde_json::from_str::<Vec<String>>(
                    &row.get::<_, String>(20)?
                ).unwrap_or_default(),
            });
            Ok((memory, row.get::<_, Option<String>>(14)?))
        })
        .map_err(|error| error.to_string())?;
    let mut memories = Vec::<VisibleMemory>::new();
    let mut indexes = HashMap::<String, usize>::new();
    for row in rows {
        let (memory, object_id) = row.map_err(|error| error.to_string())?;
        let id = memory["id"].as_str().unwrap_or_default().to_string();
        let index = if let Some(index) = indexes.get(&id) {
            *index
        } else {
            let index = memories.len();
            indexes.insert(id.clone(), index);
            memories.push(VisibleMemory {
                memory,
                object_ids: Vec::new(),
            });
            index
        };
        if let Some(object_id) = object_id {
            if !memories[index].object_ids.contains(&object_id) {
                memories[index].object_ids.push(object_id);
            }
        }
    }
    let truncated = memories.len() > max_nodes;
    memories.truncate(max_nodes);
    Ok((memories, truncated))
}

fn memory_character_ids(
    conn: &Connection,
    context: &OagQueryContext,
    actor_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT source_id FROM oag_legacy_id_map
             WHERE world_id = ?1 AND source_table = 'characters' AND target_id = ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![context.world_id, actor_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?;
    let prefix = format!("{}:canon:characters:", context.world_id);
    let mut ids = HashSet::new();
    for row in rows {
        let source_id = row.map_err(|error| error.to_string())?;
        if let Some(id) = source_id.strip_prefix(&prefix) {
            if !id.is_empty() {
                ids.insert(id.to_string());
            }
        }
    }
    Ok(ids.into_iter().collect())
}

pub fn query(
    conn: &Connection,
    mut context: OagQueryContext,
    ontology: ResolvedOntology,
    request: OagQueryRequest,
) -> Result<OagQueryResponse, String> {
    let started = std::time::Instant::now();
    let execution = (|| -> Result<OagQueryResponse, String> {
        validate_context(&context)?;
        validate_request(&request, &context, &ontology)?;
        context.as_of_turn = request.as_of_turn.unwrap_or(context.as_of_turn);
        normalize_actor(conn, &mut context, &ontology)?;
        let graph = VisibleGraph::load(
            conn,
            context.clone(),
            ontology,
            request.include_retired,
            request.include_history,
        )?;
        let (total, results, operation_truncated) = match request.op.as_str() {
            "resolve" => {
                let reference = request
                    .entity
                    .as_ref()
                    .ok_or_else(|| "resolve 查询缺少 entity".to_string())?;
                match graph.resolve(reference)? {
                    Some(object) => (1, vec![graph.object_json(&object)], false),
                    None => (0, Vec::new(), false),
                }
            }
            "entity" => entity_results(&graph, &request)?,
            "find" => find_results(&graph, &request)?,
            "traverse" => traverse_results(&graph, &request)?,
            "aggregate" => aggregate_results(&graph, &request)?,
            "absence" => absence_results(&graph, &request)?,
            "knows" => knows_results(&graph, &request)?,
            "timeline" => timeline_results(&graph, &request)?,
            _ => unreachable!(),
        };
        let truncated = total > request.limit || operation_truncated;
        let mut results = results;
        results.truncate(request.limit);
        let trace_id = uuid::Uuid::new_v4().to_string();
        let hint = if results.is_empty() {
            if request.op == "absence" {
                "当前视角可见的对象中，没有找到符合否定条件的结果；不代表隐藏信息不存在".to_string()
            } else {
                "没有查到当前视角可见的信息".to_string()
            }
        } else if request.op == "absence" {
            "否定结果仅基于当前可见子图，不断言对象从未拥有隐藏或未知的关系/属性".to_string()
        } else if truncated {
            "结果已截断；可缩小时间范围或类型条件".to_string()
        } else {
            "只依据以上结构化结果回答；没有查到就说不知道".to_string()
        };
        let response = OagQueryResponse {
            ok: true,
            op: request.op.clone(),
            count: results.len(),
            truncated,
            results,
            trace_id: trace_id.clone(),
            hint,
        };
        record_query_trace(conn, &context, &request, &graph, &response, started)?;
        Ok(response)
    })();
    if let Err(error) = &execution {
        let _ = record_failed_query_trace(conn, &context, &request, started, error);
    }
    execution
}

fn validate_request(
    request: &OagQueryRequest,
    context: &OagQueryContext,
    ontology: &ResolvedOntology,
) -> Result<(), String> {
    if !matches!(
        request.op.as_str(),
        "resolve" | "entity" | "find" | "traverse" | "aggregate" | "absence" | "knows" | "timeline"
    ) {
        return Err(
            "M2 支持 resolve、entity、find、traverse、aggregate、absence、knows 和 timeline 查询"
                .to_string(),
        );
    }
    if request.limit == 0 || request.limit > 20 {
        return Err("查询结果数量必须在 1 到 20 之间".to_string());
    }
    if request.include_retired && !context.actor.is_omniscient() {
        return Err("include_retired 仅允许导演或作者视角".to_string());
    }
    if request
        .as_of_turn
        .is_some_and(|turn| turn < 0 || turn > context.as_of_turn)
    {
        return Err("as_of_turn 必须在 0 到当前查询回合之间".to_string());
    }
    if request.from_turn.is_some_and(|turn| turn < 0)
        || request.to_turn.is_some_and(|turn| turn < 0)
    {
        return Err("时间范围不能为负数".to_string());
    }
    if request
        .from_turn
        .zip(request.to_turn)
        .is_some_and(|(from, to)| from > to)
    {
        return Err("时间范围的起始回合不能晚于结束回合".to_string());
    }
    if let Some(key) = request.object_type.as_deref() {
        if !ontology
            .ontology
            .object_types
            .iter()
            .any(|definition| definition.key == key)
        {
            return Err(format!("查询使用了未声明的对象类型：{key}"));
        }
    }
    if let Some(condition) = request.attribute.as_ref() {
        if !matches!(
            condition.op.as_str(),
            "eq" | "ne" | "gt" | "gte" | "lt" | "lte" | "in" | "contains"
        ) {
            return Err(format!("不支持的属性条件：{}", condition.op));
        }
        if condition.op == "in" && !condition.value.is_array() {
            return Err("属性 in 条件必须提供数组".to_string());
        }
    }
    if let Some(condition) = request.link.as_ref() {
        if !ontology
            .ontology
            .link_types
            .iter()
            .any(|definition| definition.key == condition.link_type)
        {
            return Err(format!(
                "查询使用了未声明的关系类型：{}",
                condition.link_type
            ));
        }
        if !matches!(condition.direction.as_str(), "out" | "in" | "both") {
            return Err("关系方向必须是 out、in 或 both".to_string());
        }
    }
    if request.op == "traverse" {
        if request.entity.is_none() {
            return Err("traverse 查询缺少起始 entity".to_string());
        }
        if request.steps.is_empty() || request.steps.len() > 3 {
            return Err("traverse 必须包含 1 到 3 个关系步骤".to_string());
        }
        for step in &request.steps {
            if !ontology
                .ontology
                .link_types
                .iter()
                .any(|definition| definition.key == step.link_type)
            {
                return Err(format!("查询使用了未声明的关系类型：{}", step.link_type));
            }
            if !matches!(step.direction.as_str(), "out" | "in" | "both") {
                return Err("关系方向必须是 out、in 或 both".to_string());
            }
        }
    }
    if request.op == "aggregate" {
        let metric = request.metric.as_deref().unwrap_or("count");
        if !matches!(
            metric,
            "count" | "count_distinct" | "sum" | "avg" | "min" | "max"
        ) {
            return Err(format!("不支持的聚合指标：{metric}"));
        }
        if matches!(metric, "sum" | "avg" | "min" | "max")
            && request
                .metric_attribute
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err("数值聚合必须提供 metric_attribute".to_string());
        }
        if request
            .group_by
            .as_deref()
            .is_some_and(|group| !matches!(group, "object_type" | "attribute" | "link_target"))
        {
            return Err("group_by 必须是 object_type、attribute 或 link_target".to_string());
        }
        if request.group_by.as_deref() == Some("attribute")
            && request
                .group_attribute
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err("按属性分组必须提供 group_attribute".to_string());
        }
        if request.group_by.as_deref() == Some("link_target") && request.link.is_none() {
            return Err("按关系目标分组必须提供 link 条件".to_string());
        }
    }
    if request.op == "absence" {
        if request.object_type.is_none() {
            return Err("absence 查询必须提供 object_type，避免把全部对象当作否定候选".to_string());
        }
        if request.attribute.is_none() && request.link.is_none() {
            return Err("absence 查询至少提供 attribute 或 link 条件".to_string());
        }
    }
    if request.op == "knows" {
        let Some(target) = request.target.as_ref() else {
            return Err("knows 查询缺少 target".to_string());
        };
        if !matches!(
            target.kind.as_str(),
            "object" | "attribute" | "link" | "event" | "memory"
        ) {
            return Err("知识目标类型必须是 object、attribute、link、event 或 memory".to_string());
        }
        if target.id.trim().is_empty() {
            return Err("知识目标不能缺少稳定 ID".to_string());
        }
    }
    if request
        .mentioned
        .as_deref()
        .is_some_and(|text| text.trim().is_empty())
    {
        return Err("mentioned 不能是空文本".to_string());
    }
    Ok(())
}

fn entity_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let reference = request
        .entity
        .as_ref()
        .ok_or_else(|| "entity 查询缺少 entity".to_string())?;
    let Some(object) = graph.resolve(reference)? else {
        return Ok((0, Vec::new(), false));
    };
    let attributes = graph
        .attributes_for(&object.id)
        .into_iter()
        .map(|attribute| graph.attribute_json(attribute))
        .collect::<Vec<_>>();
    let links = graph.links_for(&object.id);
    let mut events = graph.events_for(&object.id);
    events.sort_by(|left, right| {
        right
            .turn_index
            .cmp(&left.turn_index)
            .then_with(|| right.ordinal.cmp(&left.ordinal))
            .then_with(|| left.id.cmp(&right.id))
    });
    let events = events
        .into_iter()
        .take(request.limit)
        .map(|event| event_json(graph, event))
        .collect::<Vec<_>>();
    let memories = graph
        .memories
        .iter()
        .filter(|memory| memory.object_ids.contains(&object.id))
        .take(request.limit)
        .map(|memory| {
            let mut value = memory.memory.clone();
            let source = value["source"].as_str().unwrap_or_default().to_string();
            let turn = value["turn_index"].as_i64().unwrap_or_default() as i32;
            let confidence = value["importance"].as_f64().unwrap_or(1.0);
            add_provenance(&mut value, &source, turn, confidence);
            value
        })
        .collect::<Vec<_>>();
    let available = attributes.len() + links.len() + events.len() + memories.len();
    let mut budget = graph.max_nodes().saturating_sub(1);
    let attribute_take = attributes.len().min(budget);
    budget -= attribute_take;
    let link_take = links.len().min(budget);
    budget -= link_take;
    let event_take = events.len().min(budget);
    budget -= event_take;
    let memory_take = memories.len().min(budget);
    let truncated = graph.truncated() || available > graph.max_nodes().saturating_sub(1);
    Ok((
        1,
        vec![serde_json::json!({
            "object": graph.object_json(&object),
            "attributes": attributes.into_iter().take(attribute_take).collect::<Vec<_>>(),
            "links": links.into_iter().take(link_take).collect::<Vec<_>>(),
            "events": events.into_iter().take(event_take).collect::<Vec<_>>(),
            "memories": memories.into_iter().take(memory_take).collect::<Vec<_>>(),
        })],
        truncated,
    ))
}

fn find_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let objects = filtered_objects(graph, request)?;
    let total = objects.len();
    let results = objects
        .into_iter()
        .map(|object| graph.object_json(object))
        .collect();
    Ok((total, results, false))
}

fn filtered_objects<'a>(
    graph: &'a VisibleGraph,
    request: &OagQueryRequest,
) -> Result<Vec<&'a OagObject>, String> {
    let target = match request.link.as_ref().and_then(|link| link.to.as_ref()) {
        Some(reference) => match graph.resolve(reference)? {
            Some(object) => Some(object),
            None => return Ok(Vec::new()),
        },
        None => None,
    };
    let mut objects = graph
        .objects
        .iter()
        .filter(|object| {
            request.object_type.as_deref().is_none_or(|key| {
                ontology::type_matches(
                    &graph.ontology.ontology,
                    &object.object_type,
                    &[key.to_string()],
                )
            })
        })
        .filter(|object| {
            request.attribute.as_ref().is_none_or(|condition| {
                graph
                    .attributes_for(&object.id)
                    .iter()
                    .any(|attribute| attribute_matches(attribute, condition))
            })
        })
        .filter(|object| {
            request.link.as_ref().is_none_or(|condition| {
                graph.links.iter().any(|link| {
                    link.link_type == condition.link_type
                        && link_matches(
                            graph,
                            link,
                            &object.id,
                            &condition.direction,
                            target.as_ref().map(|target| target.id.as_str()),
                        )
                })
            })
        })
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| {
        left.name_normalized
            .cmp(&right.name_normalized)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(objects)
}

fn traverse_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let start = graph
        .resolve(
            request
                .entity
                .as_ref()
                .ok_or_else(|| "traverse 查询缺少起始 entity".to_string())?,
        )?
        .ok_or_else(|| "起始实体不在当前视角可见子图中".to_string())?;
    let mut frontier = BTreeMap::from([(
        start.id.clone(),
        TraversalPath {
            nodes: vec![start.id.clone()],
            hops: Vec::new(),
            confidence: 1.0,
        },
    )]);
    let mut budget_truncated = false;
    for step in &request.steps {
        let mut next = BTreeMap::<String, TraversalPath>::new();
        let mut expanded = 0usize;
        'paths: for path in frontier.into_values() {
            let current = path.nodes.last().cloned().unwrap_or_default();
            for link in &graph.links {
                let Some((target, direction, inverse)) =
                    traverse_endpoint(graph, link, &current, &step.link_type, &step.direction)
                else {
                    continue;
                };
                if path.nodes.contains(&target) {
                    continue;
                }
                if expanded >= graph.max_nodes() {
                    budget_truncated = true;
                    break 'paths;
                }
                expanded += 1;
                let mut hop =
                    graph.link_json(link, direction, !inverse && graph.link_is_symmetric(link));
                hop["from"] = serde_json::json!(current);
                hop["to"] = serde_json::json!(target);
                hop["link_type"] = serde_json::json!(step.link_type);
                hop["stored_link_type"] = serde_json::json!(link.link_type);
                hop["via_declared_inverse"] = serde_json::json!(inverse);
                hop["from_object"] = graph
                    .object(&current)
                    .map(|object| graph.object_json(object))
                    .unwrap_or(serde_json::Value::Null);
                hop["to_object"] = graph
                    .object(&target)
                    .map(|object| graph.object_json(object))
                    .unwrap_or(serde_json::Value::Null);
                let mut nodes = path.nodes.clone();
                nodes.push(target.clone());
                let mut hops = path.hops.clone();
                hops.push(hop);
                let candidate = TraversalPath {
                    nodes,
                    hops,
                    confidence: (path.confidence * link.confidence).clamp(0.0, 1.0),
                };
                next.entry(target).or_insert(candidate);
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    let mut results = frontier
        .into_iter()
        .filter_map(|(id, path)| {
            let object = graph.object(&id)?;
            request
                .object_type
                .as_deref()
                .is_none_or(|key| {
                    ontology::type_matches(
                        &graph.ontology.ontology,
                        &object.object_type,
                        &[key.to_string()],
                    )
                })
                .then(|| {
                    let mut value = graph.object_json(object);
                    value["path"] = serde_json::json!(path.hops);
                    value["depth"] = serde_json::json!(path.nodes.len() - 1);
                    value["confidence"] = serde_json::json!(path.confidence);
                    value
                })
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let total = results.len();
    Ok((total, results, graph.truncated() || budget_truncated))
}

#[derive(Debug, Clone)]
struct TraversalPath {
    nodes: Vec<String>,
    hops: Vec<serde_json::Value>,
    confidence: f64,
}

fn traverse_endpoint(
    graph: &VisibleGraph,
    link: &OagLink,
    current: &str,
    requested_type: &str,
    requested_direction: &str,
) -> Option<(String, &'static str, bool)> {
    let to = link.to_object_id.as_deref()?;
    let direct = link.link_type == requested_type;
    let inverse = !direct
        && graph
            .ontology
            .ontology
            .link_types
            .iter()
            .find(|definition| definition.key == link.link_type)
            .and_then(|definition| definition.inverse.as_deref())
            == Some(requested_type);
    if !direct && !inverse {
        return None;
    }
    let symmetric = direct && graph.link_is_symmetric(link);
    let (target, physical_direction) = if link.from_object_id == current {
        (to, "out")
    } else if to == current {
        (link.from_object_id.as_str(), "in")
    } else {
        return None;
    };
    if !symmetric && requested_direction != "both" {
        let logical_direction = if inverse {
            if physical_direction == "out" {
                "in"
            } else {
                "out"
            }
        } else {
            physical_direction
        };
        if logical_direction != requested_direction {
            return None;
        }
    }
    let logical_direction = if inverse {
        if physical_direction == "out" {
            "in"
        } else {
            "out"
        }
    } else {
        physical_direction
    };
    Some((target.to_string(), logical_direction, inverse))
}

fn aggregate_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let objects = filtered_objects(graph, request)?;
    let metric = request.metric.as_deref().unwrap_or("count");
    let group_by = request.group_by.as_deref();
    let mut groups = BTreeMap::<String, AggregateGroup>::new();
    if group_by.is_none() {
        groups.insert("all".to_string(), AggregateGroup::default());
    }
    for object in objects {
        let mut object_groups = Vec::<(String, serde_json::Value)>::new();
        match group_by {
            None => object_groups.push(("all".to_string(), serde_json::Value::Null)),
            Some("object_type") => object_groups.push((
                serde_json::to_string(&object.object_type).unwrap_or_default(),
                serde_json::json!(object.object_type),
            )),
            Some("attribute") => {
                let key = request.group_attribute.as_deref().unwrap_or_default();
                let mut values = graph
                    .attributes_for(&object.id)
                    .into_iter()
                    .filter(|attribute| attribute.key == key)
                    .map(|attribute| attribute.value.clone())
                    .collect::<Vec<_>>();
                values.sort_by_key(|value| value.to_string());
                values.dedup();
                if values.is_empty() {
                    object_groups.push(("null".to_string(), serde_json::Value::Null));
                } else {
                    for value in values {
                        object_groups.push((value.to_string(), value));
                    }
                }
            }
            Some("link_target") => {
                let condition = request
                    .link
                    .as_ref()
                    .ok_or_else(|| "按关系目标分组必须提供 link 条件".to_string())?;
                let target = condition
                    .to
                    .as_ref()
                    .and_then(|reference| graph.resolve(reference).ok().flatten());
                for link in &graph.links {
                    let Some((id, _, _)) = traverse_endpoint(
                        graph,
                        link,
                        &object.id,
                        &condition.link_type,
                        &condition.direction,
                    ) else {
                        continue;
                    };
                    if target.as_ref().is_some_and(|target| target.id != id) {
                        continue;
                    }
                    object_groups.push((
                        serde_json::to_string(&id).unwrap_or_default(),
                        serde_json::json!(id),
                    ));
                }
            }
            _ => unreachable!(),
        }
        object_groups.sort_by(|left, right| left.0.cmp(&right.0));
        object_groups.dedup_by(|left, right| left.0 == right.0);
        for (key, value) in object_groups {
            let group = groups.entry(key).or_default();
            group.objects.insert(object.id.clone());
            if group.value.is_none() {
                group.value = Some(value);
            }
            if let Some(attribute_key) = request.metric_attribute.as_deref() {
                for attribute in graph.attributes_for(&object.id) {
                    if attribute.key == attribute_key {
                        if let Some(number) = attribute.value.as_f64() {
                            group.values.insert(attribute.id.clone(), number);
                        }
                    }
                }
            }
        }
    }
    let mut results = groups
        .into_iter()
        .map(|(key, group)| {
            let object_count = group.objects.len();
            let group_value = group.value.clone().unwrap_or(serde_json::Value::Null);
            let value = match metric {
                "count" | "count_distinct" => serde_json::json!(object_count),
                "sum" => serde_json::json!(group.values.values().sum::<f64>()),
                "avg" => {
                    let count = group.values.len();
                    serde_json::json!(if count == 0 {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!(group.values.values().sum::<f64>() / count as f64)
                    })
                }
                "min" => group
                    .values
                    .values()
                    .copied()
                    .reduce(f64::min)
                    .map(|value| serde_json::json!(value))
                    .unwrap_or(serde_json::Value::Null),
                "max" => group
                    .values
                    .values()
                    .copied()
                    .reduce(f64::max)
                    .map(|value| serde_json::json!(value))
                    .unwrap_or(serde_json::Value::Null),
                _ => unreachable!(),
            };
            let mut result = serde_json::json!({
                "metric": metric,
                "value": value,
                "object_count": object_count,
                "numeric_count": group.values.len(),
                "group_key": key,
            });
            if let Some(group_by) = group_by {
                result["group_by"] = serde_json::json!(group_by);
                result["group_value"] = group_value;
                if group_by == "attribute" {
                    result["group_attribute"] = serde_json::json!(request.group_attribute);
                }
            }
            result
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| left["group_key"].as_str().cmp(&right["group_key"].as_str()));
    let total = results.len();
    Ok((total, results, graph.truncated()))
}

#[derive(Debug, Default)]
struct AggregateGroup {
    objects: BTreeSet<String>,
    values: BTreeMap<String, f64>,
    value: Option<serde_json::Value>,
}

fn absence_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let target = match request.link.as_ref().and_then(|link| link.to.as_ref()) {
        Some(reference) => match graph.resolve(reference)? {
            Some(object) => Some(object),
            None => return Ok((0, Vec::new(), false)),
        },
        None => None,
    };
    let object_type = request
        .object_type
        .as_deref()
        .ok_or_else(|| "absence 查询必须提供 object_type".to_string())?;
    let mut objects = graph
        .objects
        .iter()
        .filter(|object| {
            ontology::type_matches(
                &graph.ontology.ontology,
                &object.object_type,
                &[object_type.to_string()],
            )
        })
        .filter(|object| {
            request.attribute.as_ref().is_none_or(|condition| {
                !graph
                    .attributes_for(&object.id)
                    .iter()
                    .any(|attribute| attribute_matches(attribute, condition))
            })
        })
        .filter(|object| {
            request.link.as_ref().is_none_or(|condition| {
                !graph.links.iter().any(|link| {
                    link.link_type == condition.link_type
                        && link_matches(
                            graph,
                            link,
                            &object.id,
                            &condition.direction,
                            target.as_ref().map(|target| target.id.as_str()),
                        )
                })
            })
        })
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| {
        left.name_normalized
            .cmp(&right.name_normalized)
            .then_with(|| left.id.cmp(&right.id))
    });
    let results = objects
        .into_iter()
        .map(|object| {
            let mut value = graph.object_json(object);
            value["assertion"] = serde_json::json!("no_visible_match");
            value["basis"] = serde_json::json!("current_visible_graph");
            value["temporal_scope"] = serde_json::json!(if request.include_history {
                "visible_history"
            } else {
                "current"
            });
            value["caveat"] = serde_json::json!(
                "仅表示当前视角可见子图中没有匹配记录，不代表从未发生或不存在隐藏信息"
            );
            value
        })
        .collect::<Vec<_>>();
    let total = results.len();
    Ok((total, results, graph.truncated()))
}

fn knows_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let requested_target = request
        .target
        .as_ref()
        .ok_or_else(|| "knows 查询缺少 target".to_string())?;
    let (target_kind, target_id, target) = if requested_target.kind == "object" {
        let object = graph.resolve(&OagEntityReference {
            id: Some(requested_target.id.clone()),
            name: None,
            object_type: None,
        })?;
        let Some(object) = object else {
            return Ok((0, Vec::new(), false));
        };
        (
            "object".to_string(),
            object.id.clone(),
            graph.object_json(&object),
        )
    } else {
        let Some(target) = visible_target(graph, &requested_target.kind, &requested_target.id)
        else {
            return Ok((0, Vec::new(), false));
        };
        (
            requested_target.kind.clone(),
            requested_target.id.clone(),
            target,
        )
    };
    let requested_actor = match request.knowledge_actor.as_ref() {
        Some(reference) => match graph.resolve(reference)? {
            Some(object) => Some(object),
            None => return Ok((0, Vec::new(), false)),
        },
        None => None,
    };
    if !graph.is_omniscient()
        && requested_actor
            .as_ref()
            .is_some_and(|actor| Some(actor.id.as_str()) != graph.actor_id())
    {
        return Err("角色视角只能查询自己的知识，不能查询其他角色的知情记录".to_string());
    }
    let actor_filter = requested_actor
        .as_ref()
        .map(|actor| actor.id.as_str())
        .or(graph.actor_id());
    let mut records = graph
        .knowledge
        .iter()
        .filter(|item| {
            item.target_kind == target_kind
                && item.target_id == target_id
                && actor_filter.is_none_or(|actor| item.actor_id == actor)
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        left.learned_turn
            .cmp(&right.learned_turn)
            .then_with(|| left.actor_id.cmp(&right.actor_id))
            .then_with(|| left.id.cmp(&right.id))
    });
    if records.is_empty() && !graph.is_omniscient() {
        let actor = graph
            .object(graph.actor_id().unwrap_or_default())
            .map(|object| graph.object_json(object))
            .unwrap_or(serde_json::Value::Null);
        return Ok((
            1,
            vec![serde_json::json!({
                "known": false,
                "actor": actor,
                "target": target,
                "reason": "当前视角没有该目标的知识记录",
            })],
            graph.truncated(),
        ));
    }
    let results = records
        .into_iter()
        .filter_map(|item| {
            let actor = graph.object(&item.actor_id)?;
            let source_event = item.source_event_id.as_ref().and_then(|id| {
                graph
                    .events
                    .iter()
                    .find(|event| &event.id == id)
                    .filter(|event| {
                        graph.event_access.get(&event.id) == Some(&"content".to_string())
                    })
                    .map(|event| event_json(graph, event))
            });
            let source_actor = item
                .source_actor_id
                .as_deref()
                .and_then(|id| graph.object(id))
                .map(|object| graph.object_json(object));
            Some(serde_json::json!({
                "known": true,
                "actor": graph.object_json(actor),
                "target": target,
                "knowledge_type": item.knowledge_type,
                "access_kind": item.access_kind,
                "learned_turn": item.learned_turn,
                "confidence": item.confidence,
                "source_event": source_event,
                "source_actor": source_actor,
                "citation": format!("第 {} 回合 / {}", item.learned_turn, item.knowledge_type),
            }))
        })
        .collect::<Vec<_>>();
    let total = results.len();
    Ok((total, results, graph.truncated()))
}

fn visible_target(graph: &VisibleGraph, kind: &str, id: &str) -> Option<serde_json::Value> {
    match kind {
        "object" => graph.object(id).map(|object| graph.object_json(object)),
        "attribute" => graph
            .attributes
            .iter()
            .find(|attribute| attribute.id == id)
            .map(|attribute| graph.attribute_json(attribute)),
        "link" => graph
            .links
            .iter()
            .find(|link| link.id == id)
            .map(|link| graph.link_json(link, "out", false)),
        "event" => graph
            .events
            .iter()
            .find(|event| event.id == id)
            .map(|event| event_json(graph, event)),
        "memory" => graph
            .memories
            .iter()
            .find(|memory| memory.memory["id"].as_str() == Some(id))
            .map(|memory| memory.memory.clone()),
        _ => None,
    }
}

fn link_matches(
    graph: &VisibleGraph,
    link: &OagLink,
    object_id: &str,
    direction: &str,
    target_id: Option<&str>,
) -> bool {
    let from_matches = link.from_object_id == object_id;
    let to_matches = link.to_object_id.as_deref() == Some(object_id);
    let symmetric = graph.link_is_symmetric(link);
    let target_matches = |endpoint: &str| target_id.is_none_or(|target| endpoint == target);
    match direction {
        "out" if from_matches => link.to_object_id.as_deref().is_some_and(&target_matches),
        "out" if symmetric && to_matches => target_matches(&link.from_object_id),
        "in" if to_matches => target_matches(&link.from_object_id),
        "in" if symmetric && from_matches => {
            link.to_object_id.as_deref().is_some_and(&target_matches)
        }
        "both" if from_matches => link.to_object_id.as_deref().is_some_and(&target_matches),
        "both" if to_matches => target_matches(&link.from_object_id),
        _ => false,
    }
}

fn attribute_matches(attribute: &OagAttribute, condition: &OagAttributeCondition) -> bool {
    if attribute.key != condition.key {
        return false;
    }
    match condition.op.as_str() {
        "eq" => attribute.value == condition.value,
        "ne" => attribute.value != condition.value,
        "gt" | "gte" | "lt" | "lte" => {
            if let (Some(actual), Some(expected)) =
                (attribute.value.as_f64(), condition.value.as_f64())
            {
                match condition.op.as_str() {
                    "gt" => actual > expected,
                    "gte" => actual >= expected,
                    "lt" => actual < expected,
                    "lte" => actual <= expected,
                    _ => false,
                }
            } else if let (Some(actual), Some(expected)) =
                (attribute.value.as_str(), condition.value.as_str())
            {
                match condition.op.as_str() {
                    "gt" => actual > expected,
                    "gte" => actual >= expected,
                    "lt" => actual < expected,
                    "lte" => actual <= expected,
                    _ => false,
                }
            } else {
                false
            }
        }
        "in" => condition
            .value
            .as_array()
            .is_some_and(|values| values.contains(&attribute.value)),
        "contains" => match (&attribute.value, &condition.value) {
            (serde_json::Value::String(actual), serde_json::Value::String(expected)) => {
                actual.contains(expected)
            }
            (serde_json::Value::Array(actual), expected) => actual.contains(expected),
            _ => false,
        },
        _ => false,
    }
}

fn timeline_results(
    graph: &VisibleGraph,
    request: &OagQueryRequest,
) -> Result<(usize, Vec<serde_json::Value>, bool), String> {
    let participant = match request.participant.as_ref() {
        Some(reference) => match graph.resolve(reference)? {
            Some(object) => Some(object),
            None => return Ok((0, Vec::new(), false)),
        },
        None => None,
    };
    let location = match request.location.as_ref() {
        Some(reference) => match graph.resolve(reference)? {
            Some(object) => Some(object),
            None => return Ok((0, Vec::new(), false)),
        },
        None => None,
    };
    let from_turn = request.from_turn.unwrap_or(0);
    let to_turn = request
        .to_turn
        .unwrap_or(graph.as_of_turn())
        .min(graph.as_of_turn());
    let mut events = graph
        .events
        .iter()
        .filter(|event| {
            event.turn_index >= from_turn
                && event.turn_index <= to_turn
                && request
                    .event_type
                    .as_deref()
                    .is_none_or(|event_type| event.event_type == event_type)
                && participant.as_ref().is_none_or(|participant| {
                    graph
                        .event_objects
                        .iter()
                        .any(|item| item.event_id == event.id && item.object_id == participant.id)
                })
                && location.as_ref().is_none_or(|location| {
                    graph.scenes.iter().any(|scene| {
                        scene.id == event.scene_id.as_deref().unwrap_or_default()
                            && scene.location_object_id.as_deref() == Some(location.id.as_str())
                    })
                })
                && request.mentioned.as_deref().is_none_or(|mentioned| {
                    graph.event_access.get(&event.id) == Some(&"content".to_string())
                        && format!("{} {} {}", event.title, event.summary, event.details)
                            .to_lowercase()
                            .contains(&mentioned.to_lowercase())
                })
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.ordinal.cmp(&right.ordinal))
            .then_with(|| left.id.cmp(&right.id))
    });
    let total = events.len();
    let results = events
        .into_iter()
        .map(|event| event_json(graph, event))
        .collect();
    Ok((total, results, graph.truncated()))
}

fn event_json(graph: &VisibleGraph, event: &OagEvent) -> serde_json::Value {
    if graph.event_access.get(&event.id) == Some(&"envelope".to_string()) {
        return serde_json::json!({
            "id": event.id,
            "turn": event.turn_index,
            "turn_index": event.turn_index,
            "ordinal": event.ordinal,
            "event_type": event.event_type,
            "title": event.title,
            "source": "system",
            "confidence": 1.0,
            "citation": format!("第 {} 回合 / 系统记录", event.turn_index),
            "access_kind": "envelope",
        });
    }
    let participants = graph
        .event_objects
        .iter()
        .filter(|item| item.event_id == event.id)
        .filter_map(|item| {
            graph.object(&item.object_id).map(|object| {
                serde_json::json!({
                    "id": object.id,
                    "name": object.name,
                    "role": item.role,
                })
            })
        })
        .collect::<Vec<_>>();
    let scene = event
        .scene_id
        .as_ref()
        .and_then(|id| graph.scenes.iter().find(|scene| &scene.id == id));
    let mut value = serde_json::json!({
        "id": event.id,
        "turn": event.turn_index,
        "turn_index": event.turn_index,
        "ordinal": event.ordinal,
        "event_type": event.event_type,
        "title": event.title,
        "summary": event.summary,
        "details": event.details,
        "source_message_id": event.source_message_id,
        "source": "system",
        "confidence": 1.0,
        "citation": format!("第 {} 回合 / 系统记录", event.turn_index),
        "visibility": event.visibility,
        "importance": event.importance,
        "truth_status": event.truth_status,
        "created_at": event.created_at,
        "access_kind": "content",
        "participants": participants,
        "scene": scene.map(|scene| serde_json::json!({
            "id": scene.id,
            "runtime_scene_id": scene.runtime_scene_id,
            "name": scene.name,
        })),
    });
    add_provenance(
        &mut value,
        "system",
        event.turn_index,
        truth_confidence(&event.truth_status),
    );
    value
}

fn truth_confidence(truth_status: &str) -> f64 {
    if truth_status == "asserted" {
        0.7
    } else {
        1.0
    }
}

fn add_provenance(value: &mut serde_json::Value, source: &str, turn: i32, confidence: f64) {
    let source_label = match source {
        "author" => "作者设定",
        "runtime" => "运行记录",
        "system" => "系统记录",
        other => other,
    };
    value["source"] = serde_json::json!(source);
    value["turn"] = serde_json::json!(turn);
    value["confidence"] = serde_json::json!(confidence);
    value["citation"] = serde_json::json!(format!("第 {turn} 回合 / {source_label}"));
}

fn actor_trace_id(actor: &OagActor) -> Option<String> {
    match actor {
        OagActor::Object(id) => Some(format!("object:{id}")),
        OagActor::Player(id) => Some(format!("player:{id}")),
        OagActor::Director => Some("director".to_string()),
        OagActor::Author => Some("author".to_string()),
    }
}

fn record_query_trace(
    conn: &Connection,
    context: &OagQueryContext,
    request: &OagQueryRequest,
    graph: &VisibleGraph,
    response: &OagQueryResponse,
    started: std::time::Instant,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO oag_query_traces
         (id, world_id, session_id, turn_index, actor_id, as_of_turn, query_json,
          visible_nodes, filtered_count, result_json, latency_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            response.trace_id,
            context.world_id,
            context.session_id,
            context.as_of_turn,
            actor_trace_id(&context.actor),
            context.as_of_turn,
            serde_json::to_string(request).map_err(|error| error.to_string())?,
            graph.node_count() as i32,
            graph.filtered_count as i32,
            serde_json::to_string(response).map_err(|error| error.to_string())?,
            started.elapsed().as_millis() as i64,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn record_failed_query_trace(
    conn: &Connection,
    context: &OagQueryContext,
    request: &OagQueryRequest,
    started: std::time::Instant,
    error: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO oag_query_traces
         (id, world_id, session_id, turn_index, actor_id, as_of_turn, query_json,
          visible_nodes, filtered_count, result_json, latency_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, ?9, ?10)",
        params![
            uuid::Uuid::new_v4().to_string(),
            context.world_id,
            context.session_id,
            context.as_of_turn,
            actor_trace_id(&context.actor),
            context.as_of_turn,
            serde_json::to_string(request).map_err(|error| error.to_string())?,
            serde_json::to_string(&serde_json::json!({ "ok": false, "error": error }))
                .map_err(|error| error.to_string())?,
            started.elapsed().as_millis() as i64,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{oag_schema, schema};
    use crate::models::memory::{MemoryEntry, MemoryQueryParams};
    use crate::models::oag::{
        OagAttributeDefinition, OagKnowledgeTarget, OagLinkCondition, OagLinkTypeDefinition,
        OagObjectTypeDefinition, OagOntology, OagTraverseStep,
    };

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn
    }

    fn ontology() -> ResolvedOntology {
        ontology::resolve(Some(&OagOntology {
            object_types: vec![
                OagObjectTypeDefinition {
                    key: "person".to_string(),
                    label: "人物".to_string(),
                    parent: None,
                    actor: true,
                    platform_role: String::new(),
                    attributes: vec![OagAttributeDefinition {
                        key: "status".to_string(),
                        label: "状态".to_string(),
                        value_type: "string".to_string(),
                        required: false,
                        default: serde_json::Value::Null,
                        enum_options: Vec::new(),
                        functional: true,
                        temporal: true,
                    }],
                },
                OagObjectTypeDefinition {
                    key: "item".to_string(),
                    label: "物品".to_string(),
                    parent: None,
                    actor: false,
                    platform_role: String::new(),
                    attributes: Vec::new(),
                },
                OagObjectTypeDefinition {
                    key: "place".to_string(),
                    label: "地点".to_string(),
                    parent: None,
                    actor: false,
                    platform_role: "location".to_string(),
                    attributes: Vec::new(),
                },
            ],
            link_types: vec![
                OagLinkTypeDefinition {
                    key: "holds".to_string(),
                    label: "持有".to_string(),
                    domain: vec!["person".to_string()],
                    range: vec!["item".to_string()],
                    functional: false,
                    symmetric: false,
                    transitive: false,
                    temporal: true,
                    inverse: None,
                },
                OagLinkTypeDefinition {
                    key: "member_of".to_string(),
                    label: "所属".to_string(),
                    domain: vec!["person".to_string(), "place".to_string()],
                    range: vec!["place".to_string()],
                    functional: false,
                    symmetric: false,
                    transitive: false,
                    temporal: true,
                    inverse: Some("has_member".to_string()),
                },
                OagLinkTypeDefinition {
                    key: "has_member".to_string(),
                    label: "拥有成员".to_string(),
                    domain: vec!["place".to_string()],
                    range: vec!["person".to_string()],
                    functional: false,
                    symmetric: false,
                    transitive: false,
                    temporal: true,
                    inverse: Some("member_of".to_string()),
                },
                OagLinkTypeDefinition {
                    key: "allied".to_string(),
                    label: "结盟".to_string(),
                    domain: vec!["person".to_string()],
                    range: vec!["person".to_string()],
                    functional: false,
                    symmetric: true,
                    transitive: false,
                    temporal: true,
                    inverse: None,
                },
            ],
            character_object_type: Some("person".to_string()),
            location_object_type: Some("place".to_string()),
        }))
        .unwrap()
    }

    fn object(
        world: &str,
        session: &str,
        id: &str,
        name: &str,
        object_type: &str,
        visibility: &str,
        asserted_by: Option<&str>,
        created_turn: i32,
    ) -> OagObject {
        OagObject {
            id: id.to_string(),
            world_id: world.to_string(),
            session_id: session.to_string(),
            object_type: object_type.to_string(),
            canonical_key: format!("key:{id}"),
            name: name.to_string(),
            name_normalized: normalize_name(name),
            aliases: Vec::new(),
            description: String::new(),
            source: if visibility == "public" {
                "author"
            } else {
                "runtime"
            }
            .to_string(),
            visibility: visibility.to_string(),
            asserted_by_id: asserted_by.map(str::to_string),
            source_event_id: None,
            truth_status: "confirmed".to_string(),
            created_turn,
            updated_turn: created_turn,
            retired_at: String::new(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn context(actor: OagActor, turn: i32) -> OagQueryContext {
        OagQueryContext {
            world_id: "world".to_string(),
            session_id: "session".to_string(),
            actor,
            as_of_turn: turn,
            max_nodes: 200,
        }
    }

    fn request(op: &str) -> OagQueryRequest {
        OagQueryRequest {
            op: op.to_string(),
            entity: None,
            object_type: None,
            attribute: None,
            link: None,
            steps: Vec::new(),
            metric: None,
            metric_attribute: None,
            group_by: None,
            group_attribute: None,
            knowledge_actor: None,
            target: None,
            mentioned: None,
            as_of_turn: None,
            from_turn: None,
            to_turn: None,
            event_type: None,
            participant: None,
            location: None,
            include_history: false,
            include_retired: false,
            limit: 20,
        }
    }

    fn reference(id: &str) -> OagEntityReference {
        OagEntityReference {
            id: Some(id.to_string()),
            name: None,
            object_type: None,
        }
    }

    fn knowledge(
        conn: &Connection,
        actor: &str,
        kind: &str,
        target: &str,
        access: &str,
        learned: i32,
    ) {
        conn.execute(
            "INSERT INTO oag_knowledge
             (id, world_id, session_id, actor_id, target_kind, target_id, knowledge_type,
              access_kind, source_event_id, source_actor_id, learned_turn, confidence, created_at)
             VALUES (?1, 'world', 'session', ?2, ?3, ?4, 'witnessed', ?5, NULL, NULL, ?6, 1.0, 'now')
             ON CONFLICT DO NOTHING",
            params![format!("knowledge:{actor}:{kind}:{target}:{access}"), actor, kind, target, access, learned],
        )
        .unwrap();
    }

    fn attribute(
        conn: &Connection,
        id: &str,
        object_id: &str,
        key: &str,
        value: serde_json::Value,
        from: i32,
        to: Option<i32>,
        visibility: &str,
        asserted_by: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO oag_attributes
             (id, world_id, session_id, object_id, key, value_json, value_type,
              valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
              source_event_id, visibility, truth_status, supersedes_id, created_at)
             VALUES (?1, 'world', 'session', ?2, ?3, ?4, 'string', ?5, ?6, 'runtime', 0.8,
                     ?7, NULL, ?8, 'asserted', NULL, 'now')",
            params![
                id,
                object_id,
                key,
                serde_json::to_string(&value).unwrap(),
                from,
                to,
                asserted_by,
                visibility
            ],
        )
        .unwrap();
    }

    fn link(
        conn: &Connection,
        id: &str,
        from: &str,
        link_type: &str,
        to: Option<&str>,
        from_turn: i32,
        to_turn: Option<i32>,
        visibility: &str,
    ) {
        conn.execute(
            "INSERT INTO oag_links
             (id, world_id, session_id, from_object_id, link_type, to_object_id, to_value,
              original_predicate, attributes_json, valid_from_turn, valid_to_turn, source,
              confidence, asserted_by_id, source_event_id, visibility, truth_status, inferred,
              supersedes_id, created_at)
             VALUES (?1, 'world', 'session', ?2, ?3, ?4, '', '', '{}', ?5, ?6, 'runtime', 0.9,
                     NULL, NULL, ?7, 'asserted', 0, NULL, 'now')",
            params![id, from, link_type, to, from_turn, to_turn, visibility],
        )
        .unwrap();
    }

    fn event(
        conn: &Connection,
        id: &str,
        turn: i32,
        event_type: &str,
        scene_id: Option<&str>,
        details: serde_json::Value,
    ) {
        conn.execute(
            "INSERT INTO oag_events
             (id, world_id, session_id, turn_index, ordinal, scene_id, event_type, title,
              summary, details_json, source_message_id, dedupe_key, visibility, importance,
              truth_status, created_at)
             VALUES (?1, 'world', 'session', ?2, 0, ?3, ?4, ?4, '', ?5, ?1, ?1, 'scene', 0.5,
                     'confirmed', 'now')",
            params![
                id,
                turn,
                scene_id,
                event_type,
                serde_json::to_string(&details).unwrap()
            ],
        )
        .unwrap();
    }

    fn scene(conn: &Connection, id: &str, location: &str, from: i32) {
        conn.execute(
            "INSERT INTO oag_scenes
             (id, world_id, session_id, runtime_scene_id, name, location_object_id,
              background_hint, valid_from_turn, valid_to_turn, created_at)
             VALUES (?1, 'world', 'session', ?1, ?1, ?2, '', ?3, NULL, 'now')",
            params![id, location, from],
        )
        .unwrap();
    }

    #[test]
    fn traverse_respects_declared_inverse_symmetry_cycles_and_visibility() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
            object("world", "", "carol", "Carol", "person", "public", None, 0),
            object("world", "", "guild", "Guild", "place", "public", None, 0),
            object("world", "", "key", "Key", "item", "public", None, 0),
            object(
                "world", "session", "secret", "Secret", "item", "scene", None, 0,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        for (id, from, link_type, to) in [
            ("membership", "alice", "member_of", "guild"),
            ("guild-bob", "guild", "has_member", "bob"),
            ("alliance", "bob", "allied", "alice"),
            ("alliance-carol", "bob", "allied", "carol"),
            ("cycle", "carol", "allied", "alice"),
            ("visible-key", "alice", "holds", "key"),
            ("hidden-key", "alice", "holds", "secret"),
        ] {
            link(&conn, id, from, link_type, Some(to), 0, None, "public");
        }
        let inverse = request("traverse");
        let inverse = OagQueryRequest {
            entity: Some(reference("alice")),
            steps: vec![
                OagTraverseStep {
                    link_type: "member_of".to_string(),
                    direction: "out".to_string(),
                },
                OagTraverseStep {
                    link_type: "has_member".to_string(),
                    direction: "out".to_string(),
                },
            ],
            ..inverse
        };
        let response = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            inverse,
        )
        .unwrap();
        assert_eq!(response.count, 1, "{:#?}", response.results);
        assert_eq!(response.results[0]["id"], "bob");
        assert_eq!(
            response.results[0]["path"][0]["via_declared_inverse"],
            false
        );
        assert_eq!(response.results[0]["depth"], 2);
        assert!(response.results[0]["path"][0]["citation"]
            .as_str()
            .is_some_and(|citation| citation.contains("第 0 回合")));

        let inverse_step = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            OagQueryRequest {
                op: "traverse".to_string(),
                entity: Some(reference("guild")),
                steps: vec![OagTraverseStep {
                    link_type: "has_member".to_string(),
                    direction: "out".to_string(),
                }],
                ..request("traverse")
            },
        )
        .unwrap();
        assert_eq!(inverse_step.results[0]["id"], "alice");
        assert_eq!(
            inverse_step.results[0]["path"][0]["via_declared_inverse"],
            true
        );

        let symmetric = OagQueryRequest {
            op: "traverse".to_string(),
            entity: Some(reference("alice")),
            steps: vec![OagTraverseStep {
                link_type: "allied".to_string(),
                direction: "out".to_string(),
            }],
            ..request("traverse")
        };
        let response = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            symmetric,
        )
        .unwrap();
        assert_eq!(response.results[0]["id"], "bob");
        assert_eq!(response.results[0]["path"][0]["symmetric"], true);

        let hidden = OagQueryRequest {
            op: "traverse".to_string(),
            entity: Some(reference("alice")),
            steps: vec![OagTraverseStep {
                link_type: "holds".to_string(),
                direction: "out".to_string(),
            }],
            ..request("traverse")
        };
        let response = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved,
            hidden,
        )
        .unwrap();
        assert_eq!(response.count, 1);
        assert_eq!(response.results[0]["id"], "key");
        assert!(!serde_json::to_string(&response.results)
            .unwrap()
            .contains("secret"));
    }

    #[test]
    fn aggregate_counts_only_visible_objects_and_supports_metrics_and_groups() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "a", "A", "item", "public", None, 0),
            object("world", "", "b", "B", "item", "public", None, 0),
            object(
                "world", "session", "secret", "Secret", "item", "scene", None, 0,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        for (id, object_id, key, value) in [
            ("score-a", "a", "score", serde_json::json!(10)),
            ("score-b", "b", "score", serde_json::json!(20)),
            ("score-secret", "secret", "score", serde_json::json!(99)),
            ("rank-a", "a", "rank", serde_json::json!("first")),
            ("rank-b", "b", "rank", serde_json::json!("second")),
        ] {
            attribute(&conn, id, object_id, key, value, 0, None, "public", None);
        }
        link(
            &conn,
            "holds-a",
            "alice",
            "holds",
            Some("a"),
            0,
            None,
            "public",
        );
        link(
            &conn,
            "holds-b",
            "alice",
            "holds",
            Some("b"),
            0,
            None,
            "public",
        );
        let actor = || context(OagActor::Object("alice".to_string()), 1);
        for (metric, expected) in [
            ("count", 2.0),
            ("count_distinct", 2.0),
            ("sum", 30.0),
            ("avg", 15.0),
            ("min", 10.0),
            ("max", 20.0),
        ] {
            let response = query(
                &conn,
                actor(),
                resolved.clone(),
                OagQueryRequest {
                    object_type: Some("item".to_string()),
                    metric: Some(metric.to_string()),
                    metric_attribute: Some("score".to_string()),
                    ..request("aggregate")
                },
            )
            .unwrap();
            assert_eq!(response.count, 1);
            assert_eq!(response.results[0]["value"], expected);
            assert_eq!(response.results[0]["object_count"], 2);
        }
        let grouped = query(
            &conn,
            actor(),
            resolved.clone(),
            OagQueryRequest {
                object_type: Some("item".to_string()),
                metric: Some("sum".to_string()),
                metric_attribute: Some("score".to_string()),
                group_by: Some("attribute".to_string()),
                group_attribute: Some("rank".to_string()),
                ..request("aggregate")
            },
        )
        .unwrap();
        assert_eq!(grouped.count, 2);
        assert_eq!(grouped.results[0]["group_value"], "first");
        assert_eq!(grouped.results[0]["value"], serde_json::json!(10.0));
        assert_eq!(grouped.results[1]["group_value"], "second");
        assert_eq!(grouped.results[1]["value"], serde_json::json!(20.0));

        let by_target = query(
            &conn,
            actor(),
            resolved.clone(),
            OagQueryRequest {
                entity: Some(reference("alice")),
                link: Some(OagLinkCondition {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                    to: None,
                }),
                group_by: Some("link_target".to_string()),
                ..request("aggregate")
            },
        )
        .unwrap();
        assert_eq!(by_target.count, 2);
        assert_eq!(by_target.results[0]["group_value"], "a");
        assert_eq!(by_target.results[1]["group_value"], "b");
        let author = query(
            &conn,
            context(OagActor::Author, 1),
            resolved,
            OagQueryRequest {
                object_type: Some("item".to_string()),
                metric: Some("sum".to_string()),
                metric_attribute: Some("score".to_string()),
                ..request("aggregate")
            },
        )
        .unwrap();
        assert_eq!(author.results[0]["value"], serde_json::json!(129.0));
    }

    #[test]
    fn absence_is_scoped_to_visible_matches_and_never_claims_universal_truth() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "sword", "Sword", "item", "public", None, 0),
            object("world", "", "shield", "Shield", "item", "public", None, 0),
            object(
                "world", "session", "secret", "Secret", "item", "scene", None, 0,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        link(
            &conn,
            "sword-owner",
            "alice",
            "holds",
            Some("sword"),
            0,
            None,
            "public",
        );
        let response = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            OagQueryRequest {
                object_type: Some("item".to_string()),
                link: Some(OagLinkCondition {
                    link_type: "holds".to_string(),
                    direction: "in".to_string(),
                    to: Some(reference("alice")),
                }),
                ..request("absence")
            },
        )
        .unwrap();
        assert_eq!(response.count, 1, "{:#?}", response.results);
        assert_eq!(response.results[0]["id"], "shield");
        assert_eq!(response.results[0]["assertion"], "no_visible_match");
        assert!(response.results[0]["caveat"]
            .as_str()
            .is_some_and(|text| text.contains("从未")));
        assert!(!serde_json::to_string(&response.results)
            .unwrap()
            .contains("secret"));
        let hidden_target = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved,
            OagQueryRequest {
                object_type: Some("item".to_string()),
                link: Some(OagLinkCondition {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                    to: Some(reference("secret")),
                }),
                ..request("absence")
            },
        )
        .unwrap();
        assert_eq!(hidden_target.count, 0);
    }

    #[test]
    fn knows_is_actor_private_time_aware_and_redacts_envelope_sources() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
            object("world", "", "target", "Target", "item", "public", None, 0),
            object(
                "world",
                "",
                "future-target",
                "Future Target",
                "item",
                "public",
                None,
                0,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        attribute(
            &conn,
            "score",
            "target",
            "score",
            serde_json::json!(7),
            0,
            None,
            "scene",
            None,
        );
        event(
            &conn,
            "whisper",
            1,
            "whisper",
            None,
            serde_json::json!({"secret": "needle"}),
        );
        knowledge(&conn, "alice", "object", "target", "content", 1);
        knowledge(&conn, "bob", "object", "target", "content", 2);
        knowledge(&conn, "alice", "object", "future-target", "content", 4);
        knowledge(&conn, "alice", "event", "whisper", "envelope", 1);
        let target = || OagKnowledgeTarget {
            kind: "object".to_string(),
            id: "target".to_string(),
        };
        let own = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            OagQueryRequest {
                target: Some(target()),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(own.count, 1);
        assert_eq!(own.results[0]["known"], true);
        assert_eq!(own.results[0]["learned_turn"], 1);
        assert!(query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            OagQueryRequest {
                knowledge_actor: Some(reference("bob")),
                target: Some(target()),
                ..request("knows")
            },
        )
        .is_err());
        let director = query(
            &conn,
            context(OagActor::Director, 3),
            resolved.clone(),
            OagQueryRequest {
                target: Some(target()),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(director.count, 2);
        assert!(director
            .results
            .iter()
            .all(|result| result["known"] == true));
        let future = query(
            &conn,
            context(OagActor::Director, 3),
            resolved.clone(),
            OagQueryRequest {
                target: Some(OagKnowledgeTarget {
                    kind: "object".to_string(),
                    id: "future-target".to_string(),
                }),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(future.count, 0);

        let envelope_target = OagKnowledgeTarget {
            kind: "event".to_string(),
            id: "whisper".to_string(),
        };
        let envelope = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            OagQueryRequest {
                target: Some(envelope_target.clone()),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(envelope.results[0]["access_kind"], "envelope");
        assert!(envelope.results[0]["source_event"].is_null());
        let timeline = OagQueryRequest {
            mentioned: Some("NEEDLE".to_string()),
            ..request("timeline")
        };
        let actor_timeline = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            timeline.clone(),
        )
        .unwrap();
        assert_eq!(actor_timeline.count, 0);
        let author_timeline = query(
            &conn,
            context(OagActor::Author, 3),
            resolved.clone(),
            timeline,
        )
        .unwrap();
        assert_eq!(author_timeline.count, 1);

        let score_target = OagKnowledgeTarget {
            kind: "attribute".to_string(),
            id: "score".to_string(),
        };
        let hidden = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            OagQueryRequest {
                target: Some(score_target.clone()),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(hidden.count, 0);
        knowledge(&conn, "alice", "attribute", "score", "content", 3);
        let learned = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved,
            OagQueryRequest {
                target: Some(score_target),
                ..request("knows")
            },
        )
        .unwrap();
        assert_eq!(learned.results[0]["known"], true);
        assert_eq!(learned.results[0]["learned_turn"], 3);
    }

    #[test]
    fn logical_query_validation_rejects_unbounded_or_ambiguous_requests() {
        let conn = database();
        let resolved = ontology();
        let too_deep = OagQueryRequest {
            entity: Some(reference("alice")),
            steps: vec![
                OagTraverseStep {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                },
                OagTraverseStep {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                },
                OagTraverseStep {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                },
                OagTraverseStep {
                    link_type: "holds".to_string(),
                    direction: "out".to_string(),
                },
            ],
            ..request("traverse")
        };
        assert!(query(
            &conn,
            context(OagActor::Author, 1),
            resolved.clone(),
            too_deep
        )
        .is_err());
        assert!(query(
            &conn,
            context(OagActor::Author, 1),
            resolved.clone(),
            OagQueryRequest {
                object_type: Some("item".to_string()),
                ..request("absence")
            },
        )
        .is_err());
        assert!(query(
            &conn,
            context(OagActor::Author, 1),
            resolved,
            OagQueryRequest {
                metric: Some("sum".to_string()),
                ..request("aggregate")
            },
        )
        .is_err());
    }

    #[test]
    fn knowledge_time_private_claims_and_unknown_visibility_fail_closed() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
            object(
                "world", "session", "secret", "Secret", "item", "scene", None, 1,
            ),
            object(
                "world", "session", "future", "Future", "item", "public", None, 5,
            ),
            object(
                "world",
                "session",
                "bob-private",
                "Bob Private",
                "item",
                "private",
                Some("bob"),
                1,
            ),
            object(
                "world", "session", "unknown", "Unknown", "item", "mystery", None, 1,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        attribute(
            &conn,
            "attr-secret",
            "secret",
            "status",
            serde_json::json!("hidden"),
            1,
            None,
            "scene",
            None,
        );
        attribute(
            &conn,
            "attr-private",
            "alice",
            "status",
            serde_json::json!("mine"),
            1,
            None,
            "private",
            Some("alice"),
        );
        attribute(
            &conn,
            "attr-unknown",
            "alice",
            "status",
            serde_json::json!("leak"),
            1,
            None,
            "mystery",
            None,
        );
        link(
            &conn,
            "link-secret",
            "secret",
            "allied",
            Some("bob"),
            1,
            None,
            "scene",
        );
        knowledge(&conn, "alice", "object", "secret", "content", 3);
        knowledge(&conn, "alice", "attribute", "attr-secret", "content", 4);
        knowledge(&conn, "alice", "link", "link-secret", "content", 4);

        let early = VisibleGraph::load(
            &conn,
            context(OagActor::Object("alice".to_string()), 2),
            resolved.clone(),
            false,
            false,
        )
        .unwrap();
        assert!(early.object("secret").is_none());
        assert!(early.object("future").is_none());
        assert!(early.object("bob-private").is_none());
        assert!(early.object("unknown").is_none());

        let middle = VisibleGraph::load(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            false,
            false,
        )
        .unwrap();
        assert!(middle.object("secret").is_some());
        assert!(middle
            .attributes
            .iter()
            .all(|item| item.id != "attr-secret"));
        assert!(middle.links.iter().all(|item| item.id != "link-secret"));
        assert!(middle
            .attributes
            .iter()
            .any(|item| item.id == "attr-private"));
        assert!(middle
            .attributes
            .iter()
            .all(|item| item.id != "attr-unknown"));

        let late = VisibleGraph::load(
            &conn,
            context(OagActor::Object("alice".to_string()), 4),
            resolved.clone(),
            false,
            false,
        )
        .unwrap();
        assert!(late.attributes.iter().any(|item| item.id == "attr-secret"));
        assert!(late.links.iter().any(|item| item.id == "link-secret"));
        let bob = VisibleGraph::load(
            &conn,
            context(OagActor::Object("bob".to_string()), 4),
            resolved,
            false,
            false,
        )
        .unwrap();
        assert!(bob.object("bob-private").is_some());
        assert!(bob.object("secret").is_none());
    }

    #[test]
    fn half_open_history_and_as_of_find_are_deterministic() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        repo.upsert_object(&object(
            "world", "", "alice", "Alice", "person", "public", None, 0,
        ))
        .unwrap();
        repo.upsert_object(&object(
            "world", "", "key", "Key", "item", "public", None, 0,
        ))
        .unwrap();
        attribute(
            &conn,
            "status-old",
            "alice",
            "status",
            serde_json::json!("old"),
            1,
            Some(3),
            "public",
            None,
        );
        attribute(
            &conn,
            "status-new",
            "alice",
            "status",
            serde_json::json!("new"),
            3,
            None,
            "public",
            None,
        );
        link(
            &conn,
            "holds-old",
            "alice",
            "holds",
            Some("key"),
            1,
            Some(3),
            "public",
        );
        link(
            &conn,
            "holds-new",
            "alice",
            "holds",
            Some("key"),
            3,
            None,
            "public",
        );

        let at_two = VisibleGraph::load(
            &conn,
            context(OagActor::Author, 2),
            resolved.clone(),
            false,
            false,
        )
        .unwrap();
        assert_eq!(at_two.attributes[0].id, "status-old");
        assert_eq!(at_two.links[0].id, "holds-old");
        let history = VisibleGraph::load(
            &conn,
            context(OagActor::Author, 3),
            resolved.clone(),
            false,
            true,
        )
        .unwrap();
        assert_eq!(history.attributes.len(), 2);
        assert_eq!(history.links.len(), 2);
        let old = history
            .attributes
            .iter()
            .find(|item| item.id == "status-old")
            .unwrap();
        assert_eq!(history.attribute_json(old)["invalid_at_turn"], 3);

        let mut find = request("find");
        find.object_type = Some("person".to_string());
        find.attribute = Some(OagAttributeCondition {
            key: "status".to_string(),
            op: "eq".to_string(),
            value: serde_json::json!("old"),
        });
        find.as_of_turn = Some(2);
        assert_eq!(
            query(
                &conn,
                context(OagActor::Author, 3),
                resolved.clone(),
                find.clone()
            )
            .unwrap()
            .count,
            1
        );
        find.as_of_turn = Some(3);
        assert_eq!(
            query(
                &conn,
                context(OagActor::Author, 3),
                resolved.clone(),
                find.clone()
            )
            .unwrap()
            .count,
            0
        );
        find.include_history = true;
        assert_eq!(
            query(&conn, context(OagActor::Author, 3), resolved, find)
                .unwrap()
                .count,
            1
        );
    }

    #[test]
    fn whisper_envelope_hides_content_participants_and_scene() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for id in ["alice", "bob", "carol"] {
            repo.upsert_object(&object("world", "", id, id, "person", "public", None, 0))
                .unwrap();
        }
        repo.upsert_object(&object(
            "world", "", "dock", "Dock", "place", "public", None, 0,
        ))
        .unwrap();
        scene(&conn, "scene-1", "dock", 1);
        event(
            &conn,
            "event-1",
            1,
            "whisper_envelope",
            Some("scene-1"),
            serde_json::json!({ "secret": "do not leak" }),
        );
        conn.execute("INSERT INTO oag_event_objects (event_id, object_id, role) VALUES ('event-1', 'alice', 'speaker')", []).unwrap();
        conn.execute("INSERT INTO oag_event_objects (event_id, object_id, role) VALUES ('event-1', 'bob', 'recipient')", []).unwrap();
        knowledge(&conn, "alice", "event", "event-1", "content", 1);
        knowledge(&conn, "bob", "event", "event-1", "envelope", 1);

        let timeline = request("timeline");
        let alice = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 1),
            resolved.clone(),
            timeline.clone(),
        )
        .unwrap();
        assert!(alice.results[0]["details"]["secret"].is_string());
        let bob = query(
            &conn,
            context(OagActor::Object("bob".to_string()), 1),
            resolved.clone(),
            timeline.clone(),
        )
        .unwrap();
        assert_eq!(bob.results[0]["access_kind"], "envelope");
        assert!(bob.results[0].get("details").is_none());
        assert!(bob.results[0].get("participants").is_none());
        assert!(bob.results[0].get("scene").is_none());
        assert!(bob.results[0].get("source_message_id").is_none());
        assert!(!bob.results[0].to_string().contains("do not leak"));
        let carol = query(
            &conn,
            context(OagActor::Object("carol".to_string()), 1),
            resolved,
            timeline,
        )
        .unwrap();
        assert_eq!(carol.count, 0);
    }

    #[test]
    fn find_uses_direction_symmetry_and_hidden_targets_return_empty() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
            object("world", "", "key", "Key", "item", "public", None, 0),
            object(
                "world", "session", "hidden", "Hidden", "item", "scene", None, 1,
            ),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        link(
            &conn,
            "allied-1",
            "alice",
            "allied",
            Some("bob"),
            1,
            None,
            "public",
        );
        link(
            &conn,
            "holds-1",
            "alice",
            "holds",
            Some("key"),
            1,
            None,
            "public",
        );
        link(
            &conn,
            "holds-hidden",
            "alice",
            "holds",
            Some("hidden"),
            1,
            None,
            "public",
        );
        knowledge(&conn, "alice", "link", "allied-1", "content", 1);
        knowledge(&conn, "alice", "link", "holds-1", "content", 1);
        knowledge(&conn, "alice", "link", "holds-hidden", "content", 1);

        let mut find = request("find");
        find.object_type = Some("person".to_string());
        find.link = Some(crate::models::oag::OagLinkCondition {
            link_type: "allied".to_string(),
            direction: "out".to_string(),
            to: Some(reference("bob")),
        });
        let result = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 1),
            resolved.clone(),
            find.clone(),
        )
        .unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0]["id"], "alice");
        find.link.as_mut().unwrap().to = Some(reference("hidden"));
        assert_eq!(
            query(
                &conn,
                context(OagActor::Object("alice".to_string()), 1),
                resolved.clone(),
                find
            )
            .unwrap()
            .count,
            0
        );

        let mut entity = request("entity");
        entity.entity = Some(reference("bob"));
        let card = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 1),
            resolved,
            entity,
        )
        .unwrap();
        assert_eq!(card.results[0]["links"][0]["direction"], "out");
        assert_eq!(card.results[0]["links"][0]["symmetric"], true);
    }

    #[test]
    fn timeline_filters_and_memories_respect_actor_and_as_of() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
            object("world", "", "dock", "Dock", "place", "public", None, 0),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        repo.record_legacy_map(&crate::models::oag::OagLegacyIdMap {
            source_table: "characters".to_string(),
            source_id: "world:canon:characters:alice".to_string(),
            world_id: "world".to_string(),
            session_id: String::new(),
            target_table: "oag_objects".to_string(),
            target_id: "alice".to_string(),
        })
        .unwrap();
        repo.record_legacy_map(&crate::models::oag::OagLegacyIdMap {
            source_table: "characters".to_string(),
            source_id: "world:canon:characters:bob".to_string(),
            world_id: "world".to_string(),
            session_id: String::new(),
            target_table: "oag_objects".to_string(),
            target_id: "bob".to_string(),
        })
        .unwrap();
        repo.record_legacy_map(&crate::models::oag::OagLegacyIdMap {
            source_table: "characters".to_string(),
            source_id: "world:canon:characters:legacy-alice".to_string(),
            world_id: "world".to_string(),
            session_id: String::new(),
            target_table: "oag_objects".to_string(),
            target_id: "alice".to_string(),
        })
        .unwrap();
        scene(&conn, "scene-1", "dock", 1);
        event(
            &conn,
            "event-1",
            1,
            "dialogue",
            Some("scene-1"),
            serde_json::json!({}),
        );
        event(
            &conn,
            "event-2",
            2,
            "combat",
            Some("scene-1"),
            serde_json::json!({}),
        );
        event(
            &conn,
            "event-future",
            4,
            "combat",
            Some("scene-1"),
            serde_json::json!({}),
        );
        conn.execute("INSERT INTO oag_event_objects (event_id, object_id, role) VALUES ('event-1', 'alice', 'speaker')", []).unwrap();
        conn.execute("INSERT INTO oag_event_objects (event_id, object_id, role) VALUES ('event-2', 'bob', 'speaker')", []).unwrap();
        knowledge(&conn, "alice", "event", "event-1", "content", 1);
        knowledge(&conn, "alice", "event", "event-2", "content", 2);
        knowledge(&conn, "alice", "event", "event-future", "content", 4);
        let memory_repo = crate::db::repositories::memory_repo::MemoryRepository::new(&conn);
        for (id, character, turn, content) in [
            ("memory-a", "alice", 1, "Alice remembers"),
            ("memory-b", "bob", 1, "Bob remembers"),
            ("memory-future", "alice", 4, "Future memory"),
        ] {
            memory_repo
                .insert(&MemoryEntry {
                    id: id.to_string(),
                    world_id: "world".to_string(),
                    session_id: "session".to_string(),
                    character_id: character.to_string(),
                    layer: "long".to_string(),
                    content: content.to_string(),
                    source: "test".to_string(),
                    importance: 0.8,
                    created_at: "now".to_string(),
                    turn_index: turn,
                    conversation_id: None,
                    event_id: None,
                    item_id: None,
                    scene_id: None,
                    memory_type: "dialogue".to_string(),
                    speaker: None,
                    role: None,
                    location: None,
                    participants: Vec::new(),
                    keywords: Vec::new(),
                })
                .unwrap();
        }
        conn.execute("INSERT INTO oag_memory_objects (memory_id, object_id, role) VALUES ('memory-a', 'alice', 'mentioned')", []).unwrap();
        conn.execute("INSERT INTO oag_memory_objects (memory_id, object_id, role) VALUES ('memory-b', 'bob', 'mentioned')", []).unwrap();
        conn.execute(
            "UPDATE memories SET conversation_id = 'conversation-a',
                    participants_json = '[\"bob\"]', keywords_json = '[\"memory\"]'
             WHERE id = 'memory-a'",
            [],
        )
        .unwrap();

        let mut legacy_resolve = request("resolve");
        legacy_resolve.entity = Some(reference("legacy-alice"));
        let legacy_result = query(
            &conn,
            context(OagActor::Author, 3),
            resolved.clone(),
            legacy_resolve,
        )
        .unwrap();
        assert_eq!(legacy_result.results[0]["id"], "alice");

        let mut timeline = request("timeline");
        timeline.participant = Some(reference("alice"));
        timeline.location = Some(reference("dock"));
        timeline.from_turn = Some(1);
        timeline.to_turn = Some(3);
        let result = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            timeline.clone(),
        )
        .unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0]["id"], "event-1");
        timeline.participant = Some(reference("hidden"));
        assert_eq!(
            query(
                &conn,
                context(OagActor::Object("alice".to_string()), 3),
                resolved.clone(),
                timeline
            )
            .unwrap()
            .count,
            0
        );

        let mut entity = request("entity");
        entity.entity = Some(reference("alice"));
        let card = query(
            &conn,
            context(OagActor::Object("alice".to_string()), 3),
            resolved.clone(),
            entity.clone(),
        )
        .unwrap();
        assert_eq!(card.results[0]["memories"].as_array().unwrap().len(), 1);
        assert_eq!(card.results[0]["memories"][0]["id"], "memory-a");
        assert_eq!(
            card.results[0]["memories"][0]["conversation_id"],
            "conversation-a"
        );
        assert_eq!(card.results[0]["memories"][0]["participants"][0], "bob");
        assert_eq!(card.results[0]["memories"][0]["keywords"][0], "memory");
        let author = query(
            &conn,
            context(OagActor::Author, 3),
            resolved,
            OagQueryRequest {
                entity: Some(reference("bob")),
                ..entity
            },
        )
        .unwrap();
        assert!(author.results[0]["memories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|memory| memory["id"] == "memory-b"));

        let all = crate::db::repositories::memory_repo::MemoryRepository::new(&conn)
            .list(&MemoryQueryParams {
                world_id: Some("world".to_string()),
                session_id: Some("session".to_string()),
                character_id: None,
                layer: None,
                limit: None,
            })
            .unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn query_validation_limits_and_trace_are_enforced() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        for item in [
            object("world", "", "alice", "Alice", "person", "public", None, 0),
            object("world", "", "bob", "Bob", "person", "public", None, 0),
        ] {
            repo.upsert_object(&item).unwrap();
        }
        attribute(
            &conn,
            "a1",
            "alice",
            "status",
            serde_json::json!("one"),
            1,
            None,
            "public",
            None,
        );
        attribute(
            &conn,
            "a2",
            "alice",
            "status",
            serde_json::json!("two"),
            1,
            None,
            "public",
            None,
        );
        link(
            &conn,
            "l1",
            "alice",
            "allied",
            Some("bob"),
            1,
            None,
            "public",
        );
        let mut ctx = context(OagActor::Object("alice".to_string()), 3);
        ctx.max_nodes = 2;
        let mut entity = request("entity");
        entity.entity = Some(reference("alice"));
        let result = query(&conn, ctx.clone(), resolved.clone(), entity).unwrap();
        assert!(result.truncated);
        assert_eq!(result.results[0]["attributes"].as_array().unwrap().len(), 1);
        assert!(result.results[0]
            .get("links")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());

        let mut invalid = request("traverse");
        assert!(query(&conn, ctx.clone(), resolved.clone(), invalid.clone()).is_err());
        invalid = request("find");
        invalid.limit = 21;
        assert!(query(&conn, ctx.clone(), resolved.clone(), invalid).is_err());
        let mut future = request("find");
        future.as_of_turn = Some(4);
        assert!(query(&conn, ctx.clone(), resolved.clone(), future).is_err());
        let mut retired = request("find");
        retired.include_retired = true;
        assert!(query(&conn, ctx.clone(), resolved.clone(), retired).is_err());
        let mut ambiguous = request("resolve");
        ambiguous.entity = Some(OagEntityReference {
            id: None,
            name: Some("missing".to_string()),
            object_type: None,
        });
        assert_eq!(
            query(&conn, ctx.clone(), resolved.clone(), ambiguous)
                .unwrap()
                .count,
            0
        );

        let trace_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_query_traces", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(trace_count >= 6);
        let actor: String = conn
            .query_row(
                "SELECT actor_id FROM oag_query_traces ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actor, "object:alice");
    }

    #[test]
    fn retired_objects_are_author_only_and_names_do_not_choose_ambiguously() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        let mut retired = object("world", "", "retired", "Same", "item", "public", None, 0);
        retired.retired_at = "now".to_string();
        repo.upsert_object(&retired).unwrap();
        repo.upsert_object(&object(
            "world", "", "active", "Same", "item", "public", None, 0,
        ))
        .unwrap();
        repo.upsert_object(&object(
            "world",
            "other-session",
            "other-session",
            "Other",
            "item",
            "public",
            None,
            0,
        ))
        .unwrap();
        let mut resolve = request("resolve");
        resolve.entity = Some(OagEntityReference {
            id: None,
            name: Some("Same".to_string()),
            object_type: Some("item".to_string()),
        });
        assert!(
            query(&conn, context(OagActor::Author, 1), resolved.clone(), {
                let mut ambiguous = resolve.clone();
                ambiguous.include_retired = true;
                ambiguous
            })
            .unwrap_err()
            .contains("歧义")
        );
        resolve.include_retired = false;
        assert_eq!(
            query(
                &conn,
                context(OagActor::Author, 1),
                resolved.clone(),
                resolve.clone()
            )
            .unwrap()
            .results[0]["id"],
            "active"
        );
        resolve.entity = Some(reference("other-session"));
        let other_session = query(
            &conn,
            context(OagActor::Author, 1),
            resolved.clone(),
            resolve.clone(),
        )
        .unwrap();
        assert_eq!(other_session.count, 0, "{other_session:#?}");
        resolve.entity = Some(reference("retired"));
        resolve.include_retired = true;
        assert_eq!(
            query(&conn, context(OagActor::Author, 1), resolved, resolve)
                .unwrap()
                .count,
            1
        );
    }
}
