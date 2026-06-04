import io
import json
import re
import shutil
import uuid
import zipfile
from pathlib import Path, PurePosixPath
from urllib.parse import quote

from fastapi import APIRouter, Depends, File, HTTPException, UploadFile
from fastapi.responses import StreamingResponse

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.characters import (
    CharacterCreateFromTemplateRequest,
    CharacterDeriveRequest,
    CharacterResponse,
    CharacterTemplateImportRequest,
    CharacterTemplateResponse,
    CharacterUpsertRequest,
)
from backend.app.api.schemas.worlds import (
    PromptTracePreviewResponse,
    WorldOpeningMessagePayload,
    WorldPackageCharacterData,
    WorldPackageCharactersData,
    WorldPackageCharacterFileEntry,
    WorldPackageManifest,
    WorldOpeningPromptPreviewResponse,
    WorldPackageWorldData,
    WorldResponse,
    WorldTemplateResponse,
    WorldUpsertRequest,
)
from backend.app.core.container import AppContainer
from backend.app.core.config import Settings
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.world import WorldDefinition, normalize_world_director_config

router = APIRouter(prefix="/api/worlds", tags=["worlds"])
PACKAGE_FORMAT = "dream-world-package"
PACKAGE_VERSION = 3
SUPPORTED_PACKAGE_VERSIONS = {1, 2, 3}
MAX_WORLD_PACKAGE_SIZE = 200 * 1024 * 1024
PACKAGE_WORLD_FILE = "world/world.json"


def _to_world_response(container: AppContainer, world: WorldDefinition) -> WorldResponse:
    director_config = normalize_world_director_config(world.director_config)
    return WorldResponse.from_domain(
        world,
        director_system_prompt_base=container.world_director.build_runtime_system_prompt_base(),
        director_runtime_system_prompt=container.world_director.build_runtime_system_prompt(
            director_config=director_config
        ),
    )


def _to_character_response(container: AppContainer, character: CharacterDefinition) -> CharacterResponse:
    return CharacterResponse.from_domain(
        character,
        runtime_system_prompt=container.dialogue_pipeline.build_character_system_prompt(
            speaker=character.name,
            speaker_profile=character,
        ),
    )


def _assets_root() -> Path:
    settings = Settings()
    assets_root = Path(settings.database_path).resolve().parent / "assets"
    assets_root.mkdir(parents=True, exist_ok=True)
    return assets_root


def _normalize_archive_relative_path(asset_path: str) -> str | None:
    value = asset_path.strip()
    if not value or value.startswith("http://") or value.startswith("https://"):
        return None
    if value.startswith("/assets/"):
        relative = value[len("/assets/") :]
    elif value.startswith("assets/"):
        relative = value[len("assets/") :]
    else:
        relative = value.lstrip("/\\")
    relative = relative.replace("\\", "/")
    normalized = PurePosixPath(relative)
    if normalized.is_absolute() or not normalized.parts or any(part in {"", ".", ".."} for part in normalized.parts):
        return None
    return normalized.as_posix()


def _canonical_asset_reference(asset_path: str) -> str | None:
    relative_path = _normalize_archive_relative_path(asset_path)
    if relative_path is None:
        return None
    return f"/assets/{relative_path}"


def _format_missing_assets_detail(prefix: str, asset_paths: list[str]) -> str:
    preview = ", ".join(asset_paths[:5])
    suffix = "" if len(asset_paths) <= 5 else f" 等 {len(asset_paths)} 项"
    return f"{prefix}：{preview}{suffix}"


def _slug_package_segment(value: str, *, fallback: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9\u4e00-\u9fff_-]+", "-", value).strip("-")
    return slug or fallback


def _character_package_directory_name(source_character_id: str | None, *, fallback_name: str) -> str:
    candidate = (source_character_id or "").strip().replace("\\", "/")
    if candidate:
        normalized = PurePosixPath(candidate)
        if (
            not normalized.is_absolute()
            and len(normalized.parts) == 1
            and normalized.parts[0] not in {"", ".", ".."}
        ):
            return normalized.parts[0]
    return _slug_package_segment(fallback_name, fallback="character")


