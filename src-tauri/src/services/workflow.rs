//! Unified world workflow contracts.
//!
//! A world is driven by events rather than by a special "game turn" path.
//! Narrative messages, UI actions, end-turn commands and agent actions all
//! enter the runtime as events. The event resolver may update authoritative
//! state and return the next workflow status; model narration is an optional
//! step performed by the caller after resolution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorldEvent {
    SessionStarted,
    PlayerMessage { content: String },
    GameAction { handler: String, input: Value },
    AgentAction { handler: String, input: Value },
    EndTurn { handler: String, input: Value },
    InteractionAnswered {
        message_id: String,
        interaction_id: String,
        answer: Value,
    },
    ToolResult { tool_call_id: String, result: Value },
}

impl WorldEvent {
    pub fn from_message(event_type: &str, content: String) -> Result<Self, String> {
        match event_type.trim() {
            "player_message" => Ok(Self::PlayerMessage { content }),
            "agent_action" => Ok(Self::AgentAction {
                handler: "__narrative__".to_string(),
                input: serde_json::json!({ "content": content }),
            }),
            other => Err(format!("Unsupported workflow event type: {other}")),
        }
    }

    pub fn actor(&self) -> &'static str {
        match self {
            Self::GameAction { .. } | Self::EndTurn { .. } => "player",
            Self::AgentAction { .. } => "agent",
            Self::SessionStarted
            | Self::PlayerMessage { .. }
            | Self::InteractionAnswered { .. }
            | Self::ToolResult { .. } => "system",
        }
    }

    pub fn handler_input(&self) -> Option<(&str, &Value)> {
        match self {
            Self::GameAction { handler, input }
            | Self::AgentAction { handler, input }
            | Self::EndTurn { handler, input } => Some((handler, input)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatus {
    pub phase: String,
    pub active_actor: Option<String>,
    pub waiting_for: Option<String>,
    pub terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEventResult {
    pub event: WorldEvent,
    pub result: Value,
    pub transition: crate::services::world_logic::GameTurnTransition,
    pub status: WorkflowStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<crate::models::session::SessionSnapshot>,
}

impl WorldEventResult {
    pub fn from_command(
        event: WorldEvent,
        result: Value,
        transition: crate::services::world_logic::GameTurnTransition,
    ) -> Self {
        let status = WorkflowStatus {
            phase: transition.phase.clone().unwrap_or_else(|| {
                if transition.game_over { "over".to_string() } else { "active".to_string() }
            }),
            active_actor: transition.next_actor.clone(),
            waiting_for: transition.next_actor.clone(),
            terminal: transition.game_over,
        };
        Self { event, result, transition, status, snapshot: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_have_stable_wire_shape_and_actor() {
        let event = WorldEvent::EndTurn {
            handler: "game.endTurn".to_string(),
            input: serde_json::json!({}),
        };
        assert_eq!(event.actor(), "player");
        assert_eq!(event.handler_input().map(|(name, _)| name), Some("game.endTurn"));
        assert_eq!(serde_json::to_value(event).unwrap()["type"], "end_turn");
    }
}
