from dataclasses import dataclass, field


@dataclass(frozen=True)
class ContextAttributeRecord:
    key: str
    value: object
    owner_type: str
    owner_relation: str


@dataclass(frozen=True)
class ContextInventoryRecord:
    item_id: str
    name: str
    category: str
    quantity: int = 1
    description: str = ""
    tags: list[str] = field(default_factory=list)
    owner_type: str = ""
    knowledge_scope: str = "public"


@dataclass(frozen=True)
class SceneState:
    world_name: str
    location: str
    time_label: str
    scene_name: str
    scene_tags: list[str] = field(default_factory=list)
    present_characters: list[str] = field(default_factory=list)
    discovered_locations: list[str] = field(default_factory=list)
    public_attributes: list[ContextAttributeRecord] = field(default_factory=list)
    public_items: list[ContextInventoryRecord] = field(default_factory=list)


PublicWorldState = SceneState