def _build_unique_archive_path(directory: str, source_path: str, used_paths: set[str]) -> str:
    relative_path = _normalize_archive_relative_path(source_path)
    base_name = Path(relative_path or "").name or "asset"
    stem = Path(base_name).stem or "asset"
    suffix = Path(base_name).suffix
    candidate = f"{directory}/{base_name}"
    index = 2
    while candidate in used_paths:
        candidate = f"{directory}/{stem}-{index}{suffix}"
        index += 1
    used_paths.add(candidate)
    return candidate


def _collect_world_template_asset_paths(template: WorldTemplateResponse) -> list[str]:
    ui_theme = template.ui_theme_config or {}
    collected: list[str] = []

    def push(asset_path: object) -> None:
        if not isinstance(asset_path, str):
            return
        value = asset_path.strip()
        if value and value not in collected:
            collected.append(value)

    for asset_path in ui_theme.get("local_background_assets", []):
        push(asset_path)

    local_scene_backgrounds = ui_theme.get("local_scene_backgrounds", {})
    if isinstance(local_scene_backgrounds, dict):
        for items in local_scene_backgrounds.values():
            if isinstance(items, list):
                for asset_path in items:
                    push(asset_path)

    for character in template.characters:
        for asset_path in character.portrait_assets:
            push(asset_path)

    return collected


def _package_download_name(world_name: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9\u4e00-\u9fff_-]+", "-", world_name).strip("-")
    return slug or f"world-package-{uuid.uuid4().hex[:8]}"


def _build_world_package_export_data(
    world: WorldDefinition,
    characters: list[CharacterDefinition],
) -> tuple[WorldPackageManifest, WorldPackageWorldData, list[tuple[WorldPackageCharacterFileEntry, WorldPackageCharacterData]]]:
    world_data = WorldPackageWorldData.from_domain(world, characters)
    character_data_items = sorted(
        (WorldPackageCharacterData.from_domain(character) for character in characters),
        key=lambda item: ((item.source_character_id or "").strip(), item.name),
    )

    assets_root = _assets_root()
    assets: list[dict[str, str | None]] = []
    missing_assets: list[str] = []
    used_archive_paths: set[str] = set()

    def register_asset(
        source_path: str,
        *,
        archive_directory: str,
        owner_type: str,
        owner_id: str | None,
    ) -> str:
        canonical_source_path = _canonical_asset_reference(source_path)
        if canonical_source_path is None:
            return source_path

        relative_path = _normalize_archive_relative_path(source_path)
        if relative_path is None:
            return source_path

        file_path = assets_root / Path(relative_path)
        if not file_path.is_file():
            missing_assets.append(canonical_source_path)
            return source_path

        archive_path = _build_unique_archive_path(archive_directory, source_path, used_archive_paths)
        assets.append(
            {
                "source_path": canonical_source_path,
                "archive_path": archive_path,
                "owner_type": owner_type,
                "owner_id": owner_id,
            }
        )
        return archive_path

    world_ui_theme = dict(world_data.ui_theme_config)
    local_background_assets = world_ui_theme.get("local_background_assets", [])
    if isinstance(local_background_assets, list):
        world_ui_theme["local_background_assets"] = [
            register_asset(
                str(asset_path),
                archive_directory="world/backgrounds/global",
                owner_type="world_background",
                owner_id="global",
            )
            for asset_path in local_background_assets
            if str(asset_path).strip()
        ]

    local_scene_backgrounds = world_ui_theme.get("local_scene_backgrounds", {})
    if isinstance(local_scene_backgrounds, dict):
        world_ui_theme["local_scene_backgrounds"] = {
            str(scene_name): [
                register_asset(
                    str(asset_path),
                    archive_directory=f"world/backgrounds/scenes/{_slug_package_segment(str(scene_name), fallback='scene')}",
                    owner_type="scene_background",
                    owner_id=str(scene_name),
                )
                for asset_path in items
                if str(asset_path).strip()
            ]
            for scene_name, items in local_scene_backgrounds.items()
            if isinstance(items, list)
        }
    world_data.ui_theme_config = world_ui_theme

    character_files: list[tuple[WorldPackageCharacterFileEntry, WorldPackageCharacterData]] = []
    for character_data in character_data_items:
        character_key = _character_package_directory_name(
            character_data.source_character_id,
            fallback_name=character_data.name,
        )
        character_directory = f"characters/{character_key}"
        character_data.portrait_assets = [
            register_asset(
                asset_path,
                archive_directory=f"{character_directory}/portraits",
                owner_type="character_portrait",
                owner_id=character_data.source_character_id,
            )
            for asset_path in character_data.portrait_assets
            if asset_path.strip()
        ]
        character_files.append(
            (
                WorldPackageCharacterFileEntry(
                    source_character_id=character_data.source_character_id or character_key,
                    character_name=character_data.name,
                    file_path=f"{character_directory}/character.json",
                ),
                character_data,
            )
        )

    if missing_assets:
        raise HTTPException(
            status_code=400,
            detail=_format_missing_assets_detail("世界包缺少本地资源文件，无法保证跨电脑导入可用", missing_assets),
        )

    manifest = WorldPackageManifest(
        format=PACKAGE_FORMAT,
        version=PACKAGE_VERSION,
        world_file=PACKAGE_WORLD_FILE,
        character_files=[entry for entry, _ in character_files],
        assets=[entry for entry in assets],
    )
    return manifest, world_data, character_files


