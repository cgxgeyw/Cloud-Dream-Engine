use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::repositories::world_repo::WorldRepository;
use crate::models::character::{CharacterDefinition, CharacterPackageData};
use crate::models::oag::{OagCanonData, OagCanonImportPreview, OagOntology};
use crate::models::world::{
    BinaryFileResponse, SavedFileResponse, WorldDefinition, WorldPackageAssetEntry,
    WorldPackageCharacterFileEntry, WorldPackageInspectionResult, WorldPackageManifest,
    WorldPackageWorldData, WorldUiBundleValidationRequest, WorldUiDiagnostic,
};
use crate::services::game_ui::GameUiService;
use crate::state::AppState;

const WORLD_PACKAGE_FORMAT: &str = "dream-world-package";
const WORLD_PACKAGE_VERSION: u32 = 10;
const WORLD_PACKAGE_FILE: &str = "world/world.json";
const WORLD_PACKAGE_DESKTOP_UI_FILE: &str = "world/ui.desktop.html";
const WORLD_PACKAGE_MOBILE_UI_FILE: &str = "world/ui.mobile.html";
const WORLD_PACKAGE_DESKTOP_UI_STYLESHEET: &str = "world/ui.desktop.css";
const WORLD_PACKAGE_MOBILE_UI_STYLESHEET: &str = "world/ui.mobile.css";
const WORLD_PACKAGE_LOGIC_FILE: &str = "world/logic.js";
const WORLD_PACKAGE_MCP_TOOLS_FILE: &str = "tools.json";
const WORLD_PACKAGE_ONTOLOGY_FILE: &str = "world/ontology.json";
const WORLD_PACKAGE_OBJECTS_FILE: &str = "world/objects.json";
const MAX_WORLD_LOGIC_BYTES: usize = 256 * 1024;
const MAX_WORLD_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_WORLD_PACKAGE_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_WORLD_PACKAGE_ENTRIES: usize = 4096;

pub struct ImportedWorldPackage {
    pub world: WorldPackageWorldData,
    pub desktop_ui_source: String,
    pub mobile_ui_source: String,
    pub desktop_ui_stylesheet: String,
    pub mobile_ui_stylesheet: String,
    pub ui_runtime_version: u32,
    pub ui_capabilities: Vec<String>,
    pub characters: Vec<CharacterPackageData>,
    pub ontology: Option<OagOntology>,
    pub canon: OagCanonData,
    pub asset_map: HashMap<String, String>,
    /// v8：包内嵌的 MCP 工具定义（导入时 upsert 到工具表）。
    pub mcp_tools: Vec<crate::models::mcp_tool::McpToolDefinition>,
}

pub struct WorldPackageService;

impl WorldPackageService {
    pub fn inspect_package_archive_with_db(
        conn: &Connection,
        data: Vec<u8>,
    ) -> WorldPackageInspectionResult {
        inspect_package_archive_internal(data, Some(conn))
    }
}

