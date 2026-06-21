use chrono::Local;
use std::collections::HashMap;

use crate::models::world::WorldDefinition;

pub fn render_prompt_variables(template: &str) -> String {
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    template
        .replace("{{current_time}}", &current_time)
        .replace("{{当前时间}}", &current_time)
}

/// Collect enabled prompt presets whose scope applies to `target` ("director" or
/// "character"), rendered with `variables` and sorted by `order`. Returns the
/// content strings ready to be injected as extra system messages.
pub fn collect_prompt_preset_contents(
    world: &WorldDefinition,
    target: &str,
    variables: &HashMap<String, String>,
) -> Vec<String> {
    let mut entries = world
        .director_config
        .get("prompt_presets")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            let object = item.as_object()?;
            let enabled = object
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let scope = object
                .get("scope")
                .and_then(|value| value.as_str())
                .unwrap_or("both")
                .trim();
            if !enabled || !(scope == "both" || scope == target) {
                return None;
            }
            let raw = object.get("content").and_then(|value| value.as_str())?;
            let mut content = raw.to_string();
            for (key, value) in variables {
                content = content.replace(&format!("{{{{{key}}}}}"), value);
            }
            let content = render_prompt_variables(&content).trim().to_string();
            if content.is_empty() {
                return None;
            }
            let order = object
                .get("order")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            Some((order, content))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(order, _)| *order);
    entries.into_iter().map(|(_, content)| content).collect()
}

pub fn resolve_runtime_context_prompt(world: &WorldDefinition) -> String {
    world
        .director_config
        .get("runtime_context_prompt")
        .and_then(|value| value.as_str())
        .map(render_prompt_variables)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn llm_chat_message_to_value(
    message: &crate::services::llm::client::ChatMessage,
) -> serde_json::Value {
    serde_json::json!({
        "role": message.role,
        "content": message.content,
        "reasoning_content": message.reasoning_content,
        "speaker": message.speaker,
        "metadata": message.metadata,
    })
}

pub fn llm_chat_messages_to_values(
    messages: &[crate::services::llm::client::ChatMessage],
) -> Vec<serde_json::Value> {
    messages.iter().map(llm_chat_message_to_value).collect()
}

pub fn build_prompt_call(
    schema_version: &str,
    recipient_type: &str,
    recipient_name: &str,
    stage: &str,
    purpose: &str,
    system_prompt: &str,
    user_prompt: &str,
    messages: Vec<serde_json::Value>,
    modules: Vec<serde_json::Value>,
    response_contract: serde_json::Value,
    raw_debug: serde_json::Value,
) -> serde_json::Value {
    let final_sent_content = messages
        .iter()
        .map(|item| {
            let role = item
                .get("role")
                .and_then(|value| value.as_str())
                .unwrap_or("user");
            let content = item
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            format!("[{role}] {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    serde_json::json!({
        "schema_version": schema_version,
        "recipient_type": recipient_type,
        "recipient_name": recipient_name,
        "stage": stage,
        "purpose": purpose,
        "system_prompt": system_prompt,
        "user_prompt": user_prompt,
        "response_contract": response_contract,
        "modules": modules,
        "messages": messages,
        "final_sent_content": final_sent_content,
        "raw_model_return": serde_json::Value::Null,
        "return_processing": serde_json::Value::Null,
        "processed_model_return": serde_json::Value::Null,
        "written_result": serde_json::Value::Null,
        "raw_debug": raw_debug,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_presets(presets: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "w".to_string(),
            name: "诗会".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: String::new(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({}),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({ "prompt_presets": presets }),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: None,
        }
    }

    #[test]
    fn collects_scope_enabled_ordered_and_rendered_presets() {
        let world = world_with_presets(serde_json::json!([
            { "content": "B second", "scope": "director", "enabled": true, "order": 2 },
            { "content": "A first for {{char}}", "scope": "both", "enabled": true, "order": 1 },
            { "content": "char only", "scope": "character", "enabled": true, "order": 0 },
            { "content": "disabled", "scope": "director", "enabled": false, "order": 0 },
            { "content": "   ", "scope": "director", "enabled": true, "order": 5 },
        ]));
        let mut vars = HashMap::new();
        vars.insert("char".to_string(), "李白".to_string());

        let director = collect_prompt_preset_contents(&world, "director", &vars);
        // both + director, enabled, non-empty; ordered by `order`; var rendered.
        assert_eq!(director, vec!["A first for 李白".to_string(), "B second".to_string()]);

        let character = collect_prompt_preset_contents(&world, "character", &vars);
        // both + character only, ordered by `order` (char only=0 before A first=1).
        assert_eq!(
            character,
            vec!["char only".to_string(), "A first for 李白".to_string()]
        );
    }

    #[test]
    fn empty_when_no_presets() {
        let world = world_with_presets(serde_json::json!([]));
        assert!(collect_prompt_preset_contents(&world, "director", &HashMap::new()).is_empty());
    }
}

