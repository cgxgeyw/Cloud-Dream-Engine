from dataclasses import dataclass, field


@dataclass(frozen=True)
class CharacterVisualState:
    character_name: str
    portrait_hint: str
    portrait_asset_path: str | None = None
    generation_prompt: str | None = None


@dataclass(frozen=True)
class AssetSelection:
    background_hint: str
    active_speaker_portrait: str
    background_asset_path: str | None = None
    active_speaker_portrait_path: str | None = None
    background_generation_prompt: str | None = None
    active_speaker_generation_prompt: str | None = None
    visible_character_portraits: list[CharacterVisualState] = field(default_factory=list)