def _read_world_package_manifest(archive: zipfile.ZipFile) -> WorldPackageManifest:
    try:
        raw_manifest = archive.read("manifest.json").decode("utf-8")
    except KeyError as exc:
        raise HTTPException(status_code=400, detail="世界包缺少 manifest.json") from exc
    except UnicodeDecodeError as exc:
        raise HTTPException(status_code=400, detail="世界包 manifest 编码无效") from exc

    try:
        payload = json.loads(raw_manifest)
    except json.JSONDecodeError as exc:
        raise HTTPException(status_code=400, detail="世界包 manifest 不是合法 JSON") from exc

    try:
        manifest = WorldPackageManifest(**payload)
    except Exception as exc:
        raise HTTPException(status_code=400, detail=f"世界包 manifest 结构无效：{exc}") from exc

    if manifest.format != PACKAGE_FORMAT or manifest.version not in SUPPORTED_PACKAGE_VERSIONS:
        raise HTTPException(status_code=400, detail="不支持的世界包格式或版本")
    return manifest


def _normalize_package_document_path(raw_path: str, *, label: str) -> str:
    value = raw_path.strip().replace("\\", "/")
    normalized = PurePosixPath(value)
    if normalized.is_absolute() or not normalized.parts or any(part in {"", ".", ".."} for part in normalized.parts):
        raise HTTPException(status_code=400, detail=f"世界包 {label} 路径无效：{raw_path}")
    return normalized.as_posix()


def _read_package_json_document(archive: zipfile.ZipFile, document_path: str, *, label: str) -> object:
    normalized_path = _normalize_package_document_path(document_path, label=label)
    try:
        raw_document = archive.read(normalized_path).decode("utf-8")
    except KeyError as exc:
        raise HTTPException(status_code=400, detail=f"世界包缺少 {label} 文件：{normalized_path}") from exc
    except UnicodeDecodeError as exc:
        raise HTTPException(status_code=400, detail=f"世界包 {label} 文件编码无效") from exc

    try:
        return json.loads(raw_document)
    except json.JSONDecodeError as exc:
        raise HTTPException(status_code=400, detail=f"世界包 {label} 不是合法 JSON") from exc


