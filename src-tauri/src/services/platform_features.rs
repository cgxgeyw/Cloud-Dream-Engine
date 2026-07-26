//! 第 12 项：世界包平台能力目录（第一批：文件能力）。
//!
//! 统一模式（每个 action 一致）：manifest 声明 → 用户允许（world_feature_grants 表，
//! 宿主控制、世界包不可写）→ 执行 → 结构化结果或带前缀的错误。
//! 桌面端配套规则：有桌面实现就走桌面实现，没有就返回 `unsupported`，
//! 不静默失败、不假装成功。安卓系统交互经 Kotlin 中间件（红线 7），
//! 本模块在安卓上只拼路径、做校验，IO 与系统调起都交给 NativeBridge。
//!
//! 错误消息统一 `code: 中文说明` 前缀，便于世界包程序化判断：
//! `unsupported:` 当前平台不支持；`not_declared:` 包未声明；
//! `not_granted:` 用户未允许；`invalid_params:` 参数问题；`io:` 读写失败。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::models::world::WorldDefinition;

pub const FEATURE_FILE_PICK: &str = "file.pick";
pub const FEATURE_FILE_READ: &str = "file.read";
pub const FEATURE_FILE_WRITE: &str = "file.write";
pub const FEATURE_FILE_SHARE: &str = "file.share";

