from dataclasses import dataclass, field


@dataclass(frozen=True)
class MemoryEventContext:
    event_id: str
    content: str
    source: str
    importance: float = 0.45
    memory_type: str = "event"
    speaker: str | None = None
    role: str | None = "system"
    location: str | None = None
    scene_id: str | None = None
    item_id: str | None = None
    participants: list[str] = field(default_factory=list)
