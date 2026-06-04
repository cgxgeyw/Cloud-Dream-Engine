from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class AttributeSchema:
    id: str
    scope: str
    key: str
    label: str
    value_type: str
    description: str = ""
    default_value: Any = None
    enum_options: list[str] = field(default_factory=list)
    display_policy: dict[str, Any] = field(default_factory=dict)
    access_policy: dict[str, Any] = field(default_factory=dict)
    mutation_policy: dict[str, Any] = field(default_factory=dict)
    influence_policy: dict[str, Any] = field(default_factory=dict)
    projection_policy: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class AttributeValue:
    id: str
    schema_id: str
    owner_type: str
    owner_id: str
    value: Any = None
    source: str = "system"
