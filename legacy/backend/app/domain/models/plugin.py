from dataclasses import dataclass, field


@dataclass(frozen=True)
class PluginDefinition:
    id: str
    name: str
    enabled: bool
    description: str
    hooks: list[str] = field(default_factory=list)