def _load_world_template_from_package(
    archive: zipfile.ZipFile,
    manifest: WorldPackageManifest,
) -> WorldTemplateResponse:
    if manifest.character_files:
        if not manifest.world_file:
            raise HTTPException(status_code=400, detail="世界包 manifest 缺少 world/world.json 配置")

        world_payload = _read_package_json_document(archive, manifest.world_file, label="world/world.json")
        try:
            world_data = WorldPackageWorldData(**world_payload)
        except Exception as exc:
            raise HTTPException(status_code=400, detail=f"世界包 world/world.json 结构无效：{exc}") from exc

        package_characters: list[WorldPackageCharacterData] = []
        character_name_by_source_id: dict[str, str] = {}
        template_characters: list[CharacterTemplateResponse] = []
        for entry in manifest.character_files:
            character_payload = _read_package_json_document(archive, entry.file_path, label=entry.file_path)
            try:
                character_data = WorldPackageCharacterData(**character_payload)
            except Exception as exc:
                raise HTTPException(status_code=400, detail=f"世界包 {entry.file_path} 结构无效：{exc}") from exc

            if character_data.source_character_id and character_data.source_character_id != entry.source_character_id:
                raise HTTPException(
                    status_code=400,
                    detail=f"世界包 {entry.file_path} 的 source_character_id 与 manifest 不一致",
                )
            character_data.source_character_id = entry.source_character_id
            package_characters.append(character_data)
            character_name_by_source_id[entry.source_character_id] = character_data.name
            template_characters.append(character_data.to_template())

        return world_data.to_template(
            template_characters,
            character_name_by_source_id=character_name_by_source_id,
        )

    if manifest.world_file or manifest.characters_file:
        if not manifest.world_file or not manifest.characters_file:
            raise HTTPException(status_code=400, detail="世界包 manifest 缺少 world.json 或 characters.json 配置")

        world_payload = _read_package_json_document(archive, manifest.world_file, label="world.json")
        characters_payload = _read_package_json_document(archive, manifest.characters_file, label="characters.json")

        try:
            world_data = WorldPackageWorldData(**world_payload)
        except Exception as exc:
            raise HTTPException(status_code=400, detail=f"世界包 world.json 结构无效：{exc}") from exc

        try:
            characters_data = WorldPackageCharactersData(**characters_payload)
        except Exception as exc:
            raise HTTPException(status_code=400, detail=f"世界包 characters.json 结构无效：{exc}") from exc

        return world_data.to_template(characters_data.characters)

    if manifest.world is None:
        raise HTTPException(status_code=400, detail="世界包 manifest 缺少世界配置")
    return manifest.world


def _validate_world_package_template_assets(
    template: WorldTemplateResponse,
    manifest: WorldPackageManifest,
) -> None:
    manifest_sources = {
        canonical_source
        for item in manifest.assets
        for canonical_source in [_canonical_asset_reference(item.source_path)]
        if canonical_source is not None
    }
    manifest_archive_paths = {
        _normalize_package_document_path(item.archive_path, label="asset")
        for item in manifest.assets
    }
    missing_assets: list[str] = []
    for asset_path in _collect_world_template_asset_paths(template):
        normalized_archive_path: str | None = None
        try:
            normalized_archive_path = _normalize_package_document_path(asset_path, label="asset")
        except HTTPException:
            normalized_archive_path = None
        canonical_source = _canonical_asset_reference(asset_path)
        if normalized_archive_path and normalized_archive_path in manifest_archive_paths:
            continue
        if canonical_source and canonical_source in manifest_sources:
            continue
        missing_assets.append(asset_path)
    if missing_assets:
        raise HTTPException(
            status_code=400,
            detail=_format_missing_assets_detail("世界包缺少被世界配置引用的本地资源", missing_assets),
        )


def _import_world_package_assets(
    archive: zipfile.ZipFile,
    manifest: WorldPackageManifest,
) -> dict[str, str]:
    assets_root = _assets_root()
    asset_map: dict[str, str] = {}

    for item in manifest.assets:
        normalized_archive_path = _normalize_package_document_path(item.archive_path, label="asset")
        archive_member = PurePosixPath(normalized_archive_path)
        relative_asset_path = (
            PurePosixPath(*archive_member.parts[1:])
            if archive_member.parts and archive_member.parts[0] == "assets"
            else archive_member
        )
        if not relative_asset_path.parts:
            raise HTTPException(status_code=400, detail=f"世界包资源路径无效：{item.archive_path}")

        try:
            source = archive.open(archive_member.as_posix())
        except KeyError as exc:
            raise HTTPException(status_code=400, detail=f"世界包缺少资源文件：{item.archive_path}") from exc

        target_subdir = Path(*relative_asset_path.parts[:-1])
        target_dir = assets_root / target_subdir
        target_dir.mkdir(parents=True, exist_ok=True)
        suffix = Path(relative_asset_path.name).suffix
        target_name = f"{uuid.uuid4().hex[:12]}{suffix}"
        target_path = target_dir / target_name
        with source, open(target_path, "wb") as handle:
            shutil.copyfileobj(source, handle)
        saved_relative = (target_subdir / target_name).as_posix() if target_subdir.parts else target_name
        canonical_source_path = _canonical_asset_reference(item.source_path)
        target_asset_path = f"/assets/{saved_relative}"
        asset_map[item.source_path] = target_asset_path
        asset_map[item.archive_path] = target_asset_path
        asset_map[normalized_archive_path] = target_asset_path
        if canonical_source_path is not None:
            asset_map[canonical_source_path] = target_asset_path

    return asset_map


