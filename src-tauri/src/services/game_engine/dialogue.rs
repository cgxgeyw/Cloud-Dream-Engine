use crate::models::character::{resolve_character_response_contract_prompt, CharacterDefinition};
use crate::services::game_engine::prompting::render_prompt_variables;

pub struct DialoguePipeline;

#[derive(Clone)]
pub struct ParsedCharacterResponse {
    pub speaker: String,
    pub content: String,
    pub intent: String,
    pub emotion: String,
    pub narration: String,
    pub raw_payload: Option<serde_json::Value>,
}

impl DialoguePipeline {
    pub fn new() -> Self {
        Self
    }

    pub fn build_character_system_prompt_with_contract(
        &self,
        speaker_name: &str,
        speaker_profile: Option<&CharacterDefinition>,
        system_prompt_template: Option<&str>,
        response_contract_prompt: Option<&str>,
    ) -> String {
        let base =
            build_character_system_prompt(speaker_name, speaker_profile, system_prompt_template);
        let contract =
            resolve_character_response_contract_prompt(response_contract_prompt.or_else(|| {
                speaker_profile.map(|profile| profile.response_contract_prompt.as_str())
            }));
        if base.trim().is_empty() {
            render_prompt_variables(&contract)
        } else {
            render_prompt_variables(&format!("{base}\n\n{contract}"))
        }
    }

    pub fn parse_character_response(
        &self,
        raw_response_content: &str,
        default_speaker: &str,
    ) -> ParsedCharacterResponse {
        if let Some(partial) =
            self.extract_partial_character_response(raw_response_content, default_speaker)
        {
            let has_closed_json = raw_response_content.trim().ends_with('}');
            if !has_closed_json {
                return partial;
            }
        }
        if let Some(payload) =
            parse_embedded_json(raw_response_content).filter(|value| value.is_object())
        {
            let fallback_content = strip_dialogue_field_artifacts(raw_response_content);
            let speaker = payload
                .get("speaker")
                .map(clean_dialogue_text_value)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| default_speaker.to_string());
            let content = payload
                .get("content")
                .or_else(|| payload.get("response"))
                .or_else(|| payload.get("message"))
                .or_else(|| payload.get("text"))
                .map(clean_dialogue_text_value)
                .filter(|value| !value.trim().is_empty())
                .or_else(|| structured_payload_content_fallback(&payload))
                .unwrap_or(fallback_content);
            let intent = payload
                .get("intent")
                .map(clean_dialogue_text_value)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "advance_objective".to_string());
            let emotion = payload
                .get("emotion")
                .map(clean_dialogue_text_value)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "focused".to_string());
            let narration = payload
                .get("narration")
                .or_else(|| payload.get("scene_narration"))
                .map(clean_dialogue_text_value)
                .unwrap_or_default();
            return ParsedCharacterResponse {
                speaker,
                content,
                intent,
                emotion,
                narration,
                raw_payload: Some(payload),
            };
        }
        if let Some(recovered) =
            extract_recoverable_character_response(raw_response_content, default_speaker)
        {
            return recovered;
        }

        ParsedCharacterResponse {
            speaker: default_speaker.to_string(),
            content: strip_dialogue_field_artifacts(raw_response_content),
            intent: "advance_objective".to_string(),
            emotion: "focused".to_string(),
            narration: String::new(),
            raw_payload: None,
        }
    }

    pub fn extract_partial_character_response(
        &self,
        raw_response_content: &str,
        default_speaker: &str,
    ) -> Option<ParsedCharacterResponse> {
        let speaker = extract_partial_json_string_field(raw_response_content, "speaker")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_speaker.to_string());
        let content = extract_partial_json_string_field(raw_response_content, "content")
            .or_else(|| extract_partial_json_string_field(raw_response_content, "response"))
            .or_else(|| extract_partial_json_string_field(raw_response_content, "message"))
            .or_else(|| extract_partial_json_string_field(raw_response_content, "text"))
            .map(|value| strip_dialogue_field_artifacts(&value))
            .filter(|value| !value.trim().is_empty())?;
        let intent = extract_partial_json_string_field(raw_response_content, "intent")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "advance_objective".to_string());
        let emotion = extract_partial_json_string_field(raw_response_content, "emotion")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "focused".to_string());
        let narration = extract_partial_json_string_field(raw_response_content, "narration")
            .or_else(|| extract_partial_json_string_field(raw_response_content, "scene_narration"))
            .unwrap_or_default();
        Some(ParsedCharacterResponse {
            speaker,
            content,
            intent,
            emotion,
            narration,
            raw_payload: None,
        })
    }
}

