from typing import Any

from pydantic import BaseModel, Field

from backend.app.api.schemas.characters import CharacterTemplateResponse
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.world import (
    WorldDefinition,
    WorldOpeningMessage,
    normalize_world_director_config,
)


class WorldOpeningMessagePayload(BaseModel):
    role: str
    content: str = Field(min_length=1)
    speaker: str | None = None

    @classmethod
    def from_domain(cls, message: WorldOpeningMessage) -> "WorldOpeningMessagePayload":
        return cls(role=message.role, content=message.content, speaker=message.speaker)

    def to_domain(self) -> WorldOpeningMessage:
        normalized_role = "system" if self.role == "system" else "agent"
        return WorldOpeningMessage(role=normalized_role, content=self.content, speaker=self.speaker)


class WorldResponse(BaseModel):
    id: str
    name: str
    genre: str
    background_prompt: str
    opening_scene: str
    summary: str
    time_system: str
    map_nodes: list[str]
    triggers: list[str]
    custom_tabs: dict[str, str]
    time_config: dict[str, Any]
    director_config: dict[str, Any]
    ui_theme_config: dict[str, Any]
    director_system_prompt_base: str = ""
    director_runtime_system_prompt: str = ""
    opening_messages: list[WorldOpeningMessagePayload]
    opening_character_ids: list[str]
    player_character_id: str | None = None

    @classmethod
    def from_domain(
        cls,
        world: WorldDefinition,
        *,
        director_system_prompt_base: str = "",
        director_runtime_system_prompt: str = "",
    ) -> "WorldResponse":
        return cls(
            id=world.id,
            name=world.name,
            genre=world.genre,
            background_prompt=world.background_prompt,
            opening_scene=world.opening_scene,
            summary=world.summary,
            time_system=world.time_system,
            map_nodes=world.map_nodes,
            triggers=world.triggers,
            custom_tabs=world.custom_tabs,
            time_config=world.time_config,
            director_config=normalize_world_director_config(world.director_config),
            ui_theme_config=world.ui_theme_config,
            director_system_prompt_base=director_system_prompt_base,
            director_runtime_system_prompt=director_runtime_system_prompt,
            opening_messages=[WorldOpeningMessagePayload.from_domain(item) for item in world.opening_messages],
            opening_character_ids=list(world.opening_character_ids),
            player_character_id=world.player_character_id,
        )


class WorldUpsertRequest(BaseModel):
    name: str = Field(min_length=1)
    genre: str = ""
    background_prompt: str = ""
    opening_scene: str = ""
    summary: str = ""
    time_system: str = ""
    map_nodes: list[str] = Field(default_factory=list)
    triggers: list[str] = Field(default_factory=list)
    custom_tabs: dict[str, str] = Field(default_factory=dict)
    time_config: dict[str, Any] = Field(default_factory=dict)
    director_config: dict[str, Any] = Field(default_factory=dict)
    ui_theme_config: dict[str, Any] = Field(default_factory=dict)
    opening_messages: list[WorldOpeningMessagePayload] = Field(default_factory=list)
    opening_character_ids: list[str] = Field(default_factory=list)
    player_character_id: str | None = None


class WorldTemplateResponse(BaseModel):
    name: str
    genre: str
    background_prompt: str
    opening_scene: str
    summary: str
    time_system: str
    map_nodes: list[str]
    triggers: list[str]
    custom_tabs: dict[str, str]
    time_config: dict[str, Any]
    director_config: dict[str, Any]
    ui_theme_config: dict[str, Any]
    opening_messages: list[WorldOpeningMessagePayload]
    opening_character_names: list[str]
    player_character_name: str | None = None
    characters: list[CharacterTemplateResponse]

    @classmethod
    def from_domain(
        cls,
        world: WorldDefinition,
        characters: list[CharacterDefinition],
    ) -> "WorldTemplateResponse":
        character_name_by_id = {item.id: item.name for item in characters if item.id.strip() and item.name.strip()}
        return cls(
            name=world.name,
            genre=world.genre,
            background_prompt=world.background_prompt,
            opening_scene=world.opening_scene,
            summary=world.summary,
            time_system=world.time_system,
            map_nodes=list(world.map_nodes),
            triggers=list(world.triggers),
            custom_tabs=dict(world.custom_tabs),
            time_config=dict(world.time_config),
            director_config=normalize_world_director_config(world.director_config),
            ui_theme_config=dict(world.ui_theme_config),
            opening_messages=[WorldOpeningMessagePayload.from_domain(item) for item in world.opening_messages],
            opening_character_names=[
                character_name_by_id[character_id]
                for character_id in world.opening_character_ids
                if character_id in character_name_by_id
            ],
            player_character_name=character_name_by_id.get(world.player_character_id or ""),
            characters=[CharacterTemplateResponse.from_domain(item) for item in characters],
        )