/// 第一批目录。manifest 声明与本表对不上的 action 在导入时即报错。
pub const KNOWN_FEATURES: [&str; 4] = [
    FEATURE_FILE_PICK,
    FEATURE_FILE_READ,
    FEATURE_FILE_WRITE,
    FEATURE_FILE_SHARE,
];

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// 世界包在 ui_theme_config.platform_features 里声明的能力列表。
pub fn declared_features(ui_theme_config: &Value) -> Vec<String> {
    ui_theme_config
        .get("platform_features")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .filter(|item| KNOWN_FEATURES.contains(&item.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

/// 导入/manifest 校验：platform_features 必须是已知 action 的字符串数组。
pub fn validate_declared_features(ui_theme_config: &Value) -> Result<(), String> {
    let Some(raw) = ui_theme_config.get("platform_features") else {
        return Ok(());
    };
    let Some(items) = raw.as_array() else {
        return Err("platform_features must be an array of feature ids.".to_string());
    };
    for item in items {
        let Some(id) = item.as_str() else {
            return Err("platform_features entries must be strings.".to_string());
        };
        if !KNOWN_FEATURES.contains(&id) {
            return Err(format!(
                "Unknown platform feature `{id}`. Known: {}.",
                KNOWN_FEATURES.join(", ")
            ));
        }
    }
    Ok(())
}

// ---- 用户授权（world_feature_grants，宿主控制） ----

pub fn is_granted(conn: &Connection, world_id: &str, feature: &str) -> Result<bool, String> {
    let granted: Option<i64> = conn
        .query_row(
            "SELECT granted FROM world_feature_grants WHERE world_id = ?1 AND feature = ?2",
            params![world_id, feature],
            |row| row.get(0),
        )
        .ok();
    Ok(granted.unwrap_or(0) != 0)
}

pub fn set_grant(
    conn: &Connection,
    world_id: &str,
    feature: &str,
    granted: bool,
) -> Result<(), String> {
    if !KNOWN_FEATURES.contains(&feature) {
        return Err(format!("Unknown platform feature `{feature}`."));
    }
    if granted {
        conn.execute(
            "INSERT INTO world_feature_grants (world_id, feature, granted, updated_at)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT (world_id, feature) DO UPDATE SET granted = 1, updated_at = ?3",
            params![world_id, feature, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "DELETE FROM world_feature_grants WHERE world_id = ?1 AND feature = ?2",
            params![world_id, feature],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldFeatureGrantStatus {
    pub feature: String,
    pub declared: bool,
    pub granted: bool,
}

/// 设置页「世界权限」区的数据：一个世界的全部声明能力与授权状态。
pub fn list_grant_status(
    conn: &Connection,
    world: &WorldDefinition,
) -> Result<Vec<WorldFeatureGrantStatus>, String> {
    let declared = declared_features(&world.ui_theme_config);
    let mut statuses = Vec::with_capacity(declared.len());
    for feature in declared {
        statuses.push(WorldFeatureGrantStatus {
            granted: is_granted(conn, &world.id, &feature)?,
            feature,
            declared: true,
        });
    }
    Ok(statuses)
}

// ---- 世界隔离目录与路径防护 ----

fn world_files_root(data_dir: &Path, world_id: &str) -> PathBuf {
    data_dir.join("world_files").join(world_id)
}

/// 世界包传来的相对路径 → 世界目录内的绝对路径。
/// 拒绝绝对路径、`..` 穿越、空路径与非法字符（红线：世界包不能碰目录外的文件）。
fn resolve_world_file(data_dir: &Path, world_id: &str, raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("invalid_params: path 不能为空。".to_string());
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err("invalid_params: path 必须是世界目录内的相对路径。".to_string());
    }
    let mut normalized = PathBuf::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err("invalid_params: path 不允许包含 .. 或空段。".to_string());
        }
        if !segment
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        {
            return Err(format!(
                "invalid_params: path 段 `{segment}` 含不允许的字符。"
            ));
        }
        normalized.push(segment);
    }
    Ok(world_files_root(data_dir, world_id).join(normalized))
}

// ---- 调用入口 ----

/// 声明 + 授权检查（世界包调用平台能力的前置闸门，与平台实现解耦、可单测）。
pub fn check_allowed(conn: &Connection, world: &WorldDefinition, feature: &str) -> Result<(), String> {
    if !KNOWN_FEATURES.contains(&feature) {
        return Err(format!(
            "unsupported: 未知的平台能力 `{feature}`（当前目录：{}）。",
            KNOWN_FEATURES.join(", ")
        ));
    }
    if !declared_features(&world.ui_theme_config)
        .iter()
        .any(|declared| declared == feature)
    {
        return Err(format!(
            "not_declared: 世界包未在 manifest 声明 `{feature}`，无法调用。"
        ));
    }
    if !is_granted(conn, &world.id, feature)? {
        return Err(format!(
            "not_granted: 「{}」的 `{feature}` 未获允许，请在 设置 → 世界权限 中开启。",
            world.name
        ));
    }
    Ok(())
}

/// 统一执行入口。调用顺序固定：声明检查 → 授权检查 → 平台实现。
pub fn invoke(
    conn: &Connection,
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    feature: &str,
    params: &Value,
) -> Result<Value, String> {
    check_allowed(conn, world, feature)?;
    dispatch(data_dir, app, world, feature, params)
}

#[cfg(not(target_os = "android"))]
fn dispatch(
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    feature: &str,
    params: &Value,
) -> Result<Value, String> {
    match feature {
        FEATURE_FILE_PICK => pick_files(app, params),
        FEATURE_FILE_READ => read_world_file(data_dir, world, params),
        FEATURE_FILE_WRITE => write_world_file(data_dir, world, params),
        FEATURE_FILE_SHARE => share_world_file(),
        _ => unreachable!(),
    }
}

#[cfg(target_os = "android")]
fn dispatch(
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    feature: &str,
    params: &Value,
) -> Result<Value, String> {
    match feature {
        FEATURE_FILE_PICK => pick_files(app, params),
        FEATURE_FILE_READ => read_world_file(data_dir, app, world, params),
        FEATURE_FILE_WRITE => write_world_file(data_dir, app, world, params),
        FEATURE_FILE_SHARE => share_world_file(data_dir, app, world, params),
        _ => unreachable!(),
    }
}

fn read_string_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("invalid_params: `{key}` 必须是非空字符串。"))
}

// ---- file.pick ----

#[cfg(not(target_os = "android"))]
fn pick_files(app: &tauri::AppHandle, params: &Value) -> Result<Value, String> {
    use tauri_plugin_dialog::DialogExt;

    let multiple = params
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let extensions: Vec<String> = params
        .get("extensions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut dialog = app.dialog().file();
    if !extensions.is_empty() {
        let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter("文件", &refs);
    }
    let paths: Vec<tauri_plugin_dialog::FilePath> = if multiple {
        dialog.blocking_pick_files().unwrap_or_default()
    } else {
        dialog.blocking_pick_file().into_iter().collect()
    };
    if paths.is_empty() {
        return Err("cancelled: 用户取消了选择。".to_string());
    }

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let local = path
            .as_path()
            .ok_or_else(|| "io: 所选文件不是本地路径，无法读取。".to_string())?;
        files.push(read_external_file(local)?);
    }
    Ok(Value::Array(files))
}

#[cfg(target_os = "android")]
fn pick_files(_app: &tauri::AppHandle, _params: &Value) -> Result<Value, String> {
    // 第一批：SAF 选择器需要 activity-result 回调链路，单独立项；明确 unsupported。
    Err("unsupported: 当前平台暂不支持 file.pick（安卓文件选择器待 SAF 链路落地）。".to_string())
}

/// 用户显式选择的文件可以来自任意位置（这本身就是授权行为），读取并限大小。
#[cfg(not(target_os = "android"))]
fn read_external_file(path: &Path) -> Result<Value, String> {
    use base64::Engine;

    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("io: 无法读取所选文件 {}：{e}", path.display()))?;
    if metadata.len() as usize > MAX_FILE_BYTES {
        return Err(format!(
            "io: 文件超过 {} MB 上限。",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("io: 无法读取所选文件 {}：{e}", path.display()))?;
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(serde_json::json!({
        "name": name,
        "size": bytes.len(),
        "data_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
    }))
}

// ---- file.read / file.write / file.share ----

#[cfg(not(target_os = "android"))]
fn read_world_file(
    data_dir: &Path,
    world: &WorldDefinition,
    params: &Value,
) -> Result<Value, String> {
    use base64::Engine;

    let path = resolve_world_file(data_dir, &world.id, read_string_param(params, "path")?)?;
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("io: 读取失败（{}）：{e}", path.display()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "io: 文件超过 {} MB 上限。",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(serde_json::json!({
        "name": name,
        "path": read_string_param(params, "path")?,
        "size": bytes.len(),
        "data_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
    }))
}

#[cfg(target_os = "android")]
fn read_world_file(
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    params: &Value,
) -> Result<Value, String> {
    use tauri::Manager;

    let path = resolve_world_file(data_dir, &world.id, read_string_param(params, "path")?)?;
    let bridge = app.state::<tauri_plugin_native_bridge::NativeBridge<tauri::Wry>>();
    let result = bridge.read_file(&path.to_string_lossy())?;
    Ok(serde_json::json!({
        "name": path.file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_default(),
        "path": read_string_param(params, "path")?,
        "size": result.size,
        "data_base64": result.data_base64,
    }))
}

#[cfg(not(target_os = "android"))]
fn write_world_file(
    data_dir: &Path,
    world: &WorldDefinition,
    params: &Value,
) -> Result<Value, String> {
    let path = resolve_world_file(data_dir, &world.id, read_string_param(params, "path")?)?;
    let bytes = decode_payload(params)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("io: 创建目录失败：{e}"))?;
    }
    std::fs::write(&path, &bytes).map_err(|e| format!("io: 写入失败（{}）：{e}", path.display()))?;
    Ok(serde_json::json!({
        "path": read_string_param(params, "path")?,
        "size": bytes.len(),
    }))
}

#[cfg(target_os = "android")]
fn write_world_file(
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    params: &Value,
) -> Result<Value, String> {
    use base64::Engine;
    use tauri::Manager;

    let path = resolve_world_file(data_dir, &world.id, read_string_param(params, "path")?)?;
    let bytes = decode_payload(params)?;
    let bridge = app.state::<tauri_plugin_native_bridge::NativeBridge<tauri::Wry>>();
    let result = bridge.write_file(
        &path.to_string_lossy(),
        &base64::engine::general_purpose::STANDARD.encode(&bytes),
    )?;
    Ok(serde_json::json!({
        "path": read_string_param(params, "path")?,
        "size": result.size,
    }))
}

/// write 的载荷：data_base64（二进制）或 text（UTF-8 文本），二选一。
fn decode_payload(params: &Value) -> Result<Vec<u8>, String> {
    use base64::Engine;

    if let Some(text) = params.get("text").and_then(Value::as_str) {
        let bytes = text.as_bytes().to_vec();
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!(
                "io: 内容超过 {} MB 上限。",
                MAX_FILE_BYTES / 1024 / 1024
            ));
        }
        return Ok(bytes);
    }
    if let Some(encoded) = params.get("data_base64").and_then(Value::as_str) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| format!("invalid_params: data_base64 不是合法 base64：{e}"))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!(
                "io: 内容超过 {} MB 上限。",
                MAX_FILE_BYTES / 1024 / 1024
            ));
        }
        return Ok(bytes);
    }
    Err("invalid_params: 需要 `text` 或 `data_base64` 之一。".to_string())
}

