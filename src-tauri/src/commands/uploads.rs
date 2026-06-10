use std::collections::HashSet;
use std::fs;
use std::path::{Component, PathBuf};

use rusqlite::params;
use tauri::State;

use crate::state::AppState;

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

#[tauri::command]
pub async fn upload_file(
    state: State<'_, AppState>,
    filename: String,
    data: Vec<u8>,
) -> Result<serde_json::Value, String> {
    if data.is_empty() {
        return Err("Upload file is empty".to_string());
    }
    if data.len() > MAX_UPLOAD_BYTES {
        return Err("File too large (max 50MB)".to_string());
    }
    save_asset_file(&state.data_dir, &filename, &data)
}

#[tauri::command]
pub async fn get_asset_base_dir(state: State<'_, AppState>) -> Result<String, String> {
    Ok(asset_root(&state.data_dir).to_string_lossy().to_string())
}

#[tauri::command]
pub async fn delete_asset(state: State<'_, AppState>, filename: String) -> Result<(), String> {
    let relative =
        normalize_asset_relative_path(&filename).ok_or_else(|| "Invalid asset path".to_string())?;
    let canonical_refs = canonical_asset_refs(&relative);
    let file_path = asset_root(&state.data_dir).join(&relative);
    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    cleanup_asset_references(&state, &canonical_refs).await?;
    Ok(())
}

pub fn save_asset_file(
    data_dir: &PathBuf,
    filename: &str,
    data: &[u8],
) -> Result<serde_json::Value, String> {
    let asset_dir = asset_root(data_dir);
    fs::create_dir_all(&asset_dir).map_err(|e| e.to_string())?;

    let safe_name = sanitize_filename(filename);
    let safe_path = PathBuf::from(&safe_name);
    let extension = safe_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string();
    let stem = safe_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset")
        .to_string();
    let stored_name = if extension.is_empty() {
        format!("{}-{}", stem, uuid::Uuid::new_v4().simple())
    } else {
        format!("{}-{}.{}", stem, uuid::Uuid::new_v4().simple(), extension)
    };

    let file_path = asset_dir.join(&stored_name);
    fs::write(&file_path, data).map_err(|e| e.to_string())?;

    let relative_path = stored_name.replace('\\', "/");
    Ok(serde_json::json!({
        "filename": stored_name,
        "relative_path": relative_path,
        "asset_path": format!("assets/{}", stored_name),
        "url": format!("/assets/{}", stored_name),
    }))
}

fn asset_root(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("assets")
}

fn sanitize_filename(filename: &str) -> String {
    let fallback = "asset".to_string();
    let path = PathBuf::from(filename);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&fallback);
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        fallback
    } else {
        sanitized
    }
}

fn normalize_asset_relative_path(filename: &str) -> Option<PathBuf> {
    let value = filename.trim().replace('\\', "/");
    let trimmed = if let Some(path) = value.strip_prefix("/assets/") {
        path
    } else if let Some(path) = value.strip_prefix("assets/") {
        path
    } else {
        value.as_str()
    };
    let path = PathBuf::from(trimmed);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(path)
}

fn canonical_asset_refs(relative: &PathBuf) -> HashSet<String> {
    let normalized_relative = relative.to_string_lossy().replace('\\', "/");
    let mut refs = HashSet::new();
    refs.insert(normalized_relative.clone());
    refs.insert(format!("assets/{normalized_relative}"));
    refs.insert(format!("/assets/{normalized_relative}"));
    refs
}

fn remove_asset_from_list(
    list: &[serde_json::Value],
    refs: &HashSet<String>,
) -> Vec<serde_json::Value> {
    list.iter()
        .filter_map(|item| {
            let value = item.as_str()?.trim().to_string();
            if value.is_empty() || refs.contains(&value) {
                None
            } else {
                Some(serde_json::Value::String(value))
            }
        })
        .collect()
}

