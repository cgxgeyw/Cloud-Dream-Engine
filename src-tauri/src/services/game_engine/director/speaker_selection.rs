use crate::models::session::ChatMessage;
use std::collections::BTreeSet;

pub(super) fn parse_planned_speakers(
    parsed_speakers: Vec<String>,
    visible_character_names: &[String],
    fallback: &[String],
    player_character_name: &str,
    player_input: &str,
    world_phase: &str,
    history_messages: &[ChatMessage],
) -> Vec<String> {
    let visible_set = visible_character_names
        .iter()
        .filter(|name| !name.trim().is_empty() && name.as_str() != player_character_name)
        .cloned()
        .collect::<BTreeSet<_>>();
    let parsed = parsed_speakers
        .into_iter()
        .filter(|name| visible_set.contains(name))
        .collect::<Vec<_>>();
    if !parsed.is_empty() {
        return parsed.into_iter().take(4).collect();
    }
    let fallback_visible = fallback
        .iter()
        .filter(|name| visible_set.contains(*name))
        .take(4)
        .cloned()
        .collect::<Vec<_>>();
    if !fallback_visible.is_empty() {
        return fallback_visible;
    }
    let visible = visible_set.into_iter().collect::<Vec<_>>();
    if visible.is_empty() {
        return Vec::new();
    }
    let speaker_limit = resolve_speaker_limit(player_input, world_phase, &visible);
    let mentioned = mentioned_character_names(player_input, &visible);
    let mut ranked = visible
        .iter()
        .map(|name| {
            let mut score = 1.0f64;
            if mentioned.iter().any(|item| item == name) {
                score += 0.85;
            }
            score += recent_speaker_penalty(history_messages, name);
            (name.clone(), score)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut selected = Vec::new();
    for name in mentioned {
        if !selected.contains(&name) {
            selected.push(name);
        }
        if selected.len() >= speaker_limit {
            break;
        }
    }
    for (name, _) in ranked {
        if selected.contains(&name) {
            continue;
        }
        selected.push(name);
        if selected.len() >= speaker_limit {
            break;
        }
    }
    if selected.is_empty() {
        return Vec::new();
    }
    selected
}

fn resolve_speaker_limit(
    player_input: &str,
    world_phase: &str,
    visible_character_names: &[String],
) -> usize {
    let visible_count = visible_character_names.len();
    if visible_count <= 1 {
        return visible_count;
    }
    let mentioned_count = mentioned_character_names(player_input, visible_character_names).len();
    let group_prompt = is_group_prompt(player_input);
    let mut limit = 2usize;
    if group_prompt || mentioned_count >= 2 || matches!(world_phase, "escalation" | "crisis") {
        limit = 3;
    }
    visible_count.min(limit.max(mentioned_count).max(1))
}

fn recent_speaker_penalty(history_messages: &[ChatMessage], character_name: &str) -> f64 {
    let recent_speakers = history_messages
        .iter()
        .rev()
        .filter(|message| message.role == "agent")
        .filter_map(|message| message.speaker.as_deref())
        .map(|speaker| speaker.trim().to_string())
        .filter(|speaker| !speaker.is_empty())
        .take(3)
        .collect::<Vec<_>>();
    let mut penalty = 0.0;
    for (index, recent_speaker) in recent_speakers.iter().enumerate() {
        if recent_speaker != character_name {
            continue;
        }
        penalty += match index {
            0 => -0.45,
            1 => -0.18,
            _ => -0.08,
        };
    }
    penalty
}

fn mentioned_character_names(
    player_input: &str,
    visible_character_names: &[String],
) -> Vec<String> {
    let input = player_input.trim();
    if input.is_empty() {
        return Vec::new();
    }
    let mut matched = visible_character_names
        .iter()
        .filter_map(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return None;
            }
            input.find(trimmed).map(|idx| (idx, trimmed.to_string()))
        })
        .collect::<Vec<_>>();
    matched.sort_by_key(|item| item.0);
    matched.into_iter().fold(Vec::new(), |mut acc, (_, name)| {
        if !acc.contains(&name) {
            acc.push(name);
        }
        acc
    })
}

fn is_group_prompt(player_input: &str) -> bool {
    [
        "你们", "大家", "各位", "together", "分别", "轮流", "都说", "everyone", "挨个",
    ]
    .iter()
    .any(|marker| player_input.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::MessageContent;

    fn agent_message(speaker: &str) -> ChatMessage {
        ChatMessage {
            role: "agent".to_string(),
            content: MessageContent::Text(String::new()),
            speaker: Some(speaker.to_string()),
            metadata: None,
        }
    }

    #[test]
    fn group_prompt_recognizes_chinese_markers() {
        for marker in ["你们", "大家", "各位", "分别", "轮流", "都说", "挨个"] {
            assert!(
                is_group_prompt(&format!("请{marker}回答")),
                "expected marker {marker} to identify a group prompt"
            );
        }

        assert!(!is_group_prompt("请李白回答"));
    }

    #[test]
    fn recent_speaker_is_ranked_after_other_eligible_characters() {
        let visible = vec!["Alice".to_string(), "Bob".to_string(), "Cara".to_string()];
        let history = vec![agent_message("Alice")];

        let selected = parse_planned_speakers(
            Vec::new(),
            &visible,
            &[],
            "Player",
            "continue",
            "opening",
            &history,
        );

        assert_eq!(selected, vec!["Bob".to_string(), "Cara".to_string()]);
    }
}
