use crate::models::session::{SessionMapEdge, SessionMapNode};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct CompiledMapTopology {
    pub scene_names: Vec<String>,
    pub nodes: Vec<SessionMapNode>,
    pub edges: Vec<SessionMapEdge>,
}

#[derive(Debug, Clone)]
struct FlatMapNode {
    id: String,
    label: String,
    parent_id: Option<String>,
}

pub fn normalize_map_topology(value: &Value) -> Value {
    if value.is_object() {
        return value.clone();
    }

    empty_map_topology()
}

pub fn extract_scene_names(value: &Value) -> Vec<String> {
    compile_map_topology(value, "").scene_names
}

pub fn compile_map_topology(value: &Value, current_scene: &str) -> CompiledMapTopology {
    let mut flat_nodes = Vec::new();
    let mut explicit_edges = Vec::new();

    if let Some(object) = value.as_object() {
        if let Some(root) = object.get("root").or_else(|| object.get("tree")) {
            collect_hierarchy_nodes(root, None, &mut flat_nodes);
        } else if let Some(nodes) = object.get("nodes").and_then(Value::as_array) {
            for node in nodes {
                collect_single_node(node, None, &mut flat_nodes);
            }
        }

        if let Some(edges) = object.get("edges").and_then(Value::as_array) {
            for edge in edges {
                let source = edge
                    .get("source")
                    .or_else(|| edge.get("from"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let target = edge
                    .get("target")
                    .or_else(|| edge.get("to"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                if let (Some(source), Some(target)) = (source, target) {
                    explicit_edges.push((source.to_string(), target.to_string()));
                }
            }
        }
    }

    let mut seen_ids = HashSet::new();
    let mut id_by_label = HashMap::new();
    let mut nodes = Vec::new();
    for flat in flat_nodes {
        let mut id = if flat.id.trim().is_empty() {
            slugify_scene_id(&flat.label)
        } else {
            slugify_scene_id(&flat.id)
        };
        if id.trim().is_empty() {
            id = format!("scene-{}", nodes.len() + 1);
        }
        let base_id = id.clone();
        let mut suffix = 2;
        while !seen_ids.insert(id.clone()) {
            id = format!("{base_id}-{suffix}");
            suffix += 1;
        }
        id_by_label.insert(flat.label.clone(), id.clone());
        nodes.push((
            SessionMapNode {
                node_id: id,
                label: flat.label,
                discovered: true,
                current: false,
            },
            flat.parent_id,
        ));
    }

    let mut edge_keys = HashSet::new();
    let mut edges = Vec::new();
    for (node, parent_id) in &nodes {
        if let Some(parent_id) = parent_id.as_deref().map(slugify_scene_id).filter(|id| !id.is_empty()) {
            add_edge(&mut edges, &mut edge_keys, &parent_id, &node.node_id);
        }
    }
    for (source, target) in explicit_edges {
        let source_id = id_by_label.get(&source).cloned().unwrap_or_else(|| slugify_scene_id(&source));
        let target_id = id_by_label.get(&target).cloned().unwrap_or_else(|| slugify_scene_id(&target));
        add_edge(&mut edges, &mut edge_keys, &source_id, &target_id);
    }

    let current_scene = current_scene.trim();
    let scene_names = nodes.iter().map(|(node, _)| node.label.clone()).collect::<Vec<_>>();
    let mut session_nodes = nodes.into_iter().map(|(mut node, _)| {
        node.current = !current_scene.is_empty() && node.label.trim() == current_scene;
        node
    }).collect::<Vec<_>>();
    if !current_scene.is_empty() && !session_nodes.iter().any(|node| node.current) {
        let node_id = slugify_scene_id(current_scene);
        session_nodes.push(SessionMapNode {
            node_id: node_id.clone(),
            label: current_scene.to_string(),
            discovered: true,
            current: true,
        });
    }

    CompiledMapTopology {
        scene_names,
        nodes: session_nodes,
        edges,
    }
}

fn collect_hierarchy_nodes(value: &Value, parent_id: Option<String>, nodes: &mut Vec<FlatMapNode>) {
    let current_id = collect_single_node(value, parent_id, nodes);
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            collect_hierarchy_nodes(child, current_id.clone(), nodes);
        }
    }
}

fn collect_single_node(value: &Value, parent_id: Option<String>, nodes: &mut Vec<FlatMapNode>) -> Option<String> {
    let object = value.as_object()?;
    let label = object
        .get("label")
        .or_else(|| object.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let id = object
        .get("id")
        .or_else(|| object.get("node_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(slugify_scene_id)
        .unwrap_or_else(|| slugify_scene_id(label));
    nodes.push(FlatMapNode {
        id: id.clone(),
        label: label.to_string(),
        parent_id,
    });
    Some(id)
}

fn add_edge(
    edges: &mut Vec<SessionMapEdge>,
    edge_keys: &mut HashSet<String>,
    source_id: &str,
    target_id: &str,
) {
    if source_id.is_empty() || target_id.is_empty() || source_id == target_id {
        return;
    }
    let key = format!("{source_id}->{target_id}");
    if !edge_keys.insert(key.clone()) {
        return;
    }
    edges.push(SessionMapEdge {
        edge_id: key,
        source_node_id: source_id.to_string(),
        target_node_id: target_id.to_string(),
    });
}

pub fn slugify_scene_id(value: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if ch.is_alphanumeric() {
            output.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            output.push('-');
            previous_dash = true;
        }
    }
    output.trim_matches('-').to_string()
}

fn empty_map_topology() -> Value {
    serde_json::json!({
        "version": 1,
        "nodes": []
    })
}
