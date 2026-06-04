from __future__ import annotations

from datetime import datetime
import re
import uuid

from backend.app.application.services.inventory_runtime_service import InventoryOperation
from backend.app.application.services.memory_runtime_models import MemoryEventContext
from backend.app.domain.models.memory import MemoryEntry
from backend.app.domain.repositories.memory import MemoryRepository


class MemoryQueryService:
    def __init__(self, memory_repository: MemoryRepository) -> None:
        self._memory_repository = memory_repository

    def list_for_character(
        self,
        *,
        world_id: str,
        character_id: str,
        session_id: str | None = None,
        conversation_id: str | None = None,
        scene_id: str | None = None,
        event_id: str | None = None,
        item_id: str | None = None,
        layers: list[str] | None = None,
        importance_min: float | None = None,
        importance_max: float | None = None,
        memory_types: list[str] | None = None,
        limit: int = 8,
    ) -> list[MemoryEntry]:
        return self._memory_repository.list_for_character(
            world_id=world_id,
            character_id=character_id,
            session_id=session_id,
            conversation_id=conversation_id,
            scene_id=scene_id,
            event_id=event_id,
            item_id=item_id,
            layers=layers,
            importance_min=importance_min,
            importance_max=importance_max,
            memory_types=memory_types,
            limit=limit,
        )

    def recall_for_character(
        self,
        *,
        world_id: str,
        session_id: str,
        character_id: str | None,
        query_text: str,
        location: str | None,
        scene_id: str | None,
        participants: list[str],
        current_event_ids: list[str] | None = None,
        current_item_ids: list[str] | None = None,
        layers: list[str] | None = None,
        memory_types: list[str] | None = None,
        limit: int = 6,
        candidate_limit: int = 200,
    ) -> list[MemoryEntry]:
        if character_id is None:
            return []

        requested_memory_types = {str(item).strip() for item in (memory_types or []) if str(item).strip()}
        dialogue_only = requested_memory_types == {"dialogue"}
        candidates = self._memory_repository.list_for_character(
            world_id=world_id,
            character_id=character_id,
            session_id=session_id if dialogue_only else None,
            conversation_id=session_id if dialogue_only else None,
            scene_id=None,
            layers=layers,
            memory_types=memory_types,
            limit=candidate_limit,
        )
        if not candidates:
            return []

        normalized_query = self._normalize_text(query_text)
        query_terms = self._build_search_terms(query_text)
        participant_terms = {self._normalize_text(item) for item in participants if item.strip()}
        location_term = self._normalize_text(location or "")
        current_event_ids = {item for item in (current_event_ids or []) if item}
        current_item_ids = {item for item in (current_item_ids or []) if item}
        newest_candidate_at = max((self._parse_created_at(entry.created_at) for entry in candidates), default=None)

        ranked = sorted(
            candidates,
            key=lambda entry: self._score_entry(
                entry=entry,
                normalized_query=normalized_query,
                query_terms=query_terms,
                participant_terms=participant_terms,
                location_term=location_term,
                scene_id=scene_id,
                current_conversation_id=session_id,
                current_event_ids=current_event_ids,
                current_item_ids=current_item_ids,
                newest_candidate_at=newest_candidate_at,
            ),
            reverse=True,
        )
        return self._select_balanced_memories(ranked=ranked, limit=limit)

    def _score_entry(
        self,
        *,
        entry: MemoryEntry,
        normalized_query: str,
        query_terms: set[str],
        participant_terms: set[str],
        location_term: str,
        scene_id: str | None,
        current_conversation_id: str | None,
        current_event_ids: set[str],
        current_item_ids: set[str],
        newest_candidate_at: datetime | None,
    ) -> tuple[float, float, str]:
        searchable_parts = [
            entry.content,
            entry.speaker or "",
            entry.location or "",
            *entry.participants,
            *entry.keywords,
        ]
        searchable_text = self._normalize_text(" ".join(searchable_parts))
        score = entry.importance

        if normalized_query and normalized_query in searchable_text:
            score += 8.0

        for term in query_terms:
            if term and term in searchable_text:
                score += 2.0 + min(len(term), 6) * 0.2

        if participant_terms:
            matched_participants = sum(
                1
                for term in participant_terms
                if term
                and (
                    term in searchable_text
                    or any(term == self._normalize_text(item) for item in entry.participants)
                    or term == self._normalize_text(entry.speaker or "")
                )
            )
            score += matched_participants * 2.5

        if location_term and location_term == self._normalize_text(entry.location or ""):
            score += 2.0

        if scene_id and scene_id == entry.scene_id:
            score += 2.2

        if current_conversation_id and entry.conversation_id == current_conversation_id:
            score += 1.8

        if current_event_ids and entry.event_id and entry.event_id in current_event_ids:
            score += 3.4

        if current_item_ids and entry.item_id and entry.item_id in current_item_ids:
            score += 3.2

        if entry.memory_type == "dialogue":
            score += 0.4
            if current_conversation_id and entry.conversation_id == current_conversation_id:
                score += 0.9

        score += self._layer_bonus(entry.layer)
        score += self._recency_bonus(entry=entry, newest_candidate_at=newest_candidate_at)
        return (score, entry.importance, entry.created_at)

    def _layer_bonus(self, layer: str) -> float:
        return {
            "working": 2.4,
            "short_term": 1.6,
            "canonical_event": 1.9,
            "archive": 0.0,
        }.get(layer, 0.0)

    def _select_balanced_memories(self, *, ranked: list[MemoryEntry], limit: int) -> list[MemoryEntry]:
        if limit <= 0:
            return []

        quotas = {
            "working": 2,
            "short_term": 2,
            "canonical_event": 1,
        }
        selected: list[MemoryEntry] = []
        selected_ids: set[str] = set()
        used_per_layer: dict[str, int] = {}

        for layer, quota in quotas.items():
            for entry in ranked:
                if len(selected) >= limit:
                    break
                if entry.id in selected_ids or entry.layer != layer:
                    continue
                if used_per_layer.get(layer, 0) >= quota:
                    continue
                selected.append(entry)
                selected_ids.add(entry.id)
                used_per_layer[layer] = used_per_layer.get(layer, 0) + 1

        for entry in ranked:
            if len(selected) >= limit:
                break
            if entry.id in selected_ids:
                continue
            selected.append(entry)
            selected_ids.add(entry.id)

        return selected

    def _build_search_terms(self, text: str) -> set[str]:
        normalized = self._normalize_text(text)
        if not normalized:
            return set()

        terms = set(re.findall(r"[a-z0-9]{3,}", normalized))
        for chunk in re.findall(r"[\u4e00-\u9fff]{2,}", normalized):
            max_length = min(len(chunk), 4)
            for size in range(2, max_length + 1):
                for start in range(0, len(chunk) - size + 1):
                    terms.add(chunk[start : start + size])

        if len(normalized) <= 24:
            terms.add(normalized)
        return {term for term in terms if term}

    def _normalize_text(self, value: str) -> str:
        compact = re.sub(r"\s+", "", value.strip().lower())
        return re.sub(r"[^\w\u4e00-\u9fff]", "", compact)

    def _parse_created_at(self, value: str) -> datetime | None:
        try:
            return datetime.strptime(value, "%Y-%m-%d %H:%M:%S")
        except ValueError:
            return None

    def _recency_bonus(self, *, entry: MemoryEntry, newest_candidate_at: datetime | None) -> float:
        created_at = self._parse_created_at(entry.created_at)
        if created_at is None or newest_candidate_at is None:
            return 0.0

        age_seconds = max((newest_candidate_at - created_at).total_seconds(), 0.0)
        age_days = age_seconds / 86400
        if age_days <= 1:
            return 1.6
        if age_days <= 3:
            return 1.1
        if age_days <= 7:
            return 0.6
        if age_days <= 14:
            return 0.2
        return 0.0