pub fn build_character_system_prompt(
    speaker: &str,
    speaker_profile: Option<&CharacterDefinition>,
    system_prompt_template: Option<&str>,
) -> String {
    if let Some(template) = system_prompt_template
        .or_else(|| speaker_profile.map(|profile| profile.system_prompt_template.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return render_character_system_prompt_template(template, speaker, speaker_profile);
    }

    String::new()
}

fn render_character_system_prompt_template(
    template: &str,
    speaker: &str,
    speaker_profile: Option<&CharacterDefinition>,
) -> String {
    let role = speaker_profile
        .map(|profile| profile.role.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let background_prompt = speaker_profile
        .map(|profile| profile.background_prompt.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("");

    let rendered = template
        .replace("{{speaker}}", speaker)
        .replace("{{role}}", role)
        .replace("{{background_prompt}}", background_prompt);

    render_prompt_variables(&rendered)
        .trim()
        .to_string()
}

fn parse_embedded_json(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            if end <= start {
                return None;
            }
            serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]).ok()
        })
}

fn strip_dialogue_field_artifacts(text: &str) -> String {
    text.replace("\\n", "\n")
        .replace("\\\"", "\"")
        .trim()
        .trim_matches('"')
        .trim()
        .to_string()
}

fn clean_dialogue_text_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(boolean) => boolean.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => strip_dialogue_field_artifacts(text),
        serde_json::Value::Array(items) => items
            .iter()
            .map(clean_dialogue_text_value)
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => map
            .get("content")
            .map(clean_dialogue_text_value)
            .unwrap_or_default(),
    }
}

fn structured_payload_content_fallback(payload: &serde_json::Value) -> Option<String> {
    let updates = payload.get("session_attribute_updates")?.as_array()?;
    if updates.is_empty() {
        return None;
    }

    let has_todo_update = updates.iter().any(|item| {
        item.get("key")
            .and_then(|value| value.as_str())
            .map(|value| value == "todo_items")
            .unwrap_or(false)
    });
    let has_completed_update = updates.iter().any(|item| {
        item.get("key")
            .and_then(|value| value.as_str())
            .map(|value| value == "completed_items")
            .unwrap_or(false)
    });

    if has_todo_update && has_completed_update {
        Some("好的，待办事项和已完成事项已更新。".to_string())
    } else if has_todo_update {
        Some("好的，已加入待办事项。".to_string())
    } else if has_completed_update {
        Some("好的，已更新已完成事项。".to_string())
    } else {
        Some("好的，状态已更新。".to_string())
    }
}

fn extract_partial_json_string_field(raw: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let key_index = raw.find(&key)?;
    let after_key = &raw[key_index + key.len()..];
    let colon_index = after_key.find(':')?;
    let mut chars = after_key[colon_index + 1..].chars();
    let mut started = false;
    let mut output = String::new();

    while let Some(ch) = chars.next() {
        if !started {
            if ch.is_whitespace() {
                continue;
            }
            if ch != '"' {
                return None;
            }
            started = true;
            continue;
        }

        if ch == '"' {
            return Some(output);
        }
        if ch == '\\' {
            let Some(escaped) = chars.next() else {
                return Some(output);
            };
            match escaped {
                '"' => output.push('"'),
                '\\' => output.push('\\'),
                '/' => output.push('/'),
                'b' => output.push('\u{0008}'),
                'f' => output.push('\u{000C}'),
                'n' => output.push('\n'),
                'r' => output.push('\r'),
                't' => output.push('\t'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let Some(next_hex) = chars.next() else {
                            return Some(output);
                        };
                        hex.push(next_hex);
                    }
                    if let Ok(code) = u16::from_str_radix(&hex, 16) {
                        if let Some(decoded) = char::from_u32(code as u32) {
                            output.push(decoded);
                        }
                    }
                }
                other => output.push(other),
            }
            continue;
        }
        output.push(ch);
    }

    Some(output)
}

