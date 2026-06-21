use crate::models::session::{RuntimeAttributeItem, SessionSnapshot, SessionState};
use crate::services::game_engine::orchestrator::DirectorDecision;
use crate::services::game_engine::rule::RuleEvaluation;
use crate::services::game_engine::trigger::TriggerEvaluation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateTransitionResult {
    pub state: SessionState,
    pub system_messages: Vec<String>,
    pub debug_lines: Vec<String>,
}

pub struct StateEngineService;

impl StateEngineService {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_turn(
        &self,
        session: &SessionSnapshot,
        player_input: &str,
        director_decision: &DirectorDecision,
        trigger_evaluation: &TriggerEvaluation,
        rule_evaluation: &RuleEvaluation,
        session_attributes: &[RuntimeAttributeItem],
    ) -> StateTransitionResult {
        let mut metrics = session.state.metrics.clone();
        let mut tags = session.state.tags.clone();
        let _ = session_attributes;

        metrics.entry("pressure".to_string()).or_insert(0.0);
        metrics.entry("focus".to_string()).or_insert(50.0);
        metrics.entry("stability".to_string()).or_insert(100.0);

        if let Some(value) = metrics.get_mut("stability") {
            *value = (*value - 3.0).max(0.0);
        }
        if ["观察", "查看", "调查"]
            .iter()
            .any(|keyword| player_input.contains(keyword))
        {
            if let Some(value) = metrics.get_mut("focus") {
                *value = (*value + 4.0).min(100.0);
            }
            ensure_tag(&mut tags, "observing");
        }
        if director_decision
            .next_location
            .as_deref()
            .map(|value| !value.trim().is_empty() && value != session.location)
            .unwrap_or(false)
        {
            ensure_tag(&mut tags, "traveling");
            if let Some(value) = metrics.get_mut("focus") {
                *value = (*value + 2.0).min(100.0);
            }
        } else {
            remove_tag(&mut tags, "traveling");
        }
        if trigger_evaluation
            .system_messages
            .iter()
            .any(|msg| msg.contains("封锁"))
        {
            if let Some(value) = metrics.get_mut("pressure") {
                *value += 6.0;
            }
            ensure_tag(&mut tags, "under_lockdown");
        }

        for (metric, delta) in &rule_evaluation.metric_deltas {
            *metrics.entry(metric.clone()).or_insert(0.0) += delta;
        }
        for tag in &rule_evaluation.add_tags {
            ensure_tag(&mut tags, tag);
        }
        for tag in &rule_evaluation.remove_tags {
            remove_tag(&mut tags, tag);
        }

        let pressure = *metrics.get("pressure").unwrap_or(&0.0);
        let mut phase = resolve_phase(&director_decision.world_phase, pressure);
        if let Some(value) = rule_evaluation
            .phase_override
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            phase = value.to_string();
        }

        let state = SessionState {
            metrics: metrics
                .into_iter()
                .map(|(k, v)| (k, (v * 100.0).round() / 100.0))
                .collect(),
            tags,
            phase: phase.clone(),
        };
        let mut debug_lines = vec![
            format!(
                "StateEngine pressure={:.2}",
                state.metrics.get("pressure").copied().unwrap_or(0.0)
            ),
            format!(
                "StateEngine focus={:.2}",
                state.metrics.get("focus").copied().unwrap_or(0.0)
            ),
            format!(
                "StateEngine stability={:.2}",
                state.metrics.get("stability").copied().unwrap_or(0.0)
            ),
            format!(
                "StateEngine tags={}",
                if state.tags.is_empty() {
                    "none".to_string()
                } else {
                    state.tags.join(", ")
                }
            ),
        ];
        if !rule_evaluation.hit_rules.is_empty() {
            debug_lines.push(format!(
                "StateEngine rules={}",
                rule_evaluation.hit_rules.join(", ")
            ));
        }
        StateTransitionResult {
            state,
            system_messages: vec![format!("状态引擎：phase -> {phase}")],
            debug_lines,
        }
    }
}

fn resolve_phase(world_phase: &str, pressure: f64) -> String {
    if world_phase == "crisis" || pressure >= 70.0 {
        return "combat-ready".to_string();
    }
    if world_phase == "escalation" || pressure >= 35.0 {
        return "alert".to_string();
    }
    "idle".to_string()
}

fn ensure_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|item| item == tag) {
        tags.push(tag.to_string());
    }
}

fn remove_tag(tags: &mut Vec<String>, tag: &str) {
    tags.retain(|item| item != tag);
}
