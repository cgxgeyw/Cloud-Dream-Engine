use crate::models::oag::{
    OagAttributeDefinition, OagLinkTypeDefinition, OagObjectTypeDefinition, OagOntology,
    OAG_RESERVED_TYPE_KEYS,
};
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedOntology {
    pub ontology: OagOntology,
    pub character_object_type: Option<String>,
    pub location_object_type: Option<String>,
    pub compatibility: bool,
}

pub fn resolve(declared: Option<&OagOntology>) -> Result<ResolvedOntology, String> {
    match declared {
        Some(ontology) => {
            let ontology = validate(ontology)?;
            let character_object_type = bind_type(
                &ontology,
                &ontology.character_object_type,
                |item| item.actor,
                "character",
                "角色",
            )?;
            let location_object_type = bind_type(
                &ontology,
                &ontology.location_object_type,
                |item| item.platform_role == "location",
                "location",
                "地点",
            )?;
            Ok(ResolvedOntology {
                ontology,
                character_object_type,
                location_object_type,
                compatibility: false,
            })
        }
        None => {
            let ontology = OagOntology {
                object_types: vec![
                    OagObjectTypeDefinition {
                        key: "character".to_string(),
                        label: "角色".to_string(),
                        parent: None,
                        actor: true,
                        platform_role: String::new(),
                        attributes: vec![OagAttributeDefinition {
                            key: "role".to_string(),
                            label: "角色定位".to_string(),
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
                        label: "地点".to_string(),
                        parent: None,
                        actor: false,
                        platform_role: "location".to_string(),
                        attributes: Vec::new(),
                    },
                ],
                link_types: Vec::new(),
                character_object_type: Some("character".to_string()),
                location_object_type: Some("location".to_string()),
            };
            Ok(ResolvedOntology {
                ontology,
                character_object_type: Some("character".to_string()),
                location_object_type: Some("location".to_string()),
                compatibility: true,
            })
        }
    }
}

pub fn validate(input: &OagOntology) -> Result<OagOntology, String> {
    if input.object_types.is_empty() {
        return Err("ontology.object_types 至少需要一个类型".to_string());
    }
    let mut ontology = input.clone();
    let mut type_keys = HashSet::new();
    for item in &mut ontology.object_types {
        item.key = normalize_key(&item.key, "object type")?;
        if OAG_RESERVED_TYPE_KEYS.contains(&item.key.as_str()) {
            return Err(format!("对象类型键 {} 是平台保留键", item.key));
        }
        if !type_keys.insert(item.key.clone()) {
            return Err(format!("对象类型键重复：{}", item.key));
        }
        item.label = item.label.trim().to_string();
        if item.label.is_empty() {
            item.label = item.key.clone();
        }
        item.parent = item
            .parent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        item.platform_role = item.platform_role.trim().to_string();
        if !item.platform_role.is_empty() && item.platform_role != "location" {
            return Err(format!(
                "对象类型 {} 的 platform_role 仅支持空值或 location",
                item.key
            ));
        }
        let mut attribute_keys = HashSet::new();
        for attribute in &mut item.attributes {
            attribute.key = normalize_key(&attribute.key, "attribute")?;
            if !attribute_keys.insert(attribute.key.clone()) {
                return Err(format!(
                    "对象类型 {} 的属性键重复：{}",
                    item.key, attribute.key
                ));
            }
            if attribute.label.trim().is_empty() {
                attribute.label = attribute.key.clone();
            }
            validate_value_type(&attribute.value_type)?;
            if attribute.value_type == "enum"
                && (attribute.enum_options.is_empty()
                    || attribute
                        .enum_options
                        .iter()
                        .any(|option| option.trim().is_empty()))
            {
                return Err(format!(
                    "枚举属性 {}.{} 必须提供 enum_options",
                    item.key, attribute.key
                ));
            }
            if !attribute.default.is_null() {
                validate_value(attribute, &attribute.default)?;
            }
        }
    }
    for item in &ontology.object_types {
        if let Some(parent) = &item.parent {
            if !type_keys.contains(parent) {
                return Err(format!("对象类型 {} 的父类型不存在：{}", item.key, parent));
            }
            let mut current = Some(item);
            let mut seen = HashSet::new();
            while let Some(next) = current {
                if !seen.insert(next.key.clone()) {
                    return Err(format!("对象类型 {} 的继承关系存在循环", item.key));
                }
                current = next
                    .parent
                    .as_ref()
                    .and_then(|key| ontology.object_types.iter().find(|item| &item.key == key));
            }
        }
    }

    let mut link_keys = HashSet::new();
    for link in &mut ontology.link_types {
        link.key = normalize_key(&link.key, "link type")?;
        if !link_keys.insert(link.key.clone()) {
            return Err(format!("关系类型键重复：{}", link.key));
        }
        if link.label.trim().is_empty() {
            link.label = link.key.clone();
        }
        link.domain = normalize_type_refs(&link.domain, &type_keys, &link.key, "domain")?;
        link.range = normalize_type_refs(&link.range, &type_keys, &link.key, "range")?;
        link.inverse = link
            .inverse
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    for link in &ontology.link_types {
        if let Some(inverse) = &link.inverse {
            if !link_keys.contains(inverse) {
                return Err(format!("关系 {} 的 inverse 不存在：{}", link.key, inverse));
            }
        }
    }
    if let Some(binding) = ontology.character_object_type.as_deref() {
        let item = ontology
            .object_types
            .iter()
            .find(|item| item.key == binding)
            .ok_or_else(|| format!("character_object_type 不存在：{}", binding))?;
        if !item.actor {
            return Err(format!(
                "character_object_type {} 未标记 actor=true",
                binding
            ));
        }
    }
    if let Some(binding) = ontology.location_object_type.as_deref() {
        let item = ontology
            .object_types
            .iter()
            .find(|item| item.key == binding)
            .ok_or_else(|| format!("location_object_type 不存在：{}", binding))?;
        if item.platform_role != "location" {
            return Err(format!(
                "location_object_type {} 未标记 platform_role=location",
                binding
            ));
        }
    }
    Ok(ontology)
}

fn bind_type(
    ontology: &OagOntology,
    explicit: &Option<String>,
    predicate: impl Fn(&OagObjectTypeDefinition) -> bool,
    preferred_key: &str,
    label: &str,
) -> Result<Option<String>, String> {
    if let Some(key) = explicit {
        return Ok(Some(key.clone()));
    }
    let mut candidates: Vec<_> = ontology
        .object_types
        .iter()
        .filter(|item| predicate(item))
        .collect();
    if candidates.iter().any(|item| item.key == preferred_key) {
        return Ok(Some(preferred_key.to_string()));
    }
    candidates.sort_by(|left, right| left.key.cmp(&right.key));
    match candidates.as_slice() {
        [] => Ok(None),
        [item] => Ok(Some(item.key.clone())),
        _ => Err(format!(
            "{}类型存在歧义，请在 ontology.json 显式指定绑定",
            label
        )),
    }
}

fn normalize_key(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(format!("{label} key 长度必须为 1..64"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(format!(
            "{label} key 只能包含字母、数字、下划线、短横线或点：{value}"
        ));
    }
    Ok(value.to_string())
}

fn normalize_type_refs(
    values: &[String],
    type_keys: &HashSet<String>,
    link_key: &str,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if value == "*" {
            if !normalized.iter().any(|item| item == "*") {
                normalized.push("*".to_string());
            }
            continue;
        }
        if !type_keys.contains(value) && !(field == "range" && is_literal_type(value)) {
            return Err(format!(
                "关系 {} 的 {field} 引用了未知类型：{}",
                link_key, value
            ));
        }
        if !normalized.iter().any(|item| item == value) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

fn is_literal_type(value: &str) -> bool {
    matches!(value, "literal" | "string" | "number" | "boolean" | "json")
}

pub fn validate_value_type(value_type: &str) -> Result<(), String> {
    match value_type {
        "string" | "number" | "boolean" | "enum" | "list" | "json" => Ok(()),
        _ => Err(format!("不支持的 OAG 属性值类型：{}", value_type)),
    }
}

pub fn validate_value(
    definition: &OagAttributeDefinition,
    value: &serde_json::Value,
) -> Result<(), String> {
    let valid = match definition.value_type.as_str() {
        "string" => value.is_string(),
        "number" => value.as_f64().is_some(),
        "boolean" => value.is_boolean(),
        "enum" => value
            .as_str()
            .is_some_and(|value| definition.enum_options.iter().any(|item| item == value)),
        "list" => value.is_array(),
        "json" => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "属性 {} 的值不符合 {} 类型",
            definition.key, definition.value_type
        ))
    }
}

pub fn type_matches(ontology: &OagOntology, actual: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() || allowed.iter().any(|item| item == "*" || item == actual) {
        return true;
    }
    let mut current = ontology.object_types.iter().find(|item| item.key == actual);
    let mut seen = HashSet::new();
    while let Some(item) = current {
        if allowed.iter().any(|allowed| allowed == &item.key) {
            return true;
        }
        if !seen.insert(item.key.clone()) {
            break;
        }
        current = item.parent.as_ref().and_then(|parent| {
            ontology
                .object_types
                .iter()
                .find(|item| &item.key == parent)
        });
    }
    false
}

pub fn attribute_definition<'a>(
    ontology: &'a OagOntology,
    object_type: &str,
    key: &str,
) -> Option<&'a OagAttributeDefinition> {
    let mut current = ontology
        .object_types
        .iter()
        .find(|item| item.key == object_type);
    let mut seen = HashSet::new();
    while let Some(item) = current {
        if !seen.insert(item.key.clone()) {
            break;
        }
        if let Some(attribute) = item
            .attributes
            .iter()
            .find(|attribute| attribute.key == key)
        {
            return Some(attribute);
        }
        current = item.parent.as_ref().and_then(|parent| {
            ontology
                .object_types
                .iter()
                .find(|item| &item.key == parent)
        });
    }
    None
}

pub fn link_definition<'a>(
    ontology: &'a OagOntology,
    key: &str,
) -> Option<&'a OagLinkTypeDefinition> {
    ontology.link_types.iter().find(|item| item.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_type(key: &str, actor: bool, role: &str) -> OagObjectTypeDefinition {
        OagObjectTypeDefinition {
            key: key.to_string(),
            label: key.to_string(),
            parent: None,
            actor,
            platform_role: role.to_string(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn ambiguous_platform_bindings_are_rejected() {
        let ontology = OagOntology {
            object_types: vec![
                object_type("npc", true, ""),
                object_type("monster", true, ""),
                object_type("place", false, "location"),
            ],
            link_types: Vec::new(),
            character_object_type: None,
            location_object_type: None,
        };
        let error = resolve(Some(&ontology)).expect_err("ambiguous actor types");
        assert!(error.contains("角色类型存在歧义"));
    }

    #[test]
    fn explicit_binding_and_inherited_constraints_are_supported() {
        let mut ontology = OagOntology {
            object_types: vec![
                object_type("being", false, ""),
                object_type("npc", true, ""),
                object_type("monster", true, ""),
                object_type("place", false, "location"),
            ],
            link_types: vec![OagLinkTypeDefinition {
                key: "located_in".to_string(),
                label: "位于".to_string(),
                domain: vec!["being".to_string()],
                range: vec!["place".to_string()],
                functional: true,
                symmetric: false,
                transitive: false,
                temporal: true,
                inverse: None,
            }],
            character_object_type: Some("npc".to_string()),
            location_object_type: Some("place".to_string()),
        };
        ontology.object_types[1].parent = Some("being".to_string());
        let resolved = resolve(Some(&ontology)).expect("resolved ontology");
        assert_eq!(resolved.character_object_type.as_deref(), Some("npc"));
        assert!(type_matches(
            &resolved.ontology,
            "npc",
            &["being".to_string()]
        ));
    }

    #[test]
    fn reserved_keys_and_inheritance_cycles_are_rejected() {
        let reserved = OagOntology {
            object_types: vec![object_type("scene", false, "")],
            link_types: Vec::new(),
            character_object_type: None,
            location_object_type: None,
        };
        assert!(validate(&reserved).is_err());

        let mut cyclic = OagOntology {
            object_types: vec![object_type("a", false, ""), object_type("b", false, "")],
            link_types: Vec::new(),
            character_object_type: None,
            location_object_type: None,
        };
        cyclic.object_types[0].parent = Some("b".to_string());
        cyclic.object_types[1].parent = Some("a".to_string());
        assert!(validate(&cyclic).is_err());
    }
}
