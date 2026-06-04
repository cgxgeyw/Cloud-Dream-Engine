from typing import Literal

from pydantic import BaseModel, Field, model_validator

from backend.app.domain.models.asset import AssetSelection, CharacterVisualState
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.session import ChatMessage, SessionMapEdge, SessionMapNode, SessionSnapshot
from backend.app.domain.models.state import SessionState


class ChatMessageResponse(BaseModel):
    role: str
    content: str
    speaker: str | None = None
    metadata: dict[str, object] | None = None

    @classmethod
    def from_domain(cls, message: ChatMessage) -> "ChatMessageResponse":
        return cls(role=message.role, content=message.content, speaker=message.speaker, metadata=message.metadata)


class SessionStateResponse(BaseModel):
    metrics: dict[str, float]
    tags: list[str]
    phase: str

    @classmethod
    def from_domain(cls, state: SessionState) -> "SessionStateResponse":
        return cls(metrics=state.metrics, tags=state.tags, phase=state.phase)


class SceneRuntimeResponse(BaseModel):
    scene_id: str
    name: str
    background_hint: str
    temporary_tags: list[str]
    present_characters: list[str]

    @classmethod
    def from_domain(cls, scene: SceneRuntime) -> "SceneRuntimeResponse":
        return cls(
            scene_id=scene.scene_id,
            name=scene.name,
            background_hint=scene.background_hint,
            temporary_tags=scene.temporary_tags,
            present_characters=scene.present_characters,
        )


class CharacterVisualStateResponse(BaseModel):
    character_name: str
    portrait_hint: str
    portrait_asset_path: str | None = None

    @classmethod
    def from_domain(cls, state: CharacterVisualState) -> "CharacterVisualStateResponse":
        return cls(
            character_name=state.character_name,
            portrait_hint=state.portrait_hint,
            portrait_asset_path=state.portrait_asset_path,
        )


class AssetSelectionResponse(BaseModel):
    background_hint: str
    active_speaker_portrait: str
    background_asset_path: str | None = None
    active_speaker_portrait_path: str | None = None
    visible_character_portraits: list[CharacterVisualStateResponse]

    @classmethod
    def from_domain(cls, asset: AssetSelection) -> "AssetSelectionResponse":
        return cls(
            background_hint=asset.background_hint,
            active_speaker_portrait=asset.active_speaker_portrait,
            background_asset_path=asset.background_asset_path,
            active_speaker_portrait_path=asset.active_speaker_portrait_path,
            visible_character_portraits=[
                CharacterVisualStateResponse.from_domain(item) for item in asset.visible_character_portraits
            ],
        )


class InventoryItemResponse(BaseModel):
    item_id: str
    name: str
    category: str
    quantity: int
    description: str
    tags: list[str]

    @classmethod
    def from_domain(cls, item: InventoryItem) -> "InventoryItemResponse":
        return cls(
            item_id=item.item_id,
            name=item.name,
            category=item.category,
            quantity=item.quantity,
            description=item.description,
            tags=item.tags,
        )


class SessionMapNodeResponse(BaseModel):
    node_id: str
    label: str
    discovered: bool
    current: bool

    @classmethod
    def from_domain(cls, node: SessionMapNode) -> "SessionMapNodeResponse":
        return cls(node_id=node.node_id, label=node.label, discovered=node.discovered, current=node.current)


class SessionMapEdgeResponse(BaseModel):
    edge_id: str
    source_node_id: str
    target_node_id: str

    @classmethod
    def from_domain(cls, edge: SessionMapEdge) -> "SessionMapEdgeResponse":
        return cls(
            edge_id=edge.edge_id,
            source_node_id=edge.source_node_id,
            target_node_id=edge.target_node_id,
        )


