use crate::models::session::{RuntimeAttributeItem, SceneRuntime, SessionSnapshot};
use crate::services::game_engine::orchestrator::DirectorDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneRuntimeResult {
    pub scene: SceneRuntime,
    pub system_messages: Vec<String>,
    pub debug_lines: Vec<String>,
}

pub struct SceneManager;

impl SceneManager {
    pub fn new() -> Self {
        Self
    }

    pub fn refresh_scene(
        &self,
        session: &SessionSnapshot,
        director_decision: &DirectorDecision,
        visible_character_names: &[String],
        session_attributes: &[RuntimeAttributeItem],
    ) -> SceneRuntimeResult {
        let weather_state = session_attributes
            .iter()
            .find(|item| item.key == "weather_state")
            .and_then(|item| item.value.as_str())
            .unwrap_or("clear")
            .to_string();
        let next_location = director_decision
            .next_location
            .clone()
            .unwrap_or_else(|| session.location.clone());
        let next_scene_name = director_decision
            .next_scene_name
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                if next_location != session.location {
                    next_location.clone()
                } else {
                    session.scene.name.clone()
                }
            });
        let background_hint = self.resolve_background_hint(
            session,
            &next_scene_name,
            &weather_state,
            director_decision.next_scene_background_hint.as_deref(),
        );
        let temporary_tags = self.resolve_scene_tags(
            session,
            &weather_state,
            &director_decision.world_phase,
            &next_scene_name,
            &[],
        );
        let changed = next_location != session.location
            || next_scene_name != session.scene.name
            || background_hint != session.scene.background_hint;

        let scene = SceneRuntime {
            scene_id: self.slugify(&next_scene_name),
            name: next_scene_name.clone(),
            background_hint: background_hint.clone(),
            temporary_tags,
            present_characters: self.build_present_characters(
                visible_character_names,
                Some(session.player_character_name.as_str()),
            ),
        };

        let mut system_messages = Vec::new();
        if changed {
            system_messages.push(format!("场景运行时：已装载 {next_scene_name}"));
        }

        let debug_lines = vec![
            format!("SceneRuntime location={next_location}"),
            format!("SceneRuntime scene_id={}", scene.scene_id),
            format!("SceneRuntime scene_name={}", scene.name),
            format!("SceneRuntime background={background_hint}"),
            format!("SceneRuntime tags={}", scene.temporary_tags.join(", ")),
            format!(
                "SceneRuntime present={}",
                scene.present_characters.join(", ")
            ),
        ];

        SceneRuntimeResult {
            scene,
            system_messages,
            debug_lines,
        }
    }

    fn resolve_background_hint(
        &self,
        session: &SessionSnapshot,
        scene_name: &str,
        weather_state: &str,
        override_hint: Option<&str>,
    ) -> String {
        if let Some(override_hint) = override_hint
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return override_hint.to_string();
        }
        if scene_name == session.scene.name && !session.scene.background_hint.trim().is_empty() {
            let current_hint = session.scene.background_hint.trim();
            let prefix = format!("{}:", self.slugify(scene_name));
            if current_hint.starts_with(&prefix) {
                return format!("{}:{}", self.slugify(scene_name), weather_state);
            }
            return current_hint.to_string();
        }
        format!("{}:{}", self.slugify(scene_name), weather_state)
    }

    fn resolve_scene_tags(
        &self,
        session: &SessionSnapshot,
        weather_state: &str,
        world_phase: &str,
        scene_name: &str,
        explicit_tags: &[String],
    ) -> Vec<String> {
        let scene_changed = scene_name != session.scene.name;
        let mut next_tags = if explicit_tags.is_empty() {
            if scene_changed {
                Vec::new()
            } else {
                session
                    .scene
                    .temporary_tags
                    .iter()
                    .filter(|tag| {
                        !tag.is_empty() && !tag.starts_with("phase:") && *tag != "scene-entered"
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        } else {
            explicit_tags
                .iter()
                .filter(|tag| !tag.is_empty())
                .cloned()
                .collect::<Vec<_>>()
        };

        next_tags.push(weather_state.to_string());
        if scene_changed {
            next_tags.push("scene-entered".to_string());
        }
        if !world_phase.trim().is_empty() {
            next_tags.push(format!("phase:{world_phase}"));
        }
        next_tags.sort();
        next_tags.dedup();
        next_tags
    }

    fn slugify(&self, value: &str) -> String {
        let slug = regex::Regex::new(r"[^a-zA-Z0-9\u4e00-\u9fff]+")
            .ok()
            .map(|re| re.replace_all(value, "-").to_string())
            .unwrap_or_else(|| value.to_string());
        let slug = slug.trim_matches('-').to_lowercase();
        if slug.is_empty() {
            "scene-default".to_string()
        } else {
            slug
        }
    }

    fn build_present_characters(
        &self,
        visible_character_names: &[String],
        player_character_name: Option<&str>,
    ) -> Vec<String> {
        let mut names = visible_character_names
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();
        if let Some(player_character_name) = player_character_name
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            names.push(player_character_name.to_string());
        }
        names.sort();
        names.dedup();
        names
    }
}
