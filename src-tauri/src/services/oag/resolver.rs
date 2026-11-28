use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::db::repositories::oag_repo::OagRepository;
use crate::models::character::CharacterDefinition;
use crate::models::oag::{OagEntityReference, OagObject, OagOntology};

use super::canon::normalize_name;
use super::ontology::{self, ResolvedOntology};

pub fn resolve(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    reference: &OagEntityReference,
    resolved_ontology: &ResolvedOntology,
    as_of_turn: i32,
) -> Result<OagObject, String> {
    if let Some(id) = reference
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return resolve_id(conn, world_id, session_id, id, as_of_turn);
    }
    let name = reference
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "实体引用缺少稳定 ID 或名称".to_string())?;
    resolve_name(
        conn,
        world_id,
        session_id,
        name,
        reference.object_type.as_deref(),
        resolved_ontology,
        as_of_turn,
    )
}

pub fn resolve_id(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    id: &str,
    as_of_turn: i32,
) -> Result<OagObject, String> {
    let repo = OagRepository::new(conn);
    if let Some(target) = repo.legacy_target("characters", &legacy_character_key(world_id, id))? {
        return scoped_object(&repo, world_id, session_id, &target, as_of_turn);
    }
    scoped_object(&repo, world_id, session_id, id, as_of_turn)
}

pub fn resolve_character(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    character_id: Option<&str>,
    character_name: &str,
    characters: &[CharacterDefinition],
    resolved_ontology: &ResolvedOntology,
    turn_index: i32,
) -> Result<OagObject, String> {
    let resolved_id = character_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let matches = characters
                .iter()
                .filter(|character| character.name == character_name)
                .collect::<Vec<_>>();
            (matches.len() == 1).then(|| matches[0].id.clone())
        });
    if let Some(id) = resolved_id.as_deref() {
        let runtime_key = format!("runtime-character:{id}");
        if let Some(object) =
            OagRepository::new(conn).object_by_canonical_key(world_id, session_id, &runtime_key)?
        {
            return active_object(object, turn_index);
        }
        if let Some(object) = OagRepository::new(conn)
            .object(id)?
            .filter(|object| object.world_id == world_id)
            .filter(|object| object.session_id.is_empty() || object.session_id == session_id)
        {
            return active_object(object, turn_index);
        }
        if character_name.trim().is_empty() {
            return Err(format!("角色稳定 ID 无法映射且名称为空：{id}"));
        }
        let character_type = resolved_ontology
            .character_object_type
            .as_deref()
            .ok_or_else(|| "当前本体没有可承载运行期角色的类型".to_string())?;
        let now = chrono::Utc::now().to_rfc3339();
        let object = OagObject {
            id: stable_id(&format!("runtime-character:{world_id}:{session_id}:{id}")),
            world_id: world_id.to_string(),
            session_id: session_id.to_string(),
            object_type: character_type.to_string(),
            canonical_key: runtime_key,
            name: character_name.to_string(),
            name_normalized: normalize_name(character_name),
            aliases: Vec::new(),
            description: String::new(),
            source: "runtime".to_string(),
            visibility: "scene".to_string(),
            asserted_by_id: None,
            source_event_id: None,
            truth_status: "confirmed".to_string(),
            created_turn: turn_index,
            updated_turn: turn_index,
            retired_at: String::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        let repo = OagRepository::new(conn);
        return Ok(repo.upsert_object(&object)?);
    }
    resolve_name(
        conn,
        world_id,
        session_id,
        character_name,
        Some(
            resolved_ontology
                .character_object_type
                .as_deref()
                .unwrap_or_default(),
        ),
        resolved_ontology,
        turn_index,
    )
}

fn resolve_name(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    name: &str,
    object_type: Option<&str>,
    resolved_ontology: &ResolvedOntology,
    as_of_turn: i32,
) -> Result<OagObject, String> {
    let candidates = list_scope(conn, world_id, session_id)?
        .into_iter()
        .filter(|object| active_at_creation(object, as_of_turn))
        .filter(|object| object.retired_at.is_empty())
        .filter(|object| {
            object_type.is_none_or(|key| {
                type_matches(
                    &resolved_ontology.ontology,
                    &object.object_type,
                    &[key.to_string()],
                )
            })
        })
        .collect::<Vec<_>>();
    let exact = candidates
        .iter()
        .filter(|object| object.name == name)
        .map(|object| object.id.clone())
        .collect::<HashSet<_>>();
    if let Some(object) = unique_object(&candidates, exact, name)? {
        return Ok(object);
    }
    let aliases = candidates
        .iter()
        .filter(|object| object.aliases.iter().any(|alias| alias == name))
        .map(|object| object.id.clone())
        .collect::<HashSet<_>>();
    if let Some(object) = unique_object(&candidates, aliases, name)? {
        return Ok(object);
    }
    let normalized = normalize_name(name);
    let normalized_matches = candidates
        .iter()
        .filter(|object| {
            object.name_normalized == normalized
                || object
                    .aliases
                    .iter()
                    .any(|alias| normalize_name(alias) == normalized)
        })
        .map(|object| object.id.clone())
        .collect::<HashSet<_>>();
    unique_object(&candidates, normalized_matches, name)?
        .ok_or_else(|| format!("实体不存在或名称歧义：{name}"))
}

fn unique_object(
    candidates: &[OagObject],
    ids: HashSet<String>,
    name: &str,
) -> Result<Option<OagObject>, String> {
    match ids.len() {
        0 => Ok(None),
        1 => Ok(candidates
            .iter()
            .find(|object| ids.contains(&object.id))
            .cloned()),
        _ => Err(format!("实体名称歧义，拒绝自动选择：{name}")),
    }
}