def _remap_asset_path(asset_path: str, asset_map: dict[str, str]) -> str:
    canonical_source_path = _canonical_asset_reference(asset_path)
    if canonical_source_path is not None and canonical_source_path in asset_map:
        return asset_map[canonical_source_path]
    return asset_map.get(asset_path, asset_path)


def _remap_ui_theme_config_assets(ui_theme_config: dict[str, object], asset_map: dict[str, str]) -> dict[str, object]:
    remapped = dict(ui_theme_config)
    local_background_assets = ui_theme_config.get("local_background_assets", [])
    if isinstance(local_background_assets, list):
        remapped["local_background_assets"] = [
            _remap_asset_path(str(item), asset_map)
            for item in local_background_assets
            if str(item).strip()
        ]

    local_scene_backgrounds = ui_theme_config.get("local_scene_backgrounds", {})
    if isinstance(local_scene_backgrounds, dict):
        remapped["local_scene_backgrounds"] = {
            str(scene_name): [
                _remap_asset_path(str(item), asset_map)
                for item in items
                if str(item).strip()
            ]
            for scene_name, items in local_scene_backgrounds.items()
            if isinstance(items, list)
        }

    return remapped


def _import_world_from_package(
    template: WorldTemplateResponse,
    *,
    container: AppContainer,
    asset_map: dict[str, str],
) -> WorldDefinition:
    remapped_ui_theme_config = _remap_ui_theme_config_assets(template.ui_theme_config, asset_map)
    created_world = container.catalog_commands.create_world(
        WorldDefinition(
            id="new",
            name=template.name,
            genre=template.genre,
            background_prompt=template.background_prompt,
            opening_scene=template.opening_scene,
            summary=template.summary,
            time_system=template.time_system,
            map_nodes=list(template.map_nodes),
            triggers=list(template.triggers),
            custom_tabs=dict(template.custom_tabs),
            time_config=dict(template.time_config),
            director_config=normalize_world_director_config(template.director_config),
            ui_theme_config=remapped_ui_theme_config,
            opening_messages=[item.to_domain() for item in template.opening_messages],
            opening_character_ids=[],
            player_character_id=None,
        )
    )

    character_name_to_id: dict[str, str] = {}
    for character in template.characters:
        created_character = container.catalog_commands.create_character(
            CharacterDefinition(
                id="new",
                name=character.name,
                world_id=created_world.id,
                role=character.role,
                background_prompt=character.background_prompt,
                model=character.model,
                memory_strategy=character.memory_strategy,
                recent_dialogue_rounds=character.recent_dialogue_rounds,
                attributes=list(character.attributes),
                portrait_assets=[_remap_asset_path(item, asset_map) for item in character.portrait_assets],
                custom_tabs=dict(character.custom_tabs),
            )
        )
        character_name_to_id[created_character.name] = created_character.id

    opening_character_ids = [
        character_name_to_id[name]
        for name in template.opening_character_names
        if name in character_name_to_id
    ]
    player_character_id = character_name_to_id.get(template.player_character_name or "")

    if not opening_character_ids and player_character_id is None:
        return created_world

    updated_world = container.catalog_commands.update_world(
        created_world.id,
        WorldDefinition(
            id=created_world.id,
            name=created_world.name,
            genre=created_world.genre,
            background_prompt=created_world.background_prompt,
            opening_scene=created_world.opening_scene,
            summary=created_world.summary,
            time_system=created_world.time_system,
            map_nodes=list(created_world.map_nodes),
            triggers=list(created_world.triggers),
            custom_tabs=dict(created_world.custom_tabs),
            time_config=dict(created_world.time_config),
            director_config=normalize_world_director_config(created_world.director_config),
            ui_theme_config=dict(created_world.ui_theme_config),
            opening_messages=list(created_world.opening_messages),
            opening_character_ids=opening_character_ids,
            player_character_id=player_character_id,
        ),
    )
    return updated_world or created_world


