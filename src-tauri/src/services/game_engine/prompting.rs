use chrono::Local;

use crate::models::world::WorldDefinition;

pub fn render_prompt_variables(template: &str) -> String {
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    template
        .replace("{{current_time}}", &current_time)
        .replace("{{当前时间}}", &current_time)
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
