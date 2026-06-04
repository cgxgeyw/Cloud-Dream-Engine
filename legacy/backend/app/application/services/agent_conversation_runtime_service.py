from __future__ import annotations

import json
from typing import Any, Callable

from backend.app.application.services.agent_runtime_manager_service import AgentRuntimeManagerService
from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.dialogue_pipeline_service import DialogueContext, DialoguePipelineService, DialogueResult
from backend.app.application.services.text_generation_service import TextGenerationService, TextGenerationUnavailableError
from backend.app.application.services.world_director_service import DirectorDecision, WorldDirectorService
from backend.app.domain.models.agent_runtime import AgentSession
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.world import WorldDefinition


class AgentConversationRuntimeService:
    def __init__(
        self,
        runtime_manager: AgentRuntimeManagerService,
        text_generation: TextGenerationService,
        world_director: WorldDirectorService,
        dialogue_pipeline: DialoguePipelineService,
    ) -> None:
        self._runtime_manager = runtime_manager
        self._text_generation = text_generation
        self._world_director = world_director
        self._dialogue_pipeline = dialogue_pipeline

    def plan_director_turn(
        self,
        *,
        agent_session: AgentSession,
        turn_index: int,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_character_names: list[str] | None = None,
        world_characters: list[CharacterDefinition] | None = None,
        player_input: str,
        session_attributes: list[RuntimeAttributeItem],
        recent_dialogue_rounds: int = 2,
        on_stream_full_text: Callable[[str], None] | None = None,
    ) -> DirectorDecision:
        fallback, director_config = self._world_director.build_heuristic_decision(
            session=session,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
        )
        prompt_trace = self._world_director.build_runtime_prompt_call(
            session=session,
            world_profile=world_profile,
            player_input=player_input,
            session_attributes=session_attributes,
            fallback=fallback,
            director_config=director_config,
            character_profiles=world_characters,
        )
        runtime_messages = [
            self._normalize_message(item)
            for item in prompt_trace.get("messages", [])
            if isinstance(item, dict)
        ]
        response, tool_loop_messages, scene_change_arguments, switch_player_arguments = self._run_director_agent_loop(
            messages=runtime_messages,
            session=session,
            world_profile=world_profile,
            world_characters=world_characters or [],
            fallback=fallback,
            temperature=0.35,
            on_stream_full_text=on_stream_full_text,
        )
        if tool_loop_messages:
            prompt_trace = self._world_director.build_runtime_prompt_call(
                session=session,
                world_profile=world_profile,
                player_input=player_input,
                session_attributes=session_attributes,
                fallback=fallback,
                director_config=director_config,
                character_profiles=world_characters,
                tool_loop_messages=tool_loop_messages,
                stage="工具调用回合",
            )
        if response.payload is None:
            self._runtime_manager.update_agent_state(
                agent_session_id=agent_session.id,
                status="failed",
                connection_state="failed",
                last_active_turn=turn_index,
            )
            return fallback

        raw_director_text = self._compose_director_raw_response(
            reasoning=response.raw_reasoning,
            content=response.raw_content,
        ) or ""
        return_processing = self._world_director.apply_return_processing(
            director_config=director_config,
            raw_text=raw_director_text,
        )
        processed_payload = (
            self._text_generation._parse_json_payload(return_processing.after)
            if return_processing.after.strip()
            else None
        ) or response.payload

        result = (
            self._world_director.build_decision_from_switch_player_character_tool(
                arguments=switch_player_arguments,
                session=session,
                fallback=fallback,
            )
            if switch_player_arguments is not None
            else self._world_director.build_decision_from_change_scene_tool(
                arguments=scene_change_arguments,
                session=session,
                fallback=fallback,
            )
            if scene_change_arguments is not None
            else self._world_director.parse_runtime_payload(
                payload=processed_payload,
                session=session,
                world_profile=world_profile,
                director_config=director_config,
                fallback=fallback,
            )
        )
        result = DirectorDecision(
            world_phase=result.world_phase,
            next_location=result.next_location,
            next_scene_name=result.next_scene_name,
            next_scene_background_hint=result.next_scene_background_hint,
            background_asset_name=result.background_asset_name,
            background_asset_path=result.background_asset_path,
            background_generation_prompt=result.background_generation_prompt,
            next_scene_tags=list(result.next_scene_tags),
            next_time_label=result.next_time_label,
            generated_characters=list(result.generated_characters),
            character_visual_directives=list(result.character_visual_directives),
            scene_change=result.scene_change,
            scene_visible_characters=list(result.scene_visible_characters) if result.scene_visible_characters is not None else None,
            planned_speakers=list(result.planned_speakers),
            switch_character_proposal=result.switch_character_proposal,
            raw_model_response=raw_director_text or None,
            prompt_trace=None,
        )
        prompt_trace = self._world_director.attach_prompt_call_result(
            prompt_trace,
            raw_model_return=raw_director_text or None,
            return_processing=return_processing,
            processed_model_return=processed_payload,
            written_result={
                "world_phase": result.world_phase,
                "next_location": result.next_location,
                "next_scene_name": result.next_scene_name,
                "next_scene_background_hint": result.next_scene_background_hint,
                "background_asset_name": result.background_asset_name,
                "background_asset_path": result.background_asset_path,
                "background_generation_prompt": result.background_generation_prompt,
                "next_time_label": result.next_time_label,
                "scene_visible_characters": result.scene_visible_characters,
                "planned_speakers": result.planned_speakers,
                "character_visual_directives": [
                    {
                        "character_name": item.character_name,
                        "portrait_hint": item.portrait_hint,
                        "portrait_asset_name": item.portrait_asset_name,
                        "portrait_asset_path": item.portrait_asset_path,
                        "generation_prompt": item.generation_prompt,
                    }
                    for item in result.character_visual_directives
                ],
                "scene_change": {
                    "scene_name": result.scene_change.scene_name,
                    "scene_description": result.scene_change.scene_description,
                    "all_characters": result.scene_change.all_characters,
                    "player_character_name": result.scene_change.player_character_name,
                } if result.scene_change is not None else None,
            },
        )
        result = DirectorDecision(
            **{
                **result.__dict__,
                "prompt_trace": prompt_trace,
            }
        )
        self._persist_incremental_transcript(
            agent_session=agent_session,
            turn_index=turn_index,
            user_message={"role": "user", "content": str(prompt_trace.get("final_sent_content") or "")},
            assistant_message={"role": "assistant", "content": json.dumps(response.payload, ensure_ascii=False)},
            state_payload={
                "director_config": director_config,
                "last_result": {
                    "world_phase": result.world_phase,
                    "next_location": result.next_location,
                    "next_scene_name": result.next_scene_name,
                    "next_scene_background_hint": result.next_scene_background_hint,
                    "next_time_label": result.next_time_label,
                    "scene_visible_characters": result.scene_visible_characters,
                    "planned_speakers": result.planned_speakers,
                    "character_visual_directives": [
                        {
                            "character_name": item.character_name,
                            "portrait_hint": item.portrait_hint,
                            "portrait_asset_name": item.portrait_asset_name,
                            "portrait_asset_path": item.portrait_asset_path,
                            "generation_prompt": item.generation_prompt,
                        }
                        for item in result.character_visual_directives
                    ],
                    "scene_change": {
                        "scene_name": result.scene_change.scene_name,
                        "player_character_name": result.scene_change.player_character_name,
                        "all_characters": result.scene_change.all_characters,
                    } if result.scene_change is not None else None,
                },
                "prompt_trace": prompt_trace,
            },
            last_ack_message_index=len(session.messages) + 1,
        )
        return result

    def _run_director_agent_loop(
        self,
        *,
        messages: list[dict[str, object]],
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_characters: list[CharacterDefinition],
        fallback: DirectorDecision,
        temperature: float,
        on_stream_full_text: Callable[[str], None] | None,
    ):
        current_messages = list(messages)
        tool_loop_messages: list[dict[str, object]] = []
        scene_change_arguments: dict[str, object] | None = None
        switch_player_arguments: dict[str, object] | None = None
        response = None
        for iteration in range(4):
            response = self._text_generation.generate_json_messages(
                messages=current_messages,
                temperature=temperature,
                on_stream_full_text=on_stream_full_text if iteration == 0 else None,
            )
            payload = response.payload if response.payload is not None else {}
            tool_calls = payload.get("tool_calls") if isinstance(payload, dict) else None
            if not isinstance(tool_calls, list) or not tool_calls:
                return response, tool_loop_messages, scene_change_arguments, switch_player_arguments

            assistant_message = {"role": "assistant", "content": json.dumps(payload, ensure_ascii=False)}
            tool_results = []
            for raw_call in tool_calls[:4]:
                if not isinstance(raw_call, dict):
                    continue
                result = self._execute_director_tool_call(
                    raw_call,
                    session=session,
                    world_profile=world_profile,
                    world_characters=world_characters,
                    fallback=fallback,
                )
                tool_results.append(result)
                if result.get("tool_name") == "change_scene" and result.get("ok") is True:
                    args = result.get("arguments")
                    if isinstance(args, dict):
                        scene_change_arguments = args
                if result.get("tool_name") == "switch_player_character" and result.get("ok") is True:
                    args = result.get("arguments")
                    if isinstance(args, dict):
                        switch_player_arguments = args

            tool_message = {
                "role": "user",
                "content": json.dumps(
                    {
                        "tool_results": tool_results,
                    },
                    ensure_ascii=False,
                ),
            }
            tool_loop_messages.extend([assistant_message, tool_message])
            current_messages.extend([assistant_message, tool_message])

        return response, tool_loop_messages, scene_change_arguments, switch_player_arguments

    def _execute_director_tool_call(
        self,
        raw_call: dict[str, object],
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_characters: list[CharacterDefinition],
        fallback: DirectorDecision,
    ) -> dict[str, object]:
        tool_name = str(
            raw_call.get("tool_name")
            or raw_call.get("name")
            or raw_call.get("tool")
            or raw_call.get("toolName")
            or ""
        ).strip()
        function_payload = raw_call.get("function")
        if isinstance(function_payload, dict):
            tool_name = str(function_payload.get("name") or tool_name).strip()
            arguments = function_payload.get("arguments")
        else:
            arguments = raw_call.get("arguments", raw_call.get("args", raw_call.get("input")))
        if isinstance(arguments, str):
            try:
                parsed_arguments = json.loads(arguments)
            except json.JSONDecodeError:
                parsed_arguments = None
            arguments = parsed_arguments if isinstance(parsed_arguments, dict) else {}
        if not isinstance(arguments, dict):
            arguments = {}
        call_id = str(raw_call.get("id") or f"call-{tool_name or 'unknown'}").strip()
        if tool_name == "list_scenes":
            scene_names = list(dict.fromkeys([
                *(world_profile.map_nodes if world_profile is not None else []),
                session.location,
                session.scene.name,
            ]))
            return {
                "id": call_id,
                "tool_name": tool_name,
                "ok": True,
                "scenes": [
                    {"scene_name": name, "is_current": name in {session.location, session.scene.name}}
                    for name in scene_names
                    if str(name).strip()
                ],
            }
        if tool_name == "list_characters":
            return {
                "id": call_id,
                "tool_name": tool_name,
                "ok": True,
                "characters": [
                    {
                        "name": character.name,
                        "role": character.role,
                        "is_player": character.name == session.player_character_name,
                        "is_visible": character.name in session.visible_characters,
                    }
                    for character in world_characters
                ],
            }
        if tool_name == "change_scene":
            validation_error = self._validate_change_scene_arguments(arguments)
            if validation_error:
                return {
                    "id": call_id,
                    "tool_name": tool_name,
                    "ok": False,
                    "error": validation_error,
                    "arguments": arguments,
                }
            decision = self._world_director.build_decision_from_change_scene_tool(
                arguments=arguments,
                session=session,
                fallback=fallback,
            )
            return {
                "id": call_id,
                "tool_name": tool_name,
                "ok": True,
                "arguments": arguments,
                "planned_state": {
                    "scene_name": decision.next_scene_name,
                    "scene_description": decision.next_scene_background_hint,
                    "player_character": decision.scene_change.player_character_name if decision.scene_change else session.player_character_name,
                    "visible_characters": decision.scene_visible_characters or [],
                    "planned_speakers": decision.planned_speakers,
                    "generated_characters": [item.name for item in decision.generated_characters],
                },
            }
        if tool_name == "switch_player_character":
            validation_error = self._validate_switch_player_character_arguments(arguments, world_characters)
            if validation_error:
                return {
                    "id": call_id,
                    "tool_name": tool_name,
                    "ok": False,
                    "error": validation_error,
                    "arguments": arguments,
                }
            target_name = str(arguments.get("target_character_name") or "").strip()
            visible_characters = arguments.get("visible_characters")
            resolved_visible = [
                str(item).strip()
                for item in visible_characters
                if str(item).strip() and str(item).strip() != target_name
            ] if isinstance(visible_characters, list) else [
                name
                for name in [*session.visible_characters, session.player_character_name]
                if name and name != target_name
            ]
            return {
                "id": call_id,
                "tool_name": tool_name,
                "ok": True,
                "arguments": arguments,
                "planned_state": {
                    "player_character": target_name,
                    "scene_name": str(arguments.get("scene_name") or session.scene.name or session.location),
                    "visible_characters": list(dict.fromkeys(resolved_visible)),
                },
            }
        return {
            "id": call_id,
            "tool_name": tool_name or "unknown",
            "ok": False,
            "error": "unknown_tool",
        }

    def _validate_change_scene_arguments(self, arguments: dict[str, object]) -> str:
        required_string_fields = ("scene_name", "scene_description", "player_character")
        for field_name in required_string_fields:
            if not str(arguments.get(field_name) or "").strip():
                return f"missing_{field_name}"
        all_characters = arguments.get("all_characters")
        if not isinstance(all_characters, list) or not [str(item).strip() for item in all_characters if str(item).strip()]:
            return "missing_all_characters"
        player_character = str(arguments.get("player_character") or "").strip()
        if player_character and player_character not in {str(item).strip() for item in all_characters}:
            return "player_character_not_in_all_characters"
        return ""

    def _validate_switch_player_character_arguments(
        self,
        arguments: dict[str, object],
        world_characters: list[CharacterDefinition],
    ) -> str:
        target_name = str(arguments.get("target_character_name") or "").strip()
        if not target_name:
            return "missing_target_character_name"
        character_names = {character.name.strip() for character in world_characters if character.name.strip()}
        if target_name not in character_names:
            return "target_character_not_found"
        return ""

    def generate_character_turn(
        self,
        *,
        agent_session: AgentSession,
        turn_index: int,
        context: DialogueContext,
        recent_dialogue_rounds: int = 2,
        on_stream_text: Callable[[str], None] | None = None,
        on_stream_reasoning: Callable[[str], None] | None = None,
        on_stream_full_text: Callable[[str], None] | None = None,
    ) -> DialogueResult:
        prompt_trace = self._dialogue_pipeline.build_runtime_prompt_call(context)
        messages = [
            self._normalize_message(item)
            for item in prompt_trace.get("messages", [])
            if isinstance(item, dict)
        ]
        preferred_model = context.speaker_profile.model if context.speaker_profile and context.speaker_profile.model else None
        response = self._text_generation.generate_json_messages(
            messages=messages,
            preferred_model=preferred_model,
            temperature=0.7,
            on_stream_text=on_stream_text,
            on_stream_reasoning=on_stream_reasoning,
            on_stream_full_text=on_stream_full_text,
        )

        if response.payload is None:
            fallback_result = self._build_character_fallback_result(
                context=context,
                response=response,
                prompt_trace=prompt_trace,
            )
            if fallback_result is None:
                self._runtime_manager.update_agent_state(
                    agent_session_id=agent_session.id,
                    status="failed",
                    connection_state="failed",
                    last_active_turn=turn_index,
                )
                raise self._build_character_generation_error(response)
            result = fallback_result
        else:
            director_config = (
                world_profile.director_config
                if (world_profile := context.world_profile) is not None
                else {}
            )
            raw_character_text = self._compose_director_raw_response(
                reasoning=response.raw_reasoning,
                content=response.raw_content,
            ) or ""
            return_processing = self._dialogue_pipeline.apply_return_processing(
                director_config=director_config,
                raw_text=raw_character_text,
            )
            processed_payload = (
                self._text_generation._parse_json_payload(return_processing.after)
                if return_processing.after.strip()
                else None
            ) or response.payload
            parsed = self._dialogue_pipeline.parse_runtime_payload(
                context=context,
                payload=processed_payload,
                debug_lines=response.debug_lines,
            )
            result = DialogueResult(
                speaker=parsed.speaker,
                content=parsed.content,
                intent=parsed.intent,
                emotion=parsed.emotion,
                reasoning=(response.raw_reasoning or "").strip() or None,
                prompt_trace=None,
                debug_lines=list(parsed.debug_lines),
            )
            prompt_trace = self._dialogue_pipeline.attach_prompt_call_result(
                prompt_trace,
                raw_model_return=raw_character_text or None,
                return_processing=return_processing,
                processed_model_return=processed_payload,
                written_result={
                    "speaker": result.speaker,
                    "content": result.content,
                    "intent": result.intent,
                    "emotion": result.emotion,
                },
            )
            result = DialogueResult(
                speaker=result.speaker,
                content=result.content,
                intent=result.intent,
                emotion=result.emotion,
                reasoning=result.reasoning,
                prompt_trace=prompt_trace,
                debug_lines=list(result.debug_lines),
            )

        assistant_content = json.dumps(response.payload, ensure_ascii=False) if response.payload is not None else result.content
        self._persist_incremental_transcript(
            agent_session=agent_session,
            turn_index=turn_index,
            user_message={"role": "user", "content": str(prompt_trace.get("final_sent_content") or "")},
            assistant_message={"role": "assistant", "content": assistant_content},
            state_payload={
                "speaker": result.speaker,
                "intent": result.intent,
                "emotion": result.emotion,
                "prompt_trace": prompt_trace,
            },
            last_ack_message_index=len(context.session.messages),
        )
        return result

    def _build_character_fallback_result(
        self,
        *,
        context: DialogueContext,
        response,
        prompt_trace: dict[str, object],
    ) -> DialogueResult | None:
        raw_content = str(response.raw_content or "").strip()
        if not raw_content:
            return None
        return DialogueResult(
            speaker=context.speaker,
            content=raw_content,
            intent="advance_objective",
            emotion="focused",
            reasoning=(response.raw_reasoning or "").strip() or None,
            prompt_trace=prompt_trace,
            debug_lines=[
                "DialoguePipeline fallback_from_raw_content",
                f"DialoguePipeline speaker={context.speaker}",
                "DialoguePipeline intent=advance_objective",
                "DialoguePipeline emotion=focused",
                f"DialoguePipeline recalled_history={len(context.speaker_memories)}",
                *response.debug_lines,
            ],
        )

    def _build_character_generation_error(self, response) -> TextGenerationUnavailableError:
        if any("no_text_model_configured" in line for line in response.debug_lines):
            return TextGenerationUnavailableError(
                "文本模型未配置，无法生成角色发言。",
                debug_lines=response.debug_lines,
            )
        if any("missing_base_url" in line for line in response.debug_lines):
            return TextGenerationUnavailableError(
                "文本模型缺少调用地址，无法生成角色发言。",
                debug_lines=response.debug_lines,
            )
        return TextGenerationUnavailableError(
            "文本模型不可用，无法生成角色发言。",
            debug_lines=response.debug_lines,
        )

    def _compose_director_raw_response(self, *, reasoning: str | None, content: str | None) -> str | None:
        parts: list[str] = []
        if isinstance(reasoning, str) and reasoning.strip():
            parts.append(reasoning.strip())
        if isinstance(content, str) and content.strip():
            parts.append(content.strip())
        combined = "\n\n".join(parts).strip()
        return combined or None

    def _build_messages_for_director(
        self,
        *,
        agent_session: AgentSession,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_character_names: list[str] | None,
        director_config: dict[str, object],
        recent_dialogue_rounds: int,
    ) -> list[dict[str, object]]:
        init_messages = self._ensure_director_init(
            agent_session=agent_session,
            session=session,
            world_profile=world_profile,
            world_character_names=world_character_names,
            director_config=director_config,
        )
        return init_messages

    def _build_messages_for_character(
        self,
        *,
        agent_session: AgentSession,
        context: DialogueContext,
        recent_dialogue_rounds: int,
    ) -> list[dict[str, object]]:
        init_messages = self._ensure_character_init(
            agent_session=agent_session,
            context=context,
        )
        return init_messages

    def _load_recent_incremental_turns(
        self,
        agent_session_id: str,
        recent_dialogue_rounds: int,
    ) -> list[dict[str, object]]:
        checkpoints = self._runtime_manager.list_checkpoints(agent_session_id, limit=50)
        turn_state_checkpoints = [cp for cp in checkpoints if cp.checkpoint_type == "turn_state"]
        recent = turn_state_checkpoints[: max(0, recent_dialogue_rounds)]
        recent.reverse()

        messages: list[dict[str, object]] = []
        for checkpoint in recent:
            delta = checkpoint.payload.get("delta_messages")
            if isinstance(delta, list):
                messages.extend(self._normalize_message(item) for item in delta)
        return messages

    def _ensure_director_init(
        self,
        *,
        agent_session: AgentSession,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_character_names: list[str] | None,
        director_config: dict[str, object],
    ) -> list[dict[str, object]]:
        checkpoints = self._runtime_manager.list_checkpoints(agent_session.id, limit=50)
        init_checkpoint = next(
            (
                checkpoint
                for checkpoint in checkpoints
                if checkpoint.checkpoint_type == "initialization"
                and (
                    checkpoint.payload.get("initialized") is True
                    or (
                        isinstance(checkpoint.payload.get("messages"), list)
                        and checkpoint.payload.get("messages")
                    )
                )
            ),
            None,
        )
        if init_checkpoint is not None:
            raw_messages = init_checkpoint.payload.get("messages")
            if isinstance(raw_messages, list):
                return [self._normalize_message(item) for item in raw_messages]
            return []

        system_prompt = self._world_director.build_runtime_system_prompt(director_config=director_config)
        base_messages = self._build_director_init_messages(
            system_prompt=system_prompt,
            session=session,
            world_profile=world_profile,
            world_character_names=world_character_names,
        )
        self._runtime_manager.append_checkpoint(
            agent_session_id=agent_session.id,
            turn_index=0,
            checkpoint_type="initialization",
            payload={
                "initialized": True,
                "messages": base_messages,
                "world_name": world_profile.name if world_profile else "",
            },
        )
        now = self._now()
        self._runtime_manager.update_agent_state(
            agent_session_id=agent_session.id,
            status="active",
            connection_state="connected",
            last_active_turn=0,
            initialized_at=now,
        )
        return base_messages

    def _ensure_character_init(
        self,
        *,
        agent_session: AgentSession,
        context: DialogueContext,
    ) -> list[dict[str, object]]:
        checkpoints = self._runtime_manager.list_checkpoints(agent_session.id, limit=50)
        init_checkpoint = next(
            (
                checkpoint
                for checkpoint in checkpoints
                if checkpoint.checkpoint_type == "initialization"
                and (
                    checkpoint.payload.get("initialized") is True
                    or isinstance(checkpoint.payload.get("messages"), list)
                )
            ),
            None,
        )
        if init_checkpoint is not None:
            raw_messages = init_checkpoint.payload.get("messages")
            if isinstance(raw_messages, list):
                return [self._normalize_message(item) for item in raw_messages]
            return []

        system_prompt = self._dialogue_pipeline.build_runtime_system_prompt(context)
        init_payload = self._dialogue_pipeline.build_runtime_init_payload(context)
        base_messages = self._build_character_init_messages(
            system_prompt=system_prompt,
            init_payload=init_payload,
        )
        self._runtime_manager.append_checkpoint(
            agent_session_id=agent_session.id,
            turn_index=0,
            checkpoint_type="initialization",
            payload={
                "initialized": True,
                "messages": base_messages,
                "speaker": context.speaker,
                "character_id": context.speaker_profile.id if context.speaker_profile else None,
            },
        )
        now = self._now()
        self._runtime_manager.update_agent_state(
            agent_session_id=agent_session.id,
            status="active",
            connection_state="connected",
            last_active_turn=0,
            initialized_at=now,
        )
        return base_messages

    def _persist_incremental_transcript(
        self,
        *,
        agent_session: AgentSession,
        turn_index: int,
        user_message: dict[str, object],
        assistant_message: dict[str, object],
        state_payload: dict[str, object],
        last_ack_message_index: int,
    ) -> None:
        checkpoint = self._runtime_manager.append_checkpoint(
            agent_session_id=agent_session.id,
            turn_index=turn_index,
            checkpoint_type="turn_state",
            payload={
                "delta_messages": [user_message, assistant_message],
                **state_payload,
            },
        )
        self._runtime_manager.update_agent_state(
            agent_session_id=agent_session.id,
            status="active",
            connection_state="connected",
            checkpoint_id=checkpoint.id,
            last_active_turn=turn_index,
            last_ack_message_index=last_ack_message_index,
        )

    @staticmethod
    def _now() -> str:
        from datetime import datetime

        return datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    def _normalize_message(self, item: object) -> dict[str, object]:
        if not isinstance(item, dict):
            return {"role": "user", "content": str(item)}
        role = str(item.get("role") or "user")
        raw_content = item.get("content")
        content = raw_content if isinstance(raw_content, list) else str(raw_content or "")
        return {"role": role, "content": content}

    def _build_director_init_messages(
        self,
        *,
        system_prompt: str,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        world_character_names: list[str] | None,
    ) -> list[dict[str, object]]:
        if not system_prompt.strip():
            return []
        return [{"role": "system", "content": system_prompt}]

    def _build_character_init_messages(
        self,
        *,
        system_prompt: str,
        init_payload: str,
    ) -> list[dict[str, object]]:
        messages: list[dict[str, object]] = []
        if system_prompt.strip():
            messages.append({"role": "system", "content": system_prompt})
        if init_payload.strip():
            messages.append({"role": "system", "content": init_payload})
        return messages
