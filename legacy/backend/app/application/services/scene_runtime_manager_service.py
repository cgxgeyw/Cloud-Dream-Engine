from dataclasses import dataclass, field
import re

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.session import SessionSnapshot


@dataclass(frozen=True)
class SceneRuntimeResult:
    scene: SceneRuntime
    system_messages: list[str] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class SceneRuntimeManagerService:
    def refresh_scene(
        self,
        session: SessionSnapshot,
        director_decision: DirectorDecision,
        visible_character_names: list[str],
        session_attributes: list[RuntimeAttributeItem],
    ) -> SceneRuntimeResult:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        weather_state = str(attr_map.get("weather_state", "clear"))
        next_location = director_decision.next_location or session.location
        next_scene_name = (
            str(director_decision.next_scene_name or "").strip()
            or (next_location if next_location != session.location else session.scene.name)
            or next_location
        )
        background_hint = self._resolve_background_hint(
            session=session,
            scene_name=next_scene_name,
            weather_state=weather_state,
            override=director_decision.next_scene_background_hint,
        )
        temporary_tags = self._resolve_scene_tags(
            session=session,
            weather_state=weather_state,
            world_phase=director_decision.world_phase,
            scene_name=next_scene_name,
            explicit_tags=director_decision.next_scene_tags,
        )
        changed = (
            next_location != session.location
            or next_scene_name != session.scene.name
            or background_hint != session.scene.background_hint
        )

        scene = SceneRuntime(
            scene_id=self._slugify(next_scene_name),
            name=next_scene_name,
            background_hint=background_hint,
            temporary_tags=temporary_tags,
            present_characters=self._build_present_characters(
                visible_character_names=visible_character_names,
                player_character_name=session.player_character_name,
            ),
        )

        system_messages: list[str] = []
        if changed:
            system_messages.append(f"场景运行时：已装载 {next_scene_name}")

        debug_lines = [
            f"SceneRuntime location={next_location}",
            f"SceneRuntime scene_id={scene.scene_id}",
            f"SceneRuntime scene_name={scene.name}",
            f"SceneRuntime background={scene.background_hint}",
            "SceneRuntime tags=" + ", ".join(scene.temporary_tags),
            "SceneRuntime present=" + ", ".join(scene.present_characters),
        ]

        return SceneRuntimeResult(
            scene=scene,
            system_messages=system_messages,
            debug_lines=debug_lines,
        )

    def _build_background_hint(self, scene_name: str, weather_state: str) -> str:
        slug = self._slugify(scene_name)
        return f"{slug}:{weather_state}"

    def _resolve_background_hint(
        self,
        *,
        session: SessionSnapshot,
        scene_name: str,
        weather_state: str,
        override: str | None,
    ) -> str:
        if override and override.strip():
            return override.strip()
        if scene_name == session.scene.name and session.scene.background_hint.strip():
            current_hint = session.scene.background_hint.strip()
            if current_hint.startswith(f"{self._slugify(scene_name)}:"):
                return self._build_background_hint(scene_name=scene_name, weather_state=weather_state)
            return current_hint
        return self._build_background_hint(scene_name=scene_name, weather_state=weather_state)

    def _resolve_scene_tags(
        self,
        *,
        session: SessionSnapshot,
        weather_state: str,
        world_phase: str,
        scene_name: str,
        explicit_tags: list[str],
    ) -> list[str]:
        scene_changed = scene_name != session.scene.name
        if explicit_tags:
            base_tags = [tag for tag in explicit_tags if tag]
        elif scene_changed:
            base_tags = []
        else:
            base_tags = [
                tag
                for tag in session.scene.temporary_tags
                if tag and not tag.startswith("phase:") and tag != "scene-entered"
            ]

        next_tags = [*base_tags, weather_state]
        if scene_changed:
            next_tags.append("scene-entered")
        if world_phase:
            next_tags.append(f"phase:{world_phase}")
        return list(dict.fromkeys(next_tags))

    def _slugify(self, value: str) -> str:
        slug = re.sub(r"[^a-zA-Z0-9\u4e00-\u9fff]+", "-", value).strip("-").lower()
        return slug or "scene-default"

    def _build_present_characters(
        self,
        *,
        visible_character_names: list[str],
        player_character_name: str | None,
    ) -> list[str]:
        names = [name.strip() for name in visible_character_names if name.strip()]
        if player_character_name and player_character_name.strip():
            names.append(player_character_name.strip())
        return list(dict.fromkeys(names))
