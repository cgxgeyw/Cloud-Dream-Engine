use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::db::repositories::attribute_repo::AttributeRepository;
use crate::models::character::CharacterDefinition;
use crate::models::oag::OagEntityReference;
use crate::models::session::{ChatMessage, SceneRuntime, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;

use super::ontology::{self, ResolvedOntology};
use super::resolver;

#[derive(Debug)]
struct OagError(String);

impl std::fmt::Display for OagError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<rusqlite::Error> for OagError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<String> for OagError {
    fn from(error: String) -> Self {
        Self(error)
    }
}

pub struct OagTurnWriterInput<'a> {
    pub conn: &'a Connection,
    pub world: &'a WorldDefinition,
    pub previous: &'a SessionSnapshot,
    pub updated: &'a SessionSnapshot,
    pub characters: &'a [CharacterDefinition],
    pub turn_index: i32,
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub resolved_ontology: &'a ResolvedOntology,
}

pub fn write_turn(input: &OagTurnWriterInput<'_>) -> Result<(), String> {
    input
        .conn
        .execute_batch("SAVEPOINT oag_turn_write")
        .map_err(|error| error.to_string())?;
    let result = persist_turn(input);
    match result {
        Ok(()) => match input.conn.execute_batch("RELEASE oag_turn_write") {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = input
                    .conn
                    .execute_batch("ROLLBACK TO oag_turn_write; RELEASE oag_turn_write");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = input
                .conn
                .execute_batch("ROLLBACK TO oag_turn_write; RELEASE oag_turn_write");
            Err(error.to_string())
        }
    }
}