@router.get("", response_model=list[WorldResponse])
def list_worlds(container: AppContainer = Depends(get_app_container)):
    return [_to_world_response(container, item) for item in container.catalog_queries.list_worlds()]


@router.post("/import-package", response_model=WorldResponse)
async def import_world_package(
    file: UploadFile = File(...),
    container: AppContainer = Depends(get_app_container),
):
    package_bytes = await file.read()
    if len(package_bytes) > MAX_WORLD_PACKAGE_SIZE:
        raise HTTPException(status_code=413, detail="世界包过大，最大支持 200MB")

    try:
        with zipfile.ZipFile(io.BytesIO(package_bytes)) as archive:
            manifest = _read_world_package_manifest(archive)
            template = _load_world_template_from_package(archive, manifest)
            _validate_world_package_template_assets(template, manifest)
            asset_map = _import_world_package_assets(archive, manifest)
    except zipfile.BadZipFile as exc:
        raise HTTPException(status_code=400, detail="上传的文件不是合法 zip 世界包") from exc

    try:
        created_world = _import_world_from_package(template, container=container, asset_map=asset_map)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return _to_world_response(container, created_world)


@router.get("/{world_id}", response_model=WorldResponse)
def get_world(world_id: str, container: AppContainer = Depends(get_app_container)):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    return _to_world_response(container, world)


@router.get("/{world_id}/opening-prompt-preview", response_model=WorldOpeningPromptPreviewResponse)
def get_world_opening_prompt_preview(
    world_id: str,
    player_character_id: str | None = None,
    player_input: str = "继续",
    container: AppContainer = Depends(get_app_container),
):
    try:
        preview = container.session_orchestrator.build_opening_prompt_preview(
            world_id=world_id,
            player_character_id=player_character_id,
            player_input=player_input,
        )
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

    return WorldOpeningPromptPreviewResponse(
        opening_calls_llm=bool(preview.get("opening_calls_llm")),
        opening_messages=[
            WorldOpeningMessagePayload(
                role=str(item.get("role") or "system"),
                content=str(item.get("content") or ""),
                speaker=str(item.get("speaker")) if item.get("speaker") is not None else None,
            )
            for item in preview.get("opening_messages", [])
            if isinstance(item, dict) and str(item.get("content") or "").strip()
        ],
        sample_player_input=str(preview.get("sample_player_input") or ""),
        planned_speakers=[
            str(item).strip()
            for item in preview.get("planned_speakers", [])
            if str(item).strip()
        ],
        world_director_prompt_trace=dict(preview.get("world_director_prompt_trace") or {}),
        character_prompt_traces=[
            PromptTracePreviewResponse(
                speaker=str(item.get("speaker")).strip() if item.get("speaker") is not None else None,
                prompt_trace=dict(item.get("prompt_trace") or {}),
            )
            for item in preview.get("character_prompt_traces", [])
            if isinstance(item, dict)
        ],
        notes=[
            str(item).strip()
            for item in preview.get("notes", [])
            if str(item).strip()
        ],
    )


@router.get("/{world_id}/characters", response_model=list[CharacterResponse])
def list_world_characters(world_id: str, container: AppContainer = Depends(get_app_container)):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    return [_to_character_response(container, item) for item in container.catalog_queries.list_characters_for_world(world_id)]


