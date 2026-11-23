use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::sticker::{StickerItem, StickerPack};
use crate::models::world::WorldDefinition;

const MAX_STICKER_BYTES: u64 = 2 * 1024 * 1024;
const STICKER_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

#[derive(Debug, Clone)]
pub struct EffectiveSticker {
    pub key: String,
    pub label: String,
    pub description: String,
    pub asset_path: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn list_packs(conn: &Connection) -> Result<Vec<StickerPack>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.world_id, p.name, p.description, p.enabled, \
                    (SELECT COUNT(*) FROM stickers s WHERE s.pack_id = p.id), \
                    p.created_at, p.updated_at \
             FROM sticker_packs p \
             ORDER BY p.world_id IS NOT NULL, p.name, p.created_at",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(StickerPack {
                id: row.get(0)?,
                world_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                sticker_count: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn get_pack(conn: &Connection, id: &str) -> Result<StickerPack, String> {
    conn.query_row(
        "SELECT p.id, p.world_id, p.name, p.description, p.enabled, \
                (SELECT COUNT(*) FROM stickers s WHERE s.pack_id = p.id), \
                p.created_at, p.updated_at \
         FROM sticker_packs p WHERE p.id = ?1",
        params![id],
        |row| {
            Ok(StickerPack {
                id: row.get(0)?,
                world_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                sticker_count: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "图库不存在".to_string())
}

pub fn create_pack(
    conn: &Connection,
    name: &str,
    description: &str,
    world_id: Option<&str>,
) -> Result<StickerPack, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("图库名称不能为空".to_string());
    }
    if name.chars().count() > 60 {
        return Err("图库名称不能超过 60 个字符".to_string());
    }
    let world_id = world_id.map(str::trim).filter(|value| !value.is_empty());
    if let Some(world_id) = world_id {
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM worlds WHERE id = ?1",
                params![world_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            return Err("所属世界不存在".to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = now();
    conn.execute(
        "INSERT INTO sticker_packs (id, world_id, name, description, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![id, world_id, name, description.trim(), timestamp],
    )
    .map_err(|e| e.to_string())?;
    get_pack(conn, &id)
}

pub fn update_pack(
    conn: &Connection,
    id: &str,
    name: &str,
    description: &str,
    enabled: bool,
) -> Result<StickerPack, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("图库名称不能为空".to_string());
    }
    if name.chars().count() > 60 {
        return Err("图库名称不能超过 60 个字符".to_string());
    }
    let affected = conn
        .execute(
            "UPDATE sticker_packs SET name = ?1, description = ?2, enabled = ?3, updated_at = ?4 \
             WHERE id = ?5",
            params![name, description.trim(), enabled as i64, now(), id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图库不存在".to_string());
    }
    get_pack(conn, id)
}

pub fn delete_pack(conn: &Connection, id: &str) -> Result<(), String> {
    let affected = conn
        .execute("DELETE FROM sticker_packs WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图库不存在".to_string());
    }
    Ok(())
}

pub fn list_stickers(conn: &Connection, pack_id: &str) -> Result<Vec<StickerItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, pack_id, key, label, description, asset_path, sort_order, enabled, \
                    created_at, updated_at \
             FROM stickers WHERE pack_id = ?1 ORDER BY sort_order, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![pack_id], |row| {
            Ok(StickerItem {
                id: row.get(0)?,
                pack_id: row.get(1)?,
                key: row.get(2)?,
                label: row.get(3)?,
                description: row.get(4)?,
                asset_path: row.get(5)?,
                sort_order: row.get(6)?,
                enabled: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn normalize_key(raw: &str) -> Result<String, String> {
    let key = raw.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("key 不能为空".to_string());
    }
    if key.chars().count() > 32 {
        return Err("key 不能超过 32 个字符".to_string());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err("key 只能使用小写字母、数字、短横线和下划线".to_string());
    }
    Ok(key)
}

fn canonical_asset_path(data_dir: &Path, raw: &str) -> Result<String, String> {
    let value = raw.trim().replace('\\', "/");
    let relative = value
        .strip_prefix("/assets/")
        .or_else(|| value.strip_prefix("assets/"))
        .unwrap_or_else(|| value.trim_start_matches('/'));
    if relative.is_empty() {
        return Err("图片路径不能为空".to_string());
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("图片路径无效".to_string());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !STICKER_EXTENSIONS.contains(&extension.as_str()) {
        return Err("只支持 png、jpg、webp、gif 图片".to_string());
    }
    let file_path = data_dir.join("assets").join(path);
    let metadata =
        std::fs::metadata(&file_path).map_err(|_| format!("图片文件不存在：{}", relative))?;
    if metadata.len() > MAX_STICKER_BYTES {
        return Err("单张图片不能超过 2MB".to_string());
    }
    Ok(format!("assets/{relative}"))
}

fn ensure_key_available(
    conn: &Connection,
    pack_id: &str,
    key: &str,
    excluding_id: Option<&str>,
) -> Result<(), String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM stickers WHERE pack_id = ?1 AND key = ?2 AND id <> ?3",
            params![pack_id, key, excluding_id.unwrap_or("")],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if existing.is_some() {
        return Err(format!("同一个图库里 key 不能重复：{key}"));
    }
    Ok(())
}

pub fn create_sticker(
    conn: &Connection,
    data_dir: &Path,
    pack_id: &str,
    key: &str,
    label: &str,
    description: &str,
    asset_path: &str,
) -> Result<StickerItem, String> {
    get_pack(conn, pack_id)?;
    let key = normalize_key(key)?;
    ensure_key_available(conn, pack_id, &key, None)?;
    let asset_path = canonical_asset_path(data_dir, asset_path)?;
    let label = label.trim();
    let label = if label.is_empty() {
        key.as_str()
    } else {
        label
    };
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM stickers WHERE pack_id = ?1",
            params![pack_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = now();
    conn.execute(
        "INSERT INTO stickers \
         (id, pack_id, key, label, description, asset_path, sort_order, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8)",
        params![
            id,
            pack_id,
            key,
            label,
            description.trim(),
            asset_path,
            sort_order,
            timestamp
        ],
    )
    .map_err(|e| e.to_string())?;
    get_sticker(conn, &id)
}

fn get_sticker(conn: &Connection, id: &str) -> Result<StickerItem, String> {
    conn.query_row(
        "SELECT id, pack_id, key, label, description, asset_path, sort_order, enabled, \
                created_at, updated_at \
         FROM stickers WHERE id = ?1",
        params![id],
        |row| {
            Ok(StickerItem {
                id: row.get(0)?,
                pack_id: row.get(1)?,
                key: row.get(2)?,
                label: row.get(3)?,
                description: row.get(4)?,
                asset_path: row.get(5)?,
                sort_order: row.get(6)?,
                enabled: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "图片不存在".to_string())
}

pub fn update_sticker(
    conn: &Connection,
    data_dir: &Path,
    id: &str,
    key: &str,
    label: &str,
    description: &str,
    asset_path: &str,
    enabled: bool,
) -> Result<StickerItem, String> {
    let existing = get_sticker(conn, id)?;
    let key = normalize_key(key)?;
    ensure_key_available(conn, &existing.pack_id, &key, Some(id))?;
    let asset_path = canonical_asset_path(data_dir, asset_path)?;
    let label = label.trim();
    let label = if label.is_empty() {
        key.as_str()
    } else {
        label
    };
    conn.execute(
        "UPDATE stickers SET key = ?1, label = ?2, description = ?3, asset_path = ?4, \
                enabled = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            key,
            label,
            description.trim(),
            asset_path,
            enabled as i64,
            now(),
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    get_sticker(conn, id)
}

pub fn delete_sticker(conn: &Connection, id: &str) -> Result<(), String> {
    let affected = conn
        .execute("DELETE FROM stickers WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图片不存在".to_string());
    }
    Ok(())
}

pub fn reorder_stickers(
    conn: &Connection,
    pack_id: &str,
    sticker_ids: &[String],
) -> Result<(), String> {
    for (index, sticker_id) in sticker_ids.iter().enumerate() {
        conn.execute(
            "UPDATE stickers SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND pack_id = ?4",
            params![index as i64, now(), sticker_id, pack_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn get_world_bindings(conn: &Connection, pack_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT world_id FROM world_sticker_bindings WHERE pack_id = ?1 ORDER BY world_id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![pack_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn pack_keys(conn: &Connection, pack_id: &str) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT key FROM stickers WHERE pack_id = ?1 AND enabled = 1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![pack_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect::<HashSet<String>>())
}

fn conflicting_keys(
    conn: &Connection,
    world_id: &str,
    pack_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT s.key \
             FROM stickers s JOIN sticker_packs p ON p.id = s.pack_id \
             WHERE s.enabled = 1 AND p.enabled = 1 AND p.id <> ?2 AND (\
                 p.world_id = ?1 OR (p.world_id IS NULL AND EXISTS (\
                     SELECT 1 FROM world_sticker_bindings b \
                     WHERE b.world_id = ?1 AND b.pack_id = p.id)))",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id, pack_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn set_world_binding(
    conn: &Connection,
    pack_id: &str,
    world_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let pack = get_pack(conn, pack_id)?;
    if pack.world_id.is_some() {
        return Err("世界专属图库会自动生效，无需绑定".to_string());
    }
    if enabled {
        let own = pack_keys(conn, pack_id)?;
        let conflicts = conflicting_keys(conn, world_id, pack_id)?
            .into_iter()
            .filter(|key| own.contains(key))
            .collect::<Vec<_>>();
        if !conflicts.is_empty() {
            return Err(format!(
                "与这个世界已有图片的 key 冲突：{}",
                conflicts.join("、")
            ));
        }
        conn.execute(
            "INSERT OR IGNORE INTO world_sticker_bindings (world_id, pack_id) VALUES (?1, ?2)",
            params![world_id, pack_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "DELETE FROM world_sticker_bindings WHERE world_id = ?1 AND pack_id = ?2",
            params![world_id, pack_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn effective_stickers(
    conn: &Connection,
    world_id: &str,
) -> Result<Vec<EffectiveSticker>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.key, s.label, s.description, s.asset_path \
             FROM stickers s JOIN sticker_packs p ON p.id = s.pack_id \
             WHERE s.enabled = 1 AND p.enabled = 1 AND (\
                 p.world_id = ?1 OR (p.world_id IS NULL AND EXISTS (\
                     SELECT 1 FROM world_sticker_bindings b \
                     WHERE b.world_id = ?1 AND b.pack_id = p.id))) \
             ORDER BY p.world_id IS NULL, p.name, s.sort_order, s.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![world_id], |row| {
            Ok(EffectiveSticker {
                key: row.get(0)?,
                label: row.get(1)?,
                description: row.get(2)?,
                asset_path: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    Ok(rows
        .filter_map(Result::ok)
        .filter(|sticker| seen.insert(sticker.key.clone()))
        .collect())
}

/// 把世界包里旧式的 `ui_theme_config.stickers` 迁移成世界专属图库。
fn migrate_world_legacy_stickers(
    conn: &Connection,
    world_id: &str,
    theme_config: &mut serde_json::Value,
) -> Result<bool, String> {
    let Some(items) = theme_config
        .get("stickers")
        .and_then(serde_json::Value::as_array)
        .cloned()
    else {
        return Ok(false);
    };
    if items.is_empty() {
        return Ok(false);
    }

    let pack_id = format!("legacy-pack-{world_id}");
    let timestamp = now();
    let world_name = conn
        .query_row(
            "SELECT name FROM worlds WHERE id = ?1",
            params![world_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let pack_name = if world_name.trim().is_empty() {
        "世界内嵌图库".to_string()
    } else {
        format!("{}表情包库", world_name.trim())
    };
    conn.execute(
        "INSERT OR IGNORE INTO sticker_packs \
         (id, world_id, name, description, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, '从世界包内嵌图片自动迁移', 1, ?4, ?4)",
        params![pack_id, world_id, pack_name, timestamp],
    )
    .map_err(|e| e.to_string())?;

    let mut kept_keys = HashSet::new();
    for (index, item) in items.iter().enumerate() {
        let Some(key) = item
            .get("key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(asset_path) = item
            .get("asset_path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !kept_keys.insert(key.to_string()) {
            continue;
        }
        let label = item
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(key);
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO stickers \
             (id, pack_id, key, label, description, asset_path, sort_order, enabled, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8) \
             ON CONFLICT(pack_id, key) DO UPDATE SET \
                 label = excluded.label, description = excluded.description, \
                 asset_path = excluded.asset_path, sort_order = excluded.sort_order, \
                 enabled = 1, updated_at = excluded.updated_at",
            params![
                format!("{pack_id}-{index}-{key}"),
                pack_id,
                key,
                label,
                description,
                asset_path,
                index as i64,
                timestamp
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    let current_keys = pack_keys(conn, &pack_id)?;
    for key in current_keys {
        if !kept_keys.contains(&key) {
            conn.execute(
                "DELETE FROM stickers WHERE pack_id = ?1 AND key = ?2",
                params![pack_id, key],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(object) = theme_config.as_object_mut() {
        object.remove("stickers");
    }
    conn.execute(
        "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = ?2",
        params![
            serde_json::to_string(theme_config).map_err(|e| e.to_string())?,
            world_id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// 把当前世界生效的图库图片写回 `ui_theme_config.stickers`，供工具注入与导出复用。
pub fn attach_world_stickers(conn: &Connection, world: &mut WorldDefinition) -> Result<(), String> {
    let world_id = world.id.clone();
    migrate_world_legacy_stickers(conn, &world_id, &mut world.ui_theme_config)?;
    let stickers = effective_stickers(conn, &world_id)?;
    let payload = stickers
        .iter()
        .map(|sticker| {
            serde_json::json!({
                "key": sticker.key,
                "label": sticker.label,
                "description": sticker.description,
                "asset_path": sticker.asset_path,
            })
        })
        .collect::<Vec<_>>();
    world.ui_theme_config["stickers"] = serde_json::Value::Array(payload);
    Ok(())
}

/// 侧栏一打开就能看到历史世界的内嵌图片：批量执行一次惰性迁移。
pub fn migrate_all_legacy_stickers(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE sticker_packs SET name = COALESCE(\
             (SELECT NULLIF(w.name, '') || '表情包库' FROM worlds w WHERE w.id = sticker_packs.world_id), \
             '世界内嵌图库') \
         WHERE id LIKE 'legacy-pack-%' AND name IN ('世界内嵌表情包', '世界内嵌图库')",
        [],
    )
    .map_err(|e| e.to_string())?;
    let rows = {
        let mut stmt = conn
            .prepare("SELECT id, ui_theme_config_json FROM worlds")
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (world_id, raw) in rows {
        let mut config = serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap_or_else(|_| serde_json::json!({}));
        migrate_world_legacy_stickers(conn, &world_id, &mut config)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::create_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO worlds (id, name) VALUES ('world-1', 'World')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn legacy_stickers_migrate_into_a_world_pack() {
        let conn = test_conn();
        let mut theme = serde_json::json!({
            "stickers": [
                { "key": "feast", "label": "开饭", "description": "", "asset_path": "/assets/stickers/feast.webp" }
            ]
        });
        conn.execute(
            "UPDATE worlds SET ui_theme_config_json = ?1 WHERE id = 'world-1'",
            params![serde_json::to_string(&theme).unwrap()],
        )
        .unwrap();
        migrate_world_legacy_stickers(&conn, "world-1", &mut theme).unwrap();
        let packs = list_packs(&conn).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].world_id.as_deref(), Some("world-1"));
        let stickers = list_stickers(&conn, &packs[0].id).unwrap();
        assert_eq!(stickers.len(), 1);
        assert_eq!(stickers[0].key, "feast");
        assert!(theme.get("stickers").is_none());
        let effective = effective_stickers(&conn, "world-1").unwrap();
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].label, "开饭");
    }

    #[test]
    fn public_packs_only_apply_when_bound() {
        let conn = test_conn();
        let pack = create_pack(&conn, "公共", "", None).unwrap();
        let timestamp = now();
        conn.execute(
            "INSERT INTO stickers \
             (id, pack_id, key, label, description, asset_path, sort_order, enabled, created_at, updated_at) \
             VALUES ('s1', ?1, 'hi', '嗨', '', 'assets/hi.webp', 0, 1, ?2, ?2)",
            params![pack.id, timestamp],
        )
        .unwrap();
        assert!(effective_stickers(&conn, "world-1").unwrap().is_empty());
        set_world_binding(&conn, &pack.id, "world-1", true).unwrap();
        assert_eq!(effective_stickers(&conn, "world-1").unwrap().len(), 1);
    }

    #[test]
    fn binding_rejects_conflicting_keys() {
        let conn = test_conn();
        let first = create_pack(&conn, "A", "", None).unwrap();
        let second = create_pack(&conn, "B", "", None).unwrap();
        let timestamp = now();
        for (id, pack_id) in [("s1", &first.id), ("s2", &second.id)] {
            conn.execute(
                "INSERT INTO stickers \
                 (id, pack_id, key, label, description, asset_path, sort_order, enabled, created_at, updated_at) \
                 VALUES (?1, ?2, 'hi', '嗨', '', 'assets/hi.webp', 0, 1, ?3, ?3)",
                params![id, pack_id, timestamp],
            )
            .unwrap();
        }
        set_world_binding(&conn, &first.id, "world-1", true).unwrap();
        assert!(set_world_binding(&conn, &second.id, "world-1", true).is_err());
    }
}
