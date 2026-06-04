from dataclasses import dataclass, field
from typing import Callable

from backend.app.application.services.agent_conversation_runtime_service import AgentConversationRuntimeService
from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.dialogue_pipeline_service import (
    DialogueContext,
    DialoguePipelineService,
)
from backend.app.application.services.runtime_context_models import SceneState
from backend.app.application.services.trigger_engine_service import TriggerEvaluation
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.agent_runtime import AgentSession
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.memory import MemoryEntry
from backend.app.domain.models.session import ChatMessage, ContentPart, SessionSnapshot
from backend.app.domain.models.world import WorldDefinition


@dataclass(frozen=True)
class CharacterRuntimeResponse:
    speaker: str
    content: str
    intent: str
    emotion: str
    reasoning: str | None = None
    prompt_trace: dict[str, object] | None = None
    debug_lines: list[str] = field(default_factory=list)


class CharacterRuntimeService:
    def __init__(
        self,
        dialogue_pipeline: DialoguePipelineService,
        agent_conversation_runtime: AgentConversationRuntimeService | None = None,
    ) -> None:
        self._dialogue_pipeline = dialogue_pipeline
        self._agent_conversation_runtime = agent_conversation_runtime

    def generate_response(
        self,
        session: SessionSnapshot,
        speaker: str,
        speaker_profile: CharacterDefinition | None,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        speaker_memories: list[MemoryEntry],
        recent_dialogue: list[ChatMessage],
        scene_state: SceneState | None,
        director_decision: DirectorDecision,
        trigger_evaluation: TriggerEvaluation,
        agent_session: AgentSession | None = None,
        turn_index: int = 0,
        on_stream_text: Callable[[str], None] | None = None,
        on_stream_reasoning: Callable[[str], None] | None = None,
        on_stream_full_text: Callable[[str], None] | None = None,
        player_media_parts: list[ContentPart] | None = None,
    ) -> CharacterRuntimeResponse:
        context = DialogueContext(
            session=session,
            speaker=speaker,
            speaker_profile=speaker_profile,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
            speaker_memories=speaker_memories,
            recent_messages=recent_dialogue,
            director_decision=director_decision,
            trigger_evaluation=trigger_evaluation,
            scene_state=scene_state,
            player_media_parts=list(player_media_parts or []),
        )
        if agent_session is not None and self._agent_conversation_runtime is not None:
            recent_dialogue_rounds = speaker_profile.recent_dialogue_rounds if speaker_profile else 2
            result = self._agent_conversation_runtime.generate_character_turn(
                agent_session=agent_session,
                turn_index=turn_index,
                context=context,
                recent_dialogue_rounds=recent_dialogue_rounds,
                on_stream_text=on_stream_text,
                on_stream_reasoning=on_stream_reasoning,
                on_stream_full_text=on_stream_full_text,
            )
        else:
            result = self._dialogue_pipeline.generate(context)

        debug_lines = [
            f"CharacterRuntime speaker={speaker}",
            f"CharacterRuntime intent={result.intent}",
            f"CharacterRuntime emotion={result.emotion}",
            *result.debug_lines,
        ]

        return CharacterRuntimeResponse(
            speaker=result.speaker,
            content=result.content,
            intent=result.intent,
            emotion=result.emotion,
            reasoning=result.reasoning,
            prompt_trace=result.prompt_trace,
            debug_lines=debug_lines,
        )

    def clean_streaming_content(self, content: str) -> str:
        return self._dialogue_pipeline._clean_dialogue_text(content)

    def build_prompt_trace_preview(
        self,
        session: SessionSnapshot,
        speaker: str,
        speaker_profile: CharacterDefinition | None,
        world_profile: WorldDefinition | None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        speaker_memories: list[MemoryEntry],
        recent_dialogue: list[ChatMessage],
        scene_state: SceneState | None,
        director_decision: DirectorDecision,
        trigger_evaluation: TriggerEvaluation,
    ) -> dict[str, object]:
        context = DialogueContext(
            session=session,
            speaker=speaker,
            speaker_profile=speaker_profile,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
            speaker_memories=speaker_memories,
            recent_messages=recent_dialogue,
            director_decision=director_decision,
            trigger_evaluation=trigger_evaluation,
            scene_state=scene_state,
        )
        return self._dialogue_pipeline.build_runtime_prompt_call(context, stage="玩家第一次输入")