pub fn record_trace(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    turn_index: i32,
    step: &str,
    status: &str,
    detail: serde_json::Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO oag_write_traces
         (id, world_id, session_id, turn_index, step, status, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uuid::Uuid::new_v4().to_string(),
            world_id,
            session_id,
            turn_index,
            step,
            status,
            serde_json::to_string(&detail).map_err(|error| error.to_string())?,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn persist_turn(input: &OagTurnWriterInput<'_>) -> Result<(), OagError> {
    let now = chrono::Utc::now().to_rfc3339();
    let participants = resolve_participants(input, &input.updated.scene)?;
    let previous_scene_id = if input.previous.scene.scene_id.is_empty() {
        None
    } else {
        Some(ensure_scene(
            input,
            &input.previous.scene,
            &input.previous.location,
            input.turn_index.saturating_sub(1),
            &now,
        )?)
    };
    let scene_id = ensure_scene(
        input,
        &input.updated.scene,
        &input.updated.location,
        input.turn_index,
        &now,
    )?;
    if previous_scene_id.as_deref() != Some(scene_id.as_str()) {
        if let Some(previous_scene_id) = previous_scene_id.as_deref() {
            input.conn.execute(
                "UPDATE oag_scenes SET valid_to_turn = ?1
                 WHERE id = ?2 AND valid_to_turn IS NULL AND valid_from_turn < ?1",
                params![input.turn_index, previous_scene_id],
            )?;
            input.conn.execute(
                "UPDATE oag_scene_participants SET to_turn = ?1
                 WHERE scene_id = ?2 AND to_turn IS NULL AND from_turn < ?1",
                params![input.turn_index, previous_scene_id],
            )?;
        }
    } else {
        let previous_participants = resolve_participants(input, &input.previous.scene)?;
        let current_ids = participants
            .iter()
            .map(|object| object.id.as_str())
            .collect::<HashSet<_>>();
        for object in previous_participants {
            if !current_ids.contains(object.id.as_str()) {
                input.conn.execute(
                    "UPDATE oag_scene_participants SET to_turn = ?1
                     WHERE scene_id = ?2 AND object_id = ?3 AND to_turn IS NULL AND from_turn < ?1",
                    params![input.turn_index, scene_id, object.id],
                )?;
            }
        }
    }
    for object in &participants {
        ensure_scene_participant(input, &scene_id, object, input.turn_index)?;
    }
    for observer in &participants {
        for target in &participants {
            write_knowledge(
                input,
                observer,
                "object",
                &target.id,
                "witnessed",
                "content",
                None,
                None,
                input.turn_index,
                &now,
            )?;
        }
    }

    let turn_event_id = write_event(
        input,
        "turn",
        None,
        &scene_id,
        &format!("第 {} 回合", input.turn_index),
        serde_json::json!({ "turn_index": input.turn_index }),
        &participants,
        &[],
        "envelope",
        "witnessed",
        input.turn_index,
        &now,
    )?;
    if state_changed(input) {
        write_event(
            input,
            "state_change",
            None,
            &scene_id,
            "场景或世界状态发生变化",
            serde_json::json!({
                "scene_id": input.updated.scene.scene_id,
                "scene_name": input.updated.scene.name,
                "location": input.updated.location,
                "time_label": input.updated.time_label,
                "phase": input.updated.state.phase,
            }),
            &participants,
            &[],
            "envelope",
            "witnessed",
            input.turn_index,
            &now,
        )?;
    }
    for message in current_turn_messages(input) {
        write_message_event(input, message, &participants, &now)?;
    }
    write_attribute_changes(input, &turn_event_id, &now)?;
    Ok(())
}

fn resolve_participants(
    input: &OagTurnWriterInput<'_>,
    scene: &SceneRuntime,
) -> Result<Vec<crate::models::oag::OagObject>, OagError> {
    let mut objects: Vec<crate::models::oag::OagObject> = Vec::new();
    for name in &scene.present_characters {
        let id = if name == &input.updated.player_character_name {
            Some(input.updated.player_character_id.as_str())
        } else {
            input
                .characters
                .iter()
                .find(|character| character.name == *name)
                .map(|character| character.id.as_str())
        };
        let object = resolver::resolve_character(
            input.conn,
            &input.world.id,
            &input.updated.id,
            id,
            name,
            input.characters,
            input.resolved_ontology,
            input.turn_index,
        )?;
        if !objects.iter().any(|existing| existing.id == object.id) {
            objects.push(object);
        }
    }
    Ok(objects)
}

fn ensure_scene(
    input: &OagTurnWriterInput<'_>,
    scene: &SceneRuntime,
    location_name: &str,
    turn_index: i32,
    now: &str,
) -> Result<String, OagError> {
    let id = stable_id(&format!(
        "scene:{}:{}:{}",
        input.world.id, input.updated.id, scene.scene_id
    ));
    let location = if input.resolved_ontology.location_object_type.is_some()
        && !location_name.trim().is_empty()
    {
        match resolver::resolve(
            input.conn,
            &input.world.id,
            &input.updated.id,
            &OagEntityReference {
                id: None,
                name: Some(location_name.to_string()),
                object_type: input.resolved_ontology.location_object_type.clone(),
            },
            input.resolved_ontology,
            turn_index,
        ) {
            Ok(object) => Some(object.id),
            Err(_) => None,
        }
    } else {
        None
    };
    input.conn.execute(
        "INSERT INTO oag_scenes
         (id, world_id, session_id, runtime_scene_id, name, location_object_id,
          background_hint, valid_from_turn, valid_to_turn, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)
         ON CONFLICT(session_id, runtime_scene_id) DO NOTHING",
        params![
            id,
            input.world.id,
            input.updated.id,
            scene.scene_id,
            scene.name,
            location,
            scene.background_hint,
            turn_index,
            now,
        ],
    )?;
    let persisted: String = input.conn.query_row(
        "SELECT id FROM oag_scenes WHERE session_id = ?1 AND runtime_scene_id = ?2",
        params![input.updated.id, scene.scene_id],
        |row| row.get(0),
    )?;
    Ok(persisted)
}

fn ensure_scene_participant(
    input: &OagTurnWriterInput<'_>,
    scene_id: &str,
    object: &crate::models::oag::OagObject,
    turn_index: i32,
) -> Result<(), OagError> {
    let existing: Option<i32> = input
        .conn
        .query_row(
            "SELECT from_turn FROM oag_scene_participants
             WHERE scene_id = ?1 AND object_id = ?2 AND to_turn IS NULL
             ORDER BY from_turn DESC LIMIT 1",
            params![scene_id, object.id],
            |row| row.get(0),
        )
        .ok();
    if existing.is_some() {
        return Ok(());
    }
    input.conn.execute(
        "INSERT INTO oag_scene_participants
         (id, scene_id, object_id, role, presence_mode, from_turn, to_turn)
         VALUES (?1, ?2, ?3, 'participant', 'present', ?4, NULL)
         ON CONFLICT(scene_id, object_id, from_turn, presence_mode) DO NOTHING",
        params![
            stable_id(&format!(
                "scene-participant:{scene_id}:{}:{turn_index}",
                object.id
            )),
            scene_id,
            object.id,
            turn_index,
        ],
    )?;
    Ok(())
}

fn current_turn_messages<'a>(input: &'a OagTurnWriterInput<'a>) -> Vec<&'a ChatMessage> {
    input
        .updated
        .messages
        .iter()
        .filter(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("turn_index"))
                .and_then(|value| value.as_i64())
                == Some(input.turn_index as i64)
                && matches!(message.role.as_str(), "player" | "agent" | "narration")
                && !message.content.trim().is_empty()
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn write_message_event(
    input: &OagTurnWriterInput<'_>,
    message: &ChatMessage,
    scene_participants: &[crate::models::oag::OagObject],
    now: &str,
) -> Result<(), OagError> {
    let metadata = message
        .metadata
        .as_ref()
        .ok_or_else(|| format!("消息 {} 缺少观众元数据，拒绝写入 OAG", message.message_id))?;
    let witnesses = resolve_metadata_audience(
        input,
        metadata,
        "witness_ids",
        "witnesses",
        scene_participants,
    )?;
    if witnesses.is_empty() {
        return Err(OagError(format!(
            "消息 {} 没有可靠观众，拒绝写入 OAG",
            message.message_id
        )));
    }
    let recipients = resolve_metadata_audience(
        input,
        metadata,
        "recipient_ids",
        "recipients",
        scene_participants,
    )?;
    let speaker = metadata
        .get("speaker_character_id")
        .and_then(|value| value.as_str())
        .map(|id| {
            resolver::resolve_character(
                input.conn,
                &input.world.id,
                &input.updated.id,
                Some(id),
                message.speaker.as_deref().unwrap_or_default(),
                input.characters,
                input.resolved_ontology,
                input.turn_index,
            )
        })
        .transpose()?;
    let scene_id = metadata
        .get("scene_id")
        .and_then(|value| value.as_str())
        .map(|runtime_scene_id| {
            stable_id(&format!(
                "scene:{}:{}:{runtime_scene_id}",
                input.world.id, input.updated.id
            ))
        })
        .unwrap_or_else(|| {
            stable_id(&format!(
                "scene:{}:{}:{}",
                input.world.id, input.updated.id, input.updated.scene.scene_id
            ))
        });
    let mut content_audience = witnesses.clone();
    if let Some(speaker) = &speaker {
        if !content_audience
            .iter()
            .any(|object| object.id == speaker.id)
        {
            content_audience.push(speaker.clone());
        }
    }
    if !recipients.is_empty() {
        content_audience.retain(|object| {
            speaker
                .as_ref()
                .is_some_and(|speaker| speaker.id == object.id)
                || recipients.iter().any(|recipient| recipient.id == object.id)
        });
        for recipient in &recipients {
            if !content_audience
                .iter()
                .any(|object| object.id == recipient.id)
            {
                content_audience.push(recipient.clone());
            }
        }
    }
    let whisper = !recipients.is_empty();
    let event_type = if whisper {
        "whisper_envelope"
    } else {
        "dialogue"
    };
    let summary = if whisper {
        "发生了一次耳语".to_string()
    } else {
        message
            .speaker
            .as_deref()
            .map(|speaker| format!("{speaker}说了一句话"))
            .unwrap_or_else(|| "叙事消息已记录".to_string())
    };
    let content_ids = content_audience
        .iter()
        .map(|object| object.id.clone())
        .collect::<HashSet<_>>();
    let envelope_audience = if whisper {
        witnesses
            .iter()
            .filter(|object| !content_ids.contains(&object.id))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let event_id = write_event(
        input,
        event_type,
        Some(message),
        &scene_id,
        &summary,
        serde_json::json!({
            "message_kind": metadata.get("message_kind"),
            "role": message.role,
            "scene_id": metadata.get("scene_id"),
            "source_message_id": message.message_id,
            "content_access_count": content_audience.len(),
            "envelope_only_count": envelope_audience.len(),
        }),
        &content_audience,
        &envelope_audience,
        "content",
        if whisper { "told" } else { "witnessed" },
        input.turn_index,
        now,
    )?;
    if let Some(ref speaker) = speaker {
        input.conn.execute(
            "INSERT INTO oag_event_objects (event_id, object_id, role)
             VALUES (?1, ?2, 'speaker') ON CONFLICT(event_id, object_id, role) DO NOTHING",
            params![event_id, speaker.id],
        )?;
    }
    for audience in content_audience {
        write_knowledge(
            input,
            &audience,
            "event",
            &event_id,
            if whisper { "told" } else { "witnessed" },
            "content",
            Some(&event_id),
            speaker.as_ref().map(|speaker| speaker.id.as_str()),
            input.turn_index,
            now,
        )?;
    }
    for audience in envelope_audience {
        write_knowledge(
            input,
            &audience,
            "event",
            &event_id,
            "witnessed",
            "envelope",
            Some(&event_id),
            speaker.as_ref().map(|speaker| speaker.id.as_str()),
            input.turn_index,
            now,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_metadata_audience(
    input: &OagTurnWriterInput<'_>,
    metadata: &serde_json::Value,
    ids_key: &str,
    names_key: &str,
    scene_participants: &[crate::models::oag::OagObject],
) -> Result<Vec<crate::models::oag::OagObject>, OagError> {
    let ids = metadata
        .get(ids_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !ids.is_empty() {
        let mut objects: Vec<crate::models::oag::OagObject> = Vec::new();
        for id in ids {
            let object = resolver::resolve_character(
                input.conn,
                &input.world.id,
                &input.updated.id,
                Some(&id),
                "",
                input.characters,
                input.resolved_ontology,
                input.turn_index,
            )?;
            if !objects.iter().any(|existing| existing.id == object.id) {
                objects.push(object);
            }
        }
        return Ok(objects);
    }
    let names = metadata
        .get(names_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut objects: Vec<crate::models::oag::OagObject> = Vec::new();
    for name in names {
        let known = scene_participants
            .iter()
            .find(|object| object.name == name)
            .cloned();
        let object = match known {
            Some(object) => object,
            None => resolver::resolve_character(
                input.conn,
                &input.world.id,
                &input.updated.id,
                None,
                &name,
                input.characters,
                input.resolved_ontology,
                input.turn_index,
            )?,
        };
        if !objects.iter().any(|existing| existing.id == object.id) {
            objects.push(object);
        }
    }
    Ok(objects)
}

#[allow(clippy::too_many_arguments)]
fn write_event(
    input: &OagTurnWriterInput<'_>,
    event_type: &str,
    message: Option<&ChatMessage>,
    scene_id: &str,
    summary: &str,
    details: serde_json::Value,
    content_audience: &[crate::models::oag::OagObject],
    envelope_audience: &[crate::models::oag::OagObject],
    default_access: &str,
    content_knowledge_type: &str,
    learned_turn: i32,
    now: &str,
) -> Result<String, OagError> {
    let discriminator = message
        .map(|message| message.message_id.as_str())
        .unwrap_or(event_type);
    let id = stable_id(&format!(
        "event:{}:{}:{}:{event_type}:{discriminator}",
        input.world.id, input.updated.id, input.turn_index,
    ));
    input.conn.execute(
        "INSERT INTO oag_events
         (id, world_id, session_id, turn_index, ordinal, scene_id, event_type, title,
          summary, details_json, source_message_id, dedupe_key, visibility, importance,
          truth_status, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?7, ?8, ?9, ?1, 'scene', ?10,
                 'confirmed', ?11)
         ON CONFLICT(session_id, dedupe_key) DO NOTHING",
        params![
            id,
            input.world.id,
            input.updated.id,
            input.turn_index,
            scene_id,
            event_type,
            summary,
            serde_json::to_string(&details).map_err(|error| error.to_string())?,
            message.map(|message| message.message_id.as_str()),
            if message.is_some() { 0.6 } else { 0.4 },
            now,
        ],
    )?;
    for object in content_audience {
        input.conn.execute(
            "INSERT INTO oag_event_objects (event_id, object_id, role)
             VALUES (?1, ?2, 'participant') ON CONFLICT(event_id, object_id, role) DO NOTHING",
            params![id, object.id],
        )?;
        write_audience(
            input,
            message
                .map(|message| message.message_id.as_str())
                .unwrap_or(&id),
            scene_id,
            object,
            default_access,
            learned_turn,
        )?;
        write_knowledge(
            input,
            object,
            "event",
            &id,
            content_knowledge_type,
            default_access,
            Some(&id),
            None,
            learned_turn,
            now,
        )?;
    }
    for object in envelope_audience {
        input.conn.execute(
            "INSERT INTO oag_event_objects (event_id, object_id, role)
             VALUES (?1, ?2, 'observer') ON CONFLICT(event_id, object_id, role) DO NOTHING",
            params![id, object.id],
        )?;
        write_audience(
            input,
            message
                .map(|message| message.message_id.as_str())
                .unwrap_or(&id),
            scene_id,
            object,
            "envelope",
            learned_turn,
        )?;
        write_knowledge(
            input,
            object,
            "event",
            &id,
            "witnessed",
            "envelope",
            Some(&id),
            None,
            learned_turn,
            now,
        )?;
    }
    Ok(id)
}

fn write_audience(
    input: &OagTurnWriterInput<'_>,
    message_id: &str,
    scene_id: &str,
    actor: &crate::models::oag::OagObject,
    access_kind: &str,
    learned_turn: i32,
) -> Result<(), OagError> {
    input.conn.execute(
        "INSERT INTO oag_turn_audience
         (id, world_id, session_id, turn_index, message_id, scene_id, actor_id,
          access_kind, presence_mode, learned_turn)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'present', ?9)
         ON CONFLICT(session_id, message_id, actor_id, access_kind) DO NOTHING",
        params![
            stable_id(&format!("audience:{message_id}:{}:{access_kind}", actor.id)),
            input.world.id,
            input.updated.id,
            input.turn_index,
            message_id,
            scene_id,
            actor.id,
            access_kind,
            learned_turn,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_knowledge(
    input: &OagTurnWriterInput<'_>,
    actor: &crate::models::oag::OagObject,
    target_kind: &str,
    target_id: &str,
    knowledge_type: &str,
    access_kind: &str,
    source_event_id: Option<&str>,
    source_actor_id: Option<&str>,
    learned_turn: i32,
    now: &str,
) -> Result<(), OagError> {
    let source = source_event_id.unwrap_or("");
    input.conn.execute(
        "INSERT INTO oag_knowledge
         (id, world_id, session_id, actor_id, target_kind, target_id, knowledge_type,
          access_kind, source_event_id, source_actor_id, learned_turn, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULLIF(?9, ''), ?10, ?11, 1.0, ?12)
         ON CONFLICT DO NOTHING",
        params![
            stable_id(&format!(
                "knowledge:{}:{}:{target_kind}:{target_id}:{source}:{access_kind}",
                input.updated.id, actor.id
            )),
            input.world.id,
            input.updated.id,
            actor.id,
            target_kind,
            target_id,
            knowledge_type,
            access_kind,
            source,
            source_actor_id,
            learned_turn,
            now,
        ],
    )?;
    Ok(())
}

fn write_attribute_changes(
    input: &OagTurnWriterInput<'_>,
    source_event_id: &str,
    now: &str,
) -> Result<(), OagError> {
    let schemas = AttributeRepository::new(input.conn)
        .list_schemas(None)?
        .into_iter()
        .filter(|schema| {
            schema
                .world_id
                .as_deref()
                .is_none_or(|world_id| world_id == input.world.id)
        })
        .map(|schema| (schema.id.clone(), schema))
        .collect::<HashMap<_, _>>();
    for update in &input.runtime_application.character_attribute_updates {
        let schema = schemas
            .get(&update.schema_id)
            .ok_or_else(|| format!("运行时属性模式不存在：{}", update.schema_id))?;
        let character_name = input
            .characters
            .iter()
            .find(|character| character.id == update.character_id)
            .map(|character| character.name.as_str())
            .unwrap_or_default();
        let object = resolver::resolve_character(
            input.conn,
            &input.world.id,
            &input.updated.id,
            Some(&update.character_id),
            character_name,
            input.characters,
            input.resolved_ontology,
            input.turn_index,
        )?;
        write_attribute(input, schema, &object, &update.value, source_event_id, now)?;
    }
    for update in &input.runtime_application.session_attribute_updates {
        let schema = schemas
            .get(&update.schema_id)
            .ok_or_else(|| format!("运行时属性模式不存在：{}", update.schema_id))?;
        let object = resolver::resolve_character(
            input.conn,
            &input.world.id,
            &input.updated.id,
            Some(&input.updated.player_character_id),
            &input.updated.player_character_name,
            input.characters,
            input.resolved_ontology,
            input.turn_index,
        )?;
        write_attribute(input, schema, &object, &update.value, source_event_id, now)?;
    }
    Ok(())
}

fn write_attribute(
    input: &OagTurnWriterInput<'_>,
    schema: &crate::models::attribute::AttributeSchema,
    object: &crate::models::oag::OagObject,
    value: &serde_json::Value,
    source_event_id: &str,
    now: &str,
) -> Result<(), OagError> {
    let definition = ontology::attribute_definition(
        &input.resolved_ontology.ontology,
        &object.object_type,
        &schema.key,
    )
    .ok_or_else(|| {
        format!(
            "运行时属性 {} 未在对象类型 {} 上声明",
            schema.key, object.object_type
        )
    })?;
    ontology::validate_value(definition, value)?;
    let (self_read, other_read) = read_policy(schema)?;
    let visibility = match (self_read, other_read) {
        (true, true) => "public",
        (false, false) => "private",
        _ => {
            return Err(OagError(format!(
                "属性 {} 的 self/other 读取策略无法映射为 OAG 可见性",
                schema.key
            )))
        }
    };
    let id = stable_id(&format!(
        "attribute:{}:{}:{}",
        object.id, schema.key, input.turn_index,
    ));
    input.conn.execute(
        "INSERT INTO oag_attributes
         (id, world_id, session_id, object_id, key, value_json, value_type,
          valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
          source_event_id, visibility, truth_status, supersedes_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 'runtime', 1.0, ?4,
                 ?9, ?10, 'asserted', NULL, ?11)
         ON CONFLICT(id) DO NOTHING",
        params![
            id,
            input.world.id,
            input.updated.id,
            object.id,
            schema.key,
            serde_json::to_string(value).map_err(|error| error.to_string())?,
            definition.value_type,
            input.turn_index,
            source_event_id,
            visibility,
            now,
        ],
    )?;
    if definition.functional {
        input.conn.execute(
            "UPDATE oag_attributes SET valid_to_turn = ?1
             WHERE object_id = ?2 AND key = ?3 AND source = 'runtime'
               AND asserted_by_id = ?4 AND valid_from_turn < ?1
               AND valid_to_turn IS NULL AND id <> ?5",
            params![input.turn_index, object.id, schema.key, object.id, id],
        )?;
    }
    if visibility == "scene" {
        for participant in resolve_participants(input, &input.updated.scene)? {
            write_knowledge(
                input,
                &participant,
                "attribute",
                &id,
                "witnessed",
                "content",
                Some(source_event_id),
                Some(&object.id),
                input.turn_index,
                now,
            )?;
        }
    }
    Ok(())
}

fn read_policy(
    schema: &crate::models::attribute::AttributeSchema,
) -> Result<(bool, bool), OagError> {
    let read = |map: &HashMap<String, serde_json::Value>, key: &str| -> Result<bool, String> {
        map.get(key)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| format!("属性 {} 的 {key} 策略必须是布尔值", schema.key))
            })
            .transpose()
            .map(|value| value.unwrap_or(false))
    };
    let self_read = match schema.access_policy.get("agent_self_read") {
        Some(_) => read(&schema.access_policy, "agent_self_read")?,
        None => read(&schema.display_policy, "self_read")?,
    };
    let other_read = match schema.access_policy.get("agent_other_read") {
        Some(_) => read(&schema.access_policy, "agent_other_read")?,
        None => read(&schema.display_policy, "other_read")?,
    };
    if self_read != other_read {
        return Err(OagError(format!(
            "属性 {} 的 self/other 读取策略无法映射为 OAG 可见性",
            schema.key
        )));
    }
    Ok((self_read, other_read))
}

fn state_changed(input: &OagTurnWriterInput<'_>) -> bool {
    input.previous.scene.scene_id != input.updated.scene.scene_id
        || input.previous.location != input.updated.location
        || input.previous.time_label != input.updated.time_label
        || input.previous.state.phase != input.updated.state.phase
}

fn stable_id(value: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("dream-oag:{value}").as_bytes(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::oag_repo::OagRepository;
    use crate::db::{oag_schema, schema};
    use crate::models::oag::{OagObjectTypeDefinition, OagOntology};
    use crate::models::session::{AssetSelection, MessageContent, SceneRuntime, SessionState};

    fn session(id: &str) -> SessionSnapshot {
        SessionSnapshot {
            id: id.to_string(),
            world_name: "World".to_string(),
            location: "Dock".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Player".to_string(),
            current_line: String::new(),
            player_character_id: "player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string()],
            messages: Vec::new(),
            player_stats: Vec::new(),
            map_graph_nodes: Vec::new(),
            map_graph_edges: Vec::new(),
            system_log: Vec::new(),
            scene: SceneRuntime {
                scene_id: "dock".to_string(),
                name: "Dock".to_string(),
                background_hint: String::new(),
                temporary_tags: Vec::new(),
                present_characters: vec!["Player".to_string(), "Alice".to_string()],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    fn world() -> WorldDefinition {
        WorldDefinition {
            id: "world".to_string(),
            name: "World".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: String::new(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::Value::Null,
            time_config: serde_json::Value::Null,
            director_config: serde_json::json!({ "memory_mode": "oag" }),
            ui_theme_config: serde_json::Value::Null,
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        }
    }

    fn resolved() -> ResolvedOntology {
        ontology::resolve(Some(&OagOntology {
            object_types: vec![OagObjectTypeDefinition {
                key: "person".to_string(),
                label: "Person".to_string(),
                parent: None,
                actor: true,
                platform_role: String::new(),
                attributes: vec![crate::models::oag::OagAttributeDefinition {
                    key: "health".to_string(),
                    label: "Health".to_string(),
                    value_type: "number".to_string(),
                    required: false,
                    default: serde_json::Value::Null,
                    enum_options: Vec::new(),
                    functional: true,
                    temporal: true,
                }],
            }],
            link_types: Vec::new(),
            character_object_type: Some("person".to_string()),
            location_object_type: None,
        }))
        .unwrap()
    }

    fn message(id: &str, role: &str, speaker: &str, recipients: bool) -> ChatMessage {
        ChatMessage {
            message_id: id.to_string(),
            role: role.to_string(),
            content: MessageContent::Text("private body".to_string()),
            speaker: Some(speaker.to_string()),
            metadata: Some(serde_json::json!({
                "turn_index": 1,
                "message_kind": if role == "player" { "player_action" } else { "agent_response" },
                "scene_id": "dock",
                "speaker_character_id": if speaker == "Alice" { Some("alice") } else { Some("player") },
                "witness_ids": ["player", "alice"],
                "recipient_ids": if recipients { serde_json::json!(["alice"]) } else { serde_json::json!([]) },
            })),
            created_at: "now".to_string(),
            parent_message_id: None,
        }
    }

    fn characters() -> Vec<CharacterDefinition> {
        vec![CharacterDefinition {
            id: "alice".to_string(),
            name: "Alice".to_string(),
            world_id: "world".to_string(),
            role: String::new(),
            background_prompt: String::new(),
            model: String::new(),
            memory_strategy: String::new(),
            recent_dialogue_rounds: 1,
            attributes: Vec::new(),
            portrait_assets: Vec::new(),
            avatar_asset: String::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
            knowledge_base_ids: Vec::new(),
        }]
    }

    fn write(conn: &Connection, updated: &SessionSnapshot, runtime: &DirectorRuntimeApplication) {
        write_from(conn, &session("session"), updated, runtime, 1, characters());
    }

    fn write_from(
        conn: &Connection,
        previous: &SessionSnapshot,
        updated: &SessionSnapshot,
        runtime: &DirectorRuntimeApplication,
        turn_index: i32,
        characters: Vec<CharacterDefinition>,
    ) {
        let world = world();
        let ontology = resolved();
        write_turn(&OagTurnWriterInput {
            conn,
            world: &world,
            previous,
            updated,
            characters: &characters,
            turn_index,
            runtime_application: runtime,
            resolved_ontology: &ontology,
        })
        .unwrap();
    }

    fn health_schema(conn: &Connection) -> String {
        AttributeRepository::new(conn)
            .create_schema(&crate::models::attribute::AttributeSchemaCreateRequest {
                scope: "session_character".to_string(),
                world_id: Some("world".to_string()),
                sort_order: 0,
                key: "health".to_string(),
                label: "Health".to_string(),
                value_type: "number".to_string(),
                description: String::new(),
                default_value: serde_json::Value::Null,
                enum_options: Vec::new(),
                display_policy: HashMap::from([
                    ("self_read".to_string(), serde_json::json!(true)),
                    ("other_read".to_string(), serde_json::json!(true)),
                ]),
                access_policy: HashMap::new(),
                mutation_policy: HashMap::new(),
                influence_policy: HashMap::new(),
                projection_policy: HashMap::new(),
            })
            .unwrap()
            .id
    }

    #[test]
    fn dialogue_does_not_copy_body_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        let mut updated = session("session");
        updated.messages = vec![message("message-a", "agent", "Alice", false)];
        write(&conn, &updated, &DirectorRuntimeApplication::default());
        write(&conn, &updated, &DirectorRuntimeApplication::default());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let stored: String = conn
            .query_row(
                "SELECT details_json FROM oag_events WHERE event_type = 'dialogue'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored.contains("private body"));
        let content: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_turn_audience WHERE message_id = 'message-a' AND access_kind = 'content'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(content, 2);
    }

    #[test]
    fn whisper_hides_content_from_other_witnesses() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        let mut updated = session("session");
        updated.messages = vec![message("message-a", "agent", "Alice", true)];
        write(&conn, &updated, &DirectorRuntimeApplication::default());
        let access: Vec<String> = {
            let mut stmt = conn.prepare("SELECT access_kind FROM oag_turn_audience WHERE message_id = 'message-a' ORDER BY access_kind").unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.collect::<Result<_, _>>().unwrap()
        };
        assert_eq!(access, vec!["content", "envelope"]);
        let stored: String = conn
            .query_row(
                "SELECT details_json FROM oag_events WHERE event_type = 'whisper_envelope'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored.contains("private body"));
    }

    #[test]
    fn persistence_failure_rolls_back_every_oag_row() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_audience BEFORE INSERT ON oag_turn_audience
             BEGIN SELECT RAISE(ABORT, 'forced audience failure'); END;",
        )
        .unwrap();
        let mut updated = session("session");
        updated.messages = vec![message("message-a", "agent", "Alice", false)];
        let previous = session("session");
        let world = world();
        let ontology = resolved();
        let characters = characters();
        let error = write_turn(&OagTurnWriterInput {
            conn: &conn,
            world: &world,
            previous: &previous,
            updated: &updated,
            characters: &characters,
            turn_index: 1,
            runtime_application: &DirectorRuntimeApplication::default(),
            resolved_ontology: &ontology,
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("forced audience failure"));
        for table in ["oag_objects", "oag_scenes", "oag_events", "oag_knowledge"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn functional_claims_close_only_the_same_actor_and_keep_as_of_history() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        let schema_id = health_schema(&conn);
        let mut first = session("session");
        let mut characters = characters();
        let mut bob = characters[0].clone();
        bob.id = "bob".to_string();
        bob.name = "Bob".to_string();
        characters.push(bob);
        let mut runtime = DirectorRuntimeApplication {
            character_attribute_updates: vec![
                crate::services::game_engine::runtime_effects::ParsedCharacterAttributeUpdate {
                    character_id: "alice".to_string(),
                    schema_id: schema_id.clone(),
                    value: serde_json::json!(10),
                },
                crate::services::game_engine::runtime_effects::ParsedCharacterAttributeUpdate {
                    character_id: "bob".to_string(),
                    schema_id: schema_id.clone(),
                    value: serde_json::json!(7),
                },
            ],
            ..DirectorRuntimeApplication::default()
        };
        write_from(
            &conn,
            &session("session"),
            &first,
            &runtime,
            1,
            characters.clone(),
        );
        let alice = OagRepository::new(&conn)
            .object_by_canonical_key("world", "session", "runtime-character:alice")
            .unwrap()
            .unwrap();
        let player = OagRepository::new(&conn)
            .object_by_canonical_key("world", "session", "runtime-character:player")
            .unwrap()
            .unwrap();
        conn.execute(
            "INSERT INTO oag_attributes
             (id, world_id, session_id, object_id, key, value_json, value_type,
              valid_from_turn, valid_to_turn, source, confidence, asserted_by_id,
              visibility, truth_status, created_at)
             VALUES ('rival-claim', 'world', 'session', ?1, 'health', '99', 'number',
                     1, NULL, 'runtime', 1.0, ?2, 'public', 'asserted', 'now')",
            params![&alice.id, &player.id],
        )
        .unwrap();
        first.messages = vec![message("message-a", "agent", "Alice", false)];
        let second = first.clone();
        runtime.character_attribute_updates[0].value = serde_json::json!(6);
        write_from(&conn, &first, &second, &runtime, 2, characters);

        let superseded: i64 = conn
            .query_row(
                "SELECT valid_to_turn FROM oag_attributes
                 WHERE object_id = ?1 AND asserted_by_id = ?1 AND value_json = '10'",
                [&alice.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(superseded, 2);
        let rival: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_attributes
                 WHERE id = 'rival-claim' AND valid_to_turn IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rival, 1);
        let bob: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_attributes
                 WHERE object_id IN (SELECT id FROM oag_objects WHERE canonical_key = 'runtime-character:bob')
                   AND valid_to_turn IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bob, 1);
        let at_first_turn: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_attributes
                 WHERE object_id = ?1 AND valid_from_turn <= 1
                   AND (valid_to_turn IS NULL OR valid_to_turn > 1)",
                [&alice.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(at_first_turn, 2);
        let at_second_turn: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_attributes
                 WHERE object_id = ?1 AND valid_from_turn <= 2
                   AND (valid_to_turn IS NULL OR valid_to_turn > 2)",
                [&alice.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(at_second_turn, 2);
    }

    #[test]
    fn scene_switch_closes_old_presence_and_keeps_events_in_their_scenes() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        let previous = session("session");
        let mut updated = previous.clone();
        updated.location = "Alley".to_string();
        updated.scene = SceneRuntime {
            scene_id: "alley".to_string(),
            name: "Alley".to_string(),
            background_hint: String::new(),
            temporary_tags: Vec::new(),
            present_characters: vec!["Player".to_string(), "Alice".to_string()],
        };
        let old_message = message("message-old", "player", "Player", false);
        let mut new_message = message("message-new", "agent", "Alice", false);
        new_message.metadata.as_mut().unwrap()["scene_id"] = serde_json::json!("alley");
        updated.messages = vec![old_message, new_message];
        write(&conn, &updated, &DirectorRuntimeApplication::default());

        let dock_closed: i64 = conn
            .query_row(
                "SELECT valid_to_turn FROM oag_scenes WHERE runtime_scene_id = 'dock'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dock_closed, 1);
        let old_scene: String = conn
            .query_row(
                "SELECT s.runtime_scene_id FROM oag_events e
                 JOIN oag_scenes s ON s.id = e.scene_id
                 WHERE e.source_message_id = 'message-old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let new_scene: String = conn
            .query_row(
                "SELECT s.runtime_scene_id FROM oag_events e
                 JOIN oag_scenes s ON s.id = e.scene_id
                 WHERE e.source_message_id = 'message-new'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_scene, "dock");
        assert_eq!(new_scene, "alley");
        let misplaced_knowledge: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_turn_audience
                 WHERE message_id = 'message-old' AND scene_id <> ?1",
                [stable_id("scene:world:session:dock")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(misplaced_knowledge, 0);
    }

    #[test]
    fn read_policy_requires_unambiguous_boolean_mapping() {
        let mut schema = crate::models::attribute::AttributeSchema {
            id: "schema".to_string(),
            scope: "session_character".to_string(),
            world_id: None,
            sort_order: 0,
            key: "health".to_string(),
            label: "Health".to_string(),
            value_type: "number".to_string(),
            description: String::new(),
            default_value: serde_json::Value::Null,
            enum_options: Vec::new(),
            display_policy: HashMap::new(),
            access_policy: HashMap::from([
                ("agent_self_read".to_string(), serde_json::json!(true)),
                ("agent_other_read".to_string(), serde_json::json!(false)),
            ]),
            mutation_policy: HashMap::new(),
            influence_policy: HashMap::new(),
            projection_policy: HashMap::new(),
        };
        assert!(read_policy(&schema)
            .unwrap_err()
            .to_string()
            .contains("无法映射"));
        schema
            .access_policy
            .insert("agent_self_read".to_string(), serde_json::json!("yes"));
        assert!(read_policy(&schema)
            .unwrap_err()
            .to_string()
            .contains("布尔值"));
    }
}
