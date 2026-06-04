from dataclasses import dataclass, field


@dataclass(frozen=True)
class SceneRuntime:
    scene_id: str
    name: str
    background_hint: str
    temporary_tags: list[str] = field(default_factory=list)
    present_characters: list[str] = field(default_factory=list)