fn inspect_package_archive_internal(
    data: Vec<u8>,
    conn: Option<&Connection>,
) -> WorldPackageInspectionResult {
    let mut diagnostics = Vec::new();
    let mut archive = match zip::ZipArchive::new(Cursor::new(data)) {
        Ok(archive) => archive,
        Err(error) => {
            diagnostics.push(package_diagnostic(
                "invalid_zip",
                format!("无法读取世界包 ZIP：{error}"),
                "archive",
            ));
            return WorldPackageInspectionResult {
                ok: false,
                format: None,
                version: None,
                diagnostics,
                oag_preview: None,
            };
        }
    };
    if let Err(error) = validate_archive_limits(&mut archive) {
        diagnostics.push(package_diagnostic(
            "archive_limits_exceeded",
            error,
            "archive",
        ));
    }
    let manifest = match read_json_from_zip::<WorldPackageManifest>(&mut archive, "manifest.json") {
        Ok(manifest) => manifest,
        Err(error) => {
            diagnostics.push(package_diagnostic(
                "invalid_manifest",
                format!("manifest.json 无效：{error}"),
                "manifest.json",
            ));
            return WorldPackageInspectionResult {
                ok: false,
                format: None,
                version: None,
                diagnostics,
                oag_preview: None,
            };
        }
    };
    if manifest.format != WORLD_PACKAGE_FORMAT {
        diagnostics.push(package_diagnostic(
            "unsupported_package_format",
            format!("不支持的世界包格式：{}", manifest.format),
            "manifest.format",
        ));
    }
    if manifest.version != WORLD_PACKAGE_VERSION {
        diagnostics.push(package_diagnostic(
            "unsupported_package_version",
            format!(
                "不支持的世界包版本：{}，当前版本为 {}。",
                manifest.version, WORLD_PACKAGE_VERSION
            ),
            "manifest.version",
        ));
    }
    for (path, field) in [
        (manifest.world_file.as_deref(), "manifest.world_file"),
        (
            manifest.desktop_ui_file.as_deref(),
            "manifest.desktop_ui_file",
        ),
        (
            manifest.mobile_ui_file.as_deref(),
            "manifest.mobile_ui_file",
        ),
        (manifest.logic_file.as_deref(), "manifest.logic_file"),
        (
            manifest.desktop_ui_stylesheet_file.as_deref(),
            "manifest.desktop_ui_stylesheet_file",
        ),
        (
            manifest.mobile_ui_stylesheet_file.as_deref(),
            "manifest.mobile_ui_stylesheet_file",
        ),
        (
            manifest.mcp_tools_file.as_deref(),
            "manifest.mcp_tools_file",
        ),
        (manifest.ontology_file.as_deref(), "manifest.ontology_file"),
        (manifest.objects_file.as_deref(), "manifest.objects_file"),
    ] {
        if let Some(path) = path {
            if let Err(error) = validate_manifest_path(path, field) {
                diagnostics.push(package_diagnostic("invalid_manifest_path", error, field));
            }
        }
    }
    for entry in &manifest.character_files {
        if let Err(error) =
            validate_manifest_path(&entry.file_path, "manifest.character_files.file_path")
        {
            diagnostics.push(package_diagnostic(
                "invalid_manifest_path",
                error,
                "manifest.character_files",
            ));
        }
    }
    for asset in &manifest.assets {
        if let Err(error) =
            validate_manifest_path(&asset.archive_path, "manifest.assets.archive_path")
        {
            diagnostics.push(package_diagnostic(
                "invalid_manifest_path",
                error,
                "manifest.assets",
            ));
        }
    }
    let desktop_path = manifest
        .desktop_ui_file
        .as_deref()
        .unwrap_or(WORLD_PACKAGE_DESKTOP_UI_FILE);
    let mobile_path = manifest
        .mobile_ui_file
        .as_deref()
        .unwrap_or(WORLD_PACKAGE_MOBILE_UI_FILE);
    let desktop = read_text_from_zip(&mut archive, desktop_path);
    let mobile = read_text_from_zip(&mut archive, mobile_path);
    let desktop_stylesheet = manifest
        .desktop_ui_stylesheet_file
        .as_deref()
        .map(|path| read_text_from_zip(&mut archive, path));
    let mobile_stylesheet = manifest
        .mobile_ui_stylesheet_file
        .as_deref()
        .map(|path| read_text_from_zip(&mut archive, path));
    let service = GameUiService::new();
    for (platform, source) in [("desktop", desktop), ("mobile", mobile)] {
        match source {
            Ok(source) => {
                let result = service.validate_world_ui_document(
                    crate::models::world::WorldUiDocumentRequest {
                        source,
                        platform: Some(platform.to_string()),
                    },
                );
                diagnostics.extend(
                    result
                        .errors
                        .into_iter()
                        .map(|diagnostic| WorldUiDiagnostic {
                            path: diagnostic.path.map(|path| format!("{platform}/{path}")),
                            ..diagnostic
                        }),
                );
            }
            Err(error) => diagnostics.push(package_diagnostic(
                "missing_ui_file",
                format!("{platform} UI 文件无法读取：{error}"),
                if platform == "desktop" {
                    desktop_path
                } else {
                    mobile_path
                },
            )),
        }
    }
    for (platform, path, stylesheet) in [
        (
            "desktop",
            manifest.desktop_ui_stylesheet_file.as_deref(),
            desktop_stylesheet,
        ),
        (
            "mobile",
            manifest.mobile_ui_stylesheet_file.as_deref(),
            mobile_stylesheet,
        ),
    ] {
        if let Some(result) = stylesheet {
            if let Err(error) = result {
                diagnostics.push(package_diagnostic(
                    "missing_stylesheet_file",
                    format!("{platform} stylesheet 文件无法读取：{error}"),
                    path.unwrap_or("stylesheet"),
                ));
            }
        }
    }
    let world_path = manifest.world_file.as_deref().unwrap_or(WORLD_PACKAGE_FILE);
    let package_world = match read_json_from_zip::<WorldPackageWorldData>(&mut archive, world_path)
    {
        Ok(package_world) => {
            if let Err(error) = crate::services::world_storage::validate_storage_config(
                &package_world.ui_storage_config,
            ) {
                diagnostics.push(package_diagnostic(
                    "invalid_world_storage_config",
                    error,
                    "world.storage",
                ));
            }
            if let Err(error) = crate::services::world_storage::validate_logic_config(
                &package_world.ui_logic_config,
            ) {
                diagnostics.push(package_diagnostic(
                    "invalid_world_logic_config",
                    error,
                    "world.logic",
                ));
            }
            if let Some(source) = package_world
                .ui_logic_config
                .get("source")
                .and_then(Value::as_str)
            {
                diagnostics.extend(crate::services::game_ui::validate_world_logic_source(
                    source,
                ));
            }
            Some(package_world)
        }
        Err(_) => {
            diagnostics.push(package_diagnostic(
                "missing_world_file",
                "无法读取 world/world.json。",
                world_path,
            ));
            None
        }
    };
    if let Some(logic_path) = manifest.logic_file.as_deref() {
        match read_text_from_zip(&mut archive, logic_path) {
            Ok(source) => diagnostics.extend(
                crate::services::game_ui::validate_world_logic_source(&source),
            ),
            Err(error) => diagnostics.push(package_diagnostic(
                "missing_logic_file",
                format!("logic.js 无法读取：{error}"),
                logic_path,
            )),
        }
    }
    let oag_data = read_oag_package_data(&mut archive, &manifest);
    if let Err(error) = &oag_data {
        diagnostics.push(package_diagnostic("invalid_oag_data", error.clone(), "oag"));
    }
    let mut oag_preview: Option<OagCanonImportPreview> = None;
    if let (Some(conn), Some(package_world), Ok((ontology, canon))) =
        (conn, package_world.as_ref(), oag_data)
    {
        match preview_package_oag(
            &mut archive,
            &manifest,
            conn,
            package_world,
            ontology,
            canon,
        ) {
            Ok(preview) => oag_preview = Some(preview),
            Err(error) => diagnostics.push(package_diagnostic("invalid_oag_preview", error, "oag")),
        }
    }
    WorldPackageInspectionResult {
        ok: diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != "error"),
        format: Some(manifest.format),
        version: Some(manifest.version),
        diagnostics,
        oag_preview,
    }
}

