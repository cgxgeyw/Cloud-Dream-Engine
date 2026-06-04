from pydantic import BaseModel

from backend.app.api.schemas.attributes import (
    RuntimeAttributeGroupResponse,
    RuntimeAttributeItemResponse,
)
from backend.app.api.schemas.memories import MemoryEntryResponse
from backend.app.api.schemas.sessions import SessionSnapshotResponse
from backend.app.domain.models.agent_runtime import AgentCheckpoint, AgentSession, TurnJournalEntry
from backend.app.application.services.attribute_runtime_service import (
    RuntimeAttributeGroup,
    RuntimeAttributeItem,
    SpeakerSelectionResult,
)
from backend.app.application.services.debug_service import CharacterMemoryGroup, SessionDebugReadModel
from backend.app.application.services.debug_service import CharacterPromptTrace, DirectorPromptTrace, LlmCallRecord, PromptCallRecord


class SpeakerSelectionPreviewResponse(BaseModel):
    speaker: str
    debug_lines: list[str]

    @classmethod
    def from_domain(cls, result: SpeakerSelectionResult) -> "SpeakerSelectionPreviewResponse":
        return cls(speaker=result.speaker, debug_lines=result.debug_lines)


class CharacterMemoryGroupResponse(BaseModel):
    character_id: str
    character_name: str
    memories: list[MemoryEntryResponse]

    @classmethod
    def from_domain(cls, group: CharacterMemoryGroup) -> "CharacterMemoryGroupResponse":
        return cls(
            character_id=group.character_id,
            character_name=group.character_name,
            memories=[MemoryEntryResponse.from_domain(item) for item in group.memories],
        )


class AgentSessionResponse(BaseModel):
    id: str
    session_id: str
    agent_type: str
    status: str
    connection_state: str
    scene_presence_state: str
    character_id: str | None = None
    character_name: str | None = None
    checkpoint_id: str | None = None
    last_active_turn: int
    last_ack_message_index: int
    prompt_version: str
    runtime_key: str | None = None
    initialized_at: str | None = None
    created_at: str = ""
    updated_at: str = ""

    @classmethod
    def from_domain(cls, item: AgentSession) -> "AgentSessionResponse":
        return cls(
            id=item.id,
            session_id=item.session_id,
            agent_type=item.agent_type,
            status=item.status,
            connection_state=item.connection_state,
            scene_presence_state=item.scene_presence_state,
            character_id=item.character_id,
            character_name=item.character_name,
            checkpoint_id=item.checkpoint_id,
            last_active_turn=item.last_active_turn,
            last_ack_message_index=item.last_ack_message_index,
            prompt_version=item.prompt_version,
            runtime_key=item.runtime_key,
            initialized_at=item.initialized_at,
            created_at=item.created_at,
            updated_at=item.updated_at,
        )


class AgentCheckpointResponse(BaseModel):
    id: str
    agent_session_id: str
    turn_index: int
    checkpoint_type: str
    payload: dict[str, object]
    created_at: str

    @classmethod
    def from_domain(cls, item: AgentCheckpoint) -> "AgentCheckpointResponse":
        return cls(
            id=item.id,
            agent_session_id=item.agent_session_id,
            turn_index=item.turn_index,
            checkpoint_type=item.checkpoint_type,
            payload=item.payload,
            created_at=item.created_at,
        )


class TurnJournalEntryResponse(BaseModel):
    id: str
    session_id: str
    turn_index: int
    step: str
    status: str
    payload: dict[str, object]
    created_at: str

    @classmethod
    def from_domain(cls, item: TurnJournalEntry) -> "TurnJournalEntryResponse":
        return cls(
            id=item.id,
            session_id=item.session_id,
            turn_index=item.turn_index,
            step=item.step,
            status=item.status,
            payload=item.payload,
            created_at=item.created_at,
        )


class CharacterPromptTraceResponse(BaseModel):
    turn_index: int
    step: str
    speaker: str
    prompt_trace: dict[str, object]

    @classmethod
    def from_domain(cls, item: CharacterPromptTrace) -> "CharacterPromptTraceResponse":
        return cls(
            turn_index=item.turn_index,
            step=item.step,
            speaker=item.speaker,
            prompt_trace=item.prompt_trace,
        )


class DirectorPromptTraceResponse(BaseModel):
    turn_index: int
    step: str
    prompt_trace: dict[str, object]

    @classmethod
    def from_domain(cls, item: DirectorPromptTrace) -> "DirectorPromptTraceResponse":
        return cls(
            turn_index=item.turn_index,
            step=item.step,
            prompt_trace=item.prompt_trace,
        )