fn extract_recoverable_character_response(
    raw: &str,
    default_speaker: &str,
) -> Option<ParsedCharacterResponse> {
    let speaker = extract_json_string_field_with_boundaries(
        raw,
        "speaker",
        &[
            "content",
            "message",
            "text",
            "intent",
            "emotion",
            "narration",
            "scene_narration",
        ],
    )
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| default_speaker.to_string());
    let content = extract_json_string_field_with_boundaries(
        raw,
        "content",
        &["intent", "emotion", "narration", "scene_narration"],
    )
    .or_else(|| {
        extract_json_string_field_with_boundaries(
            raw,
            "message",
            &["intent", "emotion", "narration", "scene_narration"],
        )
    })
    .or_else(|| {
        extract_json_string_field_with_boundaries(
            raw,
            "text",
            &["intent", "emotion", "narration", "scene_narration"],
        )
    })
    .map(|value| strip_dialogue_field_artifacts(&value))
    .filter(|value| !value.trim().is_empty())?;
    let intent = extract_json_string_field_with_boundaries(
        raw,
        "intent",
        &["emotion", "narration", "scene_narration"],
    )
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "advance_objective".to_string());
    let emotion = extract_json_string_field_with_boundaries(
        raw,
        "emotion",
        &["narration", "scene_narration"],
    )
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "focused".to_string());
    let narration = extract_json_string_field_with_boundaries(raw, "narration", &[])
        .or_else(|| extract_json_string_field_with_boundaries(raw, "scene_narration", &[]))
        .unwrap_or_default();
    Some(ParsedCharacterResponse {
        speaker,
        content,
        intent,
        emotion,
        narration,
        raw_payload: None,
    })
}

fn extract_json_string_field_with_boundaries(
    raw: &str,
    field: &str,
    next_fields: &[&str],
) -> Option<String> {
    let key = format!("\"{field}\"");
    let key_index = raw.find(&key)?;
    let after_key = &raw[key_index + key.len()..];
    let colon_index = after_key.find(':')?;
    let value_slice = &after_key[colon_index + 1..];
    let start_quote = value_slice.find('"')?;
    let content_start = start_quote + 1;

    let mut end_index: Option<usize> = None;
    for next_field in next_fields {
        let marker = format!("\"{next_field}\"");
        if let Some(marker_relative_index) = value_slice[content_start..].find(&marker) {
            let marker_index = content_start + marker_relative_index;
            if let Some(quote_index) = value_slice[..marker_index].rfind('"') {
                end_index = Some(match end_index {
                    Some(existing) => existing.min(quote_index),
                    None => quote_index,
                });
            }
        }
    }

    if end_index.is_none() {
        if let Some(object_end_index) = value_slice.rfind('}') {
            if let Some(quote_index) = value_slice[..object_end_index].rfind('"') {
                end_index = Some(quote_index);
            }
        }
    }

    let end_index = end_index?;
    if end_index < content_start {
        return None;
    }
    Some(value_slice[content_start..end_index].to_string())
}

#[cfg(test)]
mod tests {
    use super::DialoguePipeline;

    #[test]
    fn extracts_partial_character_content_from_streamed_json() {
        let pipeline = DialoguePipeline::new();
        let parsed = pipeline
            .extract_partial_character_response(
                "{\"speaker\":\"林黛玉\",\"content\":\"你既来了，",
                "林黛玉",
            )
            .expect("partial response");
        assert_eq!(parsed.speaker, "林黛玉");
        assert_eq!(parsed.content, "你既来了，");
    }

    #[test]
    fn extracts_partial_character_content_with_escape_sequences() {
        let pipeline = DialoguePipeline::new();
        let parsed = pipeline
            .extract_partial_character_response("{\"content\":\"第一句\\n第二句\\\"", "袭人")
            .expect("partial response");
        assert_eq!(parsed.content, "第一句\n第二句");
    }

    #[test]
    fn parses_response_field_as_character_content() {
        let pipeline = DialoguePipeline::new();
        let parsed = pipeline.parse_character_response(
            "{\"response\":\"好的，已加入待办事项。\",\"session_attribute_updates\":[{\"key\":\"todo_items\",\"value\":[\"吃饭\"]}]}",
            "行程助手",
        );
        assert_eq!(parsed.content, "好的，已加入待办事项。");
        assert!(parsed.raw_payload.is_some());
    }

    #[test]
    fn hides_raw_json_for_attribute_only_payload() {
        let pipeline = DialoguePipeline::new();
        let parsed = pipeline.parse_character_response(
            "{\"session_attribute_updates\":[{\"key\":\"todo_items\",\"value\":[\"吃饭\"]}]}",
            "行程助手",
        );
        assert_eq!(parsed.content, "好的，已加入待办事项。");
        assert!(parsed.raw_payload.is_some());
    }
}
