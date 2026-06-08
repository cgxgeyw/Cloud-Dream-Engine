use crate::models::world::WorldDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceMode {
    WorldSim,
    AgentChat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryWriteMode {
    Session,
    Character,
    WorldAndCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceRuntimeConfig {
    pub service_mode: ServiceMode,
    pub default_agent_id: Option<String>,
    pub memory_write_mode: MemoryWriteMode,
}

pub(crate) fn resolve_service_runtime_config(world: &WorldDefinition) -> ServiceRuntimeConfig {
    resolve_service_runtime_config_from_director_config(&world.director_config)
}

pub(crate) fn agent_chat_virtual_player_id() -> &'static str {
    "agent-chat-user"
}

pub(crate) fn agent_chat_virtual_player_name() -> String {
    "\u{7528}\u{6237}".to_string()
}

pub(crate) fn resolve_service_runtime_config_from_director_config(
    director_config: &serde_json::Value,
) -> ServiceRuntimeConfig {
    let service_mode = director_config
        .get("service_mode")
        .and_then(|value| value.as_str())
        .map(parse_service_mode)
        .unwrap_or(ServiceMode::WorldSim);
    let default_agent_id = director_config
        .get("default_agent_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let runtime_policy = director_config.get("runtime_policy");
    let memory_write_mode = runtime_policy
        .and_then(|value| value.get("memory_write_mode"))
        .or_else(|| director_config.get("memory_write_mode"))
        .and_then(|value| value.as_str())
        .map(parse_memory_write_mode)
        .unwrap_or(MemoryWriteMode::Session);

    ServiceRuntimeConfig {
        service_mode,
        default_agent_id,
        memory_write_mode,
    }
}

fn parse_service_mode(value: &str) -> ServiceMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "agent_chat" => ServiceMode::AgentChat,
        _ => ServiceMode::WorldSim,
    }
}

fn parse_memory_write_mode(value: &str) -> MemoryWriteMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "character" => MemoryWriteMode::Character,
        "world_and_character" => MemoryWriteMode::WorldAndCharacter,
        _ => MemoryWriteMode::Session,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_mode_defaults_to_world_sim() {
        let config = resolve_service_runtime_config_from_director_config(&serde_json::json!({}));

        assert_eq!(config.service_mode, ServiceMode::WorldSim);
        assert_eq!(config.memory_write_mode, MemoryWriteMode::Session);
    }

    #[test]
    fn invalid_mode_defaults_to_world_sim() {
        let config = resolve_service_runtime_config_from_director_config(&serde_json::json!({
            "service_mode": "surprise"
        }));

        assert_eq!(config.service_mode, ServiceMode::WorldSim);
    }

    #[test]
    fn agent_chat_reads_default_agent_and_memory_policy() {
        let config = resolve_service_runtime_config_from_director_config(&serde_json::json!({
            "service_mode": "agent_chat",
            "default_agent_id": " char-a ",
            "runtime_policy": {
                "memory_write_mode": "world_and_character"
            }
        }));

        assert_eq!(config.service_mode, ServiceMode::AgentChat);
        assert_eq!(config.default_agent_id.as_deref(), Some("char-a"));
        assert_eq!(config.memory_write_mode, MemoryWriteMode::WorldAndCharacter);
    }
}
