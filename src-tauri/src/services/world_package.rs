use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::models::character::{CharacterDefinition, CharacterPackageData};
use crate::models::world::{
    BinaryFileResponse, SavedFileResponse, WorldDefinition, WorldPackageAssetEntry,
    WorldPackageCharacterFileEntry, WorldPackageManifest, WorldPackageWorldData,
};
use crate::state::AppState;

const WORLD_PACKAGE_FORMAT: &str = "dream-world-package";
const WORLD_PACKAGE_VERSION: u32 = 5;
const WORLD_PACKAGE_FILE: &str = "world/world.json";
const WORLD_PACKAGE_DESKTOP_UI_FILE: &str = "world/ui.desktop.jsonc";
const WORLD_PACKAGE_MOBILE_UI_FILE: &str = "world/ui.mobile.jsonc";

pub struct ImportedWorldPackage {
    pub world: WorldPackageWorldData,
    pub desktop_ui_source: String,
    pub mobile_ui_source: String,
    pub characters: Vec<CharacterPackageData>,
    pub asset_map: HashMap<String, String>,
}

pub struct WorldPackageService;

impl WorldPackageService {
    pub fn build_package(
        data_dir: &Path,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
    ) -> Result<BinaryFileResponse, String> {
        let assets_root = assets_root(data_dir);
        let manifest = build_manifest(world, characters, &assets_root)?;
        let world_data = to_world_package_data(world, characters);
        let character_data: Vec<(WorldPackageCharacterFileEntry, CharacterPackageData)> =
            characters
                .iter()
                .map(|character| {
                    let dir_name =
                        slugify(&(character.id.clone() + "-" + &character.name), "character");
                    (
                        WorldPackageCharacterFileEntry {
                            source_character_id: character.id.clone(),
                            character_name: character.name.clone(),
                            file_path: format!("characters/{dir_name}/character.json"),
                        },
                        CharacterPackageData {
                            source_character_id: character.id.clone(),
                            name: character.name.clone(),
                            role: character.role.clone(),
                            background_prompt: character.background_prompt.clone(),
                            model: character.model.clone(),
                            memory_strategy: character.memory_strategy.clone(),
                            recent_dialogue_rounds: character.recent_dialogue_rounds,
                            attributes: character.attributes.clone(),
                            portrait_assets: character.portrait_assets.clone(),
                            avatar_asset: character.avatar_asset.clone(),
                            system_prompt_template: character.system_prompt_template.clone(),
                            response_contract_prompt: character.response_contract_prompt.clone(),
                            narration_prompt: character.narration_prompt.clone(),
                            runtime_system_prompt: character.runtime_system_prompt.clone(),
                        },
                    )
                })
                .collect();

        let mut buffer = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            archive
                .start_file("manifest.json", options)
                .map_err(|e| e.to_string())?;
            archive
                .write_all(
                    serde_json::to_string_pretty(&manifest)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;

            archive
                .start_file(WORLD_PACKAGE_FILE, options)
                .map_err(|e| e.to_string())?;
            archive
                .write_all(
                    serde_json::to_string_pretty(&world_data)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;

            if let Some(source) = world
                .ui_theme_config
                .get("desktop_file")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_DESKTOP_UI_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            if let Some(source) = world
                .ui_theme_config
                .get("mobile_file")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_MOBILE_UI_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            for (entry, character) in &character_data {
                archive
                    .start_file(&entry.file_path, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(
                        serde_json::to_string_pretty(character)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
            }

            for asset in &manifest.assets {
                let relative = normalize_asset_relative_path(&asset.source_path)
                    .ok_or_else(|| format!("Invalid asset path: {}", asset.source_path))?;
                let file_path = assets_root.join(relative);
                if file_path.is_file() {
                    archive
                        .start_file(&asset.archive_path, options)
                        .map_err(|e| e.to_string())?;
                    let bytes = fs::read(file_path).map_err(|e| e.to_string())?;
                    archive.write_all(&bytes).map_err(|e| e.to_string())?;
                }
            }

            archive.finish().map_err(|e| e.to_string())?;
        }

        Ok(BinaryFileResponse {
            filename: format!("{}.zip", slugify(&world.name, "world-package")),
            bytes: buffer.into_inner(),
        })
    }

    pub async fn save_package_to_downloads(
        app: &AppHandle,
        state: &AppState,
        package: BinaryFileResponse,
    ) -> Result<SavedFileResponse, String> {
        let target_dir = resolve_export_target_dir(app, state).await?;
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        let target_path = unique_file_path(&target_dir, &package.filename);
        fs::write(&target_path, &package.bytes).map_err(|e| e.to_string())?;

        Ok(SavedFileResponse {
            filename: package.filename,
            saved_path: target_path.to_string_lossy().to_string(),
        })
    }

    pub fn import_package_archive(
        data_dir: &Path,
        data: Vec<u8>,
    ) -> Result<ImportedWorldPackage, String> {
        let assets_root = assets_root(data_dir);
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| e.to_string())?;
        let manifest: WorldPackageManifest = read_json_from_zip(&mut archive, "manifest.json")
            .map_err(|e| format!("Invalid manifest: {e}"))?;
        if manifest.format != WORLD_PACKAGE_FORMAT || manifest.version != WORLD_PACKAGE_VERSION {
            return Err("Unsupported world package format".to_string());
        }

        let mut asset_map = HashMap::new();
        for asset in &manifest.assets {
            let mut file = open_zip_entry(&mut archive, &asset.archive_path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            let target_asset = persist_asset_file(&assets_root, &asset.archive_path, &bytes)?;
            asset_map.insert(asset.source_path.clone(), target_asset.clone());
            asset_map.insert(asset.archive_path.clone(), target_asset);
        }

        let world_file = manifest
            .world_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_FILE.to_string());
        let package_world: WorldPackageWorldData = read_json_from_zip(&mut archive, &world_file)
            .map_err(|e| format!("Invalid world data: {e}"))?;
        let desktop_ui_file = manifest
            .desktop_ui_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_DESKTOP_UI_FILE.to_string());
        let desktop_ui_source =
            read_text_from_zip(&mut archive, &desktop_ui_file).unwrap_or_default();
        let mobile_ui_file = manifest
            .mobile_ui_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_MOBILE_UI_FILE.to_string());
        let mobile_ui_source =
            read_text_from_zip(&mut archive, &mobile_ui_file).unwrap_or_default();
        let mut package_characters = Vec::new();
        for entry in &manifest.character_files {
            let mut character: CharacterPackageData =
                read_json_from_zip(&mut archive, &entry.file_path)
                    .map_err(|e| format!("Invalid character data: {e}"))?;
            if character.source_character_id.trim().is_empty() {
                character.source_character_id = entry.source_character_id.clone();
            }
            package_characters.push(character);
        }
        if package_characters.is_empty() {
            return Err("World package is missing character files".to_string());
        }

        Ok(ImportedWorldPackage {
            world: package_world,
            desktop_ui_source,
            mobile_ui_source,
            characters: package_characters,
            asset_map,
        })
    }

