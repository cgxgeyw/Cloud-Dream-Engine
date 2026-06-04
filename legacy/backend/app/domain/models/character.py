from dataclasses import dataclass, field


@dataclass(frozen=True)
class CharacterDefinition:
    id: str
    name: str
    world_id: str
    role: str
    background_prompt: str
    model: str
    memory_strategy: str
    recent_dialogue_rounds: int = 2
    attributes: list[str] = field(default_factory=list)
    portrait_assets: list[str] = field(default_factory=list)
    custom_tabs: dict[str, str] = field(default_factory=dict)
