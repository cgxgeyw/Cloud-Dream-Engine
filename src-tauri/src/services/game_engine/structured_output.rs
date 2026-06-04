use crate::services::game_engine::dialogue::ParsedCharacterResponse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredFailureStage {
    DirectorMain,
    DirectorToolFollowup,
    SpeakerResponse,
}

impl StructuredFailureStage {
    pub fn retry_kind(self) -> &'static str {
        match self {
            Self::DirectorMain => "director_main",
            Self::DirectorToolFollowup => "director_tool_followup",
            Self::SpeakerResponse => "speaker_response",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredOutputFailure {
    pub stage: StructuredFailureStage,
    pub failure_code: String,
    pub summary: String,
    pub provider: String,
    pub model_id: String,
    pub turn_index: i32,
    pub speaker_name: Option<String>,
    pub raw_text_excerpt: String,
    pub repair_summary: Option<String>,
    pub schema_errors: Vec<String>,
    pub domain_errors: Vec<String>,
}

impl StructuredOutputFailure {
    pub fn action_type(&self) -> &'static str {
        match self.stage {
            StructuredFailureStage::DirectorMain | StructuredFailureStage::DirectorToolFollowup => {
                "director_retry_required"
            }
            StructuredFailureStage::SpeakerResponse => "structured_output_error",
        }
    }

    pub fn message_kind(&self) -> &'static str {
        match self.stage {
            StructuredFailureStage::DirectorMain | StructuredFailureStage::DirectorToolFollowup => {
                "system_action"
            }
            StructuredFailureStage::SpeakerResponse => "llm_structured_error",
        }
    }

    pub fn display_title(&self) -> &'static str {
        match self.stage {
            StructuredFailureStage::DirectorMain | StructuredFailureStage::DirectorToolFollowup => {
                "世界主控回复异常"
            }
            StructuredFailureStage::SpeakerResponse => "角色回复异常",
        }
    }

    pub fn display_content(&self) -> String {
        match self.stage {
            StructuredFailureStage::DirectorMain | StructuredFailureStage::DirectorToolFollowup => {
                "导演返回的结构化数据无效，系统已停止本回合推进。请决定是否重发。".to_string()
            }
            StructuredFailureStage::SpeakerResponse => {
                let speaker = self
                    .speaker_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("当前角色");
                format!("{speaker} 的结构化回复无效，当前回合已暂停。")
            }
        }
    }
}

pub fn validate_director_payload(
    parsed: &serde_json::Value,
    player_character_name: &str,
    visible_characters: &[String],
    world_character_roster: &[String],
    provider: &str,
    model_id: &str,
    turn_index: i32,
    raw_text: &str,
    repair_summary: Option<String>,
) -> Result<(), StructuredOutputFailure> {
    let stage = if parsed.get("tool_results").is_some() {
        StructuredFailureStage::DirectorToolFollowup
    } else {
        StructuredFailureStage::DirectorMain
    };

    let Some(object) = parsed.as_object() else {
        return Err(build_failure(
            stage,
            "json_parse_failed",
            "导演输出无法解析为 JSON 对象",
            provider,
            model_id,
            turn_index,
            None,
            raw_text,
            repair_summary,
            vec!["response must be a JSON object".to_string()],
            Vec::new(),
        ));
    };

    if object.is_empty() {
        return Err(build_failure(
            stage,
            "json_repair_failed",
            "导演输出为空对象，无法继续推进剧情",
            provider,
            model_id,
            turn_index,
            None,
            raw_text,
            repair_summary,
            vec!["response object is empty".to_string()],
            Vec::new(),
        ));
    }

    let mut schema_errors = Vec::new();
    let mut domain_errors = Vec::new();

    if let Some(value) = object.get("planned_speakers") {
        match value.as_array() {
            Some(items) => {
                for item in items {
                    if item.as_str().map(|v| !v.trim().is_empty()).unwrap_or(false) {
                        continue;
                    }
                    schema_errors
                        .push("planned_speakers must contain only non-empty strings".to_string());
                    break;
                }
            }
            None => schema_errors.push("planned_speakers must be an array of strings".to_string()),
        }
    }

    if let Some(value) = object.get("scene_visible_characters") {
        match value.as_array() {
            Some(items) => {
                for item in items {
                    if item.as_str().map(|v| !v.trim().is_empty()).unwrap_or(false) {
                        continue;
                    }
                    schema_errors.push(
                        "scene_visible_characters must contain only non-empty strings".to_string(),
                    );
                    break;
                }
            }
            None => {
                schema_errors
                    .push("scene_visible_characters must be an array of strings".to_string());
            }
        }
    }

    if let Some(value) = object.get("switch_character_proposal") {
        if !value.is_null() {
            let Some(proposal) = value.as_object() else {
                schema_errors.push("switch_character_proposal must be an object".to_string());
                if !schema_errors.is_empty() || !domain_errors.is_empty() {
                    return Err(build_failure(
                        stage,
                        "schema_validation_failed",
                        "导演输出字段结构无效",
                        provider,
                        model_id,
                        turn_index,
                        None,
                        raw_text,
                        repair_summary,
                        schema_errors,
                        domain_errors,
                    ));
                }
                unreachable!();
            };
            let target_name = proposal
                .get("target_character_name")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .unwrap_or("");
            if target_name.is_empty() {
                schema_errors.push(
                    "switch_character_proposal.target_character_name is required".to_string(),
                );
            } else if target_name == player_character_name {
                domain_errors.push(
                    "switch_character_proposal.target_character_name cannot equal current player"
                        .to_string(),
                );
            }
        }
    }

    let allowed_visible = visible_characters
        .iter()
        .chain(world_character_roster.iter())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .chain(
            object
                .get("generated_characters")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("name").and_then(|value| value.as_str()))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(items) = object
        .get("planned_speakers")
        .and_then(|value| value.as_array())
    {
        for item in items {
            let Some(name) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if name == player_character_name {
                domain_errors
                    .push("planned_speakers cannot include the player character".to_string());
                break;
            }
            if !allowed_visible.contains(name) {
                domain_errors.push(format!(
                    "planned_speakers contains unknown or not-visible character: {name}"
                ));
                break;
            }
        }
    }
    if !schema_errors.is_empty() || !domain_errors.is_empty() {
        return Err(build_failure(
            stage,
            if !schema_errors.is_empty() {
                "schema_validation_failed"
            } else {
                "domain_validation_failed"
            },
            "导演结构化输出校验失败",
            provider,
            model_id,
            turn_index,
            None,
            raw_text,
            repair_summary,
            schema_errors,
            domain_errors,
        ));
    }

    Ok(())
}