    pub fn read_package_from_path(path: &str) -> Result<Vec<u8>, String> {
        let normalized_path = normalize_selected_package_path(path);
        fs::read(&normalized_path).map_err(|e| format!("Failed to read world package: {e}"))
    }

    pub fn remap_world_ui_theme_assets(
        value: serde_json::Value,
        asset_map: &HashMap<String, String>,
    ) -> serde_json::Value {
        remap_world_ui_theme_assets(value, asset_map)
    }
}

async fn resolve_export_target_dir(app: &AppHandle, state: &AppState) -> Result<PathBuf, String> {
    if let Ok(download_dir) = app.path().download_dir() {
        if !download_dir.as_os_str().is_empty() {
            return Ok(download_dir);
        }
    }

    let db = state.db.lock().await;
    let mut stmt = db
        .conn()
        .prepare("SELECT export_directory FROM settings WHERE id = 1")
        .map_err(|e| e.to_string())?;
    let export_directory: String = stmt.query_row([], |row| row.get(0)).unwrap_or_default();
    drop(stmt);
    drop(db);

    if !export_directory.trim().is_empty() {
        return Ok(PathBuf::from(export_directory.trim()));
    }

    Ok(state.data_dir.join("exports"))
}

fn unique_file_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    let ext = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    for index in 2..1000 {
        let next_name = if ext.is_empty() {
            format!("{stem}-{index}")
        } else {
            format!("{stem}-{index}.{ext}")
        };
        let next_path = dir.join(next_name);
        if !next_path.exists() {
            return next_path;
        }
    }

    candidate
}

