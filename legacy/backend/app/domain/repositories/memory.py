from typing import Protocol

from backend.app.domain.models.memory import MemoryEntry


class MemoryRepository(Protocol):
    def list_for_character(
        self,
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
    ) -> list[MemoryEntry]: ...

    def append_entries(self, entries: list[MemoryEntry]) -> list[MemoryEntry]: ...