class LlmCallRecordResponse(BaseModel):
    turn_index: int
    step: str
    speaker: str
    input_payload: dict[str, object]
    output_payload: object = None

    @classmethod
    def from_domain(cls, item: LlmCallRecord) -> "LlmCallRecordResponse":
        return cls(
            turn_index=item.turn_index,
            step=item.step,
            speaker=item.speaker,
            input_payload=item.input_payload,
            output_payload=item.output_payload,
        )


class PromptCallRecordResponse(BaseModel):
    turn_index: int
    step: str
    recipient_type: str
    recipient_name: str
    prompt_call: dict[str, object]

    @classmethod
    def from_domain(cls, item: PromptCallRecord) -> "PromptCallRecordResponse":
        return cls(
            turn_index=item.turn_index,
            step=item.step,
            recipient_type=item.recipient_type,
            recipient_name=item.recipient_name,
            prompt_call=item.prompt_call,
        )


class SessionDebugResponse(BaseModel):
    session: SessionSnapshotResponse
    runtime_session_attributes: list[RuntimeAttributeItemResponse]
    runtime_character_attributes: list[RuntimeAttributeGroupResponse]
    speaker_selection_preview: SpeakerSelectionPreviewResponse
    memory_groups: list[CharacterMemoryGroupResponse]
    agent_sessions: list[AgentSessionResponse]
    latest_checkpoints: list[AgentCheckpointResponse]
    turn_journal: list[TurnJournalEntryResponse]
    director_prompt_traces: list[DirectorPromptTraceResponse]
    character_prompt_traces: list[CharacterPromptTraceResponse]
    llm_calls: list[LlmCallRecordResponse]
    prompt_calls: list[PromptCallRecordResponse]
    event_chain: list[str]
    available_modules: list[str]

    @classmethod
    def from_domain(cls, debug_model: SessionDebugReadModel) -> "SessionDebugResponse":
        return cls(
            session=SessionSnapshotResponse.from_domain(debug_model.session),
            runtime_session_attributes=[
                RuntimeAttributeItemResponse(
                    schema_id=item.schema.id,
                    key=item.schema.key,
                    label=item.schema.label,
                    value_type=item.schema.value_type,
                    value=item.value.value,
                    source=item.value.source,
                    display_policy=item.schema.display_policy,
                    influence_policy=item.schema.influence_policy,
                )
                for item in debug_model.runtime_session_attributes
            ],
            runtime_character_attributes=[
                RuntimeAttributeGroupResponse(
                    owner_type=group.owner_type,
                    owner_id=group.owner_id,
                    owner_label=group.owner_label,
                    items=[
                        RuntimeAttributeItemResponse(
                            schema_id=item.schema.id,
                            key=item.schema.key,
                            label=item.schema.label,
                            value_type=item.schema.value_type,
                            value=item.value.value,
                            source=item.value.source,
                            display_policy=item.schema.display_policy,
                            influence_policy=item.schema.influence_policy,
                        )
                        for item in group.items
                    ],
                )
                for group in debug_model.runtime_character_attributes
            ],
            speaker_selection_preview=SpeakerSelectionPreviewResponse.from_domain(
                debug_model.speaker_selection_preview or SpeakerSelectionResult(speaker="系统", debug_lines=[])
            ),
            memory_groups=[CharacterMemoryGroupResponse.from_domain(item) for item in debug_model.memory_groups],
            agent_sessions=[AgentSessionResponse.from_domain(item) for item in debug_model.agent_sessions],
            latest_checkpoints=[
                AgentCheckpointResponse.from_domain(item) for item in debug_model.latest_checkpoints
            ],
            turn_journal=[TurnJournalEntryResponse.from_domain(item) for item in debug_model.turn_journal],
            director_prompt_traces=[
                DirectorPromptTraceResponse.from_domain(item) for item in debug_model.director_prompt_traces
            ],
            character_prompt_traces=[
                CharacterPromptTraceResponse.from_domain(item) for item in debug_model.character_prompt_traces
            ],
            llm_calls=[LlmCallRecordResponse.from_domain(item) for item in debug_model.llm_calls],
            prompt_calls=[PromptCallRecordResponse.from_domain(item) for item in debug_model.prompt_calls],
            event_chain=debug_model.event_chain,
            available_modules=debug_model.available_modules,
        )
