from dataclasses import dataclass, field
import json
import re
import sqlite3

from backend.app.application.services.attribute_runtime_service import AttributeRuntimeService, RuntimeAttributeItem
from backend.app.application.services.prompt_runtime_service import PromptModule, PromptRuntimeService
from backend.app.application.services.text_generation_service import TextGenerationService
from backend.app.core.config import Settings
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.session import ChatMessage, SessionSnapshot
from backend.app.domain.models.world import (
    DEFAULT_WORLD_DIRECTOR_CONFIG,
    WorldDefinition,
    normalize_world_director_config,
)


@dataclass(frozen=True)
class GeneratedCharacterDraft:
    name: str
    world_name: str
    role: str
    background_prompt: str
    model: str
    memory_strategy: str
    attributes: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class SwitchCharacterProposal:
    target_character_name: str
    reason: str
    next_location: str | None = None
    scene_name: str | None = None
    scene_background_hint: str | None = None
    scene_tags: list[str] = field(default_factory=list)
    visible_characters: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class SceneChangeToolResult:
    scene_name: str
    scene_description: str
    new_characters: list[GeneratedCharacterDraft] = field(default_factory=list)
    all_characters: list[str] = field(default_factory=list)
    player_character_name: str | None = None
    background_generation_prompt: str | None = None


@dataclass(frozen=True)
class CharacterVisualDirective:
    character_name: str
    portrait_hint: str = ""
    portrait_asset_name: str | None = None
    portrait_asset_path: str | None = None
    generation_prompt: str | None = None


@dataclass(frozen=True)
class DirectorDecision:
    world_phase: str
    next_location: str | None = None
    next_scene_name: str | None = None
    next_scene_background_hint: str | None = None
    background_asset_name: str | None = None
    background_asset_path: str | None = None
    background_generation_prompt: str | None = None
    next_scene_tags: list[str] = field(default_factory=list)
    next_time_label: str | None = None
    generated_characters: list[GeneratedCharacterDraft] = field(default_factory=list)
    character_visual_directives: list[CharacterVisualDirective] = field(default_factory=list)
    scene_change: SceneChangeToolResult | None = None
    scene_visible_characters: list[str] | None = None
    planned_speakers: list[str] = field(default_factory=list)
    switch_character_proposal: SwitchCharacterProposal | None = None
    raw_model_response: str | None = None
    prompt_trace: dict[str, object] | None = None