class WorldPackageWorldData(BaseModel):
    name: str
    genre: str
    background_prompt: str
    opening_scene: str
    summary: str
    time_system: str
    map_nodes: list[str]
    triggers: list[str]
    custom_tabs: dict[str, str]
    time_config: dict[str, Any]
    director_config: dict[str, Any]
    ui_theme_config: dict[str, Any]
    opening_messages: list[WorldOpeningMessagePayload]
    opening_character_names: list[str] = Field(default_factory=list)
    player_character_name: str | None = None
    opening_character_source_ids: list[str] = Field(default_factory=list)
    player_character_source_id: str | None = None

    @classmethod
    def from_domain(
        cls,
        world: WorldDefinition,
        characters: list[CharacterDefinition],
    ) -> "WorldPackageWorldData":
        character_ids = {item.id for item in characters if item.id.strip()}
        return cls(
            name=world.name,
            genre=world.genre,
            background_prompt=world.background_prompt,
            opening_scene=world.opening_scene,
            summary=world.summary,
            time_system=world.time_system,
            map_nodes=list(world.map_nodes),
            triggers=list(world.triggers),
            custom_tabs=dict(world.custom_tabs),
            time_config=dict(world.time_config),
            director_config=normalize_world_director_config(world.director_config),
            ui_theme_config=dict(world.ui_theme_config),
            opening_messages=[WorldOpeningMessagePayload.from_domain(item) for item in world.opening_messages],
            opening_character_source_ids=[
                character_id
                for character_id in world.opening_character_ids
                if character_id in character_ids
            ],
            player_character_source_id=world.player_character_id if world.player_character_id in character_ids else None,
        )

    def to_template(
        self,
        characters: list[CharacterTemplateResponse],
        *,
        character_name_by_source_id: dict[str, str] | None = None,
    ) -> WorldTemplateResponse:
        character_name_by_source_id = character_name_by_source_id or {}
        opening_character_names = (
            [
                character_name_by_source_id[source_id]
                for source_id in self.opening_character_source_ids
                if source_id in character_name_by_source_id
            ]
            if self.opening_character_source_ids
            else list(self.opening_character_names)
        )
        player_character_name = (
            character_name_by_source_id.get(self.player_character_source_id or "")
            if self.player_character_source_id
            else self.player_character_name
        )
        return WorldTemplateResponse(
            name=self.name,
            genre=self.genre,
            background_prompt=self.background_prompt,
            opening_scene=self.opening_scene,
            summary=self.summary,
            time_system=self.time_system,
            map_nodes=list(self.map_nodes),
            triggers=list(self.triggers),
            custom_tabs=dict(self.custom_tabs),
            time_config=dict(self.time_config),
            director_config=dict(self.director_config),
            ui_theme_config=dict(self.ui_theme_config),
            opening_messages=list(self.opening_messages),
            opening_character_names=opening_character_names,
            player_character_name=player_character_name,
            characters=list(characters),
        )


class WorldPackageCharacterData(BaseModel):
    source_character_id: str | None = None
    name: str
    role: str
    background_prompt: str
    model: str
    memory_strategy: str
    recent_dialogue_rounds: int
    attributes: list[str]
    portrait_assets: list[str]
    custom_tabs: dict[str, str]

    @classmethod
    def from_domain(cls, character: CharacterDefinition) -> "WorldPackageCharacterData":
        return cls(
            source_character_id=character.id,
            name=character.name,
            role=character.role,
            background_prompt=character.background_prompt,
            model=character.model,
            memory_strategy=character.memory_strategy,
            recent_dialogue_rounds=character.recent_dialogue_rounds,
            attributes=list(character.attributes),
            portrait_assets=list(character.portrait_assets),
            custom_tabs=dict(character.custom_tabs),
        )

    def to_template(self) -> CharacterTemplateResponse:
        return CharacterTemplateResponse(
            name=self.name,
            role=self.role,
            background_prompt=self.background_prompt,
            model=self.model,
            memory_strategy=self.memory_strategy,
            recent_dialogue_rounds=self.recent_dialogue_rounds,
            attributes=list(self.attributes),
            portrait_assets=list(self.portrait_assets),
            custom_tabs=dict(self.custom_tabs),
        )


class PromptTracePreviewResponse(BaseModel):
    speaker: str | None = None
    prompt_trace: dict[str, Any]


class WorldOpeningPromptPreviewResponse(BaseModel):
    opening_calls_llm: bool
    opening_messages: list[WorldOpeningMessagePayload]
    sample_player_input: str
    planned_speakers: list[str]
    world_director_prompt_trace: dict[str, Any]
    character_prompt_traces: list[PromptTracePreviewResponse]
    notes: list[str]


class WorldPackageCharactersData(BaseModel):
    characters: list[CharacterTemplateResponse] = Field(default_factory=list)

    @classmethod
    def from_template(cls, template: WorldTemplateResponse) -> "WorldPackageCharactersData":
        return cls(characters=list(template.characters))


class WorldPackageCharacterFileEntry(BaseModel):
    source_character_id: str
    character_name: str
    file_path: str


class WorldPackageAssetEntry(BaseModel):
    source_path: str
    archive_path: str
    owner_type: str | None = None
    owner_id: str | None = None


class WorldPackageManifest(BaseModel):
    format: str = "dream-world-package"
    version: int = 3
    world: WorldTemplateResponse | None = None
    world_file: str | None = None
    characters_file: str | None = None
    character_files: list[WorldPackageCharacterFileEntry] = Field(default_factory=list)
    assets: list[WorldPackageAssetEntry] = Field(default_factory=list)
