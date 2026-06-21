use crate::models::session::{RuntimeAttributeItem, SessionSnapshot};
use crate::services::game_engine::orchestrator::DirectorDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerAttributeUpdate {
    pub owner_type: String,
    pub owner_id: String,
    pub schema_key: String,
    pub value: serde_json::Value,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMemoryEvent {
    pub event_id: String,
    pub content: String,
    pub source: String,
    pub importance: f64,
    pub memory_type: String,
    pub speaker: Option<String>,
    pub role: Option<String>,
    pub location: Option<String>,
    pub scene_id: Option<String>,
    pub item_id: Option<String>,
    pub participants: Vec<String>,
    pub character_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriggerEvaluation {
    pub system_messages: Vec<String>,
    pub attribute_updates: Vec<TriggerAttributeUpdate>,
    pub memory_events: Vec<TriggerMemoryEvent>,
    pub debug_lines: Vec<String>,
}

pub struct TriggerEngineService;

impl TriggerEngineService {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_turn(
        &self,
        session: &SessionSnapshot,
        player_input: &str,
        director_decision: &DirectorDecision,
        session_attributes: &[RuntimeAttributeItem],
    ) -> TriggerEvaluation {
        let _ = session_attributes;

        let mut evaluation = TriggerEvaluation::default();
        let next_location = director_decision
            .next_location
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(location) = next_location {
            if location != session.location {
                let message = format!("触发器：已进入 {location}");
                evaluation.system_messages.push(message.clone());
                evaluation.attribute_updates.push(TriggerAttributeUpdate {
                    owner_type: "session".to_string(),
                    owner_id: session.id.clone(),
                    schema_key: "active_objective".to_string(),
                    value: serde_json::Value::String(format!("调查 {location}")),
                    source: "trigger".to_string(),
                });
                evaluation.memory_events.push(TriggerMemoryEvent {
                    event_id: "trigger:scene_enter".to_string(),
                    content: message,
                    source: "trigger_engine".to_string(),
                    importance: 0.42,
                    memory_type: "event".to_string(),
                    speaker: None,
                    role: Some("system".to_string()),
                    location: Some(location.to_string()),
                    scene_id: Some(session.scene.scene_id.clone()),
                    item_id: None,
                    participants: build_participants(session),
                    character_names: build_participants(session),
                });
                evaluation
                    .debug_lines
                    .push(format!("TriggerEngine scene_enter -> {location}"));
            }
        }


        if player_input.contains("观察")
            && next_location
                .map(|value| value == session.location)
                .unwrap_or(false)
        {
            let message = "触发器：观察行为命中，当前场景出现新的细节线索。".to_string();
            evaluation.system_messages.push(message.clone());
            evaluation.memory_events.push(TriggerMemoryEvent {
                event_id: "trigger:observe_hit".to_string(),
                content: message,
                source: "trigger_engine".to_string(),
                importance: 0.48,
                memory_type: "event".to_string(),
                speaker: None,
                role: Some("system".to_string()),
                location: Some(
                    next_location
                        .unwrap_or(session.location.as_str())
                        .to_string(),
                ),
                scene_id: Some(session.scene.scene_id.clone()),
                item_id: None,
                participants: build_participants(session),
                character_names: build_participants(session),
            });
            evaluation
                .debug_lines
                .push("TriggerEngine keyword -> observe".to_string());
        }

        evaluation
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
