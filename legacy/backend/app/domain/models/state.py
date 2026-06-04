from dataclasses import dataclass, field


@dataclass(frozen=True)
class SessionState:
    metrics: dict[str, float] = field(default_factory=dict)
    tags: list[str] = field(default_factory=list)
    phase: str = "idle"