fn normalize_selected_package_path(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("file://") {
        if cfg!(windows) && stripped.starts_with('/') && stripped.as_bytes().get(2) == Some(&b':') {
            return stripped[1..].to_string();
        }
        return stripped.to_string();
    }
    path.to_string()
}

fn assets_root(data_dir: &Path) -> PathBuf {
    let root = data_dir.join("assets");
    let _ = fs::create_dir_all(&root);
    root
}

fn normalize_asset_relative_path(asset_path: &str) -> Option<PathBuf> {
    let value = asset_path.trim().replace('\\', "/");
    let trimmed = if let Some(path) = value.strip_prefix("/assets/") {
        path
    } else if let Some(path) = value.strip_prefix("assets/") {
        path
    } else {
        value.as_str()
    };
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

struct AssetDescriptor {
    source_path: String,
    owner_type: String,
    owner_id: String,
}

fn collect_asset_descriptors(
    world: &WorldDefinition,
    characters: &[CharacterDefinition],
) -> Vec<AssetDescriptor> {
    let mut assets = Vec::new();
    let empty_assets = serde_json::json!({});
    let asset_config = world.ui_theme_config.get("assets").unwrap_or(&empty_assets);
    if let Some(backgrounds) = world
        .ui_theme_config
        .get("assets")
        .and_then(|value| value.get("local_background_assets"))
        .and_then(|value| value.as_array())
    {
        for asset in backgrounds {
            if let Some(path) = asset.as_str() {
                push_asset(&mut assets, path, "world_background", "global".to_string());
            }
        }
    }
    if let Some(scene_map) = asset_config
        .get("local_scene_backgrounds")
        .and_then(|value| value.as_object())
    {
        for (scene_id, value) in scene_map {
            if let Some(items) = value.as_array() {
                for item in items {
                    if let Some(path) = item.as_str() {
                        push_asset(&mut assets, path, "scene_background", scene_id.clone());
                    }
                }
            }
        }
    }
    for character in characters {
        push_asset(
            &mut assets,
            &character.avatar_asset,
            "character_avatar",
            character.id.clone(),
        );
        for asset in &character.portrait_assets {
            push_asset(
                &mut assets,
                asset,
                "character_portrait",
                character.id.clone(),
            );
        }
    }
    assets
}

fn push_asset(target: &mut Vec<AssetDescriptor>, path: &str, owner_type: &str, owner_id: String) {
    let normalized = path.trim();
    if !normalized.is_empty()
        && !target
            .iter()
            .any(|existing| existing.source_path == normalized)
    {
        target.push(AssetDescriptor {
            source_path: normalized.to_string(),
            owner_type: owner_type.to_string(),
            owner_id,
        });
    }
}

fn build_manifest(
    world: &WorldDefinition,
    characters: &[CharacterDefinition],
    assets_root: &Path,
) -> Result<WorldPackageManifest, String> {
    let asset_descriptors = collect_asset_descriptors(world, characters);
    let mut missing_assets = Vec::new();
    let assets = asset_descriptors
        .into_iter()
        .filter_map(|descriptor| {
            let relative = normalize_asset_relative_path(&descriptor.source_path)?;
            let file_path = assets_root.join(&relative);
            if !file_path.is_file() {
                missing_assets.push(descriptor.source_path.clone());
                return None;
            }
            Some(WorldPackageAssetEntry {
                source_path: descriptor.source_path.clone(),
                archive_path: format!("assets/{}", relative.to_string_lossy().replace('\\', "/")),
                owner_type: Some(descriptor.owner_type),
                owner_id: Some(descriptor.owner_id),
            })
        })
        .collect();

    if !missing_assets.is_empty() {
        missing_assets.sort();
        missing_assets.dedup();
        return Err(format!(
            "Missing local assets required for export: {}",
            missing_assets.join(", ")
        ));
    }

    let character_files = characters
        .iter()
        .map(|character| {
            let dir_name = slugify(&(character.id.clone() + "-" + &character.name), "character");
            WorldPackageCharacterFileEntry {
                source_character_id: character.id.clone(),
                character_name: character.name.clone(),
                file_path: format!("characters/{dir_name}/character.json"),
            }
        })
        .collect();

    Ok(WorldPackageManifest {
        format: WORLD_PACKAGE_FORMAT.to_string(),
        version: WORLD_PACKAGE_VERSION,
        world: None,
        world_file: Some(WORLD_PACKAGE_FILE.to_string()),
        desktop_ui_file: Some(WORLD_PACKAGE_DESKTOP_UI_FILE.to_string()),
        mobile_ui_file: Some(WORLD_PACKAGE_MOBILE_UI_FILE.to_string()),
        characters_file: None,
        character_files,
        assets,
    })
}

fn to_world_package_data(
    world: &WorldDefinition,
    characters: &[CharacterDefinition],
) -> WorldPackageWorldData {
    let character_name_by_id = characters
        .iter()
        .map(|character| (character.id.clone(), character.name.clone()))
        .collect::<HashMap<_, _>>();
    WorldPackageWorldData {
        name: world.name.clone(),
        genre: world.genre.clone(),
        background_prompt: world.background_prompt.clone(),
        opening_scene: world.opening_scene.clone(),
        summary: world.summary.clone(),
        time_system: world.time_system.clone(),
        map_nodes: world.map_nodes.clone(),
        triggers: world.triggers.clone(),
        time_config: world.time_config.clone(),
        director_config: world.director_config.clone(),
        ui_assets_config: world
            .ui_theme_config
            .get("assets")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        opening_messages: world.opening_messages.clone(),
        opening_character_names: world
            .opening_character_ids
            .iter()
            .filter_map(|id| character_name_by_id.get(id).cloned())
            .collect(),
        player_character_name: world
            .player_character_id
            .as_ref()
            .and_then(|id| character_name_by_id.get(id).cloned()),
        opening_character_source_ids: world.opening_character_ids.clone(),
        player_character_source_id: world.player_character_id.clone(),
    }
}

fn remap_world_ui_theme_assets(
    value: serde_json::Value,
    asset_map: &HashMap<String, String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(|item| remap_world_ui_theme_assets(item, asset_map))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, remap_world_ui_theme_assets(value, asset_map)))
                .collect(),
        ),
        serde_json::Value::String(text) => {
            serde_json::Value::String(asset_map.get(&text).cloned().unwrap_or(text))
        }
        other => other,
    }
}

