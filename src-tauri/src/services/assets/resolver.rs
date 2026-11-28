use std::fs;
use std::path::{Path, PathBuf};

use crate::models::character::CharacterDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::session::{AssetSelection, CharacterVisualState, SceneRuntime, SessionSnapshot};
use crate::models::world::WorldDefinition;
use crate::services::assets::image_gen::{
    normalize_provider, preferred_image_size, ImageGenerator, ImageRequest,
};

pub struct AssetResolver {
    image_generator: ImageGenerator,
}

impl AssetResolver {
    pub fn new() -> Self {
        Self {
            image_generator: ImageGenerator::new(),
        }
    }

    pub async fn resolve(
        &self,
        data_dir: &Path,
        session: &SessionSnapshot,
        scene: &SceneRuntime,
        current_speaker: &str,
        world: Option<&WorldDefinition>,
        characters: &[CharacterDefinition],
        image_model: Option<&ModelConfig>,
        director_decision: Option<&serde_json::Value>,
        allow_generation: bool,
    ) -> AssetSelection {
        let ui_theme = world
            .map(|item| item.ui_theme_config.clone())
            .unwrap_or_else(|| serde_json::json!({}));
        let empty_assets = serde_json::json!({});
        let asset_config = ui_theme.get("assets").unwrap_or(&empty_assets);

        let background_source_mode =
            resolve_source_mode(asset_config.get("background_source_mode"), "local-first");
        let portrait_source_mode = resolve_source_mode(
            asset_config
                .get("portrait_source_mode")
                .or_else(|| asset_config.get("background_source_mode")),
            "local-first",
        );
        let generation_enabled = asset_config
            .get("runtime_image_generation_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let generation_allowed = allow_generation && generation_enabled;

        let background_hint = scene.background_hint.trim().to_string();
        let background_generation_prompt =
            resolve_background_generation_prompt(session, director_decision);
        let background_generation_name = director_decision
            .and_then(|value| value.get("background_generation_name"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let background_generation_requested = director_decision
            .and_then(|value| value.get("background_generation_prompt"))
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        let local_background_assets =
            normalize_asset_list(asset_config.get("local_background_assets"));
        let scene_background_assets =
            normalize_asset_groups(asset_config.get("local_scene_backgrounds"));
        let directed_background_name = director_decision
            .and_then(|value| value.get("background_asset_name"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let directed_background_path = director_decision
            .and_then(|value| value.get("background_asset_path"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let background_selection_requested =
            directed_background_name.is_some() || directed_background_path.is_some();
        let local_background = select_named_background_asset(
            &scene_background_assets,
            &local_background_assets,
            directed_background_name.as_deref(),
            directed_background_path.as_deref(),
        )
        .or_else(|| {
            if scene.name == session.scene.name {
                session.assets.background_asset_path.clone()
            } else {
                None
            }
        })
        .or_else(|| {
            select_default_background_asset(
                &scene.name,
                &session.location,
                &scene_background_assets,
                &local_background_assets,
            )
        });

        let cached_background = if scene.name == session.scene.name
            && session.assets.background_asset_path.as_deref().is_some_and(|path| path.starts_with("/assets/generated/"))
        {
            Some((
                session.assets.background_generation_prompt.as_str(),
                session.assets.background_asset_path.as_deref().unwrap_or_default(),
            ))
        } else {
            None
        };
        let should_generate_background = !matches!(background_source_mode, SourceMode::LocalOnly)
            && !(matches!(background_source_mode, SourceMode::LocalFirst)
                && local_background.is_some()
                && !background_generation_requested)
            && !(cached_background.is_some() && !background_generation_requested);
        let generated_background_asset_path = if should_generate_background {
            maybe_generate_asset_path(
                data_dir,
                &self.image_generator,
                background_generation_prompt.as_deref(),
                "background",
                background_generation_name.as_deref(),
                generation_allowed,
                image_model,
                cached_background,
            )
            .await
        } else {
            cached_background.map(|(_, path)| path.to_string())
        };
        let background_asset_path = if background_selection_requested {
            local_background.or(generated_background_asset_path).or_else(|| {
                if scene.name == session.scene.name {
                    session.assets.background_asset_path.clone()
                } else {
                    None
                }
            })
        } else if background_generation_requested {
            generated_background_asset_path
                .or(local_background)
                .or_else(|| {
                    if scene.name == session.scene.name {
                        session.assets.background_asset_path.clone()
                    } else {
                        None
                    }
                })
        } else {
            match background_source_mode {
                SourceMode::GeneratedOnly => generated_background_asset_path
                    .clone()
                    .or(local_background)
                    .or_else(|| {
                        if scene.name == session.scene.name {
                            session.assets.background_asset_path.clone()
                        } else {
                            None
                        }
                    }),
                SourceMode::GeneratedFirst => generated_background_asset_path
                    .clone()
                    .or(local_background)
                    .or_else(|| {
                        if scene.name == session.scene.name {
                            session.assets.background_asset_path.clone()
                        } else {
                            None
                        }
                    }),
                SourceMode::LocalOnly => local_background.or(generated_background_asset_path.clone()).or_else(|| {
                    if scene.name == session.scene.name {
                        session.assets.background_asset_path.clone()
                    } else {
                        None
                    }
                }),
                SourceMode::LocalFirst => local_background
                    .or(generated_background_asset_path.clone())
                    .or_else(|| {
                        if scene.name == session.scene.name {
                            session.assets.background_asset_path.clone()
                        } else {
                            None
                        }
                    }),
            }
        };

        let character_visual_map = build_character_visual_map(session, director_decision);
        let portrait_map = build_portrait_map(characters);
        let active_directive = character_visual_map.get(current_speaker);
        let active_hint = resolve_portrait_hint(current_speaker, session, true, active_directive);
        let active_generation_prompt = resolve_generation_prompt(
            current_speaker,
            session,
            active_directive,
            director_decision,
        );
        let active_generation_name = active_directive.and_then(|value| value.generation_name.clone());
        let active_generation_requested = active_directive
            .and_then(|value| value.generation_prompt.as_deref())
            .is_some_and(|value| !value.trim().is_empty());
        let active_selection_requested = active_directive.is_some_and(|value| {
            value.asset_path.is_some() || value.asset_name.is_some()
        });
        let active_cached_portrait = if session
            .assets
            .active_speaker_portrait_path
            .as_deref()
            .is_some_and(|path| path.starts_with("/assets/generated/"))
        {
            Some((
                session.assets.active_speaker_generation_prompt.as_str(),
                session.assets.active_speaker_portrait_path.as_deref().unwrap_or_default(),
            ))
        } else {
            None
        };
        let active_local_asset = select_named_asset(
            portrait_map
                .get(current_speaker)
                .map(|value| value.as_slice())
                .unwrap_or(&[]),
            active_directive.and_then(|value| value.asset_name.as_deref()),
            active_directive.and_then(|value| value.asset_path.as_deref()),
        )
        .or_else(|| {
            portrait_map
                .get(current_speaker)
                .and_then(|items| items.first().cloned())
        });

        let active_speaker_portrait_path = select_portrait_path(
            data_dir,
            &self.image_generator,
            portrait_source_mode,
            generation_allowed,
            active_generation_prompt.as_deref(),
            active_generation_name.as_deref(),
            active_local_asset,
            session.assets.active_speaker_portrait_path.clone(),
            image_model,
            active_generation_requested,
            active_selection_requested,
            active_cached_portrait,
        )
        .await;

        let mut visible_character_portraits = Vec::new();
        for name in &scene.present_characters {
            let directive = character_visual_map.get(name.as_str());
            let portrait_hint =
                resolve_portrait_hint(name, session, name == current_speaker, directive);
            let generation_prompt =
                resolve_generation_prompt(name, session, directive, director_decision);
            let generation_name = directive.and_then(|value| value.generation_name.clone());
            let generation_requested = directive
                .and_then(|value| value.generation_prompt.as_deref())
                .is_some_and(|value| !value.trim().is_empty());
            let selection_requested = directive
                .is_some_and(|value| value.asset_path.is_some() || value.asset_name.is_some());
            let local_asset = select_named_asset(
                portrait_map
                    .get(name)
                    .map(|value| value.as_slice())
                    .unwrap_or(&[]),
                directive.and_then(|value| value.asset_name.as_deref()),
                directive.and_then(|value| value.asset_path.as_deref()),
            )
            .or_else(|| {
                portrait_map
                    .get(name)
                    .and_then(|items| items.first().cloned())
            });
            let existing_path = existing_portrait_path(session, name);
            let cached_portrait = if existing_path.as_deref().is_some_and(|path| path.starts_with("/assets/generated/")) {
                Some((
                    session.assets.visible_character_portraits.iter()
                        .find(|item| item.character_name == *name)
                        .map(|item| item.generation_prompt.as_str())
                        .unwrap_or_default(),
                    existing_path.as_deref().unwrap_or_default(),
                ))
            } else {
                None
            };
            let portrait_asset_path = if name == current_speaker {
                active_speaker_portrait_path.clone()
            } else {
                select_portrait_path(
                    data_dir,
                    &self.image_generator,
                    portrait_source_mode,
                    generation_allowed,
                    generation_prompt.as_deref(),
                    generation_name.as_deref(),
                    local_asset,
                    existing_path,
                    image_model,
                    generation_requested,
                    selection_requested,
                    cached_portrait,
                )
                .await
            };
            visible_character_portraits.push(CharacterVisualState {
                character_name: name.clone(),
                portrait_hint,
                portrait_asset_path,
                generation_prompt: generation_prompt.unwrap_or_default(),
            });
        }

        AssetSelection {
            background_hint,
            active_speaker_portrait: active_hint,
            background_asset_path,
            active_speaker_portrait_path,
            background_generation_prompt: background_generation_prompt.unwrap_or_default(),
            active_speaker_generation_prompt: active_generation_prompt.unwrap_or_default(),
            visible_character_portraits,
        }
    }
}

#[derive(Clone, Copy)]
enum SourceMode {
    GeneratedFirst,
    LocalFirst,
    GeneratedOnly,
    LocalOnly,
}

#[derive(Clone)]
struct CharacterVisualDirectiveView {
    hint: String,
    generation_name: Option<String>,
    generation_prompt: Option<String>,
    asset_name: Option<String>,
    asset_path: Option<String>,
}

fn resolve_source_mode(raw: Option<&serde_json::Value>, fallback: &str) -> SourceMode {
    let value = raw
        .and_then(|item| item.as_str())
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .unwrap_or(fallback);
    match value {
        "generated-first" => SourceMode::GeneratedFirst,
        "generated-only" => SourceMode::GeneratedOnly,
        "local-only" => SourceMode::LocalOnly,
        _ => SourceMode::LocalFirst,
    }
}

fn normalize_asset_list(raw: Option<&serde_json::Value>) -> Vec<String> {
    raw.and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn normalize_asset_groups(raw: Option<&serde_json::Value>) -> Vec<(String, Vec<String>)> {
    let mut groups = Vec::new();
    let Some(map) = raw.and_then(|item| item.as_object()) else {
        return groups;
    };
    for (key, value) in map {
        let scene_name = key.trim().to_string();
        if scene_name.is_empty() {
            continue;
        }
        let assets = value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !assets.is_empty() {
            groups.push((scene_name, assets));
        }
    }
    groups
}

fn normalize_asset_match_text(value: &str) -> String {
    let lowercase = value.trim().to_lowercase();
    if lowercase.is_empty() {
        return String::new();
    }
    let stem = Path::new(&lowercase)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(lowercase.as_str());
    stem.chars().filter(|ch| ch.is_alphanumeric()).collect()
}

fn select_named_asset(paths: &[String], name: Option<&str>, path: Option<&str>) -> Option<String> {
    let normalized_paths = paths
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if normalized_paths.is_empty() {
        return None;
    }
    if let Some(requested_path) = path
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if let Some(matched) = normalized_paths
            .iter()
            .find(|item| item.as_str() == requested_path || item.ends_with(requested_path))
        {
            return Some(matched.clone());
        }
    }
    let requested_name = name
        .map(normalize_asset_match_text)
        .filter(|value| !value.is_empty());
    if let Some(requested_name) = requested_name {
        if let Some(exact) = normalized_paths
            .iter()
            .find(|item| normalize_asset_match_text(item) == requested_name)
        {
            return Some(exact.clone());
        }
        if let Some(fuzzy) = normalized_paths
            .iter()
            .find(|item| normalize_asset_match_text(item).contains(&requested_name))
        {
            return Some(fuzzy.clone());
        }
    }
    None
}

fn select_named_background_asset(
    groups: &[(String, Vec<String>)],
    local_assets: &[String],
    name: Option<&str>,
    path: Option<&str>,
) -> Option<String> {
    let mut all_paths = Vec::new();
    for (_, values) in groups {
        all_paths.extend(values.iter().cloned());
    }
    all_paths.extend(local_assets.iter().cloned());
    if let Some(by_path) = select_named_asset(&all_paths, None, path) {
        return Some(by_path);
    }
    let requested_name = name
        .map(normalize_asset_match_text)
        .filter(|item| !item.is_empty());
    if let Some(requested_name) = requested_name {
        for (scene_name, values) in groups {
            if normalize_asset_match_text(scene_name) == requested_name && !values.is_empty() {
                return Some(values[0].clone());
            }
        }
        for (scene_name, values) in groups {
            let normalized_scene = normalize_asset_match_text(scene_name);
            if (requested_name.contains(&normalized_scene)
                || normalized_scene.contains(&requested_name))
                && !values.is_empty()
            {
                return Some(values[0].clone());
            }
        }
    }
    select_named_asset(&all_paths, name, None)
}

fn select_default_background_asset(
    scene_name: &str,
    location: &str,
    groups: &[(String, Vec<String>)],
    local_assets: &[String],
) -> Option<String> {
    for candidate in [scene_name, location] {
        let requested = normalize_asset_match_text(candidate);
        if requested.is_empty() {
            continue;
        }
        for (configured_name, values) in groups {
            let normalized = normalize_asset_match_text(configured_name);
            if (requested == normalized
                || requested.contains(&normalized)
                || normalized.contains(&requested))
                && !values.is_empty()
            {
                return Some(values[0].clone());
            }
        }
    }
    for (_, values) in groups {
        if let Some(first) = values.first() {
            return Some(first.clone());
        }
    }
    local_assets.first().cloned()
}

fn resolve_background_generation_prompt(
    session: &SessionSnapshot,
    director_decision: Option<&serde_json::Value>,
) -> Option<String> {
    director_decision
        .and_then(|value| value.get("background_generation_prompt"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let existing = session
                .assets
                .background_generation_prompt
                .trim()
                .to_string();
            if existing.is_empty() {
                None
            } else {
                Some(existing)
            }
        })
}

fn build_character_visual_map(
    session: &SessionSnapshot,
    director_decision: Option<&serde_json::Value>,
) -> std::collections::HashMap<String, CharacterVisualDirectiveView> {
    let mut map = std::collections::HashMap::new();
    if let Some(items) = director_decision
        .and_then(|value| value.get("character_visual_directives"))
        .and_then(|value| value.as_array())
    {
        for item in items {
            let Some(character_name) = item
                .get("character_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let hint = item
                .get("portrait_hint")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let generation_name = item
                .get("generation_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let generation_prompt = item
                .get("generation_prompt")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let asset_name = item
                .get("portrait_asset_name")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let asset_path = item
                .get("portrait_asset_path")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            map.insert(
                character_name,
                CharacterVisualDirectiveView {
                    hint,
                    generation_name,
                    generation_prompt,
                    asset_name,
                    asset_path,
                },
            );
        }
        return map;
    }

    for item in &session.assets.visible_character_portraits {
        let name = item.character_name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let hint = item.portrait_hint.trim().to_string();
        let generation_prompt = if item.generation_prompt.trim().is_empty() {
            None
        } else {
            Some(item.generation_prompt.trim().to_string())
        };
        if hint.is_empty() && generation_prompt.is_none() {
            continue;
        }
        map.insert(
            name,
            CharacterVisualDirectiveView {
                hint,
                generation_name: None,
                generation_prompt,
                asset_name: None,
                asset_path: None,
            },
        );
    }
    map
}

fn build_portrait_map(
    characters: &[CharacterDefinition],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut map = std::collections::HashMap::new();
    for character in characters {
        let name = character.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let assets = character
            .portrait_assets
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        map.insert(name, assets);
    }
    map
}

fn resolve_portrait_hint(
    character_name: &str,
    session: &SessionSnapshot,
    active: bool,
    directive: Option<&CharacterVisualDirectiveView>,
) -> String {
    if let Some(directive) = directive {
        if !directive.hint.trim().is_empty() {
            return directive.hint.trim().to_string();
        }
    }
    if !active {
        for item in &session.assets.visible_character_portraits {
            if item.character_name == character_name && !item.portrait_hint.trim().is_empty() {
                return item.portrait_hint.trim().to_string();
            }
        }
    }
    if character_name == session.current_speaker
        && !session.assets.active_speaker_portrait.trim().is_empty()
    {
        return session.assets.active_speaker_portrait.trim().to_string();
    }
    if active {
        format!("{character_name}:speaking")
    } else {
        format!("{character_name}:idle")
    }
}

fn resolve_generation_prompt(
    character_name: &str,
    session: &SessionSnapshot,
    directive: Option<&CharacterVisualDirectiveView>,
    director_decision: Option<&serde_json::Value>,
) -> Option<String> {
    if let Some(directive) = directive {
        if let Some(prompt) = directive.generation_prompt.clone() {
            return Some(prompt);
        }
    }
    if director_decision.is_none() && character_name == session.current_speaker {
        let value = session
            .assets
            .active_speaker_generation_prompt
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn existing_portrait_path(session: &SessionSnapshot, character_name: &str) -> Option<String> {
    if character_name == session.current_speaker {
        return session.assets.active_speaker_portrait_path.clone();
    }
    for item in &session.assets.visible_character_portraits {
        if item.character_name == character_name && item.portrait_asset_path.is_some() {
            return item.portrait_asset_path.clone();
        }
    }
    None
}

async fn select_portrait_path(
    data_dir: &Path,
    image_generator: &ImageGenerator,
    mode: SourceMode,
    generation_allowed: bool,
    generation_prompt: Option<&str>,
    generation_name: Option<&str>,
    local_asset: Option<String>,
    existing_path: Option<String>,
    image_model: Option<&ModelConfig>,
    generation_requested: bool,
    selection_requested: bool,
    cached_portrait: Option<(&str, &str)>,
) -> Option<String> {
    if selection_requested {
        return local_asset.or(existing_path);
    }
    if !generation_requested
        && (matches!(mode, SourceMode::LocalOnly)
            || (matches!(mode, SourceMode::LocalFirst) && local_asset.is_some()))
    {
        return local_asset.or(existing_path);
    }
    let generated = maybe_generate_asset_path(
        data_dir,
        image_generator,
        generation_prompt,
        "portrait",
        generation_name,
        generation_allowed,
        image_model,
        cached_portrait,
    )
    .await;
    if generation_requested {
        return generated.or(local_asset).or(existing_path);
    }
    match mode {
        SourceMode::GeneratedOnly => generated.or(local_asset).or(existing_path),
        SourceMode::GeneratedFirst => generated.or(local_asset).or(existing_path),
        SourceMode::LocalOnly => local_asset.or(existing_path).or(generated),
        SourceMode::LocalFirst => local_asset.or(existing_path).or(generated),
    }
}

async fn maybe_generate_asset_path(
    data_dir: &Path,
    image_generator: &ImageGenerator,
    prompt: Option<&str>,
    kind: &str,
    name: Option<&str>,
    generation_allowed: bool,
    image_model: Option<&ModelConfig>,
    cached: Option<(&str, &str)>,
) -> Option<String> {
    let prompt = prompt
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())?;
    if name.is_none() {
        if let Some((cached_prompt, cached_path)) = cached {
            if cached_prompt == prompt && cached_path.starts_with("/assets/generated/") {
                return Some(cached_path.to_string());
            }
        }
    }
    if !generation_allowed {
        return None;
    }
    let model = image_model?;
    if model.base_url.trim().is_empty() {
        return None;
    }
    let (default_width, default_height) = if kind == "background" {
        (1536, 1024)
    } else {
        (1024, 1536)
    };
    let (width, height) = preferred_image_size(&model.model_id, default_width, default_height);
    let image = image_generator
        .generate(
            normalize_provider(&model.provider).as_str(),
            &model.base_url,
            &model.api_key,
            Some(model.model_id.as_str()),
            &ImageRequest {
                prompt: prompt.to_string(),
                negative_prompt: None,
                width: Some(width),
                height: Some(height),
                steps: Some(20),
                cfg_scale: Some(7.0),
                seed: None,
            },
        )
        .await
        .ok()?;
    persist_generated_image(data_dir, kind, name, &image.image_data, &image.format).ok()
}

fn persist_generated_image(
    data_dir: &Path,
    kind: &str,
    name: Option<&str>,
    bytes: &[u8],
    format: &str,
) -> Result<String, String> {
    let ext = match format.trim().to_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        "webp" => "webp",
        _ => "png",
    };
    let generated_dir: PathBuf = data_dir.join("assets").join("generated");
    fs::create_dir_all(&generated_dir).map_err(|e| e.to_string())?;
    let name = sanitize_generated_name(name.unwrap_or_default(), 80);
    let name = if name.is_empty() { kind } else { name.as_str() };
    let filename = format!("{kind}-{name}-{}.{}", uuid::Uuid::new_v4().simple(), ext);
    let path = generated_dir.join(&filename);
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(format!("/assets/generated/{filename}"))
}

fn sanitize_generated_name(value: &str, max_bytes: usize) -> String {
    let mut result = String::new();
    let mut separator_pending = false;
    for character in value.trim().chars() {
        if character.is_alphanumeric() {
            if separator_pending && !result.is_empty() {
                if result.len() + 1 > max_bytes {
                    break;
                }
                result.push('-');
            }
            separator_pending = false;
            if result.len() + character.len_utf8() > max_bytes {
                break;
            }
            result.push(character);
        } else if !result.is_empty() {
            separator_pending = true;
        }
    }
    result.trim_matches('-').to_string()
}

pub fn register_generated_session_assets(
    conn: &rusqlite::Connection,
    session: &SessionSnapshot,
) -> Result<(), String> {
    let generated_path = |path: &str| path.starts_with("/assets/generated/");
    let mut portrait_paths = std::collections::BTreeMap::<String, Vec<String>>::new();
    if let Some(path) = session.assets.active_speaker_portrait_path.as_deref() {
        if generated_path(path) {
            portrait_paths
                .entry(session.current_speaker.clone())
                .or_default()
                .push(path.to_string());
        }
    }
    for item in &session.assets.visible_character_portraits {
        if let Some(path) = item.portrait_asset_path.as_deref() {
            if generated_path(path) {
                portrait_paths
                    .entry(item.character_name.clone())
                    .or_default()
                    .push(path.to_string());
            }
        }
    }
    let background_path = session
        .assets
        .background_asset_path
        .as_deref()
        .filter(|path| generated_path(path));
    if background_path.is_none() && portrait_paths.is_empty() {
        return Ok(());
    }
    if let Some(path) = background_path {
        let mut stmt = conn
            .prepare("SELECT ui_theme_config_json FROM worlds WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let raw = stmt
            .query_row([&session.world_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut config = serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap_or_else(|_| serde_json::json!({}));
        if !config.is_object() {
            config = serde_json::json!({});
        }
        if config.get("assets").is_none() {
            config["assets"] = serde_json::json!({});
        }
        let mut assets = config["assets"].as_object().cloned().unwrap_or_default();
        let mut paths = normalize_asset_list(assets.get("local_background_assets"));
        if !paths.iter().any(|item| item == path) {
            paths.push(path.to_string());
        }
        assets.insert("local_background_assets".to_string(), serde_json::json!(paths));
        config["assets"] = serde_json::Value::Object(assets);
        conn.execute(
            "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = ?2",
            rusqlite::params![serde_json::to_string(&config).map_err(|e| e.to_string())?, session.world_id],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut stmt = conn
        .prepare("SELECT id, name, portrait_assets_json FROM characters WHERE world_id = ?1")
        .map_err(|e| e.to_string())?;
    let characters = stmt
        .query_map([&session.world_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (id, name, raw) in characters {
        let Some(paths) = portrait_paths.get(&name) else {
            continue;
        };
        let mut assets = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
        let mut changed = false;
        for path in paths {
            if !assets.iter().any(|item| item == path) {
                assets.push(path.clone());
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE characters SET portrait_assets_json = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(&assets).map_err(|e| e.to_string())?, id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