fn prune_world_ui_theme_config(config: &mut serde_json::Value, refs: &HashSet<String>) -> bool {
    let Some(obj) = config.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(items) = obj
        .get("local_background_assets")
        .and_then(|value| value.as_array())
        .cloned()
    {
        let next = remove_asset_from_list(&items, refs);
        if next != items {
            obj.insert(
                "local_background_assets".to_string(),
                serde_json::Value::Array(next),
            );
            changed = true;
        }
    }
    if let Some(map) = obj
        .get("local_scene_backgrounds")
        .and_then(|value| value.as_object())
        .cloned()
    {
        let mut next_map = serde_json::Map::new();
        for (scene, values) in map {
            let items = values.as_array().cloned().unwrap_or_default();
            let next = remove_asset_from_list(&items, refs);
            if !next.is_empty() {
                next_map.insert(scene, serde_json::Value::Array(next));
            }
        }
        if serde_json::Value::Object(next_map.clone())
            != obj
                .get("local_scene_backgrounds")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}))
        {
            obj.insert(
                "local_scene_backgrounds".to_string(),
                serde_json::Value::Object(next_map),
            );
            changed = true;
        }
    }
    changed
}

fn prune_session_assets(assets: &mut serde_json::Value, refs: &HashSet<String>) -> bool {
    let Some(obj) = assets.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    for key in ["background_asset_path", "active_speaker_portrait_path"] {
        if let Some(value) = obj.get(key).and_then(|value| value.as_str()) {
            if refs.contains(value.trim()) {
                obj.insert(key.to_string(), serde_json::Value::Null);
                changed = true;
            }
        }
    }
    if let Some(items) = obj
        .get("visible_character_portraits")
        .and_then(|value| value.as_array())
        .cloned()
    {
        let mut next = Vec::with_capacity(items.len());
        for item in items {
            let mut entry = item;
            if let Some(path) = entry
                .get("portrait_asset_path")
                .and_then(|value| value.as_str())
            {
                if refs.contains(path.trim()) {
                    if let Some(entry_obj) = entry.as_object_mut() {
                        entry_obj
                            .insert("portrait_asset_path".to_string(), serde_json::Value::Null);
                        changed = true;
                    }
                }
            }
            next.push(entry);
        }
        obj.insert(
            "visible_character_portraits".to_string(),
            serde_json::Value::Array(next),
        );
    }
    changed
}

async fn cleanup_asset_references(
    state: &State<'_, AppState>,
    refs: &HashSet<String>,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let conn = db.conn();

    {
        let mut stmt = conn
            .prepare("SELECT id, ui_theme_config_json FROM worlds")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, raw) = row.map_err(|e| e.to_string())?;
            let mut value = serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::json!({}));
            if prune_world_ui_theme_config(&mut value, refs) {
                conn.execute(
                    "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = ?2",
                    params![
                        serde_json::to_string(&value).map_err(|e| e.to_string())?,
                        id
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT id, portrait_assets_json, avatar_asset FROM characters")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, raw, avatar_asset) = row.map_err(|e| e.to_string())?;
            let parsed = serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::json!([]));
            let items = parsed.as_array().cloned().unwrap_or_default();
            let next = remove_asset_from_list(&items, refs);
            let next_avatar_asset = if refs.contains(avatar_asset.as_str()) {
                ""
            } else {
                avatar_asset.as_str()
            };
            if next != items || next_avatar_asset != avatar_asset {
                conn.execute(
                    "UPDATE characters SET portrait_assets_json = ?1, avatar_asset = ?2 WHERE id = ?3",
                    params![
                        serde_json::to_string(&next).map_err(|e| e.to_string())?,
                        next_avatar_asset,
                        id
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT id, assets_json FROM sessions")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, raw) = row.map_err(|e| e.to_string())?;
            let mut assets = serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::json!({}));
            if prune_session_assets(&mut assets, refs) {
                conn.execute(
                    "UPDATE sessions SET assets_json = ?1 WHERE id = ?2",
                    params![
                        serde_json::to_string(&assets).map_err(|e| e.to_string())?,
                        id
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT id, home_background_strategy FROM settings WHERE id = 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next() {
            let (_, home_background_strategy) = row.map_err(|e| e.to_string())?;
            if refs.contains(home_background_strategy.trim()) {
                conn.execute(
                    "UPDATE settings SET home_background_strategy = '' WHERE id = 1",
                    [],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}