class WorldDirectorService:
    DEFAULT_DIRECTOR_CONFIG = DEFAULT_WORLD_DIRECTOR_CONFIG
    PLAYER_VIEW_SWITCH_PATTERN = re.compile(r"^(.+?)鐨勮瑙掑凡鍚敤")

    def __init__(
        self,
        text_generation: TextGenerationService | None = None,
        attribute_runtime: AttributeRuntimeService | None = None,
    ) -> None:
        self._text_generation = text_generation
        self._attribute_runtime = attribute_runtime
        self._prompt_runtime = PromptRuntimeService()

    def plan_turn(
        self,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
    ) -> DirectorDecision:
        heuristic_decision, director_config = self.build_heuristic_decision(
            session=session,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
        )
        llm_decision = self._plan_with_model(
            session=session,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
            fallback=heuristic_decision,
            director_config=director_config,
        )
        if llm_decision is not None:
            return llm_decision

        return heuristic_decision

    def build_heuristic_decision(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
    ) -> tuple[DirectorDecision, dict[str, object]]:
        director_config = self._resolve_director_config(world_profile)
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        world_tension = self._as_number(attr_map.get("world_tension"), 0)
        world_phase = self._resolve_world_phase(world_tension)
        planned_speakers = self._fallback_planned_speakers(session=session, player_input=player_input)
        heuristic_decision = DirectorDecision(
            world_phase=world_phase,
            next_location=session.location,
            next_scene_name=session.scene.name,
            next_scene_background_hint=session.scene.background_hint,
            next_scene_tags=list(session.scene.temporary_tags),
            next_time_label=session.time_label,
            generated_characters=[],
            scene_visible_characters=None,
            planned_speakers=planned_speakers,
        )
        return heuristic_decision, director_config

    def build_runtime_system_prompt(self, *, director_config: dict[str, object]) -> str:
        return self._build_system_prompt(director_config)

    def build_runtime_system_prompt_base(self) -> str:
        return ""

    def build_runtime_turn_payload(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        fallback: DirectorDecision,
        director_config: dict[str, object],
        character_profiles: list[CharacterDefinition] | None = None,
    ) -> str:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        return self._build_user_prompt(
            session=session,
            world_profile=world_profile,
            player_input=player_input,
            attr_map=attr_map,
            fallback=fallback,
            director_config=director_config,
            character_profiles=character_profiles,
        )

    def build_runtime_prompt_trace(
        self,
        *,
        director_config: dict[str, object],
        turn_payload: str,
        init_messages: list[dict[str, object]] | None = None,
        tool_loop_messages: list[dict[str, object]] | None = None,
    ) -> dict[str, object]:
        system_prompt = self.build_runtime_system_prompt(director_config=director_config)
        messages = [
            {
                "role": str(item.get("role") or "user"),
                "content": str(item.get("content") or ""),
            }
            for item in (list(init_messages) if init_messages is not None else [])
            if isinstance(item, dict)
        ]
        if not messages and system_prompt.strip():
            messages.append({"role": "system", "content": system_prompt})
        if not messages or str(messages[-1].get("content") or "") != turn_payload:
            messages.append({"role": "user", "content": turn_payload})
        if tool_loop_messages:
            messages.extend(
                {
                    "role": str(item.get("role") or "user"),
                    "content": str(item.get("content") or ""),
                }
                for item in tool_loop_messages
                if isinstance(item, dict)
            )
        return {
            "schema_version": "world_director_prompt_v1",
            "system_prompt": system_prompt,
            "turn_payload": turn_payload,
            "messages": messages,
            "tool_loop_messages": list(tool_loop_messages or []),
        }

    def parse_runtime_payload(
        self,
        *,
        payload: dict[str, object],
        session: SessionSnapshot,
        world_profile: WorldDefinition | None = None,
        director_config: dict[str, object],
        fallback: DirectorDecision,
    ) -> DirectorDecision:
        world_phase = str(payload.get("world_phase") or fallback.world_phase)
        if world_phase not in {"opening", "escalation", "crisis"}:
            world_phase = fallback.world_phase

        map_nodes = self._session_map_nodes(session)
        next_location = payload.get("next_location")
        if next_location is not None:
            next_location = self._normalize_llm_text(next_location) or None
        if not bool(director_config["allow_scene_transition"]):
            next_location = session.location
        elif next_location is None:
            next_location = fallback.next_location
        next_scene_name = self._parse_next_scene_name(
            payload.get("next_scene_name"),
            session=session,
            next_location=next_location,
            fallback=fallback.next_scene_name,
        )
        next_scene_background_hint = self._parse_next_scene_background_hint(
            payload.get("next_scene_background_hint"),
            session=session,
            next_scene_name=next_scene_name,
            fallback=fallback.next_scene_background_hint,
        )
        background_generation_prompt = self._parse_generation_prompt(
            payload.get("background_generation_prompt"),
        )
        background_asset_name = self._normalize_llm_text(payload.get("background_asset_name")) or None
        background_asset_path = self._normalize_llm_text(payload.get("background_asset_path")) or None
        next_scene_tags = self._parse_next_scene_tags(
            payload.get("next_scene_tags"),
            session=session,
            next_scene_name=next_scene_name,
            fallback=fallback.next_scene_tags,
        )
        next_time_label = self._parse_next_time_label(
            payload.get("next_time_label"),
            session=session,
            world_profile=world_profile,
            fallback=fallback.next_time_label,
        )

        generated_characters = (
            self._parse_generated_characters(self._collect_generated_character_items(payload), session)
            if bool(director_config["allow_npc_spawn"])
            else []
        )
        scene_visible_characters = self._parse_scene_visible_characters(
            payload.get("scene_visible_characters"),
            player_character_name=session.player_character_name,
            fallback=fallback.scene_visible_characters,
        )
        merged_visible_characters = (
            list(scene_visible_characters)
            if scene_visible_characters is not None
            else self._merge_visible_characters(session.visible_characters, generated_characters)
        )
        planned_speakers = self._parse_planned_speakers(
            payload.get("planned_speakers"),
            visible_character_names=merged_visible_characters,
            fallback=fallback.planned_speakers,
            player_character_name=session.player_character_name,
        )
        character_visual_directives = self._parse_character_visual_directives(
            payload.get("character_visual_directives"),
        )

        return DirectorDecision(
            world_phase=world_phase,
            next_location=next_location,
            next_scene_name=next_scene_name,
            next_scene_background_hint=next_scene_background_hint,
            background_asset_name=background_asset_name,
            background_asset_path=background_asset_path,
            background_generation_prompt=background_generation_prompt,
            next_scene_tags=next_scene_tags,
            next_time_label=next_time_label,
            generated_characters=generated_characters,
            character_visual_directives=character_visual_directives,
            scene_change=None,
            scene_visible_characters=scene_visible_characters,
            planned_speakers=planned_speakers,
            switch_character_proposal=self._parse_switch_character_proposal(
                payload.get("switch_character_proposal"),
                visible_character_names=merged_visible_characters,
                player_character_name=session.player_character_name,
            ) or fallback.switch_character_proposal,
            raw_model_response=None,
            prompt_trace=None,
        )

    def _plan_with_model(
        self,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        fallback: DirectorDecision,
        director_config: dict[str, object],
    ) -> DirectorDecision | None:
        if self._text_generation is None:
            return None

        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        model_result = self._text_generation.generate_json(
            system_prompt=self._build_system_prompt(director_config),
            user_prompt=self._build_user_prompt(
                session=session,
                world_profile=world_profile,
                player_input=player_input,
                attr_map=attr_map,
                fallback=fallback,
                director_config=director_config,
            ),
        )
        if model_result.payload is None:
            return None

        payload = model_result.payload
        parsed_decision = self.parse_runtime_payload(
            payload=payload,
            session=session,
            world_profile=world_profile,
            director_config=director_config,
            fallback=fallback,
        )
        return DirectorDecision(
            world_phase=parsed_decision.world_phase,
            next_location=parsed_decision.next_location,
            next_scene_name=parsed_decision.next_scene_name,
            next_scene_background_hint=parsed_decision.next_scene_background_hint,
            background_asset_name=parsed_decision.background_asset_name,
            background_asset_path=parsed_decision.background_asset_path,
            background_generation_prompt=parsed_decision.background_generation_prompt,
            next_scene_tags=list(parsed_decision.next_scene_tags),
            next_time_label=parsed_decision.next_time_label,
            generated_characters=list(parsed_decision.generated_characters),
            character_visual_directives=list(parsed_decision.character_visual_directives),
            scene_change=parsed_decision.scene_change,
            scene_visible_characters=(
                list(parsed_decision.scene_visible_characters)
                if parsed_decision.scene_visible_characters is not None
                else None
            ),
            planned_speakers=list(parsed_decision.planned_speakers),
            switch_character_proposal=parsed_decision.switch_character_proposal,
            raw_model_response=(model_result.raw_content or "").strip() or None,
            prompt_trace=None,
        )

    def _build_basic_setting(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
    ) -> dict[str, object]:
        return {
            "world_name": session.world_name,
            "genre": world_profile.genre if world_profile else "",
            "background_prompt": world_profile.background_prompt if world_profile else "",
            "summary": world_profile.summary if world_profile else "",
            "opening_scene": world_profile.opening_scene if world_profile else "",
            "time_system": world_profile.time_system if world_profile else "",
            "map_nodes": self._session_map_nodes(session)[:8],
        }

    def _build_current_state_minimal(
        self,
        *,
        session: SessionSnapshot,
        player_input: str,
        attr_map: dict[str, object],
    ) -> dict[str, object]:
        return {
            "player_input": player_input,
            "player_character_name": session.player_character_name,
            "location": session.location,
            "time_label": session.time_label,
            "visible_characters": list(session.visible_characters),
            "scene": {
                "name": session.scene.name,
                "temporary_tags": list(session.scene.temporary_tags),
                "present_characters": list(session.scene.present_characters),
            },
            "session_state": {
                "tags": list(session.state.tags),
                "phase": session.state.phase,
            },
            "session_attributes": {
                key: value
                for key, value in attr_map.items()
                if key in {"world_tension", "weather_state", "active_objective"}
            },
        }

    def _build_recent_turn_summary(
        self,
        *,
        messages: list[ChatMessage],
        previous_rounds: int,
        current_player_name: str | None,
    ) -> list[dict[str, object]]:
        summary: list[dict[str, object]] = []
        history = self._build_history_dialogue(
            messages=messages,
            previous_rounds=previous_rounds,
            current_player_name=current_player_name,
        )
        for item in history[-4:]:
            role = str(item.get("role") or "").strip()
            speaker = str(item.get("speaker") or "").strip()
            if not role:
                continue
            summary.append(
                {
                    "role": role,
                    "speaker": speaker,
                }
            )
        return summary

    def _build_visual_capabilities(
        self,
        world_profile: WorldDefinition | None,
        *,
        director_config: dict[str, object] | None = None,
        session: SessionSnapshot | None = None,
        character_profiles: list[CharacterDefinition] | None = None,
    ) -> dict[str, object]:
        ui_theme = world_profile.ui_theme_config if world_profile else {}
        allowed_tool_ids = director_config.get("allowed_mcp_tool_ids", []) if director_config is not None else []
        image_generation_available = (
            isinstance(allowed_tool_ids, list)
            and "mcp-tool-image-generation" in {str(item).strip() for item in allowed_tool_ids if str(item).strip()}
        )
        scene_backgrounds = ui_theme.get("local_scene_backgrounds")
        local_scene_names = (
            [str(name).strip() for name in scene_backgrounds.keys() if str(name).strip()]
            if isinstance(scene_backgrounds, dict)
            else []
        )
        local_background_assets = ui_theme.get("local_background_assets")
        background_options: list[dict[str, object]] = []
        if isinstance(scene_backgrounds, dict):
            for scene_name, assets in scene_backgrounds.items():
                if not isinstance(assets, list):
                    continue
                background_options.extend(
                    {
                        "name": str(scene_name).strip(),
                        "path": str(asset).strip(),
                        "scope": "scene",
                    }
                    for asset in assets
                    if str(scene_name).strip() and str(asset).strip()
                )
        if isinstance(local_background_assets, list):
            background_options.extend(
                {
                    "name": str(asset).strip(),
                    "path": str(asset).strip(),
                    "scope": "global",
                }
                for asset in local_background_assets
                if str(asset).strip()
            )
        visible_names = set(session.visible_characters if session is not None else [])
        present_names = set(session.scene.present_characters if session is not None else [])
        portrait_options: list[dict[str, object]] = []
        for character in character_profiles or []:
            character_name = character.name.strip()
            if not character_name:
                continue
            for asset in character.portrait_assets:
                asset_path = str(asset).strip()
                if not asset_path:
                    continue
                portrait_options.append(
                    {
                        "character_name": character_name,
                        "name": asset_path,
                        "path": asset_path,
                        "visible": character_name in visible_names,
                        "present": character_name in present_names,
                    }
                )
        return {
            "background_source_mode": str(ui_theme.get("background_source_mode") or "local-first"),
            "portrait_source_mode": str(
                ui_theme.get("portrait_source_mode")
                or ui_theme.get("background_source_mode")
                or "local-first"
            ),
            "global_background_pool_count": len(local_background_assets) if isinstance(local_background_assets, list) else 0,
            "scene_background_pool_names": local_scene_names,
            "background_options": background_options[:24],
            "portrait_options": portrait_options[:48],
            "image_generation_tool": {
                "available": image_generation_available,
                "tool_id": "mcp-tool-image-generation" if image_generation_available else None,
                "request_fields": [
                    "background_generation_prompt",
                    "character_visual_directives[].generation_prompt",
                ] if image_generation_available else [],
            },
        }

    def _build_director_tool_capabilities(self, *, director_config: dict[str, object]) -> list[dict[str, object]]:
        allowed_tool_ids = director_config.get("allowed_mcp_tool_ids", [])
        allowed = {str(item).strip() for item in allowed_tool_ids if str(item).strip()} if isinstance(allowed_tool_ids, list) else set()
        builtin_tool_ids = {
            "mcp-tool-list-scenes",
            "mcp-tool-list-characters",
            "mcp-tool-change-scene",
            "mcp-tool-switch-player-character",
        }
        tool_descriptions = self._load_mcp_tool_descriptions(allowed | builtin_tool_ids)

        tools = [
            {
                "tool_name": "list_scenes",
                "description": tool_descriptions.get("mcp-tool-list-scenes", ""),
                "arguments_schema": {"type": "object", "properties": {}},
            },
            {
                "tool_name": "list_characters",
                "description": tool_descriptions.get("mcp-tool-list-characters", ""),
                "arguments_schema": {"type": "object", "properties": {}},
            },
            {
                "tool_name": "change_scene",
                "description": tool_descriptions.get("mcp-tool-change-scene", ""),
                "arguments_schema": {
                    "type": "object",
                    "required": ["scene_name", "scene_description", "all_characters", "player_character"],
                    "properties": {
                        "scene_name": {"type": "string"},
                        "scene_description": {"type": "string"},
                        "new_characters": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": {"type": "string"},
                                    "role": {"type": "string"},
                                    "background_prompt": {"type": "string"},
                                },
                            },
                        },
                        "all_characters": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "player_character": {"type": "string"},
                        "background_generation_prompt": {"type": "string"},
                    },
                },
            },
            {
                "tool_name": "switch_player_character",
                "description": tool_descriptions.get("mcp-tool-switch-player-character", ""),
                "arguments_schema": {
                    "type": "object",
                    "required": ["target_character_name"],
                    "properties": {
                        "target_character_name": {"type": "string"},
                        "reason": {"type": "string"},
                        "visible_characters": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "scene_name": {"type": "string"},
                        "scene_background_hint": {"type": "string"},
                    },
                },
            },
        ]
        if "mcp-tool-image-generation" in allowed:
            tools.append(
                {
                    "tool_name": "generate_image",
                    "description": tool_descriptions.get("mcp-tool-image-generation", ""),
                    "arguments_schema": {
                        "type": "object",
                        "required": ["kind", "prompt"],
                        "properties": {
                            "kind": {"type": "string", "enum": ["background", "portrait"]},
                            "prompt": {"type": "string"},
                            "character_name": {"type": "string"},
                        },
                    },
                }
            )
        return tools

    def _load_mcp_tool_descriptions(self, tool_ids: set[str]) -> dict[str, str]:
        if not tool_ids:
            return {}
        placeholders = ", ".join("?" for _ in tool_ids)
        try:
            with sqlite3.connect(Settings().database_path) as connection:
                rows = connection.execute(
                    f"SELECT id, description FROM mcp_tools WHERE id IN ({placeholders})",
                    tuple(sorted(tool_ids)),
                ).fetchall()
        except sqlite3.Error:
            return {}
        return {
            str(row[0]): str(row[1] or "").strip()
            for row in rows
            if str(row[0] or "").strip() and str(row[1] or "").strip()
        }

    def _normalize_world_triggers(self, world_profile: WorldDefinition | None) -> list[str]:
        if world_profile is None:
            return []
        normalized: list[str] = []
        for item in world_profile.triggers:
            if isinstance(item, dict):
                value = str(item.get("description") or item.get("name") or "").strip()
            else:
                value = str(item or "").strip()
            if value:
                normalized.append(value)
        return normalized

    def _normalize_opening_messages(self, world_profile: WorldDefinition | None) -> list[dict[str, object]]:
        if world_profile is None:
            return []
        normalized: list[dict[str, object]] = []
        for message in world_profile.opening_messages:
            content = str(message.content or "").strip()
            if not content:
                continue
            payload: dict[str, object] = {
                "role": message.role,
                "content": content,
            }
            if message.speaker:
                payload["speaker"] = message.speaker
            normalized.append(payload)
        return normalized

    def _fallback_planned_speakers(
        self,
        *,
        session: SessionSnapshot,
        player_input: str,
    ) -> list[str]:
        visible_character_names = [
            name
            for name in session.visible_characters
            if name.strip() and name != session.player_character_name
        ]
        if not visible_character_names:
            return []
        if self._attribute_runtime is not None:
            selection = self._attribute_runtime.select_turn_speakers(
                session_id=session.id,
                visible_character_names=visible_character_names,
                player_input=player_input,
                max_speakers=min(3, len(visible_character_names)),
            )
            if selection.speakers:
                return selection.speakers
        return visible_character_names[: min(3, len(visible_character_names))]

    def _parse_generated_characters(
        self,
        raw_items: object,
        session: SessionSnapshot,
    ) -> list[GeneratedCharacterDraft]:
        if not isinstance(raw_items, list):
            return []

        parsed: list[GeneratedCharacterDraft] = []
        existing_names: set[str] = set()
        for item in raw_items:
            if not isinstance(item, dict):
                continue
            name = self._normalize_llm_text(item.get("name")) or self._normalize_llm_text(item.get("character_name"))
            if not name or name in existing_names or name in session.visible_characters:
                continue
            existing_names.add(name)
            attributes = item.get("attributes")
            role = self._normalize_llm_text(item.get("role")) or self._normalize_llm_text(item.get("identity"))
            background_prompt = (
                self._normalize_llm_text(item.get("background_prompt"))
                or self._normalize_llm_text(item.get("description"))
                or self._normalize_llm_text(item.get("profile"))
            )
            if not background_prompt:
                location = self._normalize_llm_text(item.get("initial_location"))
                background_prompt = " / ".join(part for part in [role, location] if part)
            parsed.append(
                GeneratedCharacterDraft(
                    name=name,
                    world_name=self._normalize_llm_text(item.get("world_name")) or session.world_name,
                    role=role,
                    background_prompt=background_prompt,
                    model=self._normalize_llm_text(item.get("model")),
                    memory_strategy=self._normalize_llm_text(item.get("memory_strategy")),
                    attributes=[value for value in (self._normalize_llm_text(attr) for attr in attributes) if value]
                    if isinstance(attributes, list)
                    else [],
                )
            )

        return parsed[:4]

    def _collect_generated_character_items(self, payload: dict[str, object]) -> list[object]:
        items: list[object] = []
        raw_top_level = payload.get("generated_characters")
        if isinstance(raw_top_level, list):
            items.extend(raw_top_level)

        raw_switch_proposal = payload.get("switch_character_proposal")
        if isinstance(raw_switch_proposal, dict):
            raw_nested = raw_switch_proposal.get("generated_characters")
            if isinstance(raw_nested, list):
                items.extend(raw_nested)

        return items

    def _parse_planned_speakers(
        self,
        raw_items: object,
        *,
        visible_character_names: list[str],
        fallback: list[str],
        player_character_name: str | None = None,
    ) -> list[str]:
        visible_name_set = {name for name in visible_character_names if name.strip() and name != player_character_name}
        if not isinstance(raw_items, list):
            return [name for name in fallback if name in visible_name_set]

        parsed: list[str] = []
        for item in raw_items:
            name = self._normalize_llm_text(item)
            if not name or name not in visible_name_set or name in parsed:
                continue
            parsed.append(name)

        if parsed:
            return parsed[:4]
        return [name for name in fallback if name in visible_name_set]

    def _parse_scene_visible_characters(
        self,
        raw_items: object,
        *,
        player_character_name: str | None,
        fallback: list[str] | None,
    ) -> list[str] | None:
        if raw_items is None:
            return list(fallback) if fallback is not None else None
        if not isinstance(raw_items, list):
            return list(fallback) if fallback is not None else None

        parsed: list[str] = []
        for item in raw_items:
            name = self._normalize_llm_text(item)
            if not name or name == player_character_name or name in parsed:
                continue
            parsed.append(name)
        return parsed

    def _parse_next_scene_name(
        self,
        raw_value: object,
        *,
        session: SessionSnapshot,
        next_location: str | None,
        fallback: str | None,
    ) -> str:
        candidate = self._normalize_llm_text(raw_value)
        if candidate:
            return candidate
        if next_location and next_location != session.location:
            return next_location
        return self._normalize_llm_text(fallback) or self._normalize_llm_text(session.scene.name) or session.location

    def _parse_next_scene_background_hint(
        self,
        raw_value: object,
        *,
        session: SessionSnapshot,
        next_scene_name: str,
        fallback: str | None,
    ) -> str | None:
        candidate = self._normalize_llm_text(raw_value)
        if candidate:
            return candidate
        if next_scene_name == session.scene.name:
            return self._normalize_llm_text(fallback) or self._normalize_llm_text(session.scene.background_hint) or None
        return None

    def _parse_generation_prompt(self, raw_value: object) -> str | None:
        prompt = self._normalize_llm_text(raw_value)
        return prompt or None

    def _parse_next_scene_tags(
        self,
        raw_value: object,
        *,
        session: SessionSnapshot,
        next_scene_name: str,
        fallback: list[str],
    ) -> list[str]:
        if isinstance(raw_value, list):
            parsed = [value for value in (self._normalize_llm_text(item) for item in raw_value) if value]
            return list(dict.fromkeys(parsed))
        if next_scene_name == session.scene.name:
            return list(dict.fromkeys([tag for tag in (fallback or session.scene.temporary_tags) if tag]))
        return []

    def _parse_character_visual_directives(
        self,
        raw_items: object,
    ) -> list[CharacterVisualDirective]:
        if not isinstance(raw_items, list):
            return []

        parsed: list[CharacterVisualDirective] = []
        seen: set[str] = set()
        for item in raw_items:
            if not isinstance(item, dict):
                continue
            character_name = self._normalize_llm_text(item.get("character_name"))
            if not character_name or character_name in seen:
                continue
            portrait_hint = self._normalize_llm_text(item.get("portrait_hint"))
            portrait_asset_name = self._normalize_llm_text(item.get("portrait_asset_name")) or None
            portrait_asset_path = self._normalize_llm_text(item.get("portrait_asset_path")) or None
            generation_prompt = self._parse_generation_prompt(item.get("generation_prompt"))
            if not portrait_hint and not portrait_asset_name and not portrait_asset_path and generation_prompt is None:
                continue
            seen.add(character_name)
            parsed.append(
                CharacterVisualDirective(
                    character_name=character_name,
                    portrait_hint=portrait_hint,
                    portrait_asset_name=portrait_asset_name,
                    portrait_asset_path=portrait_asset_path,
                    generation_prompt=generation_prompt,
                )
            )
        return parsed

    def build_decision_from_change_scene_tool(
        self,
        *,
        arguments: dict[str, object],
        session: SessionSnapshot,
        fallback: DirectorDecision,
    ) -> DirectorDecision:
        scene_name = self._normalize_llm_text(arguments.get("scene_name")) or fallback.next_scene_name or session.scene.name
        scene_description = (
            self._normalize_llm_text(arguments.get("scene_description"))
            or fallback.next_scene_background_hint
            or session.scene.background_hint
            or scene_name
        )
        player_character_name = self._normalize_llm_text(arguments.get("player_character")) or session.player_character_name
        all_characters = [
            name
            for name in (self._normalize_llm_text(item) for item in arguments.get("all_characters", []))
            if name
        ] if isinstance(arguments.get("all_characters"), list) else []
        if player_character_name and player_character_name not in all_characters:
            all_characters.append(player_character_name)

        generated_characters = self._parse_generated_characters(arguments.get("new_characters"), session)
        existing_generated_names = {item.name for item in generated_characters}
        if player_character_name and player_character_name not in session.visible_characters and player_character_name not in existing_generated_names:
            # The orchestrator will reuse an existing character if it exists, otherwise this draft creates the new player character.
            generated_characters.append(
                GeneratedCharacterDraft(
                    name=player_character_name,
                    world_name=session.world_name,
                    role="鐜╁鎿嶆帶浜虹墿",
                    background_prompt=scene_description,
                    model="",
                    memory_strategy="",
                    attributes=[],
                )
            )
        for name in all_characters:
            if name == player_character_name or name in session.visible_characters or name in {item.name for item in generated_characters}:
                continue
            generated_characters.append(
                GeneratedCharacterDraft(
                    name=name,
                    world_name=session.world_name,
                    role="鍦烘櫙浜虹墿",
                    background_prompt=scene_description,
                    model="",
                    memory_strategy="",
                    attributes=[],
                )
            )

        visible_characters = [
            name
            for name in dict.fromkeys(all_characters)
            if name and name != player_character_name
        ]
        scene_change = SceneChangeToolResult(
            scene_name=scene_name,
            scene_description=scene_description,
            new_characters=generated_characters,
            all_characters=list(dict.fromkeys(all_characters)),
            player_character_name=player_character_name,
            background_generation_prompt=self._parse_generation_prompt(arguments.get("background_generation_prompt")),
        )
        return DirectorDecision(
            world_phase=fallback.world_phase,
            next_location=scene_name,
            next_scene_name=scene_name,
            next_scene_background_hint=scene_description,
            background_generation_prompt=scene_change.background_generation_prompt or fallback.background_generation_prompt,
            next_scene_tags=list(fallback.next_scene_tags),
            next_time_label=fallback.next_time_label,
            generated_characters=generated_characters[:6],
            character_visual_directives=list(fallback.character_visual_directives),
            scene_change=scene_change,
            scene_visible_characters=visible_characters,
            planned_speakers=visible_characters[:4],
            switch_character_proposal=None,
        )

    def build_decision_from_switch_player_character_tool(
        self,
        *,
        arguments: dict[str, object],
        session: SessionSnapshot,
        fallback: DirectorDecision,
    ) -> DirectorDecision:
        target_character_name = self._normalize_llm_text(arguments.get("target_character_name"))
        if not target_character_name:
            return fallback
        scene_name = self._normalize_llm_text(arguments.get("scene_name")) or session.scene.name or session.location
        scene_description = (
            self._normalize_llm_text(arguments.get("scene_background_hint"))
            or fallback.next_scene_background_hint
            or session.scene.background_hint
            or scene_name
        )
        visible_characters = [
            name
            for name in (
                self._normalize_llm_text(item)
                for item in arguments.get("visible_characters", [])
            )
            if name and name != target_character_name
        ] if isinstance(arguments.get("visible_characters"), list) else []
        if not visible_characters:
            visible_characters = [
                name
                for name in list(session.visible_characters)
                if name and name != target_character_name
            ]
            if session.player_character_name and session.player_character_name != target_character_name:
                visible_characters.append(session.player_character_name)
        visible_characters = list(dict.fromkeys(visible_characters))
        all_characters = list(dict.fromkeys([target_character_name, *visible_characters]))
        scene_change = SceneChangeToolResult(
            scene_name=scene_name,
            scene_description=scene_description,
            new_characters=[],
            all_characters=all_characters,
            player_character_name=target_character_name,
            background_generation_prompt=None,
        )
        return DirectorDecision(
            world_phase=fallback.world_phase,
            next_location=session.location,
            next_scene_name=scene_name,
            next_scene_background_hint=scene_description,
            background_generation_prompt=fallback.background_generation_prompt,
            next_scene_tags=list(fallback.next_scene_tags),
            next_time_label=fallback.next_time_label,
            generated_characters=list(fallback.generated_characters),
            character_visual_directives=list(fallback.character_visual_directives),
            scene_change=scene_change,
            scene_visible_characters=visible_characters,
            planned_speakers=visible_characters[:4],
            switch_character_proposal=None,
        )

    def _resolve_next_scene_name(
        self,
        *,
        session: SessionSnapshot,
        player_input: str,
        next_location: str,
        allow_scene_transition: bool,
    ) -> str:
        return session.scene.name

    def _resolve_world_phase(self, world_tension: float) -> str:
        if world_tension >= 70:
            return "crisis"
        if world_tension >= 35:
            return "escalation"
        return "opening"

    def _resolve_next_location(
        self,
        player_input: str,
        current_location: str,
        map_nodes: list[str],
        world_phase: str,
        allow_scene_transition: bool,
    ) -> str:
        return current_location

    def _resolve_generated_characters(
        self,
        session: SessionSnapshot,
        world_phase: str,
        world_tension: float,
        weather_state: str,
        allow_npc_spawn: bool,
    ) -> list[GeneratedCharacterDraft]:
        return []

    def _resolve_planned_speakers(
        self,
        *,
        session: SessionSnapshot,
        visible_character_names: list[str],
        player_input: str,
        world_phase: str,
    ) -> tuple[list[str], list[str]]:
        player_char = session.player_character_name
        visible = list(dict.fromkeys(name for name in visible_character_names if name.strip() and name != player_char))
        if not visible:
            return [], ["WorldDirector speaker_plan=none"]

        speaker_limit = self._resolve_speaker_limit(
            player_input=player_input,
            world_phase=world_phase,
            visible_character_names=visible,
            player_character_name=player_char,
        )
        if self._attribute_runtime is not None:
            selection = self._attribute_runtime.select_turn_speakers(
                session_id=session.id,
                visible_character_names=visible,
                player_input=player_input,
                max_speakers=speaker_limit,
            )
            if selection.speakers:
                return selection.speakers, selection.debug_lines

        mentioned_names = self._mentioned_character_names(
            player_input=player_input,
            visible_character_names=visible,
            player_character_name=player_char,
        )
        recent_speakers = [
            message.speaker.strip()
            for message in reversed(session.messages)
            if message.role == "agent" and message.speaker and message.speaker.strip()
        ]
        recent_index = {name: index for index, name in enumerate(recent_speakers)}
        remaining = sorted(
            [name for name in visible if name not in mentioned_names],
            key=lambda name: (recent_index.get(name, 99), visible.index(name)),
        )
        selected = list(mentioned_names)
        for name in remaining:
            if len(selected) >= speaker_limit:
                break
            selected.append(name)

        if not selected:
            selected = [visible[0]]
        return selected, [f"WorldDirector heuristic_speakers={selected}"]

    def _resolve_speaker_limit(
        self,
        *,
        player_input: str,
        world_phase: str,
        visible_character_names: list[str],
        player_character_name: str | None = None,
    ) -> int:
        visible_count = len(visible_character_names)
        if visible_count <= 1:
            return visible_count

        mentioned_count = len(
            self._mentioned_character_names(
                player_input=player_input,
                visible_character_names=visible_character_names,
                player_character_name=player_character_name,
            )
        )
        group_prompt = self._is_group_prompt(player_input)
        limit = 2
        if group_prompt or mentioned_count >= 2 or world_phase in {"escalation", "crisis"}:
            limit = 3
        return min(visible_count, max(limit, mentioned_count, 1))

    def _merge_visible_characters(
        self,
        visible_character_names: list[str],
        generated_characters: list[GeneratedCharacterDraft],
    ) -> list[str]:
        merged = list(dict.fromkeys(name for name in visible_character_names if name.strip()))
        for draft in generated_characters:
            if draft.name and draft.name not in merged:
                merged.append(draft.name)
        return merged

    def _session_map_nodes(self, session: SessionSnapshot) -> list[str]:
        labels = [node.label.strip() for node in session.map_graph_nodes if node.label.strip()]
        if labels:
            return labels
        if session.location.strip():
            return [session.location.strip()]
        return []

    def build_runtime_prompt_call(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        fallback: DirectorDecision,
        director_config: dict[str, object],
        character_profiles: list[CharacterDefinition] | None = None,
        tool_loop_messages: list[dict[str, object]] | None = None,
        stage: str = "普通回合",
    ) -> dict[str, object]:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        payload = json.loads(
            self._build_user_prompt(
                session=session,
                world_profile=world_profile,
                player_input=player_input,
                attr_map=attr_map,
                fallback=fallback,
                director_config=director_config,
                character_profiles=character_profiles,
            )
        )
        variables = self._template_variables(
            session=session,
            world_profile=world_profile,
            char_name="世界主控",
        )
        modules: list[PromptModule] = [
            *self._prompt_runtime.prompt_modules_for_presets(
                director_config=director_config,
                target="director",
                variables=variables,
            )
        ]
        director_prompt = self._prompt_runtime.render_template(
            str(director_config.get("world_director_prompt") or ""),
            variables,
        )
        if director_prompt.strip():
            modules.append(
                PromptModule(
                    name="世界主控提示词",
                    source="世界设计 / 世界主控提示词",
                    content=director_prompt,
                    editable=True,
                )
            )
        modules.extend(
            [
                PromptModule("客观世界资料", "世界配置与会话", self._prompt_runtime.objective_json(payload.get("basic_setting", {})), False),
                PromptModule("当前状态", "运行时状态", self._prompt_runtime.objective_json(payload.get("current_state", {})), False),
                PromptModule("聊天记录", "会话记录", self._prompt_runtime.objective_json(payload.get("chat_history", [])), False),
                PromptModule("工具资料", "系统工具注册表", self._prompt_runtime.objective_json(payload.get("tool_data", {})), False),
            ]
        )
        if tool_loop_messages:
            modules.append(
                PromptModule(
                    "工具执行结果",
                    "工具运行结果",
                    self._prompt_runtime.objective_json(tool_loop_messages),
                    False,
                )
            )
        return self._prompt_runtime.build_prompt_call(
            recipient_type="director",
            recipient_name="世界主控",
            stage=stage,
            purpose="决定世界状态、工具调用和发言顺序",
            modules=modules,
            raw_debug={"turn_payload": payload, "tool_loop_messages": list(tool_loop_messages or [])},
        )

    def apply_return_processing(self, *, director_config: dict[str, object], raw_text: str):
        return self._prompt_runtime.apply_return_rules(
            director_config=director_config,
            target="director",
            raw_text=raw_text,
        )

    def attach_prompt_call_result(
        self,
        prompt_call: dict[str, object],
        *,
        raw_model_return: str | None,
        return_processing,
        processed_model_return: object,
        written_result: object,
    ) -> dict[str, object]:
        return self._prompt_runtime.attach_result(
            prompt_call,
            raw_model_return=raw_model_return,
            return_processing=return_processing,
            processed_model_return=processed_model_return,
            written_result=written_result,
        )

    def _build_system_prompt(self, director_config: dict[str, object]) -> str:
        return str(director_config.get("world_director_prompt") or "").strip()

    def _build_user_prompt(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        player_input: str,
        attr_map: dict[str, object],
        fallback: DirectorDecision,
        director_config: dict[str, object],
        character_profiles: list[CharacterDefinition] | None = None,
    ) -> str:
        history_rounds = self._history_dialogue_rounds(director_config)
        visual_capabilities = self._build_visual_capabilities(
            world_profile,
            director_config=director_config,
            session=session,
            character_profiles=character_profiles,
        )
        return json.dumps(
            {
                "basic_setting": self._build_basic_setting(session=session, world_profile=world_profile),
                "chat_history": self._build_history_dialogue(
                    messages=session.messages,
                    previous_rounds=history_rounds,
                    current_player_name=session.player_character_name,
                ),
                "current_state": self._build_current_state_minimal(
                    session=session,
                    player_input=player_input,
                    attr_map=attr_map,
                ),
                "tool_data": {
                    "tool_protocol": self._build_tool_protocol(),
                    "available_tools": self._build_director_tool_capabilities(director_config=director_config),
                    "visual_capabilities": visual_capabilities,
                },
            },
            ensure_ascii=False,
            indent=2,
        )

    def _template_variables(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        char_name: str,
    ) -> dict[str, str]:
        return {
            "user": str(session.player_character_name or "").strip() or "鐜╁",
            "char": char_name,
            "world": str(world_profile.name if world_profile else session.world_name),
            "scene": str(session.scene.name or session.location or ""),
            "time": str(session.time_label or ""),
        }

    def _build_tool_protocol(self) -> dict[str, object]:
        return {
            "call_shape": {
                "tool_calls": [
                    {
                        "id": "call-1",
                        "tool_name": "list_scenes | list_characters | change_scene | switch_player_character | generate_image",
                        "arguments": {},
                    }
                ]
            },
            "rules": [
                "需要查询场景、查询角色、切换场景、切换玩家操控角色、生成图片时，先返回 tool_calls，不要假装已经执行工具。",
                "拿到 tool_results 后，再返回最终世界状态；最终状态不要再包含已完成的 tool_calls。",
                "玩家明确要求去某地、转世、附身、换主角、引入某个角色或生成视觉资产时，优先调用对应工具。",
                "目标场景或目标角色不在当前状态中时，先调用 list_scenes 或 list_characters；若需要创建/进入新场景，用 change_scene。",
            ],
            "examples": [
                {
                    "when": "玩家要求转世为某个角色，但当前角色列表未确认目标是否存在",
                    "return": {
                        "tool_calls": [
                            {"id": "call-1", "tool_name": "list_characters", "arguments": {}}
                        ]
                    },
                },
                {
                    "when": "玩家要求去一个新地点或剧情自然进入新地点",
                    "return": {
                        "tool_calls": [
                            {"id": "call-1", "tool_name": "list_scenes", "arguments": {}}
                        ]
                    },
                },
            ],
        }

    def _resolve_director_config(self, world_profile: WorldDefinition | None) -> dict[str, object]:
        raw = world_profile.director_config if world_profile else {}
        return normalize_world_director_config(raw)

    def _mentioned_character_names(
        self,
        *,
        player_input: str,
        visible_character_names: list[str],
        player_character_name: str | None = None,
    ) -> list[str]:
        if not player_input:
            return []

        named_matches: list[tuple[int, str]] = []
        for character_name in visible_character_names:
            if character_name == player_character_name:
                continue
            position = player_input.find(character_name)
            if position == -1:
                continue
            named_matches.append((position, character_name))
        named_matches.sort(key=lambda item: item[0])
        return list(dict.fromkeys(name for _, name in named_matches))

    def _is_group_prompt(self, player_input: str) -> bool:
        markers = ["你们", "大家", "各位", "一起", "分别", "轮流", "都说", "每个人", "挨个"]
        return any(marker in player_input for marker in markers)

    def _find_current_map_index(self, current_location: str, map_nodes: list[str]) -> int | None:
        if current_location in map_nodes:
            return map_nodes.index(current_location)

        for index, node in enumerate(map_nodes):
            if node and node in current_location:
                return index
        return None

    def _as_number(self, value: object, fallback: float) -> float:
        if isinstance(value, (int, float)):
            return float(value)
        return fallback

    def _parse_switch_character_proposal(
        self,
        raw: object,
        *,
        visible_character_names: list[str],
        player_character_name: str | None,
    ) -> SwitchCharacterProposal | None:
        if not isinstance(raw, dict):
            return None
        target_name = self._normalize_llm_text(raw.get("target_character_name"))
        if not target_name:
            return None
        if target_name == player_character_name:
            return None
        reason = self._normalize_llm_text(raw.get("reason")) or target_name
        return SwitchCharacterProposal(target_character_name=target_name, reason=reason)

    def _parse_switch_character_proposal_v2(
        self,
        raw: object,
        *,
        visible_character_names: list[str],
        player_character_name: str | None,
    ) -> SwitchCharacterProposal | None:
        if isinstance(raw, str):
            target_name = self._normalize_llm_text(raw)
            if not target_name or target_name == player_character_name:
                return None
            return SwitchCharacterProposal(
                target_character_name=target_name,
                reason=target_name,
            )
        if not isinstance(raw, dict):
            return None
        target_name = self._normalize_llm_text(raw.get("target_character_name"))
        if not target_name or target_name == player_character_name:
            return None
        reason = self._normalize_llm_text(raw.get("reason")) or target_name
        next_location = self._normalize_llm_text(raw.get("next_location")) or None
        scene_name = self._normalize_llm_text(raw.get("scene_name")) or None
        scene_background_hint = self._normalize_llm_text(raw.get("scene_background_hint")) or None
        scene_tags = [
            value
            for item in raw.get("scene_tags", [])
            for value in [self._normalize_llm_text(item)]
            if value
        ] if isinstance(raw.get("scene_tags"), list) else []
        visible_characters = [
            name
            for name in (
                self._normalize_llm_text(item)
                for item in raw.get("visible_characters", [])
            )
            if name and name != player_character_name and name != target_name
        ] if isinstance(raw.get("visible_characters"), list) else []
        return SwitchCharacterProposal(
            target_character_name=target_name,
            reason=reason,
            next_location=next_location,
            scene_name=scene_name,
            scene_background_hint=scene_background_hint,
            scene_tags=list(dict.fromkeys(scene_tags)),
            visible_characters=list(dict.fromkeys(visible_characters)),
        )

    def _normalize_llm_text(self, raw_value: object) -> str:
        if raw_value is None:
            return ""
        value = str(raw_value).strip()
        if value.lower() in {"none", "null", "undefined"}:
            return ""
        return value

    def _history_dialogue_rounds(self, director_config: dict[str, object]) -> int:
        raw_value = director_config.get("history_dialogue_rounds", 6)
        if isinstance(raw_value, int):
            return max(0, min(raw_value, 20))
        return 6

    def _build_time_context(self, world_profile: WorldDefinition | None) -> dict[str, object]:
        config = self._normalize_time_config(world_profile.time_config if world_profile else {})
        return {
            "mode": config["mode"],
            "start_label": config["start_label"],
            "start_time": config["start_time"],
            "slots": config["slots"],
        }

    def _parse_next_time_label(
        self,
        raw_value: object,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        fallback: str | None,
    ) -> str | None:
        if raw_value is None:
            return fallback

        candidate = str(raw_value).strip()
        if not candidate:
            return fallback

        config = self._normalize_time_config(world_profile.time_config if world_profile else {})
        if config["mode"] == "24h":
            return candidate if self._parse_clock_minutes(candidate) is not None else fallback

        labels = [str(item).strip() for item in config["labels"] if str(item).strip()]
        if not labels:
            return candidate
        if candidate == session.time_label or candidate in labels:
            return candidate
        return fallback

    def _normalize_time_config(self, raw: dict[str, object] | None) -> dict[str, object]:
        base: dict[str, object] = {
            "mode": "labels",
            "labels": ["娓呮櫒", "涓崍", "鏅氫笂"],
            "slots": [
                {"label": "娓呮櫒", "clock": "06:00"},
                {"label": "涓崍", "clock": "12:00"},
                {"label": "鏅氫笂", "clock": "20:00"},
            ],
            "start_label": "娓呮櫒",
            "start_time": "08:00",
        }
        if not isinstance(raw, dict):
            return base

        slot_labels: list[str] = []
        slots = raw.get("slots")
        if isinstance(slots, list):
            normalized_slots: list[dict[str, str]] = []
            for item in slots:
                if not isinstance(item, dict):
                    continue
                label = str(item.get("label", "")).strip()
                clock = str(item.get("clock", "")).strip()
                if not label and not clock:
                    continue
                normalized_slots.append({"label": label, "clock": clock})

            if normalized_slots:
                base["slots"] = normalized_slots
                slot_labels = [item["label"] for item in normalized_slots if item["label"]]
                if slot_labels:
                    base["labels"] = slot_labels
                    base["start_label"] = slot_labels[0]

        labels = raw.get("labels")
        if isinstance(labels, list) and not slot_labels:
            normalized_labels = [str(item).strip() for item in labels if str(item).strip()]
            if normalized_labels:
                base["labels"] = normalized_labels
                base["slots"] = [{"label": item, "clock": ""} for item in normalized_labels]
                base["start_label"] = normalized_labels[0]

        if raw.get("mode") == "24h":
            base["mode"] = "24h"

        start_label = raw.get("start_label")
        if isinstance(start_label, str) and start_label.strip():
            base["start_label"] = start_label.strip()

        start_time = raw.get("start_time")
        if isinstance(start_time, str) and self._parse_clock_minutes(start_time) is not None:
            base["start_time"] = start_time

        return base

    def _parse_clock_minutes(self, value: str) -> int | None:
        parts = value.split(":", 1)
        if len(parts) != 2:
            return None
        try:
            hour = int(parts[0])
            minute = int(parts[1])
        except ValueError:
            return None
        if hour < 0 or hour > 23 or minute < 0 or minute > 59:
            return None
        return hour * 60 + minute

    def _build_history_dialogue(
        self,
        *,
        messages: list[object],
        previous_rounds: int,
        current_player_name: str | None,
    ) -> list[dict[str, object]]:
        if previous_rounds <= 0:
            return []

        normalized_messages = self._annotate_player_history_speakers(
            messages=messages,
            current_player_name=current_player_name,
        )
        selected: list[object] = []
        player_messages_seen = 0
        for message in reversed(normalized_messages):
            role = getattr(message, "role", None)
            content = str(getattr(message, "content", "") or "").strip()
            if not role or not content:
                continue
            selected.append(message)
            if role == "player":
                player_messages_seen += 1
                if player_messages_seen >= previous_rounds:
                    break

        selected.reverse()
        history: list[dict[str, object]] = []
        for message in selected:
            role = str(getattr(message, "role", "") or "").strip()
            content = str(getattr(message, "content", "") or "").strip()
            speaker = getattr(message, "speaker", None)
            metadata = getattr(message, "metadata", None)
            if not role or not content:
                continue
            payload: dict[str, object] = {
                "role": role,
                "content": content,
            }
            if speaker:
                payload["speaker"] = speaker
            if isinstance(metadata, dict) and metadata:
                payload["metadata"] = metadata
            history.append(payload)
        return history

    def _annotate_player_history_speakers(
        self,
        *,
        messages: list[object],
        current_player_name: str | None,
    ) -> list[object]:
        has_switch_marker = any(
            getattr(message, "role", None) == "system"
            and self.PLAYER_VIEW_SWITCH_PATTERN.match(str(getattr(message, "content", "") or "").strip())
            for message in messages
        )
        resolved_player_speaker = (
            str(current_player_name or "").strip() or "鐜╁"
            if current_player_name and not has_switch_marker
            else "鐜╁"
        )
        annotated: list[object] = []
        for message in messages:
            role = str(getattr(message, "role", "") or "").strip()
            content = str(getattr(message, "content", "") or "").strip()
            if role == "system":
                match = self.PLAYER_VIEW_SWITCH_PATTERN.match(content)
                if match and match.group(1).strip():
                    resolved_player_speaker = match.group(1).strip()
                annotated.append(message)
                continue
            if role == "player":
                raw_speaker = str(getattr(message, "speaker", "") or "").strip()
                speaker = raw_speaker if raw_speaker and raw_speaker != "player" else resolved_player_speaker
                metadata = getattr(message, "metadata", None)
                annotated.append(
                    type(message)(
                        role=role,
                        content=content,
                        speaker=speaker,
                        metadata=metadata,
                    )
                )
                continue
            annotated.append(message)
        return annotated

    _parse_switch_character_proposal = _parse_switch_character_proposal_v2
