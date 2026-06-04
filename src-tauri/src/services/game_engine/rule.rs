use crate::models::rule::RuleDefinition;
use crate::models::session::{RuntimeAttributeItem, SessionSnapshot, SessionState};
use crate::services::game_engine::orchestrator::DirectorDecision;
use crate::services::game_engine::trigger::TriggerEvaluation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleAttributeUpdate {
    pub owner_type: String,
    pub owner_id: String,
    pub schema_key: String,
    pub value: serde_json::Value,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMemoryEvent {
    pub event_id: String,
    pub content: String,
    pub source: String,
    pub importance: f64,
    pub memory_type: String,
    pub location: Option<String>,
    pub scene_id: Option<String>,
    pub participants: Vec<String>,
    pub character_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub system_messages: Vec<String>,
    pub attribute_updates: Vec<RuleAttributeUpdate>,
    pub memory_events: Vec<RuleMemoryEvent>,
    pub metric_deltas: HashMap<String, f64>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub phase_override: Option<String>,
    pub debug_lines: Vec<String>,
    pub hit_rules: Vec<String>,
}

pub struct RuleEngineService;

impl RuleEngineService {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_turn(
        &self,
        session: &SessionSnapshot,
        player_input: &str,
        director_decision: &DirectorDecision,
        trigger_evaluation: &TriggerEvaluation,
        session_attributes: &[RuntimeAttributeItem],
        current_state: &SessionState,
        rules: &[RuleDefinition],
    ) -> RuleEvaluation {
        let attr_map = session_attributes
            .iter()
            .map(|item| (item.key.clone(), item.value.clone()))
            .collect::<HashMap<_, _>>();
        let mut evaluation = RuleEvaluation::default();

        for rule in rules
            .iter()
            .filter(|item| item.enabled && item.scope.trim() == "session")
        {
            if !self.matches(
                rule,
                &attr_map,
                player_input,
                session,
                director_decision,
                trigger_evaluation,
                current_state,
            ) {
                continue;
            }
            evaluation.hit_rules.push(rule.name.clone());
            evaluation
                .debug_lines
                .push(format!("RuleEngine hit={}", rule.name));

            let mut generated_memory = false;
            for effect in &rule.effects {
                let Some(effect_obj) = effect.as_object() else {
                    continue;
                };
                let effect_type = effect_obj
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                match effect_type {
                    "message" => {
                        let text = effect_obj
                            .get("text")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        if !text.is_empty() {
                            evaluation.system_messages.push(text.clone());
                            evaluation.memory_events.push(RuleMemoryEvent {
                                event_id: format!("rule:{}", rule.id),
                                content: text,
                                source: "rule_engine".to_string(),
                                importance: 0.46,
                                memory_type: "event".to_string(),
                                location: Some(
                                    director_decision
                                        .next_location
                                        .clone()
                                        .unwrap_or_else(|| session.location.clone()),
                                ),
                                scene_id: Some(session.scene.scene_id.clone()),
                                participants: build_participants(session),
                                character_names: build_participants(session),
                            });
                            generated_memory = true;
                        }
                    }
                    "attribute_set" => {
                        let schema_key = effect_obj
                            .get("schema_key")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        if !schema_key.is_empty() {
                            evaluation.attribute_updates.push(RuleAttributeUpdate {
                                owner_type: effect_obj
                                    .get("owner_type")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("session")
                                    .to_string(),
                                owner_id: effect_obj
                                    .get("owner_id")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or(session.id.as_str())
                                    .to_string(),
                                schema_key,
                                value: effect_obj
                                    .get("value")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null),
                                source: "rule".to_string(),
                            });
                        }
                    }
                    "metric_delta" => {
                        let key = effect_obj
                            .get("metric")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        if !key.is_empty() {
                            let delta = effect_obj
                                .get("delta")
                                .and_then(|value| value.as_f64())
                                .unwrap_or(0.0);
                            *evaluation.metric_deltas.entry(key).or_insert(0.0) += delta;
                        }
                    }
                    "add_tag" => {
                        if let Some(tag) = effect_obj
                            .get("tag")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            evaluation.add_tags.push(tag.to_string());
                        }
                    }
                    "remove_tag" => {
                        if let Some(tag) = effect_obj
                            .get("tag")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            evaluation.remove_tags.push(tag.to_string());
                        }
                    }
                    "phase_override" => {
                        evaluation.phase_override = effect_obj
                            .get("phase")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string);
                    }
                    _ => {}
                }
            }
            if !generated_memory {
                evaluation.memory_events.push(RuleMemoryEvent {
                    event_id: format!("rule:{}", rule.id),
                    content: format!("规则生效：{}", rule.name),
                    source: "rule_engine".to_string(),
                    importance: 0.4,
                    memory_type: "event".to_string(),
                    location: Some(
                        director_decision
                            .next_location
                            .clone()
                            .unwrap_or_else(|| session.location.clone()),
                    ),
                    scene_id: Some(session.scene.scene_id.clone()),
                    participants: build_participants(session),
                    character_names: build_participants(session),
                });
            }
        }
        if !evaluation.hit_rules.is_empty() {
            evaluation.debug_lines.push(format!(
                "RuleEngine matched={}",
                evaluation.hit_rules.join(", ")
            ));
        }
        evaluation
    }

    fn matches(
        &self,
        rule: &RuleDefinition,
        attributes: &HashMap<String, serde_json::Value>,
        player_input: &str,
        session: &SessionSnapshot,
        director_decision: &DirectorDecision,
        trigger_evaluation: &TriggerEvaluation,
        current_state: &SessionState,
    ) -> bool {
        let Some(condition) = rule.condition.as_object() else {
            return false;
        };
        let condition_type = condition
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or_default();

        match condition_type {
            "attribute_threshold" => {
                let key = condition
                    .get("attribute_key")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let operator = condition
                    .get("operator")
                    .and_then(|value| value.as_str())
                    .unwrap_or(">=");
                let expected = condition.get("value");
                compare_values(attributes.get(key), expected, operator)
            }
            "player_input_contains" => condition
                .get("value")
                .and_then(|value| value.as_str())
                .map(|needle| player_input.contains(needle))
                .unwrap_or(false),
            "phase_equals" => condition
                .get("value")
                .and_then(|value| value.as_str())
                .map(|expected| {
                    director_decision.world_phase == expected || current_state.phase == expected
                })
                .unwrap_or(false),
            "scene_changed" => director_decision
                .next_location
                .as_deref()
                .map(|location| location != session.location)
                .unwrap_or(false),
            "trigger_message_contains" => condition
                .get("value")
                .and_then(|value| value.as_str())
                .map(|needle| {
                    trigger_evaluation
                        .system_messages
                        .iter()
                        .any(|msg| msg.contains(needle))
                })
                .unwrap_or(false),
            _ => false,
        }
    }
}

fn compare_values(
    actual: Option<&serde_json::Value>,
    expected: Option<&serde_json::Value>,
    operator: &str,
) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let Some(actual) = actual else {
        return false;
    };
    let actual_num = actual.as_f64();
    let expected_num = expected.as_f64();
    if let (Some(left), Some(right)) = (actual_num, expected_num) {
        return match operator {
            ">=" => left >= right,
            ">" => left > right,
            "<=" => left <= right,
            "<" => left < right,
            "==" => (left - right).abs() < f64::EPSILON,
            _ => false,
        };
    }
    match operator {
        "==" => actual == expected,
        _ => false,
    }
}

fn build_participants(session: &SessionSnapshot) -> Vec<String> {
    let mut names = session.visible_characters.clone();
    if !names.contains(&session.player_character_name) {
        names.push(session.player_character_name.clone());
    }
    names.sort();
    names.dedup();
    names
}
