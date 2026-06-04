from dataclasses import dataclass, field


@dataclass(frozen=True)
class InventoryItem:
    item_id: str
    name: str
    category: str
    quantity: int = 1
    description: str = ""
    tags: list[str] = field(default_factory=list)
    owner_type: str = "player"
    owner_id: str = "player"
    visibility: str = "private"
    disclosed_to: list[str] = field(default_factory=list)