class MemoryCommandService:
    def __init__(self, memory_repository: MemoryRepository) -> None:
        self._memory_repository = memory_repository

    def append_entries(self, entries: list[MemoryEntry]) -> list[MemoryEntry]:
        return self._memory_repository.append_entries(entries)

    def build_turn_entries(
        self,
        *,
        world_id: str,
        session_id: str,
        turn_index: int,
        visible_characters: list[tuple[str, str]],
        player_character_name: str | None = None,
        speaker_responses: list[tuple[str | None, str, str]] | None = None,
        speaker_id: str | None = None,
        speaker_name: str | None = None,
        player_input: str,
        response_text: str = "",
        observed_facts: list[str],
        location: str,
        scene_id: str | None,
        memory_events: list[MemoryEventContext] | None = None,
        inventory_operations: list[InventoryOperation] | None = None,
    ) -> list[MemoryEntry]:
        now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        entries: list[MemoryEntry] = []
        resolved_player_name = str(player_character_name or "").strip() or "玩家"
        participant_names = list(dict.fromkeys([resolved_player_name, *[name for _, name in visible_characters]]))
        memory_events = memory_events or []
        inventory_operations = inventory_operations or []
        normalized_speaker_responses = [
            (character_id, speaker, content)
            for character_id, speaker, content in (speaker_responses or [])
            if speaker and content.strip()
        ]
        if not normalized_speaker_responses and speaker_name and response_text.strip():
            normalized_speaker_responses = [(speaker_id, speaker_name, response_text)]

        if player_input.strip():
            for character_id, _ in visible_characters:
                self._append_layered_entries(
                    entries=entries,
                    layers=["working", "archive"],
                    world_id=world_id,
                    session_id=session_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=now,
                    content=player_input,
                    source="player_action",
                    importance=0.65,
                    memory_type="dialogue",
                    speaker=resolved_player_name,
                    role="player",
                    location=location,
                    scene_id=scene_id,
                    event_id=None,
                    item_id=None,
                    participants=participant_names,
                )

        for response_character_id, response_speaker_name, response_text_value in normalized_speaker_responses:
            heard_character_ids = {character_id for character_id, _ in visible_characters}
            if response_character_id:
                heard_character_ids.add(response_character_id)

            for character_id in heard_character_ids:
                self._append_layered_entries(
                    entries=entries,
                    layers=["working", "short_term", "archive"],
                    world_id=world_id,
                    session_id=session_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=now,
                    content=response_text_value,
                    source="speaker_response",
                    importance=0.75 if character_id == response_character_id else 0.68,
                    memory_type="dialogue",
                    speaker=response_speaker_name,
                    role="agent",
                    location=location,
                    scene_id=scene_id,
                    event_id=None,
                    item_id=None,
                    participants=participant_names,
                )

        for fact in observed_facts:
            if not fact.strip():
                continue
            for character_id, _ in visible_characters:
                self._append_layered_entries(
                    entries=entries,
                    layers=["short_term", "archive"],
                    world_id=world_id,
                    session_id=session_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=now,
                    content=fact,
                    source="world_update",
                    importance=0.45,
                    memory_type="event",
                    speaker=None,
                    role="system",
                    location=location,
                    scene_id=scene_id,
                    event_id=None,
                    item_id=None,
                    participants=participant_names,
                )

        for event in memory_events:
            if not event.content.strip():
                continue
            event_participants = event.participants or participant_names
            event_layers = ["short_term", "archive"]
            if event.importance >= 0.7:
                event_layers.insert(0, "canonical_event")
            for character_id, _ in visible_characters:
                self._append_layered_entries(
                    entries=entries,
                    layers=event_layers,
                    world_id=world_id,
                    session_id=session_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=now,
                    content=event.content,
                    source=event.source,
                    importance=event.importance,
                    memory_type=event.memory_type,
                    speaker=event.speaker,
                    role=event.role,
                    location=event.location or location,
                    scene_id=event.scene_id or scene_id,
                    event_id=event.event_id,
                    item_id=event.item_id,
                    participants=event_participants,
                )

        for operation in inventory_operations:
            content = self._describe_inventory_operation(operation)
            if not content:
                continue
            operation_participants = participant_names
            if operation.disclosed_to_add:
                operation_participants = list(dict.fromkeys([*participant_names, *operation.disclosed_to_add]))
            for character_id, _ in visible_characters:
                self._append_layered_entries(
                    entries=entries,
                    layers=["short_term", "archive"],
                    world_id=world_id,
                    session_id=session_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=now,
                    content=content,
                    source="inventory_runtime",
                    importance=0.5,
                    memory_type="event",
                    speaker=None,
                    role="system",
                    location=location,
                    scene_id=scene_id,
                    event_id=f"inventory:{operation.action}",
                    item_id=operation.item_id,
                    participants=operation_participants,
                )

        return entries

    def _append_layered_entries(
        self,
        *,
        entries: list[MemoryEntry],
        layers: list[str],
        world_id: str,
        session_id: str,
        turn_index: int,
        character_id: str,
        created_at: str,
        content: str,
        source: str,
        importance: float,
        memory_type: str,
        speaker: str | None,
        role: str | None,
        location: str,
        scene_id: str | None,
        event_id: str | None,
        item_id: str | None,
        participants: list[str],
    ) -> None:
        for layer in list(dict.fromkeys(layer for layer in layers if layer)):
            entries.append(
                self._build_entry(
                    session_id=session_id,
                    world_id=world_id,
                    turn_index=turn_index,
                    character_id=character_id,
                    created_at=created_at,
                    layer=layer,
                    content=content,
                    source=source,
                    importance=importance,
                    memory_type=memory_type,
                    speaker=speaker,
                    role=role,
                    location=location,
                    scene_id=scene_id,
                    event_id=event_id,
                    item_id=item_id,
                    participants=participants,
                )
            )

    def _build_entry(
        self,
        *,
        world_id: str,
        session_id: str,
        turn_index: int,
        character_id: str,
        created_at: str,
        layer: str,
        content: str,
        source: str,
        importance: float,
        memory_type: str,
        speaker: str | None,
        role: str | None,
        location: str,
        scene_id: str | None,
        event_id: str | None,
        item_id: str | None,
        participants: list[str],
    ) -> MemoryEntry:
        keywords = self._extract_keywords(content=content, participants=participants, location=location)
        return MemoryEntry(
            id=f"mem-{uuid.uuid4().hex[:10]}",
            world_id=world_id,
            session_id=session_id,
            turn_index=turn_index,
            conversation_id=session_id,
            character_id=character_id,
            layer=layer,
            content=content,
            source=source,
            importance=importance,
            created_at=created_at,
            event_id=event_id,
            item_id=item_id,
            scene_id=scene_id,
            memory_type=memory_type,
            speaker=speaker,
            role=role,
            location=location,
            participants=participants,
            keywords=keywords,
        )

    def _extract_keywords(self, *, content: str, participants: list[str], location: str) -> list[str]:
        raw_tokens = set()
        normalized_content = re.sub(r"\s+", "", content.strip().lower())
        raw_tokens.update(re.findall(r"[a-z0-9]{3,}", normalized_content))

        for chunk in re.findall(r"[\u4e00-\u9fff]{2,}", normalized_content):
            max_length = min(len(chunk), 4)
            for size in range(2, max_length + 1):
                for start in range(0, len(chunk) - size + 1):
                    raw_tokens.add(chunk[start : start + size])

        for item in [location, *participants]:
            stripped = item.strip().lower()
            if stripped:
                raw_tokens.add(stripped)

        ranked = sorted({token for token in raw_tokens if token}, key=lambda item: (-len(item), item))
        return ranked[:48]

    def _describe_inventory_operation(self, operation: InventoryOperation) -> str:
        quantity = abs(operation.quantity_delta)
        quantity_text = f" x{quantity}" if quantity > 1 else ""

        if operation.action == "add":
            return f"玩家获得了{operation.item_name}{quantity_text}"
        if operation.action == "transfer":
            return f"玩家交出了{operation.item_name}{quantity_text}"
        if operation.action == "use" and operation.quantity_delta < 0:
            return f"玩家使用并消耗了{operation.item_name}{quantity_text}"
        if operation.action == "use":
            if operation.disclosed_to_add:
                targets = "、".join(operation.disclosed_to_add)
                return f"玩家向{targets}展示了{operation.item_name}"
            return f"玩家使用了{operation.item_name}"
        return ""

    def _build_turn_summary(
        self,
        *,
        player_input: str,
        speaker_responses: list[tuple[str | None, str, str]],
        observed_facts: list[str],
        memory_events: list[MemoryEventContext],
        inventory_operations: list[InventoryOperation],
    ) -> str:
        segments: list[str] = []
        if player_input.strip():
            segments.append(f"玩家行动：{player_input.strip()}")
        for _, speaker_name, response_text in speaker_responses[:3]:
            if response_text.strip():
                segments.append(f"{speaker_name}回应：{response_text.strip()}")
        for fact in observed_facts[:2]:
            if fact.strip():
                segments.append(f"场景事实：{fact.strip()}")
        for event in memory_events[:2]:
            if event.content.strip():
                segments.append(f"事件：{event.content.strip()}")
        for operation in inventory_operations[:2]:
            described = self._describe_inventory_operation(operation)
            if described:
                segments.append(f"物品变化：{described}")
        return "；".join(segments)