#[cfg(not(target_os = "android"))]
fn share_world_file() -> Result<Value, String> {
    // 桌面端配套规则：没有对应系统能力，明确 unsupported。
    Err("unsupported: 当前平台不支持 file.share（桌面无系统分享面板）。".to_string())
}

#[cfg(target_os = "android")]
fn share_world_file(
    data_dir: &Path,
    app: &tauri::AppHandle,
    world: &WorldDefinition,
    params: &Value,
) -> Result<Value, String> {
    use tauri::Manager;

    let path = resolve_world_file(data_dir, &world.id, read_string_param(params, "path")?)?;
    let bridge = app.state::<tauri_plugin_native_bridge::NativeBridge<tauri::Wry>>();
    bridge.share_file(&path.to_string_lossy())?;
    Ok(serde_json::json!({ "shared": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open sqlite");
        crate::db::schema::create_tables(&conn).expect("create schema");
        conn
    }

    fn world_with_features(features: &[&str]) -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "测试世界".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: String::new(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({}),
            triggers: Vec::new(),
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
            ui_theme_config: serde_json::json!({
                "platform_features": features
            }),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        }
    }

    #[test]
    fn declared_features_filters_unknown_values() {
        let config = serde_json::json!({
            "platform_features": ["file.read", "file.magic", 42]
        });
        assert_eq!(declared_features(&config), vec!["file.read".to_string()]);
        assert!(validate_declared_features(&config).is_err());
        assert!(validate_declared_features(&serde_json::json!({})).is_ok());
    }

    #[test]
    fn grants_default_deny_and_toggle() {
        let conn = conn();
        assert!(!is_granted(&conn, "world-1", FEATURE_FILE_READ).unwrap());
        set_grant(&conn, "world-1", FEATURE_FILE_READ, true).unwrap();
        assert!(is_granted(&conn, "world-1", FEATURE_FILE_READ).unwrap());
        // 授权按世界隔离
        assert!(!is_granted(&conn, "world-2", FEATURE_FILE_READ).unwrap());
        set_grant(&conn, "world-1", FEATURE_FILE_READ, false).unwrap();
        assert!(!is_granted(&conn, "world-1", FEATURE_FILE_READ).unwrap());
        // 未知能力不能写入授权表
        assert!(set_grant(&conn, "world-1", "file.magic", true).is_err());
    }

    #[test]
    fn world_file_paths_are_confined_to_the_world_directory() {
        let root = Path::new("C:/data");
        let resolved = resolve_world_file(root, "world-1", "notes/todo.txt").unwrap();
        assert!(resolved.starts_with(root.join("world_files").join("world-1")));

        for bad in [
            "../outside.txt",
            "a/../../b.txt",
            "/abs/path.txt",
            "C:/win.txt",
            "a//b.txt",
            "evil?.txt",
            "",
        ] {
            assert!(
                resolve_world_file(root, "world-1", bad).is_err(),
                "path `{bad}` should be rejected"
            );
        }
    }

    #[test]
    fn check_allowed_enforces_declaration_then_grant() {
        let conn = conn();
        let world = world_with_features(&[FEATURE_FILE_READ]);

        // 未声明 → not_declared
        let world_undeclared = world_with_features(&[]);
        assert!(check_allowed(&conn, &world_undeclared, FEATURE_FILE_READ)
            .unwrap_err()
            .starts_with("not_declared:"));

        // 声明了但未授权 → not_granted
        assert!(check_allowed(&conn, &world, FEATURE_FILE_READ)
            .unwrap_err()
            .starts_with("not_granted:"));

        // 授权后放行
        set_grant(&conn, "world-1", FEATURE_FILE_READ, true).unwrap();
        assert!(check_allowed(&conn, &world, FEATURE_FILE_READ).is_ok());

        // 未知能力 → unsupported
        assert!(check_allowed(&conn, &world, "file.magic")
            .unwrap_err()
            .starts_with("unsupported:"));

        // 桌面端配套规则：share 在桌面明确 unsupported（授权之后也是）。
        set_grant(&conn, "world-1", FEATURE_FILE_SHARE, true).unwrap();
        let world_share = world_with_features(&[FEATURE_FILE_SHARE]);
        assert!(check_allowed(&conn, &world_share, FEATURE_FILE_SHARE).is_ok());
        #[cfg(not(target_os = "android"))]
        assert!(share_world_file().unwrap_err().starts_with("unsupported:"));
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn write_then_read_round_trip_inside_world_directory() {
        let dir = std::env::temp_dir().join(format!("pf-test-{}", uuid::Uuid::new_v4()));
        let world = world_with_features(&[FEATURE_FILE_WRITE]);

        let written = write_world_file(
            &dir,
            &world,
            &serde_json::json!({ "path": "notes/todo.txt", "text": "第一行" }),
        )
        .expect("write");
        assert_eq!(written["size"], 9); // "第一行" = 3 个汉字 × 3 字节

        let read = read_world_file(
            &dir,
            &world,
            &serde_json::json!({ "path": "notes/todo.txt" }),
        )
        .expect("read");
        assert_eq!(read["name"], "todo.txt");
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(read["data_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "第一行");

        // 读不存在的文件 → io 错误，不静默成功
        assert!(read_world_file(&dir, &world, &serde_json::json!({ "path": "none.txt" }))
            .unwrap_err()
            .starts_with("io:"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
