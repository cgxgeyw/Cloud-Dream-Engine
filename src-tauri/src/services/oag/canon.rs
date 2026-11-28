use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::repositories::oag_repo::OagRepository;
use crate::db::repositories::world_record_repo::WorldRecordRepository;
use crate::models::character::CharacterPackageData;
use crate::models::oag::{
    OagAttribute, OagCanonData, OagCanonImportChange, OagCanonImportChangeKind,
    OagCanonImportPreview, OagCanonLink, OagCanonObject, OagLegacyIdMap, OagLink,
    OagLinkTypeDefinition, OagObject, OagObjectTypeDefinition, OagOntology,
};
use crate::models::world_record::WorldRecordWriteRequest;

use super::ontology::type_matches;
use super::ontology::{self, ResolvedOntology};

const ACTIVE_ONTOLOGY_COLLECTION: &str = "oag_ontology";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveOntology {
    ontology: OagOntology,
    mode: String,
}

struct PreparedCanon {
    resolved: ResolvedOntology,
    objects: Vec<OagObject>,
    attributes: Vec<OagAttribute>,
    links: Vec<OagLink>,
    legacy_maps: Vec<OagLegacyIdMap>,
    now: String,
}

pub fn import(
    conn: &Connection,
    world_id: &str,
    declared_ontology: Option<&OagOntology>,
    canon: &OagCanonData,
    characters: &[CharacterPackageData],
    character_id_map: &HashMap<String, String>,
    map_nodes: &serde_json::Value,
) -> Result<ResolvedOntology, String> {
    let prepared = prepare(
        world_id,
        declared_ontology,
        canon,
        characters,
        character_id_map,
        map_nodes,
    )?;
    conn.execute_batch("SAVEPOINT oag_canon_import")
        .map_err(|error| error.to_string())?;
    let result = persist_prepared(conn, world_id, prepared);
    match result {
        Ok(resolved) => match conn.execute_batch("RELEASE oag_canon_import") {
            Ok(()) => Ok(resolved),
            Err(error) => {
                let _ =
                    conn.execute_batch("ROLLBACK TO oag_canon_import; RELEASE oag_canon_import");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO oag_canon_import; RELEASE oag_canon_import");
            Err(error)
        }
    }
}

fn persist_prepared(
    conn: &Connection,
    world_id: &str,
    prepared: PreparedCanon,
) -> Result<ResolvedOntology, String> {
    let repo = OagRepository::new(conn);
    repo.replace_tbox(
        world_id,
        &prepared.resolved.ontology.object_types,
        &prepared.resolved.ontology.link_types,
        prepared.resolved.compatibility,
    )?;
    for object in &prepared.objects {
        repo.upsert_object(object)?;
        repo.replace_canon_attributes(&object.id)?;
    }
    for attribute in &prepared.attributes {
        repo.insert_canon_attribute(attribute)?;
    }
    repo.replace_canon_links(world_id, &prepared.links)?;
    repo.retire_canon_objects(
        world_id,
        &prepared
            .objects
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>(),
        &prepared.now,
    )?;
    for mapping in &prepared.legacy_maps {
        repo.record_legacy_map(mapping)?;
    }
    save_active_ontology(conn, world_id, &prepared.resolved)?;
    Ok(prepared.resolved)
}

pub fn load_active_ontology(
    conn: &Connection,
    world_id: &str,
) -> Result<Option<ResolvedOntology>, String> {
    let records = WorldRecordRepository::new(conn).list(world_id, ACTIVE_ONTOLOGY_COLLECTION)?;
    let active_id = active_ontology_id(world_id);
    let Some(record) = records.into_iter().find(|record| record.id == active_id) else {
        return Ok(None);
    };
    let active: ActiveOntology = serde_json::from_value(record.data)
        .map_err(|error| format!("当前 OAG 本体记录无效：{error}"))?;
    let resolved = ontology::resolve(Some(&active.ontology))?;
    Ok(Some(ResolvedOntology {
        ontology: resolved.ontology,
        character_object_type: resolved.character_object_type,
        location_object_type: resolved.location_object_type,
        compatibility: active.mode == "compatibility",
    }))
}

pub fn export(
    conn: &Connection,
    world_id: &str,
) -> Result<Option<(OagOntology, OagCanonData)>, String> {
    let Some(resolved) = load_active_ontology(conn, world_id)? else {
        return Ok(None);
    };
    let repo = OagRepository::new(conn);
    let objects = repo.list_canon_objects(world_id)?;
    let attributes = repo.list_canon_attributes(world_id)?;
    let links = repo.list_canon_links(world_id)?;
    let canonical_key_by_id = objects
        .iter()
        .map(|object| (object.id.clone(), object.canonical_key.clone()))
        .collect::<HashMap<_, _>>();
    let mut attributes_by_object: HashMap<String, serde_json::Map<String, serde_json::Value>> =
        HashMap::new();
    for attribute in attributes {
        if canonical_key_by_id.contains_key(&attribute.object_id) {
            attributes_by_object
                .entry(attribute.object_id)
                .or_default()
                .insert(attribute.key, attribute.value);
        }
    }
    let exported_objects = objects
        .into_iter()
        .map(|object| {
            Ok(crate::models::oag::OagCanonObject {
                object_type: object.object_type,
                canonical_key: object.canonical_key,
                name: object.name,
                aliases: object.aliases,
                attributes: attributes_by_object.remove(&object.id).unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut exported_links = Vec::new();
    for link in links {
        let Some(from) = canonical_key_by_id.get(&link.from_object_id) else {
            continue;
        };
        let to = if let Some(target) = link.to_object_id {
            let Some(key) = canonical_key_by_id.get(&target) else {
                continue;
            };
            serde_json::Value::String(key.clone())
        } else {
            serde_json::json!({
                "literal": serde_json::from_str::<serde_json::Value>(&link.to_value)
                    .unwrap_or(serde_json::Value::String(link.to_value))
            })
        };
        exported_links.push(OagCanonLink {
            from: from.clone(),
            link_type: link.link_type,
            to,
            original_predicate: link.original_predicate,
        });
    }
    Ok(Some((
        resolved.ontology,
        OagCanonData {
            objects: exported_objects,
            links: exported_links,
        },
    )))
}

pub fn preview(
    conn: &Connection,
    existing_world_id: Option<&str>,
    declared_ontology: Option<&OagOntology>,
    canon: &OagCanonData,
    characters: &[CharacterPackageData],
    map_nodes: &serde_json::Value,
) -> Result<OagCanonImportPreview, String> {
    let world_id = existing_world_id.unwrap_or("__package_preview__");
    let character_ids = characters
        .iter()
        .map(|character| {
            (
                character.source_character_id.clone(),
                character.source_character_id.clone(),
            )
        })
        .collect();
    let prepared = prepare(
        world_id,
        declared_ontology,
        canon,
        characters,
        &character_ids,
        map_nodes,
    )?;
    let repo = OagRepository::new(conn);
    let current_ontology = match existing_world_id {
        Some(id) => load_active_ontology(conn, id)?,
        None => None,
    };
    let ontology_change = diff_items(
        &[PreviewItem::new(
            "ontology",
            "本体模式与绑定",
            &prepared.resolved,
        )],
        &current_ontology
            .iter()
            .map(|ontology| PreviewItem::new("ontology", "本体模式与绑定", ontology))
            .collect::<Vec<_>>(),
    )
    .0
    .into_iter()
    .next();

    let current_object_types = if let Some(id) = existing_world_id {
        repo.list_object_types(id)?
            .into_iter()
            .map(|item| OagObjectTypeDefinition {
                key: item.key,
                label: item.label,
                parent: item.parent_key,
                actor: item.actor,
                platform_role: item.platform_role,
                attributes: item.attributes,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let incoming_object_types = prepared.resolved.ontology.object_types.clone();
    let (object_type_changes, object_type_unchanged) = diff_items(
        &incoming_object_types
            .iter()
            .map(|item| PreviewItem::new(&item.key, &item.label, item))
            .collect::<Vec<_>>(),
        &current_object_types
            .iter()
            .map(|item| PreviewItem::new(&item.key, &item.label, item))
            .collect::<Vec<_>>(),
    );

    let current_link_types = if let Some(id) = existing_world_id {
        repo.list_link_types(id)?
            .into_iter()
            .map(|item| OagLinkTypeDefinition {
                key: item.key,
                label: item.label,
                domain: item.domain,
                range: item.range,
                functional: item.functional,
                symmetric: item.symmetric,
                transitive: item.transitive,
                temporal: item.temporal,
                inverse: item.inverse,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let incoming_link_types = prepared.resolved.ontology.link_types.clone();
    let (link_type_changes, link_type_unchanged) = diff_items(
        &incoming_link_types
            .iter()
            .map(|item| PreviewItem::new(&item.key, &item.label, item))
            .collect::<Vec<_>>(),
        &current_link_types
            .iter()
            .map(|item| PreviewItem::new(&item.key, &item.label, item))
            .collect::<Vec<_>>(),
    );

    let (incoming_objects, incoming_links) =
        preview_canon_values(&prepared.objects, &prepared.attributes, &prepared.links);
    let (current_objects, current_links) = if let Some(id) = existing_world_id {
        preview_canon_values(
            &repo.list_canon_objects(id)?,
            &repo.list_canon_attributes(id)?,
            &repo.list_canon_links(id)?,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let (object_changes, object_unchanged) = diff_items(&incoming_objects, &current_objects);
    let (link_changes, link_unchanged) = diff_items(&incoming_links, &current_links);

    let mut preview = OagCanonImportPreview {
        world_id: existing_world_id.map(str::to_string),
        is_upgrade: existing_world_id.is_some(),
        compatibility: prepared.resolved.compatibility,
        ontology_change,
        object_type_changes,
        link_type_changes,
        object_changes,
        link_changes,
        added_count: 0,
        modified_count: 0,
        retired_count: 0,
        unchanged_count: 0,
    };
    for change in preview
        .ontology_change
        .iter()
        .chain(&preview.object_type_changes)
        .chain(&preview.link_type_changes)
        .chain(&preview.object_changes)
        .chain(&preview.link_changes)
    {
        match change.kind {
            OagCanonImportChangeKind::Added => preview.added_count += 1,
            OagCanonImportChangeKind::Modified => preview.modified_count += 1,
            OagCanonImportChangeKind::Retired => preview.retired_count += 1,
        }
    }
    preview.unchanged_count = object_type_unchanged
        + link_type_unchanged
        + object_unchanged
        + link_unchanged
        + usize::from(current_ontology.is_some() && preview.ontology_change.is_none());
    Ok(preview)
}

#[derive(Clone, Serialize)]
struct PreviewItem {
    key: String,
    label: String,
    value: serde_json::Value,
}

impl PreviewItem {
    fn new<T: Serialize>(key: &str, label: &str, value: &T) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            value: serde_json::to_value(value).expect("serialize Canon preview"),
        }
    }
}

fn diff_items(
    incoming: &[PreviewItem],
    current: &[PreviewItem],
) -> (Vec<OagCanonImportChange>, usize) {
    let current_by_key = current
        .iter()
        .map(|item| (item.key.as_str(), item))
        .collect::<HashMap<_, _>>();
    let incoming_keys = incoming
        .iter()
        .map(|item| item.key.as_str())
        .collect::<HashSet<_>>();
    let mut incoming = incoming.to_vec();
    incoming.sort_by(|left, right| left.key.cmp(&right.key));
    let mut changes = Vec::new();
    let mut unchanged = 0;
    for item in incoming {
        match current_by_key.get(item.key.as_str()) {
            None => changes.push(change(
                OagCanonImportChangeKind::Added,
                item.key,
                item.label,
                None,
                Some(item.value),
            )),
            Some(previous) if previous.value == item.value => unchanged += 1,
            Some(previous) => changes.push(change(
                OagCanonImportChangeKind::Modified,
                item.key,
                item.label,
                Some(previous.value.clone()),
                Some(item.value),
            )),
        }
    }
    let mut retired = current
        .iter()
        .filter(|item| !incoming_keys.contains(item.key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    retired.sort_by(|left, right| left.key.cmp(&right.key));
    for item in retired {
        changes.push(change(
            OagCanonImportChangeKind::Retired,
            item.key,
            item.label,
            Some(item.value),
            None,
        ));
    }
    (changes, unchanged)
}

fn change(
    kind: OagCanonImportChangeKind,
    key: String,
    label: String,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
) -> OagCanonImportChange {
    OagCanonImportChange {
        kind,
        key,
        label,
        before,
        after,
    }
}

fn preview_canon_values(
    objects: &[OagObject],
    attributes: &[OagAttribute],
    links: &[OagLink],
) -> (Vec<PreviewItem>, Vec<PreviewItem>) {
    let canonical_key_by_id = objects
        .iter()
        .map(|object| (object.id.as_str(), object.canonical_key.as_str()))
        .collect::<HashMap<_, _>>();
    let mut attributes_by_object =
        HashMap::<&str, serde_json::Map<String, serde_json::Value>>::new();
    for attribute in attributes {
        if canonical_key_by_id.contains_key(attribute.object_id.as_str()) {
            attributes_by_object
                .entry(&attribute.object_id)
                .or_default()
                .insert(attribute.key.clone(), attribute.value.clone());
        }
    }
    let object_items = objects
        .iter()
        .map(|object| {
            let value = OagCanonObject {
                object_type: object.object_type.clone(),
                canonical_key: object.canonical_key.clone(),
                name: object.name.clone(),
                aliases: object.aliases.clone(),
                attributes: attributes_by_object
                    .remove(object.id.as_str())
                    .unwrap_or_default(),
            };
            PreviewItem::new(&object.canonical_key, &object.name, &value)
        })
        .collect();
    let link_items = links
        .iter()
        .filter(|link| link.source == "author")
        .map(|link| {
            let from = canonical_key_by_id
                .get(link.from_object_id.as_str())
                .copied()
                .unwrap_or(link.from_object_id.as_str());
            let to = match link.to_object_id.as_deref() {
                Some(id) => serde_json::Value::String(
                    canonical_key_by_id
                        .get(id)
                        .copied()
                        .unwrap_or(id)
                        .to_string(),
                ),
                None => serde_json::json!({
                    "literal": serde_json::from_str::<serde_json::Value>(&link.to_value)
                        .unwrap_or_else(|_| serde_json::Value::String(link.to_value.clone()))
                }),
            };
            let value = OagCanonLink {
                from: from.to_string(),
                link_type: link.link_type.clone(),
                to,
                original_predicate: link.original_predicate.clone(),
            };
            let target = value.to.to_string();
            let key = format!("{from}:{}:{target}", value.link_type);
            let label = if value.original_predicate.is_empty() {
                value.link_type.clone()
            } else {
                value.original_predicate.clone()
            };
            PreviewItem::new(&key, &label, &value)
        })
        .collect();
    (object_items, link_items)
}

pub fn validate(
    ontology: Option<&OagOntology>,
    canon: &OagCanonData,
    characters: &[CharacterPackageData],
    map_nodes: &serde_json::Value,
) -> Result<(), String> {
    let character_ids = characters
        .iter()
        .map(|character| {
            (
                character.source_character_id.clone(),
                character.source_character_id.clone(),
            )
        })
        .collect();
    prepare(
        "package",
        ontology,
        canon,
        characters,
        &character_ids,
        map_nodes,
    )?;
    Ok(())
}

fn prepare(
    world_id: &str,
    declared_ontology: Option<&OagOntology>,
    canon: &OagCanonData,
    characters: &[CharacterPackageData],
    character_id_map: &HashMap<String, String>,
    map_nodes: &serde_json::Value,
) -> Result<PreparedCanon, String> {
    let resolved = ontology::resolve(declared_ontology)?;
    let character_type = resolved.character_object_type.clone().ok_or_else(|| {
        "世界包含角色，但本体未绑定 character_object_type 或兼容角色类型".to_string()
    })?;
    let topology = crate::services::map_topology::compile_map_topology(map_nodes, "");
    let location_type = if topology.nodes.is_empty() {
        resolved.location_object_type.clone()
    } else {
        Some(resolved.location_object_type.clone().ok_or_else(|| {
            "世界包含地图节点，但本体未绑定 location_object_type 或兼容地点类型".to_string()
        })?)
    };
    let now = chrono::Utc::now().to_rfc3339();
    let mut objects_by_key = HashMap::<String, OagObject>::new();
    let mut legacy_maps = Vec::new();
    for character in characters {
        let source_id = character.source_character_id.trim();
        if source_id.is_empty() {
            return Err("世界包角色缺少 source_character_id".to_string());
        }
        let canonical_key = format!("{character_type}:{source_id}");
        let object = canon_object(
            world_id,
            &character_type,
            &canonical_key,
            &character.name,
            Vec::new(),
            &now,
        )?;
        insert_generated_object(&mut objects_by_key, object)?;
        let target_id = objects_by_key[&canonical_key].id.clone();
        let actual_character_id = character_id_map
            .get(source_id)
            .ok_or_else(|| format!("角色 {source_id} 尚未写入世界角色表，无法建立 Canon 映射"))?;
        for id in [source_id, actual_character_id.as_str()] {
            legacy_maps.push(OagLegacyIdMap {
                source_table: "characters".to_string(),
                source_id: format!("{world_id}:canon:characters:{id}"),
                world_id: world_id.to_string(),
                session_id: String::new(),
                target_table: "oag_objects".to_string(),
                target_id: target_id.clone(),
            });
        }
    }
    if let Some(location_type) = location_type {
        for node in topology.nodes {
            let canonical_key = format!("{location_type}:{}", node.node_id);
            let object = canon_object(
                world_id,
                &location_type,
                &canonical_key,
                &node.label,
                Vec::new(),
                &now,
            )?;
            insert_generated_object(&mut objects_by_key, object)?;
        }
    }
    let mut explicit_keys = HashSet::new();
    for object in &canon.objects {
        let key = object.canonical_key.trim();
        if key.is_empty() || key.len() > 256 {
            return Err("Canon 对象 canonical_key 长度必须为 1..256".to_string());
        }
        if !explicit_keys.insert(key.to_string()) {
            return Err(format!("objects.json 的 canonical_key 重复：{key}"));
        }
        let object_type = object.object_type.trim();
        require_object_type(&resolved.ontology, object_type)?;
        let prepared = canon_object(
            world_id,
            object_type,
            key,
            &object.name,
            normalized_aliases(&object.aliases),
            &now,
        )?;
        objects_by_key.insert(key.to_string(), prepared);
    }
    let objects = objects_by_key.into_values().collect::<Vec<_>>();
    let mut attributes = Vec::new();
    for object in &objects {
        let explicit = canon
            .objects
            .iter()
            .find(|item| item.canonical_key.trim() == object.canonical_key);
        let mut values = explicit
            .map(|item| item.attributes.clone())
            .unwrap_or_default();
        if object.object_type == character_type
            && ontology::attribute_definition(&resolved.ontology, &object.object_type, "role")
                .is_some()
        {
            if let Some(character) = characters.iter().find(|item| {
                format!("{character_type}:{}", item.source_character_id.trim())
                    == object.canonical_key
            }) {
                values.insert(
                    "role".to_string(),
                    serde_json::Value::String(character.role.clone()),
                );
            }
        }
        append_attributes(
            &resolved.ontology,
            world_id,
            object,
            &values,
            &mut attributes,
        )?;
    }
    let links = prepare_links(&resolved.ontology, world_id, canon, &objects, &now)?;
    Ok(PreparedCanon {
        resolved,
        objects,
        attributes,
        links,
        legacy_maps,
        now,
    })
}

fn insert_generated_object(
    objects: &mut HashMap<String, OagObject>,
    object: OagObject,
) -> Result<(), String> {
    if objects.contains_key(&object.canonical_key) {
        return Err(format!("Canon 对象稳定键重复：{}", object.canonical_key));
    }
    objects.insert(object.canonical_key.clone(), object);
    Ok(())
}

fn canon_object(
    world_id: &str,
    object_type: &str,
    canonical_key: &str,
    name: &str,
    aliases: Vec<String>,
    now: &str,
) -> Result<OagObject, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("Canon 对象 {canonical_key} 缺少名称"));
    }
    Ok(OagObject {
        id: format!("obj:{world_id}:canon:{object_type}:{canonical_key}"),
        world_id: world_id.to_string(),
        session_id: String::new(),
        object_type: object_type.to_string(),
        canonical_key: canonical_key.to_string(),
        name: name.to_string(),
        name_normalized: normalize_name(name),
        aliases,
        description: String::new(),
        source: "author".to_string(),
        visibility: "public".to_string(),
        asserted_by_id: None,
        source_event_id: None,
        truth_status: "confirmed".to_string(),
        created_turn: 0,
        updated_turn: 0,
        retired_at: String::new(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

fn append_attributes(
    ontology: &OagOntology,
    world_id: &str,
    object: &OagObject,
    values: &serde_json::Map<String, serde_json::Value>,
    output: &mut Vec<OagAttribute>,
) -> Result<(), String> {
    let definitions = effective_attributes(ontology, &object.object_type);
    let declared = definitions
        .iter()
        .map(|item| item.key.as_str())
        .collect::<HashSet<_>>();
    for key in values.keys() {
        if !declared.contains(key.as_str()) {
            return Err(format!(
                "Canon 对象 {} 使用了未声明属性：{}",
                object.canonical_key, key
            ));
        }
    }
    for definition in definitions {
        let value = values.get(&definition.key).cloned().or_else(|| {
            definition
                .default
                .is_null()
                .then(|| None)
                .unwrap_or_else(|| Some(definition.default.clone()))
        });
        let Some(value) = value else {
            if definition.required {
                return Err(format!(
                    "Canon 对象 {} 缺少必填属性：{}",
                    object.canonical_key, definition.key
                ));
            }
            continue;
        };
        ontology::validate_value(definition, &value)?;
        let key = format!("{}:{}", object.id, definition.key);
        output.push(OagAttribute {
            id: format!("oag-attribute:{world_id}:canon:{}", stable_hash(&key)),
            world_id: world_id.to_string(),
            session_id: String::new(),
            object_id: object.id.clone(),
            key: definition.key.clone(),
            value,
            value_type: definition.value_type.clone(),
            valid_from_turn: 0,
            valid_to_turn: None,
            source: "author".to_string(),
            confidence: 1.0,
            asserted_by_id: None,
            source_event_id: None,
            visibility: "public".to_string(),
            truth_status: "confirmed".to_string(),
            supersedes_id: None,
            created_at: object.created_at.clone(),
        });
    }
    Ok(())
}

fn effective_attributes<'a>(
    ontology: &'a OagOntology,
    object_type: &str,
) -> Vec<&'a crate::models::oag::OagAttributeDefinition> {
    let mut result: Vec<&crate::models::oag::OagAttributeDefinition> = Vec::new();
    let mut current = ontology
        .object_types
        .iter()
        .find(|item| item.key == object_type);
    let mut seen = HashSet::new();
    while let Some(item) = current {
        if !seen.insert(item.key.clone()) {
            break;
        }
        for attribute in &item.attributes {
            if !result.iter().any(|existing| existing.key == attribute.key) {
                result.push(attribute);
            }
        }
        current = item.parent.as_ref().and_then(|parent| {
            ontology
                .object_types
                .iter()
                .find(|item| &item.key == parent)
        });
    }
    result
}

fn prepare_links(
    ontology: &OagOntology,
    world_id: &str,
    canon: &OagCanonData,
    objects: &[OagObject],
    now: &str,
) -> Result<Vec<OagLink>, String> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for link in &canon.links {
        let definition = ontology::link_definition(ontology, &link.link_type)
            .ok_or_else(|| format!("objects.json 使用了未声明关系：{}", link.link_type))?;
        let source = resolve_reference(&link.from, objects)?.clone();
        if !type_matches(ontology, &source.object_type, &definition.domain) {
            return Err(format!(
                "关系 {} 的起点类型不符合 domain：{}",
                link.link_type, source.canonical_key
            ));
        }
        let (target_id, target_value, target_type) = match resolve_target(&link.to, objects)? {
            Target::Object(object) => (
                Some(object.id.clone()),
                String::new(),
                Some(object.object_type.clone()),
            ),
            Target::Literal(value) => {
                if !definition.range.is_empty()
                    && !definition.range.iter().any(|item| item == "*")
                    && !definition
                        .range
                        .iter()
                        .any(|item| literal_matches_type(item, &value))
                {
                    return Err(format!("关系 {} 的字面量不符合 range", link.link_type));
                }
                let serialized =
                    serde_json::to_string(&value).map_err(|error| error.to_string())?;
                (None, serialized, None)
            }
        };
        if let Some(target_type) = target_type {
            let object_range = definition
                .range
                .iter()
                .filter(|item| !is_literal_type(item))
                .cloned()
                .collect::<Vec<_>>();
            if !definition.range.is_empty() && object_range.is_empty() {
                return Err(format!("关系 {} 的终点不接受对象", link.link_type));
            }
            if !object_range.is_empty() && !type_matches(ontology, &target_type, &object_range) {
                return Err(format!(
                    "关系 {} 的终点类型不符合 range：{}",
                    link.link_type, link.link_type
                ));
            }
        }
        let identity = format!(
            "{}:{}:{}",
            source.id,
            link.link_type,
            target_id.as_deref().unwrap_or(&target_value)
        );
        if !seen.insert(identity.clone()) {
            return Err(format!("objects.json 的关系重复：{identity}"));
        }
        output.push(OagLink {
            id: format!("oag-link:{world_id}:canon:{}", stable_hash(&identity)),
            world_id: world_id.to_string(),
            session_id: String::new(),
            from_object_id: source.id,
            link_type: link.link_type.clone(),
            to_object_id: target_id,
            to_value: target_value,
            original_predicate: link.original_predicate.trim().to_string(),
            attributes: serde_json::json!({}),
            valid_from_turn: 0,
            valid_to_turn: None,
            source: "author".to_string(),
            confidence: 1.0,
            asserted_by_id: None,
            source_event_id: None,
            visibility: "public".to_string(),
            truth_status: "confirmed".to_string(),
            inferred: false,
            supersedes_id: None,
            created_at: now.to_string(),
        });
    }
    Ok(output)
}

enum Target<'a> {
    Object(&'a OagObject),
    Literal(serde_json::Value),
}

fn resolve_target<'a>(
    value: &serde_json::Value,
    objects: &'a [OagObject],
) -> Result<Target<'a>, String> {
    if value.is_string() {
        return resolve_reference(value.as_str().unwrap_or_default(), objects).map(Target::Object);
    }
    let object = value
        .as_object()
        .ok_or_else(|| "关系 to 必须是稳定键或对象".to_string())?;
    if let Some(literal) = object.get("literal") {
        if object.len() == 1 {
            return Ok(Target::Literal(literal.clone()));
        }
    }
    for key in ["canonical_key", "id", "name"] {
        if let Some(reference) = object.get(key).and_then(serde_json::Value::as_str) {
            let mut candidates = objects
                .iter()
                .filter(|item| match key {
                    "canonical_key" => item.canonical_key == reference.trim(),
                    "id" => item.id == reference.trim(),
                    _ => {
                        item.name == reference.trim()
                            || item.aliases.iter().any(|alias| alias == reference.trim())
                    }
                })
                .collect::<Vec<_>>();
            if let Some(object_type) = object
                .get("object_type")
                .and_then(serde_json::Value::as_str)
            {
                candidates.retain(|item| item.object_type == object_type);
            }
            return unique_reference(reference, candidates).map(Target::Object);
        }
    }
    Err("关系 to 对象需要 canonical_key、id、name 或 literal".to_string())
}

fn resolve_reference<'a>(
    reference: &str,
    objects: &'a [OagObject],
) -> Result<&'a OagObject, String> {
    let reference = reference.trim();
    if let Some(object) = objects.iter().find(|item| item.canonical_key == reference) {
        return Ok(object);
    }
    if let Some(object) = objects.iter().find(|item| item.id == reference) {
        return Ok(object);
    }
    let candidates = objects
        .iter()
        .filter(|item| {
            item.name == reference || item.aliases.iter().any(|alias| alias == reference)
        })
        .collect::<Vec<_>>();
    unique_reference(reference, candidates)
}

fn unique_reference<'a>(
    reference: &str,
    mut candidates: Vec<&'a OagObject>,
) -> Result<&'a OagObject, String> {
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    match candidates.as_slice() {
        [object] => Ok(*object),
        [] => Err(format!("关系引用了不存在的 Canon 对象：{reference}")),
        _ => Err(format!("关系引用存在歧义，拒绝写入：{reference}")),
    }
}

fn require_object_type(ontology: &OagOntology, key: &str) -> Result<(), String> {
    if ontology.object_types.iter().any(|item| item.key == key) {
        Ok(())
    } else {
        Err(format!("Canon 对象使用了未声明类型：{key}"))
    }
}

fn normalized_aliases(aliases: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    aliases
        .iter()
        .map(|alias| alias.trim())
        .filter(|alias| !alias.is_empty())
        .filter(|alias| seen.insert(alias.to_string()))
        .map(str::to_string)
        .collect()
}

pub(super) fn normalize_name(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| !character.is_whitespace() && !character.is_control())
        .collect()
}

fn stable_hash(value: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("dream-oag:{value}").as_bytes(),
    )
    .to_string()
}

fn is_literal_type(value: &str) -> bool {
    matches!(value, "literal" | "string" | "number" | "boolean" | "json")
}

fn literal_matches_type(value_type: &str, value: &serde_json::Value) -> bool {
    match value_type {
        "literal" | "json" => true,
        "string" => value.is_string(),
        "number" => value.as_f64().is_some(),
        "boolean" => value.is_boolean(),
        _ => false,
    }
}

fn save_active_ontology(
    conn: &Connection,
    world_id: &str,
    resolved: &ResolvedOntology,
) -> Result<(), String> {
    let repo = WorldRecordRepository::new(conn);
    let request = WorldRecordWriteRequest {
        collection: ACTIVE_ONTOLOGY_COLLECTION.to_string(),
        data: serde_json::to_value(ActiveOntology {
            ontology: resolved.ontology.clone(),
            mode: if resolved.compatibility {
                "compatibility".to_string()
            } else {
                "world_package".to_string()
            },
        })
        .map_err(|error| error.to_string())?,
    };
    repo.upsert_with_id(world_id, &active_ontology_id(world_id), &request)?;
    Ok(())
}

fn active_ontology_id(world_id: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("dream-oag:active-ontology:{world_id}").as_bytes(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::oag::{
        OagAttributeDefinition, OagCanonObject, OagLinkTypeDefinition, OagObjectTypeDefinition,
    };

    fn database() -> Connection {
        let conn = Connection::open_in_memory().expect("database");
        crate::db::schema::create_tables(&conn).expect("schema");
        crate::db::oag_migration::apply(&conn).expect("oag schema");
        conn.execute(
            "INSERT INTO worlds (id, name) VALUES ('world-a', 'World A')",
            [],
        )
        .expect("world");
        conn
    }

    fn ontology() -> OagOntology {
        OagOntology {
            object_types: vec![
                OagObjectTypeDefinition {
                    key: "character".to_string(),
                    label: "Character".to_string(),
                    parent: None,
                    actor: true,
                    platform_role: String::new(),
                    attributes: vec![OagAttributeDefinition {
                        key: "role".to_string(),
                        label: "Role".to_string(),
                        value_type: "string".to_string(),
                        required: false,
                        default: serde_json::Value::Null,
                        enum_options: Vec::new(),
                        functional: true,
                        temporal: false,
                    }],
                },
                OagObjectTypeDefinition {
                    key: "location".to_string(),
                    label: "Location".to_string(),
                    parent: None,
                    actor: false,
                    platform_role: "location".to_string(),
                    attributes: Vec::new(),
                },
                OagObjectTypeDefinition {
                    key: "item".to_string(),
                    label: "Item".to_string(),
                    parent: None,
                    actor: false,
                    platform_role: String::new(),
                    attributes: vec![OagAttributeDefinition {
                        key: "condition".to_string(),
                        label: "Condition".to_string(),
                        value_type: "string".to_string(),
                        required: true,
                        default: serde_json::Value::Null,
                        enum_options: Vec::new(),
                        functional: true,
                        temporal: false,
                    }],
                },
            ],
            link_types: vec![
                OagLinkTypeDefinition {
                    key: "holds".to_string(),
                    label: "Holds".to_string(),
                    domain: vec!["character".to_string()],
                    range: vec!["item".to_string()],
                    functional: false,
                    symmetric: false,
                    transitive: false,
                    temporal: true,
                    inverse: None,
                },
                OagLinkTypeDefinition {
                    key: "motto".to_string(),
                    label: "Motto".to_string(),
                    domain: vec!["character".to_string()],
                    range: vec!["string".to_string()],
                    functional: true,
                    symmetric: false,
                    transitive: false,
                    temporal: false,
                    inverse: None,
                },
            ],
            character_object_type: Some("character".to_string()),
            location_object_type: Some("location".to_string()),
        }
    }

    fn character() -> CharacterPackageData {
        CharacterPackageData {
            source_character_id: "hero".to_string(),
            name: "Hero".to_string(),
            role: "adventurer".to_string(),
            background_prompt: "private prompt".to_string(),
            model: String::new(),
            memory_strategy: "default".to_string(),
            recent_dialogue_rounds: 6,
            attributes: Vec::new(),
            portrait_assets: Vec::new(),
            avatar_asset: String::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
            knowledge_base_ids: Vec::new(),
        }
    }

    fn canon() -> OagCanonData {
        OagCanonData {
            objects: vec![OagCanonObject {
                object_type: "item".to_string(),
                canonical_key: "key".to_string(),
                name: "Brass Key".to_string(),
                aliases: vec!["key".to_string()],
                attributes: serde_json::json!({ "condition": "old" })
                    .as_object()
                    .expect("attributes")
                    .clone(),
            }],
            links: vec![
                OagCanonLink {
                    from: "character:hero".to_string(),
                    link_type: "holds".to_string(),
                    to: serde_json::json!("key"),
                    original_predicate: String::new(),
                },
                OagCanonLink {
                    from: "character:hero".to_string(),
                    link_type: "motto".to_string(),
                    to: serde_json::json!({ "literal": "keep going" }),
                    original_predicate: String::new(),
                },
            ],
        }
    }

    fn import_canon(
        conn: &Connection,
        ontology: &OagOntology,
        canon: &OagCanonData,
    ) -> Result<ResolvedOntology, String> {
        let characters = vec![character()];
        let ids = HashMap::from([("hero".to_string(), "legacy-character".to_string())]);
        import(
            conn,
            "world-a",
            Some(ontology),
            canon,
            &characters,
            &ids,
            &serde_json::json!({
                "nodes": [{ "id": "town", "label": "Town" }]
            }),
        )
    }

    #[test]
    fn canon_import_is_idempotent_and_retires_missing_objects() {
        let conn = database();
        let declared = ontology();
        let data = canon();
        import_canon(&conn, &declared, &data).expect("first import");
        let first = OagRepository::new(&conn)
            .list_canon_objects("world-a")
            .expect("objects");
        assert_eq!(first.len(), 3);
        assert_eq!(
            OagRepository::new(&conn)
                .list_canon_attributes("world-a")
                .expect("attributes")
                .len(),
            2
        );
        assert_eq!(
            OagRepository::new(&conn)
                .list_canon_links("world-a")
                .expect("links")
                .len(),
            2
        );

        import_canon(&conn, &declared, &data).expect("second import");
        let second = OagRepository::new(&conn)
            .list_canon_objects("world-a")
            .expect("objects");
        assert_eq!(
            first.iter().map(|item| &item.id).collect::<Vec<_>>(),
            second.iter().map(|item| &item.id).collect::<Vec<_>>()
        );

        let empty = OagCanonData::default();
        import_canon(&conn, &declared, &empty).expect("retire import");
        let retired: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_objects
                 WHERE canonical_key = 'key' AND retired_at <> ''",
                [],
                |row| row.get(0),
            )
            .expect("retired count");
        assert_eq!(retired, 1);
        import_canon(&conn, &declared, &data).expect("restore import");
        let restored: String = conn
            .query_row(
                "SELECT retired_at FROM oag_objects WHERE canonical_key = 'key'",
                [],
                |row| row.get(0),
            )
            .expect("restored object");
        assert!(restored.is_empty());
    }

    #[test]
    fn canon_preview_reports_changes_without_writing() {
        let conn = database();
        let declared = ontology();
        let data = canon();
        import_canon(&conn, &declared, &data).expect("import");
        let before = export(&conn, "world-a").expect("export before");
        let characters = vec![character()];
        let map_nodes = serde_json::json!({
            "nodes": [{ "id": "town", "label": "Town" }]
        });
        let unchanged = preview(
            &conn,
            Some("world-a"),
            Some(&declared),
            &data,
            &characters,
            &map_nodes,
        )
        .expect("unchanged preview");
        assert!(unchanged.is_upgrade);
        assert_eq!(unchanged.added_count, 0);
        assert_eq!(unchanged.modified_count, 0);
        assert_eq!(unchanged.retired_count, 0);
        assert_eq!(unchanged.unchanged_count, 11);

        let mut modified = data.clone();
        modified.objects[0].name = "Silver Key".to_string();
        modified.objects[0]
            .attributes
            .insert("condition".to_string(), serde_json::json!("new"));
        modified.links.remove(1);
        let diff = preview(
            &conn,
            Some("world-a"),
            Some(&declared),
            &modified,
            &characters,
            &map_nodes,
        )
        .expect("changed preview");
        assert_eq!(diff.modified_count, 1);
        assert_eq!(diff.retired_count, 1);
        assert_eq!(diff.unchanged_count, 9);
        assert_eq!(diff.object_changes[0].key, "key");
        assert_eq!(diff.link_changes[0].kind, OagCanonImportChangeKind::Retired);

        let new_world = preview(&conn, None, Some(&declared), &data, &characters, &map_nodes)
            .expect("new world preview");
        assert!(!new_world.is_upgrade);
        assert_eq!(new_world.added_count, 11);
        assert_eq!(new_world.unchanged_count, 0);
        assert_eq!(
            serde_json::to_value(before).expect("before json"),
            serde_json::to_value(export(&conn, "world-a").expect("export after"))
                .expect("after json")
        );
    }

    #[test]
    fn invalid_canon_does_not_replace_active_ontology_or_objects() {
        let conn = database();
        let declared = ontology();
        import_canon(&conn, &declared, &canon()).expect("valid import");
        let before = OagRepository::new(&conn)
            .list_object_types("world-a")
            .expect("types")
            .len();
        let mut invalid = canon();
        invalid.objects[0]
            .attributes
            .insert("unknown".to_string(), serde_json::json!("value"));
        let error = import_canon(&conn, &declared, &invalid).expect_err("invalid attribute");
        assert!(error.contains("未声明属性"));
        assert_eq!(
            OagRepository::new(&conn)
                .list_object_types("world-a")
                .expect("types")
                .len(),
            before
        );
        assert_eq!(
            OagRepository::new(&conn)
                .list_canon_objects("world-a")
                .expect("objects")
                .len(),
            3
        );
    }

    #[test]
    fn canon_persistence_failure_rolls_back_the_whole_import() {
        let conn = database();
        import_canon(&conn, &ontology(), &canon()).expect("valid import");
        let before = export(&conn, "world-a")
            .expect("export before")
            .expect("oag");
        conn.execute_batch(
            "CREATE TRIGGER reject_canon_links
             BEFORE INSERT ON oag_links
             BEGIN SELECT RAISE(ABORT, 'forced link failure'); END;",
        )
        .expect("trigger");
        let error = import_canon(&conn, &ontology(), &canon()).expect_err("forced failure");
        assert!(error.contains("forced link failure"));
        let after = export(&conn, "world-a")
            .expect("export after")
            .expect("oag");
        assert_eq!(
            serde_json::to_value(before).expect("before json"),
            serde_json::to_value(after).expect("after json")
        );
    }

    #[test]
    fn active_ontology_records_are_isolated_by_world() {
        let conn = database();
        conn.execute(
            "INSERT INTO worlds (id, name) VALUES ('world-b', 'World B')",
            [],
        )
        .expect("second world");
        let declared = ontology();
        let characters = vec![character()];
        let ids = HashMap::from([("hero".to_string(), "legacy-b".to_string())]);
        for world_id in ["world-a", "world-b"] {
            import(
                &conn,
                world_id,
                Some(&declared),
                &canon(),
                &characters,
                &ids,
                &serde_json::json!({ "nodes": [] }),
            )
            .expect("import");
            assert!(load_active_ontology(&conn, world_id)
                .expect("load")
                .is_some());
        }
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_records WHERE collection = 'oag_ontology'",
                [],
                |row| row.get(0),
            )
            .expect("record count");
        assert_eq!(count, 2);
    }

    #[test]
    fn export_contains_only_canon_and_round_trips_links() {
        let conn = database();
        let declared = ontology();
        import_canon(&conn, &declared, &canon()).expect("import");
        let repo = OagRepository::new(&conn);
        let template = repo
            .object_by_canonical_key("world-a", "", "character:hero")
            .expect("character")
            .expect("character object");
        let mut session = template.clone();
        session.id = "obj:world-a:session:s1:character:temp".to_string();
        session.session_id = "s1".to_string();
        session.canonical_key = "character:temp".to_string();
        session.name = "Temporary".to_string();
        repo.upsert_object(&session).expect("session object");

        let (exported_ontology, exported) = export(&conn, "world-a").expect("export").expect("oag");
        assert_eq!(exported_ontology.object_types.len(), 3);
        assert_eq!(exported.objects.len(), 3);
        assert_eq!(exported.links.len(), 2);
        assert!(exported
            .links
            .iter()
            .any(|link| { link.to == serde_json::json!({ "literal": "keep going" }) }));
        import_canon(&conn, &declared, &exported).expect("round trip");
        assert_eq!(
            OagRepository::new(&conn)
                .list_canon_links("world-a")
                .expect("links")
                .len(),
            2
        );
    }
}