@router.post("/{world_id}/characters", response_model=CharacterResponse)
def create_world_character(
    world_id: str,
    payload: CharacterUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    try:
        created = container.catalog_commands.create_character(
            CharacterDefinition(
                id="new",
                name=payload.name,
                world_id=world_id,
                role=payload.role,
                background_prompt=payload.background_prompt,
                model=payload.model,
                memory_strategy=payload.memory_strategy,
                recent_dialogue_rounds=payload.recent_dialogue_rounds,
                attributes=payload.attributes,
                portrait_assets=payload.portrait_assets,
                custom_tabs=payload.custom_tabs,
            )
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return _to_character_response(container, created)


@router.put("/{world_id}/characters/{character_id}", response_model=CharacterResponse)
def update_world_character(
    world_id: str,
    character_id: str,
    payload: CharacterUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    existing = container.catalog_queries.get_character(character_id)
    if existing is None or existing.world_id != world_id:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    try:
        updated = container.catalog_commands.update_character(
            character_id,
            CharacterDefinition(
                id=character_id,
                name=payload.name,
                world_id=world_id,
                role=payload.role,
                background_prompt=payload.background_prompt,
                model=payload.model,
                memory_strategy=payload.memory_strategy,
                recent_dialogue_rounds=payload.recent_dialogue_rounds,
                attributes=payload.attributes,
                portrait_assets=payload.portrait_assets,
                custom_tabs=payload.custom_tabs,
            ),
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    if updated is None:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    return _to_character_response(container, updated)


@router.delete("/{world_id}/characters/{character_id}")
def delete_world_character(
    world_id: str,
    character_id: str,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    existing = container.catalog_queries.get_character(character_id)
    if existing is None or existing.world_id != world_id:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    deleted = container.catalog_commands.delete_character(character_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    return {"ok": True}


@router.post("/{world_id}/characters/{character_id}/export-template", response_model=CharacterTemplateResponse)
def export_world_character_template(
    world_id: str,
    character_id: str,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    character = container.catalog_queries.get_character(character_id)
    if character is None or character.world_id != world_id:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    return CharacterTemplateResponse.from_domain(character)


@router.post("/{world_id}/characters/import-template", response_model=CharacterResponse)
def import_world_character_template(
    world_id: str,
    payload: CharacterTemplateImportRequest,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    try:
        created = container.catalog_commands.create_character(
            CharacterDefinition(
                id="new",
                name=payload.name,
                world_id=world_id,
                role=payload.role,
                background_prompt=payload.background_prompt,
                model=payload.model,
                memory_strategy=payload.memory_strategy,
                recent_dialogue_rounds=payload.recent_dialogue_rounds,
                attributes=payload.attributes,
                portrait_assets=payload.portrait_assets,
                custom_tabs=payload.custom_tabs,
            )
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return _to_character_response(container, created)


@router.post("/{world_id}/characters/{character_id}/derive", response_model=CharacterResponse)
def derive_world_character(
    world_id: str,
    character_id: str,
    payload: CharacterDeriveRequest,
    container: AppContainer = Depends(get_app_container),
):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    character = container.catalog_queries.get_character(character_id)
    if character is None or character.world_id != world_id:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    try:
        created = container.catalog_commands.create_character(
            CharacterDefinition(
                id="new",
                name=payload.name,
                world_id=world_id,
                role=character.role,
                background_prompt=character.background_prompt,
                model=character.model,
                memory_strategy=character.memory_strategy,
                recent_dialogue_rounds=character.recent_dialogue_rounds,
                attributes=character.attributes,
                portrait_assets=character.portrait_assets,
                custom_tabs=character.custom_tabs,
            )
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return _to_character_response(container, created)


@router.post("/{world_id}/characters/{character_id}/create-in-world", response_model=CharacterResponse)
def create_character_from_world_character(
    world_id: str,
    character_id: str,
    payload: CharacterCreateFromTemplateRequest,
    container: AppContainer = Depends(get_app_container),
):
    source_world = container.catalog_queries.get_world(world_id)
    if source_world is None:
        raise HTTPException(status_code=404, detail="World not found")
    target_world = container.catalog_queries.get_world(payload.target_world_id)
    if target_world is None:
        raise HTTPException(status_code=404, detail="Target world not found")
    character = container.catalog_queries.get_character(character_id)
    if character is None or character.world_id != world_id:
        raise HTTPException(status_code=404, detail="Character not found in this world")
    try:
        created = container.catalog_commands.create_character(
            CharacterDefinition(
                id="new",
                name=payload.name,
                world_id=payload.target_world_id,
                role=character.role,
                background_prompt=character.background_prompt,
                model=character.model,
                memory_strategy=character.memory_strategy,
                recent_dialogue_rounds=character.recent_dialogue_rounds,
                attributes=character.attributes,
                portrait_assets=character.portrait_assets,
                custom_tabs=character.custom_tabs,
            )
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return _to_character_response(container, created)


@router.post("/{world_id}/export", response_model=WorldTemplateResponse)
def export_world(world_id: str, container: AppContainer = Depends(get_app_container)):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    characters = container.catalog_queries.list_characters_for_world(world_id)
    return WorldTemplateResponse.from_domain(world, characters)


@router.post("/{world_id}/export-package")
def export_world_package(world_id: str, container: AppContainer = Depends(get_app_container)):
    world = container.catalog_queries.get_world(world_id)
    if world is None:
        raise HTTPException(status_code=404, detail="World not found")
    characters = container.catalog_queries.list_characters_for_world(world_id)
    manifest, world_data, character_files = _build_world_package_export_data(world, characters)

    package_buffer = io.BytesIO()
    with zipfile.ZipFile(package_buffer, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(
            "manifest.json",
            json.dumps(manifest.model_dump(), ensure_ascii=False, indent=2),
        )
        archive.writestr(
            PACKAGE_WORLD_FILE,
            json.dumps(world_data.model_dump(), ensure_ascii=False, indent=2),
        )
        for entry, character_data in character_files:
            archive.writestr(
                entry.file_path,
                json.dumps(character_data.model_dump(), ensure_ascii=False, indent=2),
            )
        for asset in manifest.assets:
            relative_path = _normalize_archive_relative_path(asset.source_path)
            if relative_path is None:
                continue
            file_path = _assets_root() / Path(relative_path)
            if file_path.is_file():
                archive.write(file_path, arcname=asset.archive_path)

    package_buffer.seek(0)
    filename = f"{_package_download_name(world.name)}.zip"
    fallback_filename = f"world-package-{world.id}.zip"
    headers = {
        "Content-Disposition": (
            f'attachment; filename="{fallback_filename}"; filename*=UTF-8\'\'{quote(filename)}'
        )
    }
    return StreamingResponse(package_buffer, media_type="application/zip", headers=headers)


@router.post("", response_model=WorldResponse)
def create_world(payload: WorldUpsertRequest, container: AppContainer = Depends(get_app_container)):
    created = container.catalog_commands.create_world(
        WorldDefinition(
            id="new",
            name=payload.name,
            genre=payload.genre,
            background_prompt=payload.background_prompt,
            opening_scene=payload.opening_scene,
            summary=payload.summary,
            time_system=payload.time_system,
            map_nodes=payload.map_nodes,
            triggers=payload.triggers,
            custom_tabs=payload.custom_tabs,
            time_config=payload.time_config,
            director_config=normalize_world_director_config(payload.director_config),
            ui_theme_config=payload.ui_theme_config,
            opening_messages=[item.to_domain() for item in payload.opening_messages],
            opening_character_ids=list(dict.fromkeys(payload.opening_character_ids)),
            player_character_id=payload.player_character_id,
        )
    )
    return _to_world_response(container, created)


@router.put("/{world_id}", response_model=WorldResponse)
def update_world(
    world_id: str,
    payload: WorldUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    updated = container.catalog_commands.update_world(
        world_id,
        WorldDefinition(
            id=world_id,
            name=payload.name,
            genre=payload.genre,
            background_prompt=payload.background_prompt,
            opening_scene=payload.opening_scene,
            summary=payload.summary,
            time_system=payload.time_system,
            map_nodes=payload.map_nodes,
            triggers=payload.triggers,
            custom_tabs=payload.custom_tabs,
            time_config=payload.time_config,
            director_config=normalize_world_director_config(payload.director_config),
            ui_theme_config=payload.ui_theme_config,
            opening_messages=[item.to_domain() for item in payload.opening_messages],
            opening_character_ids=list(dict.fromkeys(payload.opening_character_ids)),
            player_character_id=payload.player_character_id,
        ),
    )
    if updated is None:
        raise HTTPException(status_code=404, detail="World not found")
    return _to_world_response(container, updated)


@router.delete("/{world_id}")
def delete_world(world_id: str, container: AppContainer = Depends(get_app_container)):
    deleted = container.catalog_commands.delete_world(world_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="World not found")
    return {"ok": True}


@router.delete("")
def delete_all_worlds(container: AppContainer = Depends(get_app_container)):
    deleted_count = container.catalog_commands.delete_all_worlds()
    return {"ok": True, "deleted_count": deleted_count}


@router.post("/{world_id}/duplicate", response_model=WorldResponse)
def duplicate_world(world_id: str, container: AppContainer = Depends(get_app_container)):
    duplicated = container.catalog_commands.duplicate_world(world_id)
    if duplicated is None:
        raise HTTPException(status_code=404, detail="World not found")
    return _to_world_response(container, duplicated)