fn persist_asset_file(
    assets_root: &Path,
    archive_path: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let relative = archive_path.strip_prefix("assets/").unwrap_or(archive_path);
    let relative_path = PathBuf::from(relative);
    let directory = relative_path
        .parent()
        .map(|path| assets_root.join(path))
        .unwrap_or_else(|| assets_root.to_path_buf());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let stem = relative_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = relative_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let filename = if extension.is_empty() {
        format!("{}-{}", stem, uuid::Uuid::new_v4().simple())
    } else {
        format!("{}-{}.{}", stem, uuid::Uuid::new_v4().simple(), extension)
    };
    let target = directory.join(filename);
    fs::write(&target, bytes).map_err(|e| e.to_string())?;
    let relative_saved = target
        .strip_prefix(assets_root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(format!("/assets/{relative_saved}"))
}

fn read_json_from_zip<T: serde::de::DeserializeOwned>(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    path: &str,
) -> Result<T, String> {
    let mut file = open_zip_entry(archive, path)?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn read_text_from_zip(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    path: &str,
) -> Result<String, String> {
    let mut file = open_zip_entry(archive, path)?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(|e| e.to_string())?;
    Ok(text)
}

fn open_zip_entry<'a>(
    archive: &'a mut zip::ZipArchive<Cursor<Vec<u8>>>,
    path: &str,
) -> Result<zip::read::ZipFile<'a>, String> {
    if let Some(index) = archive.index_for_name(path) {
        return archive.by_index(index).map_err(|e| e.to_string());
    }

    let alternate = if path.contains('/') {
        path.replace('/', "\\")
    } else if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        return Err("specified file not found in archive".to_string());
    };

    if let Some(index) = archive.index_for_name(&alternate) {
        return archive.by_index(index).map_err(|e| e.to_string());
    }

    Err("specified file not found in archive".to_string())
}

fn slugify(value: &str, fallback: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&character) {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}
