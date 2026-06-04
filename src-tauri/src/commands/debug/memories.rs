use tauri::State;

use crate::state::AppState;

use super::common;

pub(crate) async fn get_debug_memories_impl(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;

    let session = common::query_session(db.conn(), &session_id)?;
    let world = common::query_world(db.conn(), &session.world_name)?;
    let characters = common::query_characters(db.conn(), world.as_ref().map(|w| w.id.as_str()))?;
    let memories = common::query_memories(
        db.conn(),
        &session_id,
        world.as_ref().map(|w| w.id.as_str()),
    )?;
    let grouped_memories = common::build_grouped_memories(&memories, &characters);

    let agent_sessions = common::query_agent_sessions(db.conn(), &session_id)?;
    let latest_checkpoints = common::query_latest_checkpoints(db.conn(), &session_id)?;

    let (schema_map, session_attributes, character_attributes) =
        common::query_attributes(db.conn(), &session_id)?;

    let runtime_session_attributes = serde_json::json!({
        "committed": session_attributes
            .iter()
            .filter_map(|value| common::build_runtime_attribute_item(value, &schema_map))
            .collect::<Vec<_>>(),
    });

    let runtime_char_attributes: Vec<serde_json::Value> = {
        let mut by_char: std::collections::BTreeMap<
            String,
            Vec<&crate::models::attribute::AttributeValue>,
        > = std::collections::BTreeMap::new();
        for value in &character_attributes {
            let char_id = value
                .owner_id
                .split(':')
                .nth(1)
                .unwrap_or(&value.owner_id)
                .to_string();
            by_char.entry(char_id).or_default().push(value);
        }
        by_char
            .into_iter()
            .map(|(char_id, values)| {
                let character_name = characters
                    .iter()
                    .find(|item| item.id == char_id)
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| char_id.clone());
                serde_json::json!({
                    "character_id": char_id,
                    "character_name": character_name,
                    "committed": values
                        .iter()
                        .filter_map(|value| common::build_runtime_attribute_item(value, &schema_map))
                        .collect::<Vec<_>>(),
                })
            })
            .collect()
    };

    Ok(serde_json::json!({
        "session": session,
        "memories": memories,
        "grouped_memories": grouped_memories,
        "agent_sessions": agent_sessions,
        "latest_checkpoints": latest_checkpoints,
        "runtime_session_attributes": runtime_session_attributes,
        "runtime_char_attributes": runtime_char_attributes,
    }))
}
