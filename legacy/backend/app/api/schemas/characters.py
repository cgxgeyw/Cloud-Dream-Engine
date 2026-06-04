from pydantic import BaseModel, Field

from backend.app.domain.models.character import CharacterDefinition


class CharacterResponse(BaseModel):
    id: str
    name: str
    world_id: str
    role: str
    background_prompt: str
    model: str
    memory_strategy: str
    recent_dialogue_rounds: int
    attributes: list[str]
    portrait_assets: list[str]
    custom_tabs: dict[str, str]
    runtime_system_prompt: str | None = None

    @classmethod
    def from_domain(
        cls,
        character: CharacterDefinition,
        *,
        runtime_system_prompt: str | None = None,
    ) -> "CharacterResponse":
        return cls(
            id=character.id,
            name=character.name,
            world_id=character.world_id,
            role=character.role,
            background_prompt=character.background_prompt,
            model=character.model,
            memory_strategy=character.memory_strategy,
            recent_dialogue_rounds=character.recent_dialogue_rounds,
            attributes=character.attributes,
            portrait_assets=character.portrait_assets,
            custom_tabs=character.custom_tabs,
            runtime_system_prompt=runtime_system_prompt,
        )


class CharacterUpsertRequest(BaseModel):
    name: str = Field(min_length=1)
    world_id: str = Field(min_length=1)
    role: str = ""
    background_prompt: str = ""
    model: str = ""
    memory_strategy: str = ""
    recent_dialogue_rounds: int = Field(default=2, ge=0)
    attributes: list[str] = Field(default_factory=list)
    portrait_assets: list[str] = Field(default_factory=list)
    custom_tabs: dict[str, str] = Field(default_factory=dict)


class CharacterTemplateResponse(BaseModel):
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
    def from_domain(cls, character: CharacterDefinition) -> "CharacterTemplateResponse":
        return cls(
            name=character.name,
            role=character.role,
            background_prompt=character.background_prompt,
            model=character.model,
            memory_strategy=character.memory_strategy,
            recent_dialogue_rounds=character.recent_dialogue_rounds,
            attributes=character.attributes,
            portrait_assets=character.portrait_assets,
            custom_tabs=character.custom_tabs,
        )


class CharacterCreateFromTemplateRequest(BaseModel):
    target_world_id: str = Field(min_length=1)
    name: str = Field(min_length=1)


class CharacterDeriveRequest(BaseModel):
    name: str = Field(min_length=1)


class CharacterTemplateImportRequest(BaseModel):
    name: str = Field(min_length=1)
    role: str = ""
    background_prompt: str = ""
    model: str = ""
    memory_strategy: str = ""
    recent_dialogue_rounds: int = Field(default=2, ge=0)
    attributes: list[str] = Field(default_factory=list)
    portrait_assets: list[str] = Field(default_factory=list)
    custom_tabs: dict[str, str] = Field(default_factory=dict)
