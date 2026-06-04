from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.catalog_service import CatalogQueryService
from backend.app.application.services.image_generation_service import ImageGenerationService
from backend.app.application.services.world_director_service import (
    CharacterVisualDirective,
    DirectorDecision,
)
from backend.app.domain.models.asset import AssetSelection, CharacterVisualState
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.state import SessionState
from backend.app.domain.models.world import WorldDefinition


class AssetResolverService:
    def __init__(
        self,
        catalog_queries: CatalogQueryService,
        image_generation: ImageGenerationService,
    ) -> None:
        self._catalog_queries = catalog_queries
        self._image_generation = image_generation

    def resolve(
        self,
        session: SessionSnapshot,
        scene: SceneRuntime,
        state: SessionState,
        current_speaker: str,
        session_attributes: list[RuntimeAttributeItem],
        world_profile: WorldDefinition | None = None,
        director_decision: DirectorDecision | None = None,
        allow_generation: bool = False,
    ) -> AssetSelection:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        weather_state = str(attr_map.get("weather_state", "clear"))
        world_tension = self._as_number(attr_map.get("world_tension"), 0)
        ui_theme = world_profile.ui_theme_config if world_profile else {}
        portrait_map = self._load_portrait_map(world_profile.id if world_profile else None)

        character_visual_map = self._resolve_character_visual_map(
            session=session,
            director_decision=director_decision,
        )
        background_hint = str(scene.background_hint or "").strip() or self._resolve_background_hint(
            scene_name=scene.name,
            weather_state=weather_state,
            phase=state.phase,
            pressure=world_tension,
        )
        background_generation_prompt = self._resolve_background_generation_prompt(
            session=session,
            director_decision=director_decision,
        )
        background_asset_path = self._resolve_background_asset(
            session=session,
            scene=scene,
            ui_theme=ui_theme,
            director_decision=director_decision,
            generation_prompt=background_generation_prompt,
            allow_generation=allow_generation,
        )

        active_speaker_directive = character_visual_map.get(current_speaker)
        active_speaker_portrait = self._resolve_portrait_hint_for_character(
            character_name=current_speaker,
            weather_state=weather_state,
            phase=state.phase,
            tags=state.tags,
            active=True,
            directive=active_speaker_directive,
            session=session,
        )
        active_speaker_generation_prompt = self._resolve_generation_prompt_for_character(
            character_name=current_speaker,
            directive=active_speaker_directive,
            session=session,
            director_decision=director_decision,
        )
        active_speaker_portrait_path = self._resolve_portrait_asset(
            character_name=current_speaker,
            portrait_assets=portrait_map.get(current_speaker, []),
            ui_theme=ui_theme,
            directive=active_speaker_directive,
            generation_prompt=active_speaker_generation_prompt,
            existing_path=session.assets.active_speaker_portrait_path,
            allow_generation=allow_generation,
        )

        visible_character_portraits: list[CharacterVisualState] = []
        for name in scene.present_characters:
            directive = character_visual_map.get(name)
            portrait_hint = self._resolve_portrait_hint_for_character(
                character_name=name,
                weather_state=weather_state,
                phase=state.phase,
                tags=state.tags,
                active=name == current_speaker,
                directive=directive,
                session=session,
            )
            generation_prompt = self._resolve_generation_prompt_for_character(
                character_name=name,
                directive=directive,
                session=session,
                director_decision=director_decision,
            )
            portrait_asset_path = (
                active_speaker_portrait_path
                if name == current_speaker
                else self._resolve_portrait_asset(
                    character_name=name,
                    portrait_assets=portrait_map.get(name, []),
                    ui_theme=ui_theme,
                    directive=directive,
                    generation_prompt=generation_prompt,
                    existing_path=self._existing_portrait_path(session, name),
                    allow_generation=allow_generation,
                )
            )
            visible_character_portraits.append(
                CharacterVisualState(
                    character_name=name,
                    portrait_hint=portrait_hint,
                    portrait_asset_path=portrait_asset_path,
                    generation_prompt=generation_prompt,
                )
            )

        return AssetSelection(
            background_hint=background_hint,
            active_speaker_portrait=active_speaker_portrait,
            background_asset_path=background_asset_path,
            active_speaker_portrait_path=active_speaker_portrait_path,
            background_generation_prompt=background_generation_prompt,
            active_speaker_generation_prompt=active_speaker_generation_prompt,
            visible_character_portraits=visible_character_portraits,
        )

    def _resolve_background_asset(
        self,
        *,
        session: SessionSnapshot,
        scene: SceneRuntime,
        ui_theme: dict[str, object],
        director_decision: DirectorDecision | None,
        generation_prompt: str | None,
        allow_generation: bool,
    ) -> str | None:
        local_assets = self._normalize_asset_list(ui_theme.get("local_background_assets"))
        scene_backgrounds = self._normalize_asset_groups(ui_theme.get("local_scene_backgrounds"))
        local_match = self._select_named_background_asset(
            scene_backgrounds,
            local_assets,
            name=director_decision.background_asset_name if director_decision is not None else None,
            path=director_decision.background_asset_path if director_decision is not None else None,
        )

        if local_match:
            return local_match
        if generation_prompt is not None and allow_generation:
            generated = self._image_generation.generate_image(prompt=generation_prompt, kind="background")
            if generated.asset_path:
                return generated.asset_path

        has_visual_directive = (
            director_decision is not None
            and (
                bool(str(director_decision.background_asset_name or "").strip())
                or bool(str(director_decision.background_asset_path or "").strip())
                or generation_prompt is not None
            )
        )
        if (
            session.assets.background_asset_path
            and scene.name == session.scene.name
            and not has_visual_directive
        ):
            return session.assets.background_asset_path

        fallback_asset = self._select_default_background_asset(
            scene_name=scene.name,
            location=session.location,
            scene_backgrounds=scene_backgrounds,
            local_assets=local_assets,
        )
        if fallback_asset:
            return fallback_asset

        if director_decision is None:
            return session.assets.background_asset_path
        return None

    def _resolve_portrait_asset(
        self,
        *,
        character_name: str,
        portrait_assets: list[str],
        ui_theme: dict[str, object],
        directive: CharacterVisualDirective | None,
        generation_prompt: str | None,
        existing_path: str | None,
        allow_generation: bool,
    ) -> str | None:
        local_match = self._select_named_asset(
            portrait_assets,
            name=directive.portrait_asset_name if directive is not None else None,
            path=directive.portrait_asset_path if directive is not None else None,
        )

        if local_match:
            return local_match
        if generation_prompt is not None and allow_generation:
            generated = self._image_generation.generate_image(prompt=generation_prompt, kind="portrait")
            if generated.asset_path:
                return generated.asset_path
        if existing_path and directive is None:
            return existing_path
        if portrait_assets:
            return portrait_assets[0]
        return None

    def _resolve_background_generation_prompt(
        self,
        *,
        session: SessionSnapshot,
        director_decision: DirectorDecision | None,
    ) -> str | None:
        if director_decision is not None:
            return self._normalized_prompt(director_decision.background_generation_prompt)
        return self._normalized_prompt(session.assets.background_generation_prompt)

    def _resolve_character_visual_map(
        self,
        *,
        session: SessionSnapshot,
        director_decision: DirectorDecision | None,
    ) -> dict[str, CharacterVisualDirective]:
        if director_decision is not None:
            source = director_decision.character_visual_directives
            return {
                item.character_name: item
                for item in source
                if item.character_name.strip()
            }

        visual_map: dict[str, CharacterVisualDirective] = {}
        for item in session.assets.visible_character_portraits:
            character_name = item.character_name.strip()
            if not character_name:
                continue
            if not item.portrait_hint.strip() and self._normalized_prompt(item.generation_prompt) is None:
                continue
            visual_map[character_name] = CharacterVisualDirective(
                character_name=character_name,
                portrait_hint=item.portrait_hint,
                generation_prompt=self._normalized_prompt(item.generation_prompt),
            )
        if (
            session.current_speaker
            and session.current_speaker.strip()
            and session.current_speaker not in visual_map
            and (
                session.assets.active_speaker_portrait.strip()
                or self._normalized_prompt(session.assets.active_speaker_generation_prompt) is not None
            )
        ):
            visual_map[session.current_speaker] = CharacterVisualDirective(
                character_name=session.current_speaker,
                portrait_hint=session.assets.active_speaker_portrait,
                generation_prompt=self._normalized_prompt(session.assets.active_speaker_generation_prompt),
            )
        return visual_map

    def _resolve_portrait_hint_for_character(
        self,
        *,
        character_name: str,
        weather_state: str,
        phase: str,
        tags: list[str],
        active: bool,
        directive: CharacterVisualDirective | None,
        session: SessionSnapshot,
    ) -> str:
        if directive is not None and directive.portrait_hint.strip():
            return directive.portrait_hint.strip()
        return self._resolve_portrait_hint(
            character_name=character_name,
            weather_state=weather_state,
            phase=phase,
            tags=tags,
            active=active,
            session=session,
        )

    def _resolve_generation_prompt_for_character(
        self,
        *,
        character_name: str,
        directive: CharacterVisualDirective | None,
        session: SessionSnapshot,
        director_decision: DirectorDecision | None,
    ) -> str | None:
        if director_decision is not None:
            return self._normalized_prompt(directive.generation_prompt if directive is not None else None)
        if directive is not None:
            return self._normalized_prompt(directive.generation_prompt)
        if character_name == session.current_speaker:
            return self._normalized_prompt(session.assets.active_speaker_generation_prompt)
        return None

    def _normalized_prompt(self, value: object) -> str | None:
        prompt = str(value or "").strip()
        return prompt or None

    def _select_named_asset(
        self,
        paths: list[str],
        *,
        name: str | None,
        path: str | None,
    ) -> str | None:
        normalized_paths = [item.strip() for item in paths if item.strip()]
        if not normalized_paths:
            return None

        requested_path = str(path or "").strip()
        if requested_path:
            for asset_path in normalized_paths:
                if asset_path == requested_path or asset_path.endswith(requested_path):
                    return asset_path

        requested_name = self._normalize_asset_match_text(name)
        if not requested_name:
            return None

        for asset_path in normalized_paths:
            if self._normalize_asset_match_text(asset_path) == requested_name:
                return asset_path
        for asset_path in normalized_paths:
            if requested_name in self._normalize_asset_match_text(asset_path):
                return asset_path
        return None

    def _flatten_asset_groups(self, groups: dict[str, list[str]]) -> list[str]:
        flattened: list[str] = []
        for paths in groups.values():
            flattened.extend(paths)
        return flattened

    def _select_named_background_asset(
        self,
        scene_backgrounds: dict[str, list[str]],
        local_assets: list[str],
        *,
        name: str | None,
        path: str | None,
    ) -> str | None:
        all_paths = [*self._flatten_asset_groups(scene_backgrounds), *local_assets]
        path_match = self._select_named_asset(all_paths, name=None, path=path)
        if path_match:
            return path_match

        requested_name = self._normalize_asset_match_text(name)
        if requested_name:
            for scene_name, paths in scene_backgrounds.items():
                if self._normalize_asset_match_text(scene_name) == requested_name and paths:
                    return paths[0]
            for scene_name, paths in scene_backgrounds.items():
                normalized_scene_name = self._normalize_asset_match_text(scene_name)
                if requested_name in normalized_scene_name or normalized_scene_name in requested_name:
                    if paths:
                        return paths[0]
        return self._select_named_asset(all_paths, name=name, path=None)

    def _select_default_background_asset(
        self,
        *,
        scene_name: str,
        location: str,
        scene_backgrounds: dict[str, list[str]],
        local_assets: list[str],
    ) -> str | None:
        scene_candidates = [scene_name, location]
        for candidate in scene_candidates:
            requested_name = self._normalize_asset_match_text(candidate)
            if not requested_name:
                continue
            for configured_scene_name, paths in scene_backgrounds.items():
                normalized_scene_name = self._normalize_asset_match_text(configured_scene_name)
                if requested_name == normalized_scene_name or requested_name in normalized_scene_name or normalized_scene_name in requested_name:
                    if paths:
                        return paths[0]
        for paths in scene_backgrounds.values():
            if paths:
                return paths[0]
        return local_assets[0] if local_assets else None

    def _normalize_asset_match_text(self, value: object) -> str:
        text = str(value or "").strip().lower()
        if not text:
            return ""
        stem = text.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
        stem = stem.rsplit(".", 1)[0]
        return "".join(char for char in stem if char.isalnum())

    def _normalize_asset_list(self, raw: object) -> list[str]:
        if not isinstance(raw, list):
            return []
        return [str(item).strip() for item in raw if str(item).strip()]

    def _normalize_asset_groups(self, raw: object) -> dict[str, list[str]]:
        if not isinstance(raw, dict):
            return {}

        normalized: dict[str, list[str]] = {}
        for key, value in raw.items():
            group_key = str(key).strip()
            if not group_key:
                continue

            assets: list[str] = []
            if isinstance(value, list):
                assets = [str(item).strip() for item in value if str(item).strip()]

            if assets:
                normalized[group_key] = assets
        return normalized

    def _load_portrait_map(self, world_id: str | None) -> dict[str, list[str]]:
        if not world_id:
            return {}

        characters = self._catalog_queries.list_characters_for_world(world_id)
        return {
            character.name: [path.strip() for path in character.portrait_assets if path.strip()]
            for character in characters
            if character.name.strip()
        }

    def _resolve_source_mode(self, raw_value: object, *, fallback: str) -> str:
        value = str(raw_value or "").strip() or fallback
        if value in {"generated-first", "local-first", "generated-only", "local-only"}:
            return value
        return fallback

    def _resolve_background_hint(
        self,
        scene_name: str,
        weather_state: str,
        phase: str,
        pressure: float,
    ) -> str:
        density = "calm"
        if pressure >= 60:
            density = "crisis"
        elif pressure >= 35:
            density = "tense"

        return f"{scene_name}:{weather_state}:{phase}:{density}"

    def _resolve_portrait_hint(
        self,
        *,
        character_name: str,
        weather_state: str,
        phase: str,
        tags: list[str],
        active: bool,
        session: SessionSnapshot,
    ) -> str:
        existing = self._existing_portrait_hint(session, character_name)
        if existing and not active:
            return existing

        posture = "idle"
        if active:
            posture = "speaking"
        if "traveling" in tags:
            posture = "moving"
        if phase == "combat-ready":
            posture = "combat"

        return f"{character_name}:{weather_state}:{phase}:{posture}"

    def _existing_portrait_hint(self, session: SessionSnapshot, character_name: str) -> str:
        if character_name == session.current_speaker and session.assets.active_speaker_portrait.strip():
            return session.assets.active_speaker_portrait.strip()
        for item in session.assets.visible_character_portraits:
            if item.character_name == character_name and item.portrait_hint.strip():
                return item.portrait_hint.strip()
        return ""

    def _existing_portrait_path(self, session: SessionSnapshot, character_name: str) -> str | None:
        if character_name == session.current_speaker and session.assets.active_speaker_portrait_path:
            return session.assets.active_speaker_portrait_path
        for item in session.assets.visible_character_portraits:
            if item.character_name == character_name and item.portrait_asset_path:
                return item.portrait_asset_path
        return None

    def _as_number(self, value: object, fallback: float) -> float:
        if isinstance(value, (int, float)):
            return float(value)
        return fallback
