use crate::models::session::*;

use super::run::*;
use super::writeback::{slugify_scene_id, unique_strings};

pub(crate) fn build_runtime_updated_session_snapshot(
    input: &RuntimeMutationInput<'_>,
) -> SessionSnapshot {
    let resolved_scene_runtime = input
        .runtime_application
        .scene_runtime
        .clone()
        .unwrap_or_else(|| {
            let explicit_visible = if input.scene_visible_characters_explicit {
                input
                    .scene_visible_characters
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| input.visible_chars.to_vec())
            } else {
                input.visible_chars.to_vec()
            };
            SceneRuntime {
                scene_id: slugify_scene_id(input.next_scene_name),
                name: input.next_scene_name.to_string(),
                background_hint: input.next_scene_background_hint.clone(),
                temporary_tags: input.runtime_application.scene_tags.clone(),
                present_characters: build_turn_participants(
                    &explicit_visible,
                    &input.session.player_character_name,
                ),
            }
        });
    let mut messages = input.messages.to_vec();
    messages.extend(
        input
            .runtime_application
            .system_messages
            .iter()
            .filter(|message| should_persist_session_message(message))
            .cloned(),
    );
    let latest_agent_message = messages
        .iter()
        .rev()
        .find(|message| message.role == "agent" && !message.content.trim().is_empty());
    let latest_agent_narration = messages
        .iter()
        .rev()
        .filter(|message| message.role == "agent")
        .filter_map(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("narration"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .next();
    let current_line = latest_agent_narration
        .or_else(|| {
            input
                .current_line
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            let previous = input.session.current_line.trim();
            if previous.is_empty() {
                None
            } else {
                Some(previous.to_string())
            }
        })
        .unwrap_or_default();
    SessionSnapshot {
        id: input.session.id.clone(),
        world_name: input.session.world_name.clone(),
        location: input.next_location.to_string(),
        time_label: input.next_time_label.to_string(),
        current_speaker: latest_agent_message
            .and_then(|message| message.speaker.clone())
            .unwrap_or_default(),
        current_line,
        player_character_id: input.session.player_character_id.clone(),
        player_character_name: input.session.player_character_name.clone(),
        visible_characters: input.visible_chars.to_vec(),
        messages,
        player_stats: input
            .runtime_application
            .player_stats
            .clone()
            .unwrap_or_else(|| input.session.player_stats.clone()),
        map_graph_nodes: update_current_map_graph_nodes(
            &input.session.map_graph_nodes,
            &resolved_scene_runtime.name,
            input.next_location,
        ),
        map_graph_edges: input.session.map_graph_edges.clone(),
        inventory_items: input
            .runtime_application
            .inventory_items
            .clone()
            .unwrap_or_else(|| input.session.inventory_items.clone()),
        system_log: append_system_log(
            &merge_system_log_lines(
                &merge_system_log_lines(
                    &input.session.system_log,
                    &input.runtime_application.system_log_lines,
                ),
                &input.runtime_application.tool_call_logs,
            ),
            input.turn_index,
            input.next_scene_name,
            input.next_location,
            input.next_time_label,
            input.planned_speakers,
        ),
        scene: SceneRuntime {
            scene_id: resolved_scene_runtime.scene_id,
            name: resolved_scene_runtime.name,
            background_hint: resolved_scene_runtime.background_hint,
            temporary_tags: resolved_scene_runtime.temporary_tags,
            present_characters: resolved_scene_runtime.present_characters,
        },
        assets: input.session.assets.clone(),
        state: SessionState {
            metrics: input.runtime_application.state_metrics.clone(),
            tags: input.runtime_application.state_tags.clone(),
            phase: if input.runtime_application.state_phase.trim().is_empty() {
                input.session.state.phase.clone()
            } else {
                input.runtime_application.state_phase.clone()
            },
        },
        // 生成参数是玩家偏好，不由模型提议、也不随回合状态变化，原样带过。
        generation_params: input.session.generation_params.clone(),
    }
}

pub(crate) fn materialize_completed_speaker_messages(
    recovery_journal: &[serde_json::Value],
    turn_index: i32,
) -> Vec<ChatMessage> {
    let mut steps = recovery_journal
        .iter()
        .filter_map(|entry| {
            let step = entry.get("step").and_then(|value| value.as_str())?;
            let status = entry.get("status").and_then(|value| value.as_str())?;
            if status != "completed"
                || !step.starts_with("speaker_")
                || !step.ends_with("_completed")
            {
                return None;
            }
            let payload = entry.get("payload")?;
            let llm_output = payload.get("llm_output")?;
            let speaker = llm_output
                .get("speaker")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())?;
            let content = llm_output
                .get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .unwrap_or_default();
            let order = step
                .trim_start_matches("speaker_")
                .trim_end_matches("_completed")
                .parse::<i32>()
                .unwrap_or_default();
            Some((order, speaker, content, llm_output.clone()))
        })
        .collect::<Vec<_>>();
    steps.sort_by(|left, right| left.0.cmp(&right.0));
    steps.into_iter()
        .map(|(_, speaker, content, llm_output)| ChatMessage {
            message_id: ChatMessage::generate_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
            role: "agent".to_string(),
            content: MessageContent::Text(content),
            speaker: Some(speaker),
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "message_kind": "agent_response",
                "recovered": true,
                "narration": llm_output.get("narration").cloned().unwrap_or(serde_json::Value::Null),
                "raw_response": llm_output.get("raw_content").cloned().unwrap_or(serde_json::Value::Null),
            })),
        })
        .collect()
}