fn list_scope(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
) -> Result<Vec<OagObject>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM oag_objects
             WHERE world_id = ?1 AND (session_id = '' OR session_id = ?2)
             ORDER BY session_id, id",
        )
        .map_err(|error| error.to_string())?;
    let ids = stmt
        .query_map(params![world_id, session_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let repo = OagRepository::new(conn);
    ids.into_iter()
        .map(|id| {
            repo.object(&id)?
                .ok_or_else(|| format!("OAG 对象读取失败：{id}"))
        })
        .collect()
}

fn scoped_object(
    repo: &OagRepository<'_>,
    world_id: &str,
    session_id: &str,
    id: &str,
    as_of_turn: i32,
) -> Result<OagObject, String> {
    let object = repo
        .object(id)?
        .filter(|object| object.world_id == world_id)
        .filter(|object| object.session_id.is_empty() || object.session_id == session_id)
        .ok_or_else(|| format!("实体 ID 不存在或跨世界/会话：{id}"))?;
    active_object(object, as_of_turn)
}

fn active_at_creation(object: &OagObject, as_of_turn: i32) -> bool {
    object.created_turn <= as_of_turn
}

fn active_object(object: OagObject, as_of_turn: i32) -> Result<OagObject, String> {
    if !object.retired_at.is_empty() {
        return Err(format!("实体已退休：{}", object.id));
    }
    if !active_at_creation(&object, as_of_turn) {
        return Err(format!("实体在查询回合尚不存在：{}", object.id));
    }
    Ok(object)
}

fn type_matches(ontology: &OagOntology, actual: &str, expected: &[String]) -> bool {
    ontology::type_matches(ontology, actual, expected)
}

fn legacy_character_key(world_id: &str, character_id: &str) -> String {
    format!("{world_id}:canon:characters:{character_id}")
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
    use crate::db::{oag_schema, schema};
    use crate::models::oag::OagObjectTypeDefinition;

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn
    }

    fn ontology() -> ResolvedOntology {
        super::super::ontology::resolve(Some(&OagOntology {
            object_types: vec![OagObjectTypeDefinition {
                key: "person".to_string(),
                label: "人物".to_string(),
                parent: None,
                actor: true,
                platform_role: String::new(),
                attributes: Vec::new(),
            }],
            link_types: Vec::new(),
            character_object_type: Some("person".to_string()),
            location_object_type: None,
        }))
        .unwrap()
    }

    fn object(world: &str, session: &str, id: &str, name: &str, alias: &str) -> OagObject {
        OagObject {
            id: id.to_string(),
            world_id: world.to_string(),
            session_id: session.to_string(),
            object_type: "person".to_string(),
            canonical_key: format!("key:{id}"),
            name: name.to_string(),
            name_normalized: normalize_name(name),
            aliases: vec![alias.to_string()],
            description: String::new(),
            source: "author".to_string(),
            visibility: "public".to_string(),
            asserted_by_id: None,
            source_event_id: None,
            truth_status: "confirmed".to_string(),
            created_turn: 1,
            updated_turn: 1,
            retired_at: String::new(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn resolves_stable_id_alias_and_normalized_name() {
        let conn = database();
        let resolved = ontology();
        OagRepository::new(&conn)
            .upsert_object(&object("world", "", "obj-a", "Alice", "A"))
            .unwrap();
        for reference in [
            OagEntityReference {
                id: Some("obj-a".to_string()),
                name: None,
                object_type: None,
            },
            OagEntityReference {
                id: None,
                name: Some("A".to_string()),
                object_type: Some("person".to_string()),
            },
            OagEntityReference {
                id: None,
                name: Some(" ALICE ".to_string()),
                object_type: None,
            },
        ] {
            assert_eq!(
                resolve(&conn, "world", "session", &reference, &resolved, 2)
                    .unwrap()
                    .id,
                "obj-a"
            );
        }
    }

    #[test]
    fn rejects_ambiguous_alias_and_cross_world_id() {
        let conn = database();
        let resolved = ontology();
        let repo = OagRepository::new(&conn);
        repo.upsert_object(&object("world", "", "obj-a", "Alice", "A"))
            .unwrap();
        repo.upsert_object(&object("world", "", "obj-b", "Bob", "A"))
            .unwrap();
        let reference = OagEntityReference {
            id: None,
            name: Some("A".to_string()),
            object_type: None,
        };
        assert!(resolve(&conn, "world", "session", &reference, &resolved, 2)
            .unwrap_err()
            .contains("歧义"));
        assert!(resolve_id(&conn, "other", "session", "obj-a", 2).is_err());
    }

    #[test]
    fn runtime_character_ids_are_stable_and_session_scoped() {
        let conn = database();
        let resolved = ontology();
        let first = resolve_character(
            &conn,
            "world",
            "session-a",
            Some("character-a"),
            "Alice",
            &[],
            &resolved,
            3,
        )
        .unwrap();
        let second = resolve_character(
            &conn,
            "world",
            "session-a",
            Some("character-a"),
            "Renamed",
            &[],
            &resolved,
            4,
        )
        .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.name, "Alice");
        assert!(resolve_id(&conn, "world", "session-b", &first.id, 4).is_err());
        assert!(resolve_id(&conn, "world", "session-a", &first.id, 2).is_err());
        let other = resolve_character(
            &conn,
            "world",
            "session-b",
            Some("character-a"),
            "Alice",
            &[],
            &resolved,
            3,
        )
        .unwrap();
        assert_ne!(first.id, other.id);
        let legacy_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM oag_legacy_id_map", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(legacy_count, 0);
    }
}