class SessionSnapshotResponse(BaseModel):
    id: str
    world_name: str
    location: str
    time_label: str
    current_speaker: str
    current_line: str
    player_character_id: str | None = None
    player_character_name: str | None = None
    visible_characters: list[str]
    messages: list[ChatMessageResponse]
    player_stats: list[str]
    map_graph_nodes: list[SessionMapNodeResponse]
    map_graph_edges: list[SessionMapEdgeResponse]
    inventory_items: list[InventoryItemResponse]
    system_log: list[str]
    scene: SceneRuntimeResponse
    assets: AssetSelectionResponse
    state: SessionStateResponse

    @classmethod
    def from_domain(cls, snapshot: SessionSnapshot) -> "SessionSnapshotResponse":
        return cls(
            id=snapshot.id,
            world_name=snapshot.world_name,
            location=snapshot.location,
            time_label=snapshot.time_label,
            current_speaker=snapshot.current_speaker,
            current_line=snapshot.current_line,
            player_character_id=snapshot.player_character_id,
            player_character_name=snapshot.player_character_name,
            visible_characters=snapshot.visible_characters,
            messages=[ChatMessageResponse.from_domain(item) for item in snapshot.messages],
            player_stats=snapshot.player_stats,
            map_graph_nodes=[SessionMapNodeResponse.from_domain(item) for item in snapshot.map_graph_nodes],
            map_graph_edges=[SessionMapEdgeResponse.from_domain(item) for item in snapshot.map_graph_edges],
            inventory_items=[InventoryItemResponse.from_domain(item) for item in snapshot.inventory_items],
            system_log=snapshot.system_log,
            scene=SceneRuntimeResponse.from_domain(snapshot.scene),
            assets=AssetSelectionResponse.from_domain(snapshot.assets),
            state=SessionStateResponse.from_domain(snapshot.state),
        )


class SessionCreateRequest(BaseModel):
    world_id: str = Field(min_length=1)
    player_character_id: str | None = None


class SwitchCharacterProposalRequest(BaseModel):
    target_character_name: str | None = None
    reason: str | None = None
    location: str | None = None
    scene_name: str | None = None
    scene_background_hint: str | None = None
    scene_tags: list[str] = Field(default_factory=list)
    visible_characters: list[str] = Field(default_factory=list)


class SwitchPlayerCharacterRequest(BaseModel):
    player_character_id: str = Field(min_length=1)
    proposal: SwitchCharacterProposalRequest | None = None


class ImageUrlPayload(BaseModel):
    url: str = Field(min_length=1)


class InputAudioPayload(BaseModel):
    data: str = Field(min_length=1)
    format: str = Field(default="wav", min_length=1)


class TextContentPart(BaseModel):
    type: Literal["text"]
    text: str = Field(default="", max_length=2000)


class ImageContentPart(BaseModel):
    type: Literal["image_url"]
    image_url: ImageUrlPayload


class AudioContentPart(BaseModel):
    type: Literal["input_audio"]
    input_audio: InputAudioPayload


ContentPartRequest = TextContentPart | ImageContentPart | AudioContentPart


class PlayerActionRequest(BaseModel):
    content: str | list[ContentPartRequest]
    resend_from_turn_index: int | None = Field(default=None, ge=1)

    @model_validator(mode="after")
    def validate_content(self) -> "PlayerActionRequest":
        if isinstance(self.content, str):
            if not self.content.strip():
                raise ValueError("content is required")
            if len(self.content) > 2000:
                raise ValueError("content is too long")
            return self

        has_text = any(
            isinstance(part, TextContentPart) and part.text.strip()
            for part in self.content
        )
        has_media = any(isinstance(part, (ImageContentPart, AudioContentPart)) for part in self.content)
        if not has_text and not has_media:
            raise ValueError("content is required")
        if sum(1 for part in self.content if isinstance(part, ImageContentPart)) > 6:
            raise ValueError("too many images")
        if sum(1 for part in self.content if isinstance(part, AudioContentPart)) > 3:
            raise ValueError("too many audio clips")
        return self