pub(crate) fn should_persist_session_message(message: &ChatMessage) -> bool {
    if message.role != "system" {
        return true;
    }
    let action_type = message
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("action_type"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    matches!(
        action_type,
        "director_trace" | "switch_character" | "character_created" | "world_interaction"
    )
}

pub(crate) fn merge_system_log_lines(current: &[String], additions: &[String]) -> Vec<String> {
    let mut merged = current.to_vec();
    for line in additions {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

pub(crate) fn append_system_log(
    current: &[String],
    turn_index: i32,
    scene_name: &str,
    location: &str,
    time_label: &str,
    visible_characters: &[String],
) -> Vec<String> {
    let mut next = current.to_vec();
    if !visible_characters.is_empty() {
        next.push(format!("Speaker order: {}", visible_characters.join(" / ")));
    }
    next.push(format!(
        "Turn {}: scene={}, location={}, time={}, visible={}",
        turn_index,
        scene_name,
        location,
        time_label,
        visible_characters.join(" / ")
    ));
    next
}

pub(crate) fn update_current_map_graph_nodes(
    nodes: &[SessionMapNode],
    scene_name: &str,
    location: &str,
) -> Vec<SessionMapNode> {
    let target_node_id = [scene_name, location].into_iter().find_map(|candidate| {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            return None;
        }
        nodes
            .iter()
            .find(|node| {
                node.label.trim() == candidate
                    || node.node_id.trim().eq_ignore_ascii_case(candidate)
            })
            .map(|node| node.node_id.clone())
    });

    let Some(target_node_id) = target_node_id else {
        return nodes.to_vec();
    };

    nodes
        .iter()
        .cloned()
        .map(|mut node| {
            node.current = node.node_id == target_node_id;
            node
        })
        .collect()
}

pub(crate) fn merge_visible_characters(
    existing: &[String],
    additions: Vec<String>,
    player_character_name: &str,
) -> Vec<String> {
    let mut merged = existing.to_vec();
    for name in additions {
        if name != player_character_name && !merged.contains(&name) {
            merged.push(name);
        }
    }
    merged
}

pub(crate) fn build_turn_participants(
    visible_character_names: &[String],
    player_character_name: &str,
) -> Vec<String> {
    let mut names = unique_strings(
        visible_character_names
            .iter()
            .cloned()
            .chain(std::iter::once(player_character_name.to_string()))
            .collect::<Vec<_>>(),
    );
    if !names.iter().any(|name| name == player_character_name) {
        names.push(player_character_name.to_string());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::update_current_map_graph_nodes;
    use crate::models::session::SessionMapNode;

    fn node(id: &str, label: &str, current: bool) -> SessionMapNode {
        SessionMapNode {
            node_id: id.to_string(),
            label: label.to_string(),
            discovered: true,
            current,
        }
    }

    #[test]
    fn map_current_node_follows_scene_before_location() {
        let updated = update_current_map_graph_nodes(
            &[
                node("luoyang", "Luoyang", true),
                node("wu", "\u{5434}\u{56fd}", false),
            ],
            "\u{5434}\u{56fd}",
            "Luoyang",
        );

        assert!(!updated[0].current);
        assert!(updated[1].current);
    }

    #[test]
    fn map_current_node_uses_location_when_scene_is_not_a_map_node() {
        let updated = update_current_map_graph_nodes(
            &[node("luoyang", "Luoyang", true), node("wu", "Wu", false)],
            "Wu Capital",
            "Wu",
        );

        assert!(!updated[0].current);
        assert!(updated[1].current);
    }

    #[test]
    fn map_current_node_keeps_existing_state_when_no_map_node_matches() {
        let updated = update_current_map_graph_nodes(
            &[node("luoyang", "Luoyang", true), node("wu", "Wu", false)],
            "Unknown",
            "Elsewhere",
        );

        assert!(updated[0].current);
        assert!(!updated[1].current);
    }
}
