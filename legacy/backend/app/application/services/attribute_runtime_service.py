from dataclasses import dataclass, field
from math import inf

from backend.app.domain.models.attribute import AttributeSchema, AttributeValue
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.repositories.attribute import AttributeRepository
from backend.app.domain.repositories.catalog import CatalogRepository
from backend.app.domain.repositories.session import SessionRepository


@dataclass(frozen=True)
class RuntimeAttributeItem:
    schema: AttributeSchema
    value: AttributeValue


@dataclass(frozen=True)
class RuntimeAttributeGroup:
    owner_type: str
    owner_id: str
    owner_label: str
    items: list[RuntimeAttributeItem] = field(default_factory=list)


@dataclass(frozen=True)
class SpeakerSelectionResult:
    speaker: str
    debug_lines: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class SpeakerTurnSelectionResult:
    speakers: list[str] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class AttributeRuntimeService:
    def __init__(
        self,
        attribute_repository: AttributeRepository,
        catalog_repository: CatalogRepository,
        session_repository: SessionRepository,
    ) -> None:
        self._attribute_repository = attribute_repository
        self._catalog_repository = catalog_repository
        self._session_repository = session_repository

    def list_game_visible_attributes(self, session_id: str) -> tuple[list[RuntimeAttributeItem], list[RuntimeAttributeGroup]]:
        return self.list_player_visible_attributes(session_id=session_id)

    def list_player_visible_attributes(self, session_id: str) -> tuple[list[RuntimeAttributeItem], list[RuntimeAttributeGroup]]:
        session = self._session_repository.get_session(session_id)
        if session is None:
            return [], []

        self._ensure_projected_attributes(session)
        schemas = {schema.id: schema for schema in self._attribute_repository.list_schemas()}
        character_name_map = {item.id: item.name for item in self._catalog_repository.list_characters()}

        session_items: list[RuntimeAttributeItem] = []
        character_groups: dict[str, list[RuntimeAttributeItem]] = {}

        for value in self._list_session_runtime_values(session_id):
            schema = schemas.get(value.schema_id)
            if schema is None:
                continue

            if value.owner_type == "session":
                if self._is_player_visible(schema):
                    session_items.append(RuntimeAttributeItem(schema=schema, value=value))
                continue

            if value.owner_type == "session_character":
                if not self._is_player_visible(schema):
                    continue
                character_groups.setdefault(value.owner_id, []).append(RuntimeAttributeItem(schema=schema, value=value))

        return session_items, self._build_character_groups(character_groups, character_name_map)

    def list_debug_attributes(self, session_id: str) -> tuple[list[RuntimeAttributeItem], list[RuntimeAttributeGroup]]:
        session = self._session_repository.get_session(session_id)
        if session is None:
            return [], []

        self._ensure_projected_attributes(session)
        schemas = {schema.id: schema for schema in self._attribute_repository.list_schemas()}
        character_name_map = {item.id: item.name for item in self._catalog_repository.list_characters()}

        session_items: list[RuntimeAttributeItem] = []
        character_groups: dict[str, list[RuntimeAttributeItem]] = {}

        for value in self._list_session_runtime_values(session_id):
            schema = schemas.get(value.schema_id)
            if schema is None:
                continue

            item = RuntimeAttributeItem(schema=schema, value=value)
            if value.owner_type == "session":
                session_items.append(item)
            elif value.owner_type == "session_character":
                character_groups.setdefault(value.owner_id, []).append(item)

        return session_items, self._build_character_groups(character_groups, character_name_map)

    def list_character_visible_attributes(self, session_id: str, character_id: str | None) -> list[RuntimeAttributeItem]:
        session = self._session_repository.get_session(session_id)
        if session is None:
            return []

        self._ensure_projected_attributes(session)
        schemas = {schema.id: schema for schema in self._attribute_repository.list_schemas()}
        items: list[RuntimeAttributeItem] = []

        for value in self._list_session_runtime_values(session_id):
            schema = schemas.get(value.schema_id)
            if schema is None:
                continue

            if value.owner_type == "session":
                if self._is_character_visible_shared(schema):
                    items.append(RuntimeAttributeItem(schema=schema, value=value))
                continue

            if value.owner_type != "session_character" or character_id is None:
                continue

            _, owner_character_id = value.owner_id.split(":", 1)
            if owner_character_id == character_id and schema.access_policy.get("agent_self_read", False):
                items.append(RuntimeAttributeItem(schema=schema, value=value))
            elif owner_character_id != character_id and schema.access_policy.get("agent_other_read", False):
                items.append(RuntimeAttributeItem(schema=schema, value=value))

        return items

    def select_next_speaker(
        self,
        session_id: str,
        visible_character_names: list[str],
        player_input: str | None = None,
    ) -> SpeakerSelectionResult:
        session = self._session_repository.get_session(session_id)
        if session is None or not visible_character_names:
            return SpeakerSelectionResult(speaker="系统", debug_lines=["发言排序：无可见角色，回退到系统。"])

        self._ensure_projected_attributes(session)
        ranked, debug_lines = self._rank_visible_speakers(
            session=session,
            session_id=session_id,
            visible_character_names=visible_character_names,
            player_input=player_input,
        )
        top_name = ranked[0][0] if ranked else visible_character_names[0]
        return SpeakerSelectionResult(speaker=top_name, debug_lines=debug_lines)

    def select_turn_speakers(
        self,
        session_id: str,
        visible_character_names: list[str],
        player_input: str | None = None,
        max_speakers: int = 3,
        min_speakers: int = 1,
    ) -> SpeakerTurnSelectionResult:
        session = self._session_repository.get_session(session_id)
        if session is None or not visible_character_names:
            return SpeakerTurnSelectionResult(
                speakers=[],
                debug_lines=["发言规划：无可见角色，本回合无角色发言。"],
            )

        self._ensure_projected_attributes(session)
        ranked, debug_lines = self._rank_visible_speakers(
            session=session,
            session_id=session_id,
            visible_character_names=visible_character_names,
            player_input=player_input,
        )
        selectable = [(name, score) for name, score, _ in ranked if score > -inf]
        if not selectable:
            return SpeakerTurnSelectionResult(
                speakers=[],
                debug_lines=[*debug_lines, "发言规划：所有可见角色都被过滤，本回合无角色发言。"],
            )

        mentioned_names = self._mentioned_character_names(
            player_input=player_input,
            visible_character_names=[name for name, _ in selectable],
        )
        group_prompt = self._is_group_prompt(player_input)
        requested_limit = max(1, max_speakers)
        requested_limit = max(requested_limit, len(mentioned_names))
        if group_prompt and len(selectable) > 1:
            requested_limit = max(requested_limit, min(3, len(selectable)))
        requested_limit = min(requested_limit, len(selectable))
        minimum_required = min(max(1, min_speakers), requested_limit)
        top_score = selectable[0][1]
        score_window = 0.75 if group_prompt or mentioned_names else 0.45

        selected: list[str] = []
        for name in mentioned_names:
            if name in selected:
                continue
            selected.append(name)
            if len(selected) >= requested_limit:
                break

        for name, score in selectable:
            if name in selected:
                continue

            should_include = False
            if len(selected) < minimum_required:
                should_include = True
            elif len(selected) < requested_limit and (top_score - score) <= score_window:
                should_include = True
            elif len(selected) == 0:
                should_include = True

            if not should_include:
                continue

            selected.append(name)
            if len(selected) >= requested_limit:
                break

        if not selected:
            selected = [selectable[0][0]]

        debug_lines.append(
            "发言规划："
            + " | ".join(
                [
                    f"limit={requested_limit}",
                    f"group_prompt={group_prompt}",
                    f"mentioned={mentioned_names or ['<none>']}",
                    f"selected={selected}",
                ]
            )
        )
        return SpeakerTurnSelectionResult(speakers=selected, debug_lines=debug_lines)

    def _rank_visible_speakers(
        self,
        *,
        session: SessionSnapshot,
        session_id: str,
        visible_character_names: list[str],
        player_input: str | None,
    ) -> tuple[list[tuple[str, float, list[str]]], list[str]]:
        deduped_visible_names = list(dict.fromkeys(name for name in visible_character_names if name.strip()))
        if not deduped_visible_names:
            return [], ["发言排序：无可见角色。"]

        world = next((item for item in self._catalog_repository.list_worlds() if item.name == session.world_name), None)
        character_name_map = {item.name: item.id for item in self._catalog_repository.list_characters()}
        schemas = {schema.id: schema for schema in self._attribute_repository.list_schemas()}
        session_values = {
            value.schema_id: value
            for value in self._attribute_repository.list_values(owner_type="session", owner_id=session_id)
        }

        scores: list[tuple[str, float, list[str]]] = []
        for character_name in deduped_visible_names:
            score = 1.0
            reasons: list[str] = ["base +1.00"]
            excluded = False

            character_id = character_name_map.get(character_name)
            character_values = {}
            if character_id:
                owner_id = f"{session_id}:{character_id}"
                character_values = {
                    value.schema_id: value
                    for value in self._attribute_repository.list_values(owner_type="session_character", owner_id=owner_id)
                }

            for value in [*session_values.values(), *character_values.values()]:
                schema = schemas.get(value.schema_id)
                if schema is None:
                    continue

                policy = schema.influence_policy.get("speaker_selector")
                if not isinstance(policy, dict) or not policy.get("enabled", False):
                    continue

                mode = policy.get("mode")
                if mode == "weighted_factor" and isinstance(value.value, (int, float)):
                    weight = float(policy.get("weight", 1.0))
                    delta = float(value.value) * weight / 100.0
                    score += delta
                    reasons.append(f"{schema.key} {delta:+.2f}")

                if mode == "priority_boost":
                    truthy = bool(value.value)
                    if truthy:
                        delta = float(policy.get("boost", 0.5))
                        score += delta
                        reasons.append(f"{schema.key} +{delta:.2f}")

                if mode == "threshold_gate":
                    threshold = policy.get("value")
                    operator = policy.get("operator", ">=")
                    if not self._compare(value.value, threshold, operator):
                        excluded = True
                        reasons.append(f"{schema.key} gate failed")

            if world and character_id:
                if world.id == "urban-awakening" and character_name == "沈听岚":
                    reasons.append("recent_speaker_penalty -0.05")
                    score -= 0.05

            mention_delta = self._player_mention_boost(player_input=player_input, character_name=character_name)
            if mention_delta:
                score += mention_delta
                reasons.append(f"player_mention +{mention_delta:.2f}")

            recent_delta = self._recent_speaker_penalty(session=session, character_name=character_name)
            if recent_delta:
                score += recent_delta
                reasons.append(f"recent_turns {recent_delta:+.2f}")

            if excluded:
                score = -inf

            scores.append((character_name, score, reasons))

        ranked = sorted(scores, key=lambda item: item[1], reverse=True)
        debug_lines = [
            "发言排序：" + " | ".join(
                f"{name}={score:.2f} ({', '.join(reasons)})" if score > -inf else f"{name}=filtered ({', '.join(reasons)})"
                for name, score, reasons in ranked
            )
        ]
        return ranked, debug_lines

    def _list_session_runtime_values(self, session_id: str) -> list[AttributeValue]:
        return [
            value
            for value in self._attribute_repository.list_values()
            if (value.owner_type == "session" and value.owner_id == session_id)
            or (value.owner_type == "session_character" and value.owner_id.startswith(f"{session_id}:"))
        ]

    def _build_character_groups(
        self,
        character_groups: dict[str, list[RuntimeAttributeItem]],
        character_name_map: dict[str, str],
    ) -> list[RuntimeAttributeGroup]:
        grouped_items = []
        for owner_id, items in character_groups.items():
            _, character_id = owner_id.split(":", 1)
            grouped_items.append(
                RuntimeAttributeGroup(
                    owner_type="session_character",
                    owner_id=owner_id,
                    owner_label=character_name_map.get(character_id, character_id),
                    items=items,
                )
            )

        return grouped_items

    def _is_player_visible(self, schema: AttributeSchema) -> bool:
        return bool(schema.display_policy.get("game_visible", False) and schema.access_policy.get("player_read", False))

    def _is_character_visible_shared(self, schema: AttributeSchema) -> bool:
        return bool(schema.access_policy.get("agent_self_read", False))

    def _ensure_projected_attributes(self, session: SessionSnapshot) -> None:
        session_values = self._attribute_repository.list_values(owner_type="session", owner_id=session.id)
        if session_values:
            return

        world = next((item for item in self._catalog_repository.list_worlds() if item.name == session.world_name), None)
        if world is None:
            return

        character_name_map = {item.name: item.id for item in self._catalog_repository.list_characters()}
        character_ids = [character_name_map[name] for name in session.visible_characters if name in character_name_map]
        self._attribute_repository.project_session_attributes(session.id, world.id, character_ids)

    def _compare(self, left, right, operator: str) -> bool:
        if operator == ">=":
            return left >= right
        if operator == ">":
            return left > right
        if operator == "<=":
            return left <= right
        if operator == "<":
            return left < right
        if operator == "==":
            return left == right
        return False

    def _player_mention_boost(self, *, player_input: str | None, character_name: str) -> float:
        if not player_input:
            return 0.0
        normalized_input = player_input.strip()
        normalized_name = character_name.strip()
        if not normalized_input or not normalized_name:
            return 0.0
        return 0.85 if normalized_name in normalized_input else 0.0

    def _recent_speaker_penalty(self, *, session: SessionSnapshot, character_name: str) -> float:
        recent_agent_speakers = [
            message.speaker.strip()
            for message in reversed(session.messages)
            if message.role == "agent" and message.speaker and message.speaker.strip()
        ]
        if not recent_agent_speakers:
            return 0.0

        penalty = 0.0
        for index, recent_speaker in enumerate(recent_agent_speakers[:3]):
            if recent_speaker != character_name:
                continue
            if index == 0:
                penalty -= 0.45
            elif index == 1:
                penalty -= 0.18
            else:
                penalty -= 0.08
        return penalty

    def _mentioned_character_names(
        self,
        *,
        player_input: str | None,
        visible_character_names: list[str],
    ) -> list[str]:
        if not player_input:
            return []

        named_matches: list[tuple[int, str]] = []
        for character_name in visible_character_names:
            if not character_name:
                continue
            position = player_input.find(character_name)
            if position == -1:
                continue
            named_matches.append((position, character_name))

        named_matches.sort(key=lambda item: item[0])
        return list(dict.fromkeys(name for _, name in named_matches))

    def _is_group_prompt(self, player_input: str | None) -> bool:
        if not player_input:
            return False

        markers = [
            "你们",
            "大家",
            "各位",
            "一起",
            "分别",
            "轮流",
            "都说",
            "都讲",
            "每个人",
            "挨个",
        ]
        return any(marker in player_input for marker in markers)
