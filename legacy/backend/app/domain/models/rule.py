from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class RuleDefinition:
    id: str
    scope: str
    name: str
    enabled: bool
    priority: int
    description: str = ""
    condition: dict[str, Any] = field(default_factory=dict)
    effects: list[dict[str, Any]] = field(default_factory=list)
