from pydantic import BaseModel

from backend.app.domain.models.memory import MemoryEntry


class MemoryEntryResponse(BaseModel):
    id: str
    world_id: str
    session_id: str
    turn_index: int
    conversation_id: str | None = None
    character_id: str
    event_id: str | None = None
    item_id: str | None = None
    scene_id: str | None = None
    layer: str
    content: str
    source: str
    importance: float
    created_at: str
    memory_type: str
    speaker: str | None = None
    role: str | None = None
    location: str | None = None
    participants: list[str]
    keywords: list[str]

    @classmethod
    def from_domain(cls, memory: MemoryEntry) -> "MemoryEntryResponse":
        return cls(
            id=memory.id,
            world_id=memory.world_id,
            session_id=memory.session_id,
            turn_index=memory.turn_index,
            conversation_id=memory.conversation_id,
            character_id=memory.character_id,
            event_id=memory.event_id,
            item_id=memory.item_id,
            scene_id=memory.scene_id,
            layer=memory.layer,
            content=memory.content,
            source=memory.source,
            importance=memory.importance,
            created_at=memory.created_at,
            memory_type=memory.memory_type,
            speaker=memory.speaker,
            role=memory.role,
            location=memory.location,
            participants=memory.participants,
            keywords=memory.keywords,
        )
