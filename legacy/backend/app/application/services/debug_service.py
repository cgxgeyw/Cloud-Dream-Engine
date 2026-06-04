from dataclasses import dataclass, field

from backend.app.application.services.agent_runtime_manager_service import AgentRuntimeManagerService
from backend.app.application.services.attribute_runtime_service import (
    AttributeRuntimeService,
    RuntimeAttributeGroup,
    RuntimeAttributeItem,
    SpeakerSelectionResult,
)
from backend.app.application.services.memory_service import MemoryQueryService
from backend.app.application.services.catalog_service import CatalogQueryService
from backend.app.application.services.session_service import SessionQueryService
from backend.app.domain.models.agent_runtime import AgentCheckpoint, AgentSession, TurnJournalEntry
from backend.app.domain.models.memory import MemoryEntry
from backend.app.domain.models.session import SessionSnapshot


@dataclass(frozen=True)
class CharacterMemoryGroup:
    character_id: str
    character_name: str
    memories: list[MemoryEntry] = field(default_factory=list)


@dataclass(frozen=True)
class CharacterPromptTrace:
    turn_index: int
    step: str
    speaker: str
    prompt_trace: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True)
class DirectorPromptTrace:
    turn_index: int
    step: str
    prompt_trace: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True)
class LlmCallRecord:
    turn_index: int
    step: str
    speaker: str
    input_payload: dict[str, object] = field(default_factory=dict)
    output_payload: object = None


@dataclass(frozen=True)
class PromptCallRecord:
    turn_index: int
    step: str
    recipient_type: str
    recipient_name: str
    prompt_call: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True)
class SessionDebugReadModel:
    session: SessionSnapshot
    runtime_session_attributes: list[RuntimeAttributeItem] = field(default_factory=list)
    runtime_character_attributes: list[RuntimeAttributeGroup] = field(default_factory=list)
    speaker_selection_preview: SpeakerSelectionResult | None = None
    memory_groups: list[CharacterMemoryGroup] = field(default_factory=list)
    agent_sessions: list[AgentSession] = field(default_factory=list)
    latest_checkpoints: list[AgentCheckpoint] = field(default_factory=list)
    turn_journal: list[TurnJournalEntry] = field(default_factory=list)
    director_prompt_traces: list[DirectorPromptTrace] = field(default_factory=list)
    character_prompt_traces: list[CharacterPromptTrace] = field(default_factory=list)
    llm_calls: list[LlmCallRecord] = field(default_factory=list)
    prompt_calls: list[PromptCallRecord] = field(default_factory=list)
    event_chain: list[str] = field(default_factory=list)
    available_modules: list[str] = field(default_factory=list)


