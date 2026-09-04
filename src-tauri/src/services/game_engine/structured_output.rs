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
        if self.failure_code == "output_truncated" {
            return "输出被 token 上限截断";
        }
        match self.stage {
            StructuredFailureStage::DirectorMain | StructuredFailureStage::DirectorToolFollowup => {
                "世界主控回复异常"
            }
            StructuredFailureStage::SpeakerResponse => "角色回复异常",
        }
    }

    pub fn display_content(&self) -> String {
        // 截断是配置问题（max_tokens 太小），不是"模型返回了坏数据"。直接告诉用户
        // 该调哪个设置，否则重发多少次都是同一个结果。
        if self.failure_code == "output_truncated" {
            return "模型输出被 token 上限截断，本回合没有拿到完整数据。请在世界设置或模型配置里提高 max_tokens（推理模型建议 8000 以上）后重发。".to_string();
        }
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

#[allow(clippy::too_many_arguments)]
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
    truncated_by_token_limit: bool,
) -> Result<(), StructuredOutputFailure> {
    let stage = if parsed.get("tool_results").is_some() {
        StructuredFailureStage::DirectorToolFollowup
    } else {
        StructuredFailureStage::DirectorMain
    };

    // 输出被 max_tokens 截断时，报"预算不足"而不是"没返回 JSON"：后者会把用户
    // 引向改提示词，而真正该改的是 max_tokens。推理模型尤其容易把预算烧在思考上，
    // 此时 content 为空、raw_text 也为空。
    if truncated_by_token_limit && !director_payload_is_usable(parsed) {
        return Err(build_failure(
            stage,
            "output_truncated",
            "模型输出被 max_tokens 截断（推理内容占满了预算），本回合没有拿到完整 JSON",
            provider,
            model_id,
            turn_index,
            None,
            raw_text,
            repair_summary,
            vec![
                "response was cut off by the token limit; raise max_tokens for this world"
                    .to_string(),
            ],
            Vec::new(),
        ));
    }

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
                // M7: 此前依赖"schema_errors 刚 push 必非空 → 提前 return"的隐式不变量,
                // 末尾用 unreachable!() 兜底,任何改动 push/guard 顺序的编辑都会让它变成
                // 运行时 panic、崩掉导演回合。改为无条件返回 schema 校验错误,不再依赖该不变量。
                schema_errors.push("switch_character_proposal must be an object".to_string());
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

/// 截断的输出是否仍然可用：非空 JSON 对象即认为可用（模型先输出 JSON 再被截在
/// 尾部的情况下，宽松解析常已补全出可用对象，不该因 finish_reason 就整轮作废）。
fn director_payload_is_usable(parsed: &serde_json::Value) -> bool {
    parsed
        .as_object()
        .map(|object| !object.is_empty())
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
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
            false,
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
            false,
        )
        .expect_err("player should not be allowed in planned_speakers");

        assert_eq!(failure.failure_code, "domain_validation_failed");
        assert!(failure
            .domain_errors
            .iter()
            .any(|value| value == "planned_speakers cannot include the player character"));
    }

    /// 回归：推理模型把 max_tokens 烧在思考上时 content 为空，此前会被报成
    /// json_parse_failed（"模型没返回 JSON"），把用户引向改提示词而不是加预算。
    #[test]
    fn truncated_empty_output_reports_token_limit_instead_of_parse_failure() {
        let failure = validate_director_payload(
            &serde_json::Value::Null,
            "Player",
            &["Alice".to_string()],
            &["Alice".to_string()],
            "openai",
            "deepseek-v4-flash",
            1,
            "",
            None,
            true,
        )
        .expect_err("truncated empty output must fail");

        assert_eq!(failure.failure_code, "output_truncated");
        assert_eq!(failure.display_title(), "输出被 token 上限截断");
        assert!(
            failure.display_content().contains("max_tokens"),
            "文案要指明该调哪个设置: {}",
            failure.display_content()
        );
    }

    /// 截断但已解析出可用对象时不作废整轮：宽松解析常能补全尾部被截的 JSON。
    #[test]
    fn truncated_but_usable_payload_still_passes() {
        let parsed = serde_json::json!({ "planned_speakers": ["Alice"] });

        let result = validate_director_payload(
            &parsed,
            "Player",
            &["Alice".to_string()],
            &["Alice".to_string()],
            "openai",
            "deepseek-v4-flash",
            1,
            "{\"planned_speakers\":[\"Alice\"]}",
            None,
            true,
        );

        assert!(result.is_ok());
    }

    /// 未截断的空输出仍走原有的 json_parse_failed 路径，不被新分支吞掉。
    #[test]
    fn untruncated_non_object_still_reports_parse_failure() {
        let failure = validate_director_payload(
            &serde_json::Value::Null,
            "Player",
            &["Alice".to_string()],
            &["Alice".to_string()],
            "openai",
            "gpt-test",
            1,
            "这不是 JSON",
            None,
            false,
        )
        .expect_err("non-object output must fail");

        assert_eq!(failure.failure_code, "json_parse_failed");
    }
}