pub fn validate_character_payload(
    parsed: &ParsedCharacterResponse,
    expected_speaker: &str,
    provider: &str,
    model_id: &str,
    turn_index: i32,
    raw_text: &str,
) -> Result<(), StructuredOutputFailure> {
    let mut schema_errors = Vec::new();
    let mut domain_errors = Vec::new();

    if parsed.content.trim().is_empty() {
        schema_errors.push("content is required".to_string());
    }
    if parsed.speaker.trim().is_empty() {
        schema_errors.push("speaker is required".to_string());
    } else if parsed.speaker.trim() != expected_speaker.trim() {
        domain_errors.push(format!(
            "speaker must match the requested character: expected {}, got {}",
            expected_speaker.trim(),
            parsed.speaker.trim()
        ));
    }

    if !schema_errors.is_empty() || !domain_errors.is_empty() {
        return Err(build_failure(
            StructuredFailureStage::SpeakerResponse,
            if !schema_errors.is_empty() {
                "schema_validation_failed"
            } else {
                "domain_validation_failed"
            },
            "角色结构化输出校验失败",
            provider,
            model_id,
            turn_index,
            Some(expected_speaker.to_string()),
            raw_text,
            None,
            schema_errors,
            domain_errors,
        ));
    }

    Ok(())
}

fn build_failure(
    stage: StructuredFailureStage,
    failure_code: &str,
    summary: &str,
    provider: &str,
    model_id: &str,
    turn_index: i32,
    speaker_name: Option<String>,
    raw_text: &str,
    repair_summary: Option<String>,
    schema_errors: Vec<String>,
    domain_errors: Vec<String>,
) -> StructuredOutputFailure {
    StructuredOutputFailure {
        stage,
        failure_code: failure_code.to_string(),
        summary: summary.to_string(),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        turn_index,
        speaker_name,
        raw_text_excerpt: build_excerpt(raw_text),
        repair_summary,
        schema_errors,
        domain_errors,
    }
}

fn build_excerpt(raw_text: &str) -> String {
    let normalized = raw_text.replace('\r', "").replace('\n', " ");
    let trimmed = normalized.trim();
    if trimmed.len() <= 280 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..280])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_director_payload_allows_player_in_scene_visible_characters() {
        let parsed = serde_json::json!({
            "scene_visible_characters": ["Player", "Alice"],
            "planned_speakers": ["Alice"]
        });

        let result = validate_director_payload(
            &parsed,
            "Player",
            &["Alice".to_string()],
            &["Player".to_string(), "Alice".to_string()],
            "openai",
            "test-model",
            1,
            "{}",
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn validate_director_payload_still_rejects_player_in_planned_speakers() {
        let parsed = serde_json::json!({
            "planned_speakers": ["Player"]
        });

        let failure = validate_director_payload(
            &parsed,
            "Player",
            &["Alice".to_string()],
            &["Player".to_string(), "Alice".to_string()],
            "openai",
            "test-model",
            1,
            "{}",
            None,
        )
        .expect_err("player should not be allowed in planned_speakers");

        assert_eq!(failure.failure_code, "domain_validation_failed");
        assert!(failure
            .domain_errors
            .iter()
            .any(|value| value == "planned_speakers cannot include the player character"));
    }
}