class DebugReadService:
    def __init__(
        self,
        session_queries: SessionQueryService,
        catalog_queries: CatalogQueryService,
        attribute_runtime: AttributeRuntimeService,
        memory_queries: MemoryQueryService,
        agent_runtime_manager: AgentRuntimeManagerService,
    ) -> None:
        self._session_queries = session_queries
        self._catalog_queries = catalog_queries
        self._attribute_runtime = attribute_runtime
        self._memory_queries = memory_queries
        self._agent_runtime_manager = agent_runtime_manager

    def get_session_debug(self, session_id: str) -> SessionDebugReadModel | None:
        session = self._session_queries.get_session(session_id)
        if session is None:
            return None
        world = next((item for item in self._catalog_queries.list_worlds() if item.name == session.world_name), None)
        if world is None:
            return None

        runtime_session_attributes, runtime_character_attributes = self._attribute_runtime.list_debug_attributes(
            session_id=session_id
        )
        selection_preview = self._attribute_runtime.select_next_speaker(
            session_id=session_id,
            visible_character_names=session.visible_characters,
            player_input=session.messages[-1].content if session.messages else None,
        )

        world_characters = {item.id: item for item in self._catalog_queries.list_characters_for_world(world.id)}
        debug_character_pairs: list[tuple[str, str]] = []
        seen_character_ids: set[str] = set()

        for group in runtime_character_attributes:
            _, character_id = group.owner_id.split(":", 1)
            if character_id in seen_character_ids:
                continue
            seen_character_ids.add(character_id)
            debug_character_pairs.append((character_id, group.owner_label))

        visible_character_names = set(session.visible_characters)
        for character in world_characters.values():
            if character.id in seen_character_ids:
                continue
            if character.name in visible_character_names or character.id == session.player_character_id:
                seen_character_ids.add(character.id)
                debug_character_pairs.append((character.id, character.name))

        memory_groups = []
        for character_id, character_name in debug_character_pairs:
            memory_groups.append(
                CharacterMemoryGroup(
                    character_id=character_id,
                    character_name=character_name,
                    memories=self._memory_queries.list_for_character(
                        world_id=world.id,
                        character_id=character_id,
                        session_id=session_id,
                        conversation_id=session_id,
                        scene_id=session.scene.scene_id,
                        limit=6,
                    ),
                )
            )

        agent_sessions = self._agent_runtime_manager.list_agent_sessions(session_id)
        latest_checkpoints = [
            checkpoint
            for checkpoint in (
                self._agent_runtime_manager.get_latest_checkpoint(agent_session.id)
                for agent_session in agent_sessions
            )
            if checkpoint is not None
        ]
        turn_journal = self._agent_runtime_manager.list_turn_journal(session_id=session_id)[:24]
        director_prompt_traces = self._build_director_prompt_traces(turn_journal)
        character_prompt_traces = self._build_character_prompt_traces(turn_journal)
        llm_calls = self._build_llm_calls(turn_journal)
        prompt_calls = self._build_prompt_calls(turn_journal)

        event_chain = self._build_event_chain(session.system_log)
        available_modules = [
            "Session Orchestrator",
            "World Director",
            "Scene Runtime Manager",
            "Trigger Engine",
            "Rule Engine",
            "Inventory Runtime",
            "Speaker Selector",
            "Character Runtime",
            "Dialogue Pipeline",
            "Memory Pipeline",
            "State Engine",
        ]

        return SessionDebugReadModel(
            session=session,
            runtime_session_attributes=runtime_session_attributes,
            runtime_character_attributes=runtime_character_attributes,
            speaker_selection_preview=selection_preview,
            memory_groups=memory_groups,
            agent_sessions=agent_sessions,
            latest_checkpoints=latest_checkpoints,
            turn_journal=turn_journal,
            director_prompt_traces=director_prompt_traces,
            character_prompt_traces=character_prompt_traces,
            llm_calls=llm_calls,
            prompt_calls=prompt_calls,
            event_chain=event_chain,
            available_modules=available_modules,
        )

    def _build_event_chain(self, system_log: list[str]) -> list[str]:
        if not system_log:
            return []

        categorized: list[str] = []
        for entry in system_log:
            if "世界主控" in entry:
                categorized.append(f"World Director -> {entry}")
            elif "触发器" in entry:
                categorized.append(f"Trigger Engine -> {entry}")
            elif "规则" in entry:
                categorized.append(f"Rule Engine -> {entry}")
            elif "背包系统" in entry or "InventoryRuntime" in entry:
                categorized.append(f"Inventory Runtime -> {entry}")
            elif "状态引擎" in entry:
                categorized.append(f"State Engine -> {entry}")
            elif "发言排序" in entry:
                categorized.append(f"Speaker Selector -> {entry}")
            elif "CharacterRuntime" in entry or "DialoguePipeline" in entry:
                categorized.append(f"Character Runtime -> {entry}")
            else:
                categorized.append(entry)
        return categorized

    def _build_character_prompt_traces(
        self,
        turn_journal: list[TurnJournalEntry],
    ) -> list[CharacterPromptTrace]:
        traces: list[CharacterPromptTrace] = []
        for entry in turn_journal:
            if not entry.step.startswith("speaker_") or entry.status != "completed":
                continue
            prompt_trace = entry.payload.get("prompt_trace")
            speaker = str(entry.payload.get("speaker") or "").strip()
            if not isinstance(prompt_trace, dict) or not speaker:
                continue
            traces.append(
                CharacterPromptTrace(
                    turn_index=entry.turn_index,
                    step=entry.step,
                    speaker=speaker,
                    prompt_trace=prompt_trace,
                )
            )
        return traces

    def _build_director_prompt_traces(
        self,
        turn_journal: list[TurnJournalEntry],
    ) -> list[DirectorPromptTrace]:
        traces: list[DirectorPromptTrace] = []
        for entry in turn_journal:
            if entry.step != "director_completed" or entry.status != "completed":
                continue
            prompt_trace = entry.payload.get("prompt_trace")
            if not isinstance(prompt_trace, dict):
                continue
            traces.append(
                DirectorPromptTrace(
                    turn_index=entry.turn_index,
                    step=entry.step,
                    prompt_trace=prompt_trace,
                )
            )
        return traces

    def _build_llm_calls(self, turn_journal: list[TurnJournalEntry]) -> list[LlmCallRecord]:
        records: list[LlmCallRecord] = []
        for entry in turn_journal:
            if entry.status != "completed":
                continue
            prompt_trace = entry.payload.get("prompt_trace")
            if not isinstance(prompt_trace, dict):
                continue
            if entry.step == "director_completed":
                records.append(
                    LlmCallRecord(
                        turn_index=entry.turn_index,
                        step=entry.step,
                        speaker="世界主控",
                        input_payload=prompt_trace,
                        output_payload=entry.payload.get("llm_output"),
                    )
                )
                continue
            if entry.step.startswith("speaker_"):
                speaker = str(entry.payload.get("speaker") or "").strip()
                if not speaker:
                    continue
                records.append(
                    LlmCallRecord(
                        turn_index=entry.turn_index,
                        step=entry.step,
                        speaker=speaker,
                        input_payload=prompt_trace,
                        output_payload=entry.payload.get("llm_output"),
                    )
                )
        return records

    def _build_prompt_calls(self, turn_journal: list[TurnJournalEntry]) -> list[PromptCallRecord]:
        records: list[PromptCallRecord] = []
        for entry in turn_journal:
            if entry.status != "completed":
                continue
            prompt_trace = entry.payload.get("prompt_trace")
            if not isinstance(prompt_trace, dict):
                continue
            recipient_type = str(prompt_trace.get("recipient_type") or "").strip()
            recipient_name = str(prompt_trace.get("recipient_name") or "").strip()
            if not recipient_type or not recipient_name:
                if entry.step == "director_completed":
                    recipient_type = "director"
                    recipient_name = "世界主控"
                elif entry.step.startswith("speaker_"):
                    recipient_type = "character"
                    recipient_name = str(entry.payload.get("speaker") or "").strip()
            if not recipient_type or not recipient_name:
                continue
            records.append(
                PromptCallRecord(
                    turn_index=entry.turn_index,
                    step=entry.step,
                    recipient_type=recipient_type,
                    recipient_name=recipient_name,
                    prompt_call=prompt_trace,
                )
            )
        return records