fn preview_package_oag(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    manifest: &WorldPackageManifest,
    conn: &Connection,
    world: &WorldPackageWorldData,
    ontology: Option<OagOntology>,
    canon: OagCanonData,
) -> Result<OagCanonImportPreview, String> {
    let characters = manifest
        .character_files
        .iter()
        .map(|entry| {
            let mut character: CharacterPackageData =
                read_json_from_zip(archive, &entry.file_path)?;
            if character.source_character_id.trim().is_empty() {
                character.source_character_id = entry.source_character_id.clone();
            }
            Ok(character)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let existing_world_id = WorldRepository::new(conn)
        .list()?
        .into_iter()
        .find(|existing| existing.name.trim() == world.name.trim())
        .map(|existing| existing.id);
    crate::services::oag::canon::preview(
        conn,
        existing_world_id.as_deref(),
        ontology.as_ref(),
        &canon,
        &characters,
        &world.map_nodes,
    )
}

impl WorldPackageService {
    pub fn build_package(
        data_dir: &Path,
        bundled_oag: Option<(OagOntology, OagCanonData)>,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        mcp_tools: &[crate::models::mcp_tool::McpToolDefinition],
    ) -> Result<BinaryFileResponse, String> {
        let assets_root = assets_root(data_dir);
        if let Some(source) = world_logic_source(world) {
            if source.len() > MAX_WORLD_LOGIC_BYTES {
                return Err(format!(
                    "World logic exceeds the {} byte limit",
                    MAX_WORLD_LOGIC_BYTES
                ));
            }
        }
        // v8：把世界白名单（allowed_mcp_tool_ids）引用到的工具定义内嵌进包，
        // 导入方无需单独导入工具包。引擎硬编码内置工具不嵌入。
        let allowed_tool_ids: std::collections::BTreeSet<String> = world
            .director_config
            .get("allowed_mcp_tool_ids")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::trim).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let bundled_tools: Vec<crate::models::mcp_tool::McpToolDefinition> = mcp_tools
            .iter()
            .filter(|tool| allowed_tool_ids.contains(&tool.id))
            .filter(|tool| !crate::models::mcp_tool::is_builtin_mcp_tool_id(&tool.id))
            .cloned()
            .collect();

        if let Some((ontology, _)) = &bundled_oag {
            crate::services::oag::ontology::validate(ontology)?;
        }
        let mut manifest = build_manifest(world, characters, &assets_root)?;
        if bundled_oag.is_some() {
            manifest.ontology_file = Some(WORLD_PACKAGE_ONTOLOGY_FILE.to_string());
            manifest.objects_file = Some(WORLD_PACKAGE_OBJECTS_FILE.to_string());
        }
        if !bundled_tools.is_empty() {
            manifest.mcp_tools_file = Some(WORLD_PACKAGE_MCP_TOOLS_FILE.to_string());
        }
        let mut world_data = to_world_package_data(world, characters);
        for sticker in &mut world_data.stickers {
            if let Some(entry) = manifest
                .assets
                .iter()
                .find(|entry| entry.source_path == sticker.asset_path)
            {
                sticker.asset_path = entry.archive_path.clone();
            }
        }
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
                            // 包里的知识库导入后会换成新 id，角色级收窄无法跨包保留；
                            // 留空表示沿用目标世界主控范围（世界自带库本就自动可用）。
                            knowledge_base_ids: Vec::new(),
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

            if !bundled_tools.is_empty() {
                let tools_payload = serde_json::json!({
                    "format": "dream-mcp-tools",
                    "version": 1,
                    "tools": bundled_tools,
                });
                archive
                    .start_file(WORLD_PACKAGE_MCP_TOOLS_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(
                        serde_json::to_string_pretty(&tools_payload)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
            }

            if let Some((ontology, canon)) = &bundled_oag {
                archive
                    .start_file(WORLD_PACKAGE_ONTOLOGY_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(
                        serde_json::to_string_pretty(ontology)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
                archive
                    .start_file(WORLD_PACKAGE_OBJECTS_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(
                        serde_json::to_string_pretty(canon)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
            }

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

            if let Some(source) = world_ui_entry_value(world, "desktop", "document", "desktop_file")
                .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_DESKTOP_UI_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            if let Some(source) = world_ui_entry_value(world, "mobile", "document", "mobile_file")
                .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_MOBILE_UI_FILE, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            if let Some(source) =
                world_ui_entry_value(world, "desktop", "stylesheet", "desktop_stylesheet")
                    .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_DESKTOP_UI_STYLESHEET, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            if let Some(source) =
                world_ui_entry_value(world, "mobile", "stylesheet", "mobile_stylesheet")
                    .filter(|value| !value.trim().is_empty())
            {
                archive
                    .start_file(WORLD_PACKAGE_MOBILE_UI_STYLESHEET, options)
                    .map_err(|e| e.to_string())?;
                archive
                    .write_all(source.as_bytes())
                    .map_err(|e| e.to_string())?;
            }

            if let Some(source) = world_logic_source(world) {
                archive
                    .start_file(WORLD_PACKAGE_LOGIC_FILE, options)
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
                    .ok_or_else(|| format!("资源路径无效：{}", asset.source_path))?;
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
        validate_archive_limits(&mut archive)?;
        let manifest: WorldPackageManifest = read_json_from_zip(&mut archive, "manifest.json")
            .map_err(|e| format!("Invalid manifest: {e}"))?;
        if manifest.format != WORLD_PACKAGE_FORMAT || manifest.version != WORLD_PACKAGE_VERSION {
            return Err("不支持的世界包格式".to_string());
        }

        validate_manifest_path(
            manifest.world_file.as_deref().unwrap_or(WORLD_PACKAGE_FILE),
            "manifest.world_file",
        )?;
        validate_manifest_path(
            manifest
                .desktop_ui_file
                .as_deref()
                .unwrap_or(WORLD_PACKAGE_DESKTOP_UI_FILE),
            "manifest.desktop_ui_file",
        )?;
        validate_manifest_path(
            manifest
                .mobile_ui_file
                .as_deref()
                .unwrap_or(WORLD_PACKAGE_MOBILE_UI_FILE),
            "manifest.mobile_ui_file",
        )?;
        for (path, field) in [
            (manifest.logic_file.as_deref(), "manifest.logic_file"),
            (
                manifest.mcp_tools_file.as_deref(),
                "manifest.mcp_tools_file",
            ),
            (manifest.ontology_file.as_deref(), "manifest.ontology_file"),
            (manifest.objects_file.as_deref(), "manifest.objects_file"),
            (
                manifest.desktop_ui_stylesheet_file.as_deref(),
                "manifest.desktop_ui_stylesheet_file",
            ),
            (
                manifest.mobile_ui_stylesheet_file.as_deref(),
                "manifest.mobile_ui_stylesheet_file",
            ),
        ] {
            if let Some(path) = path {
                validate_manifest_path(path, field)?;
            }
        }
        for entry in &manifest.character_files {
            validate_manifest_path(&entry.file_path, "manifest.character_files.file_path")?;
        }
        for asset in &manifest.assets {
            validate_manifest_path(&asset.archive_path, "manifest.assets.archive_path")?;
        }

        let (ontology, canon) = read_oag_package_data(&mut archive, &manifest)?;

        // 先完整读取并校验包内容，再写入 assets 目录。导入失败时不应留下半个包的文件。
        let mut pending_assets = Vec::with_capacity(manifest.assets.len());
        for asset in &manifest.assets {
            let mut file = open_zip_entry(&mut archive, &asset.archive_path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            pending_assets.push((asset.source_path.clone(), asset.archive_path.clone(), bytes));
        }

        let world_file = manifest
            .world_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_FILE.to_string());
        let mut package_world: WorldPackageWorldData =
            read_json_from_zip(&mut archive, &world_file)
                .map_err(|e| format!("Invalid world data: {e}"))?;
        if let Some(logic_file) = manifest.logic_file.as_deref() {
            let logic_source = read_text_from_zip(&mut archive, logic_file)
                .map_err(|e| format!("Invalid world logic: {e}"))?;
            if logic_source.len() > MAX_WORLD_LOGIC_BYTES {
                return Err(format!(
                    "World logic exceeds the {} byte limit",
                    MAX_WORLD_LOGIC_BYTES
                ));
            }
            if !package_world.ui_logic_config.is_object() {
                package_world.ui_logic_config = serde_json::json!({});
            }
            let logic = package_world.ui_logic_config.as_object_mut().unwrap();
            logic.insert(
                "source".to_string(),
                serde_json::Value::String(logic_source),
            );
        }
        crate::services::world_storage::validate_storage_config(&package_world.ui_storage_config)?;
        crate::services::world_storage::validate_logic_config(&package_world.ui_logic_config)?;
        if let Some(logic_source) = package_world
            .ui_logic_config
            .get("source")
            .and_then(|value| value.as_str())
        {
            let diagnostics = crate::services::game_ui::validate_world_logic_source(logic_source);
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == "error")
                .map(|diagnostic| {
                    format!(
                        "{} at {}: {}",
                        diagnostic.code,
                        diagnostic.path.as_deref().unwrap_or("logic.js"),
                        diagnostic.message
                    )
                })
                .collect::<Vec<_>>();
            if !errors.is_empty() {
                return Err(format!("世界逻辑脚本无效：{}", errors.join("; ")));
            }
        }
        // 第 12 项：manifest 声明的平台能力必须都在当前目录内，未知 action 导入即拦。
        crate::services::platform_features::validate_declared_features(&serde_json::json!({
            "platform_features": package_world.platform_features,
        }))?;
        let desktop_ui_file = manifest
            .desktop_ui_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_DESKTOP_UI_FILE.to_string());
        let desktop_ui_source = read_text_from_zip(&mut archive, &desktop_ui_file)
            .map_err(|_| format!("世界包缺少桌面 UI 文件：{desktop_ui_file}"))?;
        let mobile_ui_file = manifest
            .mobile_ui_file
            .clone()
            .unwrap_or_else(|| WORLD_PACKAGE_MOBILE_UI_FILE.to_string());
        let mobile_ui_source = read_text_from_zip(&mut archive, &mobile_ui_file)
            .map_err(|_| format!("世界包缺少移动 UI 文件：{mobile_ui_file}"))?;
        let desktop_ui_stylesheet = match manifest.desktop_ui_stylesheet_file.as_deref() {
            Some(path) => read_text_from_zip(&mut archive, path)
                .map_err(|_| format!("世界包缺少桌面 stylesheet 文件：{path}"))?,
            None => String::new(),
        };
        let mobile_ui_stylesheet = match manifest.mobile_ui_stylesheet_file.as_deref() {
            Some(path) => read_text_from_zip(&mut archive, path)
                .map_err(|_| format!("世界包缺少移动 stylesheet 文件：{path}"))?,
            None => String::new(),
        };
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
            return Err("世界包缺少角色文件".to_string());
        }
        crate::services::oag::canon::validate(
            ontology.as_ref(),
            &canon,
            &package_characters,
            &package_world.map_nodes,
        )?;

        validate_world_package_director_config(&package_world.director_config)?;
        validate_world_package_stickers(&package_world.stickers, &manifest.assets)?;
        validate_declared_asset_references(
            &package_world.ui_assets_config,
            &package_characters,
            &manifest.assets,
        )?;
        validate_logic_reachability(
            &package_world.ui_logic_config,
            &desktop_ui_source,
            &mobile_ui_source,
        )?;

        let ui_validation =
            GameUiService::new().validate_world_ui_bundle(WorldUiBundleValidationRequest {
                desktop_file: desktop_ui_source.clone(),
                mobile_file: mobile_ui_source.clone(),
                runtime_version: manifest.ui_runtime_version,
                desktop_stylesheet: desktop_ui_stylesheet.clone(),
                mobile_stylesheet: mobile_ui_stylesheet.clone(),
                capabilities: package_world.ui_capabilities.clone(),
                storage: package_world.ui_storage_config.clone(),
                logic: package_world.ui_logic_config.clone(),
            });
        if !ui_validation.ok {
            let details = ui_validation
                .desktop
                .errors
                .iter()
                .chain(ui_validation.mobile.errors.iter())
                .chain(ui_validation.errors.iter())
                .map(|diagnostic| {
                    let path = diagnostic.path.as_deref().unwrap_or("bundle");
                    format!("{} at {}: {}", diagnostic.code, path, diagnostic.message)
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!("世界 UI 包无效：{details}"));
        }

        let mut asset_map = HashMap::new();
        for (source_path, archive_path, bytes) in pending_assets {
            let target_asset = persist_asset_file(&assets_root, &archive_path, &bytes)?;
            asset_map.insert(source_path, target_asset.clone());
            asset_map.insert(archive_path, target_asset);
        }

        let ui_capabilities = package_world.ui_capabilities.clone();
        let package_mcp_tools = match manifest.mcp_tools_file.as_deref() {
            Some(tools_file) => read_package_mcp_tools(&mut archive, tools_file)?,
            None => Vec::new(),
        };
        Ok(ImportedWorldPackage {
            world: package_world,
            desktop_ui_source,
            mobile_ui_source,
            desktop_ui_stylesheet,
            mobile_ui_stylesheet,
            ui_runtime_version: manifest.ui_runtime_version.unwrap_or(2),
            ui_capabilities,
            characters: package_characters,
            ontology,
            canon,
            asset_map,
            mcp_tools: package_mcp_tools,
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

fn validate_world_package_director_config(config: &serde_json::Value) -> Result<(), String> {
    let Some(object) = config.as_object() else {
        return Err("director_config must be an object".to_string());
    };

    if object.contains_key("message_interaction_kinds") {
        return Err(
            "director_config.message_interaction_kinds has been removed; use director_interaction_kinds for world-director actions"
                .to_string(),
        );
    }

    if let Some(kinds) = object.get("director_interaction_kinds") {
        let kinds = kinds.as_array().ok_or_else(|| {
            "director_config.director_interaction_kinds must be an array".to_string()
        })?;
        for (index, kind) in kinds.iter().enumerate() {
            let kind = kind.as_str().ok_or_else(|| {
                format!("director_config.director_interaction_kinds[{index}] must be a string")
            })?;
            if !matches!(
                kind,
                "choice" | "multi_choice" | "form" | "confirm" | "slider"
            ) {
                return Err(format!(
                    "Unsupported director interaction kind `{kind}` at director_config.director_interaction_kinds[{index}]"
                ));
            }
        }
    }

    Ok(())
}

fn validate_world_package_stickers(
    stickers: &[crate::models::world::WorldPackageSticker],
    manifest_assets: &[WorldPackageAssetEntry],
) -> Result<(), String> {
    if stickers.is_empty() {
        return Ok(());
    }
    let declared = manifest_assets
        .iter()
        .flat_map(|asset| [asset.source_path.as_str(), asset.archive_path.as_str()])
        .collect::<HashSet<_>>();
    let mut keys = HashSet::new();
    for (index, sticker) in stickers.iter().enumerate() {
        let key = sticker.key.trim();
        if key.is_empty() {
            return Err(format!("stickers[{index}].key 不能为空"));
        }
        if !keys.insert(key.to_string()) {
            return Err(format!("stickers[{index}].key 重复：{key}"));
        }
        let asset_path = sticker.asset_path.trim();
        if !asset_path.starts_with("assets/") {
            return Err(format!(
                "stickers[{index}].asset_path 必须以 assets/ 开头：{asset_path}"
            ));
        }
        if !declared.contains(asset_path) {
            return Err(format!("贴纸资源未在 manifest.assets 声明：{asset_path}"));
        }
    }
    Ok(())
}

fn validate_declared_asset_references(
    ui_assets_config: &serde_json::Value,
    characters: &[CharacterPackageData],
    manifest_assets: &[WorldPackageAssetEntry],
) -> Result<(), String> {
    let declared = manifest_assets
        .iter()
        .flat_map(|asset| [asset.source_path.as_str(), asset.archive_path.as_str()])
        .collect::<HashSet<_>>();
    let mut referenced = HashSet::new();
    collect_archive_asset_references(ui_assets_config, &mut referenced);
    for character in characters {
        for path in &character.portrait_assets {
            if path.trim().starts_with("assets/") {
                referenced.insert(path.trim().to_string());
            }
        }
        if character.avatar_asset.trim().starts_with("assets/") {
            referenced.insert(character.avatar_asset.trim().to_string());
        }
    }
    let mut missing = referenced
        .into_iter()
        .filter(|path| !declared.contains(path.as_str()))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort();
    Err(format!(
        "World package references assets that are not declared in manifest.assets: {}",
        missing.join(", ")
    ))
}

fn collect_archive_asset_references(value: &serde_json::Value, output: &mut HashSet<String>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_archive_asset_references(item, output);
            }
        }
        serde_json::Value::Object(object) => {
            for item in object.values() {
                collect_archive_asset_references(item, output);
            }
        }
        serde_json::Value::String(text) if text.trim().starts_with("assets/") => {
            output.insert(text.trim().to_string());
        }
        _ => {}
    }
}

fn validate_logic_reachability(
    logic: &serde_json::Value,
    desktop_ui_source: &str,
    mobile_ui_source: &str,
) -> Result<(), String> {
    if logic.get("runtime").and_then(serde_json::Value::as_str) != Some("sandbox-js-v1") {
        return Ok(());
    }
    let source = logic
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let has_events = logic
        .get("events")
        .and_then(serde_json::Value::as_object)
        .map(|events| !events.is_empty())
        .unwrap_or(false);
    let has_ui_action =
        desktop_ui_source.contains("logic.run") || mobile_ui_source.contains("logic.run");
    if !source.trim().is_empty() && !has_events && !has_ui_action {
        return Err(
            "World logic has no reachable entry point; bind logic.events or invoke logic.run from a UI action"
                .to_string(),
        );
    }
    if let Some(events) = logic.get("events").and_then(serde_json::Value::as_object) {
        for (event, handler) in events {
            let handler = handler.as_str().unwrap_or_default();
            let double_quoted = format!("\"{handler}\"");
            let single_quoted = format!("'{handler}'");
            if !source.contains(&double_quoted) && !source.contains(&single_quoted) {
                return Err(format!(
                    "logic.events.{event} references handler `{handler}`, but logic.js does not register that name"
                ));
            }
        }
    }
    Ok(())
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

    // L10: 序号用尽时不要回退到已存在的 candidate(会静默覆盖);改用唯一 uuid 后缀,
    // 保证返回的是一个尚不存在的路径。
    let unique_suffix = uuid::Uuid::new_v4().simple().to_string();
    let fallback_name = if ext.is_empty() {
        format!("{stem}-{unique_suffix}")
    } else {
        format!("{stem}-{unique_suffix}.{ext}")
    };
    dir.join(fallback_name)
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
    if let Some(stickers) = world
        .ui_theme_config
        .get("stickers")
        .and_then(|value| value.as_array())
    {
        for sticker in stickers {
            if let Some(path) = sticker.get("asset_path").and_then(|value| value.as_str()) {
                push_asset(&mut assets, path, "world_sticker", "global".to_string());
            }
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
        ui_runtime_version: Some(
            world
                .ui_theme_config
                .get("runtime_version")
                .and_then(|value| value.as_u64())
                .unwrap_or(3) as u32,
        ),
        desktop_ui_stylesheet_file: Some(WORLD_PACKAGE_DESKTOP_UI_STYLESHEET.to_string()),
        mobile_ui_stylesheet_file: Some(WORLD_PACKAGE_MOBILE_UI_STYLESHEET.to_string()),
        logic_file: world_logic_source(world).map(|_| WORLD_PACKAGE_LOGIC_FILE.to_string()),
        mcp_tools_file: None,
        ontology_file: None,
        objects_file: None,
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
        knowledge_bases: Some(
            serde_json::from_value(
                world
                    .ui_theme_config
                    .get("knowledge_bases")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
            )
            .unwrap_or_default(),
        ),
        name: world.name.clone(),
        genre: world.genre.clone(),
        background_prompt: world.background_prompt.clone(),
        opening_scene: world.opening_scene.clone(),
        summary: world.summary.clone(),
        time_system: world.time_system.clone(),
        map_nodes: world.map_nodes.clone(),
        time_config: world.time_config.clone(),
        director_config: world.director_config.clone(),
        ui_assets_config: world
            .ui_theme_config
            .get("assets")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        stickers: serde_json::from_value(
            world
                .ui_theme_config
                .get("stickers")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        )
        .unwrap_or_default(),
        attribute_schemas: world
            .ui_theme_config
            .get("attribute_schemas")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        character_attribute_values: world
            .ui_theme_config
            .get("character_attribute_values")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        ui_runtime_version: Some(
            world
                .ui_theme_config
                .get("runtime_version")
                .and_then(|value| value.as_u64())
                .unwrap_or(2) as u32,
        ),
        ui_capabilities: world
            .ui_theme_config
            .get("capabilities")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        platform_features: crate::services::platform_features::declared_features(
            &world.ui_theme_config,
        ),
        ui_storage_config: world
            .ui_theme_config
            .get("storage")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        ui_logic_config: world_logic_package_config(world),
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

fn world_logic_source(world: &WorldDefinition) -> Option<&str> {
    world
        .ui_theme_config
        .get("logic")
        .and_then(|value| value.get("source"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

fn world_logic_package_config(world: &WorldDefinition) -> serde_json::Value {
    let mut logic = world
        .ui_theme_config
        .get("logic")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    logic.remove("source");
    serde_json::Value::Object(logic)
}

fn world_ui_entry_value<'a>(
    world: &'a WorldDefinition,
    platform: &str,
    field: &str,
    legacy_field: &str,
) -> Option<&'a str> {
    world
        .ui_theme_config
        .get("entries")
        .and_then(|entries| entries.get(platform))
        .and_then(|entry| entry.get(field))
        .and_then(|value| value.as_str())
        .or_else(|| {
            world
                .ui_theme_config
                .get(legacy_field)
                .and_then(|value| value.as_str())
        })
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

fn validate_archive_limits(archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>) -> Result<(), String> {
    if archive.len() > MAX_WORLD_PACKAGE_ENTRIES {
        return Err(format!(
            "世界包文件数量超过上限 {}",
            MAX_WORLD_PACKAGE_ENTRIES
        ));
    }
    let mut total = 0u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = file.name().replace('\\', "/");
        validate_manifest_path(&name, "archive entry")?;
        if !names.insert(name.clone()) {
            return Err(format!("世界包包含重复文件：{name}"));
        }
        if file.size() > MAX_WORLD_PACKAGE_ENTRY_BYTES {
            return Err(format!("世界包文件超过单文件大小上限：{name}"));
        }
        total = total.saturating_add(file.size());
        if total > MAX_WORLD_PACKAGE_BYTES {
            return Err("世界包解压后总大小超过 64 MiB 上限".to_string());
        }
    }
    Ok(())
}

fn package_diagnostic(code: &str, message: impl Into<String>, path: &str) -> WorldUiDiagnostic {
    WorldUiDiagnostic {
        severity: "error".to_string(),
        code: code.to_string(),
        message: message.into(),
        path: Some(path.to_string()),
    }
}

fn validate_manifest_path(path: &str, field: &str) -> Result<(), String> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("{field} 路径无效：{path}"));
    }
    Ok(())
}

fn read_oag_package_data(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    manifest: &WorldPackageManifest,
) -> Result<(Option<OagOntology>, OagCanonData), String> {
    let ontology = match manifest.ontology_file.as_deref() {
        Some(path) => {
            validate_manifest_path(path, "manifest.ontology_file")?;
            let ontology: OagOntology = read_json_from_zip(archive, path)
                .map_err(|error| format!("Invalid ontology file: {error}"))?;
            crate::services::oag::ontology::resolve(Some(&ontology))?;
            Some(ontology)
        }
        None => None,
    };
    let canon = match manifest.objects_file.as_deref() {
        Some(path) => {
            validate_manifest_path(path, "manifest.objects_file")?;
            read_json_from_zip(archive, path)
                .map_err(|error| format!("Invalid objects file: {error}"))?
        }
        None => OagCanonData::default(),
    };
    Ok((ontology, canon))
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

/// 读取包内嵌的 MCP 工具定义（dream-mcp-tools 格式，与独立工具包一致）。
/// 引擎硬编码内置工具一律跳过，避免覆盖宿主行为。
fn read_package_mcp_tools(
    archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    path: &str,
) -> Result<Vec<crate::models::mcp_tool::McpToolDefinition>, String> {
    let payload: serde_json::Value = read_json_from_zip(archive, path)
        .map_err(|e| format!("Invalid bundled MCP tools file: {e}"))?;
    if payload.get("format").and_then(|value| value.as_str()) != Some("dream-mcp-tools") {
        return Err("内嵌工具文件格式无效（format 应为 dream-mcp-tools）".to_string());
    }
    let tools = payload
        .get("tools")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "内嵌工具文件缺少 tools 数组".to_string())?;
    let mut parsed = Vec::new();
    for (index, item) in tools.iter().enumerate() {
        let tool: crate::models::mcp_tool::McpToolDefinition = serde_json::from_value(item.clone())
            .map_err(|e| format!("内嵌工具第 {} 项解析失败：{e}", index + 1))?;
        if tool.id.trim().is_empty() || tool.name.trim().is_empty() {
            return Err(format!("内嵌工具第 {} 项缺少 id 或 name", index + 1));
        }
        if crate::models::mcp_tool::is_builtin_mcp_tool_id(&tool.id) {
            continue;
        }
        parsed.push(tool);
    }
    Ok(parsed)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sticker_world() -> WorldDefinition {
        serde_json::from_value(json!({
            "id": "sticker-test",
            "name": "贴纸测试",
            "genre": "测试",
            "background_prompt": "",
            "opening_scene": "",
            "summary": "",
            "time_system": "",
            "map_nodes": {},
            "time_config": {},
            "director_config": {},
            "ui_theme_config": {
                "stickers": [
                    {
                        "key": "feast",
                        "label": "开饭",
                        "description": "端碗开吃",
                        "asset_path": "stickers/feast.webp"
                    }
                ]
            },
            "director_system_prompt_base": "",
            "director_runtime_system_prompt": "",
            "opening_messages": [],
            "opening_character_ids": [],
            "player_character_id": null
        }))
        .expect("world json")
    }

    #[test]
    fn exported_package_rewrites_sticker_assets_to_archive_paths() {
        let data_dir = std::env::temp_dir().join(format!(
            "dream-sticker-export-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let assets_dir = data_dir.join("assets").join("stickers");
        fs::create_dir_all(&assets_dir).expect("assets dir");
        fs::write(assets_dir.join("feast.webp"), b"sticker-bytes").expect("sticker file");

        let world = sticker_world();
        let package = WorldPackageService::build_package(&data_dir, None, &world, &[], &[])
            .expect("build package");

        let mut archive = zip::ZipArchive::new(Cursor::new(package.bytes)).expect("zip");
        let manifest: WorldPackageManifest =
            read_json_from_zip(&mut archive, "manifest.json").expect("manifest");
        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(manifest.assets[0].source_path, "stickers/feast.webp");
        assert_eq!(
            manifest.assets[0].archive_path,
            "assets/stickers/feast.webp"
        );

        let world_data: WorldPackageWorldData =
            read_json_from_zip(&mut archive, WORLD_PACKAGE_FILE).expect("world");
        assert_eq!(world_data.stickers.len(), 1);
        assert_eq!(
            world_data.stickers[0].asset_path,
            "assets/stickers/feast.webp"
        );
        validate_world_package_stickers(&world_data.stickers, &manifest.assets)
            .expect("sticker validation");

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn oag_files_are_exported_and_parsed_with_the_package() {
        let data_dir = std::env::temp_dir().join(format!(
            "dream-oag-export-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ontology: OagOntology = serde_json::from_value(json!({
            "object_types": [
                { "key": "character", "label": "Character", "actor": true },
                { "key": "location", "label": "Location", "platform_role": "location" }
            ],
            "link_types": [],
            "character_object_type": "character",
            "location_object_type": "location"
        }))
        .expect("ontology");
        let canon = OagCanonData {
            objects: vec![crate::models::oag::OagCanonObject {
                object_type: "location".to_string(),
                canonical_key: "location:town".to_string(),
                name: "Town".to_string(),
                aliases: Vec::new(),
                attributes: serde_json::Map::new(),
            }],
            links: Vec::new(),
        };
        let mut world = sticker_world();
        world.ui_theme_config["stickers"] = json!([]);
        let package = WorldPackageService::build_package(
            &data_dir,
            Some((ontology.clone(), canon.clone())),
            &world,
            &[],
            &[],
        )
        .expect("package");
        let package_bytes = package.bytes.clone();
        let mut archive = zip::ZipArchive::new(Cursor::new(package.bytes)).expect("zip");
        let manifest: WorldPackageManifest =
            read_json_from_zip(&mut archive, "manifest.json").expect("manifest");
        assert_eq!(
            manifest.ontology_file.as_deref(),
            Some(WORLD_PACKAGE_ONTOLOGY_FILE)
        );
        assert_eq!(
            manifest.objects_file.as_deref(),
            Some(WORLD_PACKAGE_OBJECTS_FILE)
        );
        let (parsed_ontology, parsed_canon) =
            read_oag_package_data(&mut archive, &manifest).expect("oag data");
        assert_eq!(parsed_ontology.expect("ontology").object_types.len(), 2);
        assert_eq!(parsed_canon.objects.len(), 1);
        assert_eq!(parsed_canon.objects[0].canonical_key, "location:town");

        let conn = Connection::open_in_memory().expect("database");
        crate::db::schema::create_tables(&conn).expect("schema");
        crate::db::oag_schema::create(&conn).expect("oag schema");
        let inspection =
            WorldPackageService::inspect_package_archive_with_db(&conn, package_bytes.clone());
        let preview = inspection.oag_preview.expect("new world preview");
        assert!(!preview.is_upgrade);
        assert_eq!(preview.added_count, 4);

        conn.execute(
            "INSERT INTO worlds (id, name) VALUES ('sticker-test', '贴纸测试')",
            [],
        )
        .expect("world");
        crate::services::oag::canon::import(
            &conn,
            "sticker-test",
            Some(&ontology),
            &canon,
            &[],
            &HashMap::new(),
            &world.map_nodes,
        )
        .expect("canon import");
        let upgrade = WorldPackageService::inspect_package_archive_with_db(&conn, package_bytes);
        let preview = upgrade.oag_preview.expect("upgrade preview");
        assert!(preview.is_upgrade);
        assert_eq!(preview.modified_count, 0);
        assert_eq!(preview.retired_count, 0);
        assert_eq!(preview.unchanged_count, 4);
    }

    #[test]
    fn sticker_validation_rejects_undeclared_or_unprefixed_assets() {
        let assets = vec![WorldPackageAssetEntry {
            source_path: "stickers/feast.webp".to_string(),
            archive_path: "assets/stickers/feast.webp".to_string(),
            owner_type: Some("world_sticker".to_string()),
            owner_id: Some("global".to_string()),
        }];
        let mut sticker: crate::models::world::WorldPackageSticker = serde_json::from_value(
            json!({ "key": "feast", "asset_path": "assets/stickers/feast.webp" }),
        )
        .expect("sticker");
        validate_world_package_stickers(std::slice::from_ref(&sticker), &assets).expect("valid");

        sticker.asset_path = "stickers/feast.webp".to_string();
        assert!(validate_world_package_stickers(std::slice::from_ref(&sticker), &assets).is_err());

        sticker.asset_path = "assets/stickers/missing.webp".to_string();
        assert!(validate_world_package_stickers(std::slice::from_ref(&sticker), &assets).is_err());
    }
}
