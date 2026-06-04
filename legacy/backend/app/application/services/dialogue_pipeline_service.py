from dataclasses import dataclass, field
import json
import re

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.runtime_context_models import (
    ContextAttributeRecord,
    ContextInventoryRecord,
    SceneState,
)
from backend.app.application.services.prompt_runtime_service import PromptModule, PromptRuntimeService
from backend.app.application.services.text_generation_service import TextGenerationService
from backend.app.application.services.trigger_engine_service import TriggerEvaluation
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.memory import MemoryEntry
from backend.app.domain.models.session import ChatMessage, ContentPart, SessionSnapshot
from backend.app.domain.models.world import WorldDefinition
from backend.app.domain.models.world import normalize_world_director_config


@dataclass(frozen=True)
class DialogueContext:
    session: SessionSnapshot
    speaker: str
    speaker_profile: CharacterDefinition | None
    world_profile: WorldDefinition | None
    player_input: str
    session_attributes: list[RuntimeAttributeItem]
    speaker_memories: list[MemoryEntry]
    recent_messages: list[ChatMessage]
    director_decision: DirectorDecision
    trigger_evaluation: TriggerEvaluation
    scene_state: SceneState | None = None
    player_media_parts: list[ContentPart] = field(default_factory=list)


@dataclass(frozen=True)
class DialogueResult:
    speaker: str
    content: str
    intent: str
    emotion: str
    reasoning: str | None = None
    prompt_trace: dict[str, object] | None = None
    debug_lines: list[str] = field(default_factory=list)


class DialoguePipelineService:
    def __init__(self, text_generation: TextGenerationService | None = None) -> None:
        self._text_generation = text_generation
        self._prompt_runtime = PromptRuntimeService()

    def generate(self, context: DialogueContext) -> DialogueResult:
        return self._generate_with_model(context)

    def _generate_with_model(self, context: DialogueContext) -> DialogueResult:
        if self._text_generation is None:
            raise ValueError("閺堫亪鍘ょ純顔芥瀮閺堫剚膩閸ㄥ绱濋弮鐘崇《閻㈢喐鍨氱憴鎺曞閸欐垼鈻堥妴?")

        preferred_model = context.speaker_profile.model if context.speaker_profile and context.speaker_profile.model else None
        model_result = self._text_generation.generate_json_messages(
            messages=self._build_generation_messages(context),
            preferred_model=preferred_model,
            temperature=0.7,
        )
        if model_result.payload is None:
            if any("no_text_model_configured" in line for line in model_result.debug_lines):
                raise ValueError("閺堫亪鍘ょ純顔芥瀮閺堫剚膩閸ㄥ绱濋弮鐘崇《閻㈢喐鍨氱憴鎺曞閸欐垼鈻堥妴?")
            if any("missing_base_url" in line for line in model_result.debug_lines):
                raise ValueError("閺傚洦婀板Ο鈥崇€风紓鍝勭毌鐠嬪啰鏁ら崷鏉挎絻閿涘本妫ゅ▔鏇犳晸閹存劘顫楅懝鎻掑絺鐟封偓閵?")
            raise ValueError("閺傚洦婀板Ο鈥崇€锋稉宥呭讲閻㈩煉绱濋弮鐘崇《閻㈢喐鍨氱憴鎺曞閸欐垼鈻堥妴?")

        payload = model_result.payload
        content = str(payload.get("content", "")).strip()
        if not content:
            raise ValueError("閺傚洦婀板Ο鈥崇€烽張顏囩箲閸ョ偞婀侀弫鍫濆絺鐟封偓閸愬懎顔愰妴?")

        intent = str(payload.get("intent") or "advance_objective").strip() or "advance_objective"
        emotion = str(payload.get("emotion") or "focused").strip() or "focused"
        speaker = str(payload.get("speaker") or context.speaker).strip() or context.speaker

        return DialogueResult(
            speaker=speaker,
            content=content,
            intent=intent,
            emotion=emotion,
            reasoning=(model_result.raw_reasoning or "").strip() or None,
            debug_lines=[
                f"DialoguePipeline speaker={speaker}",
                f"DialoguePipeline intent={intent}",
                f"DialoguePipeline emotion={emotion}",
                f"DialoguePipeline recalled_history={len(context.speaker_memories)}",
                *model_result.debug_lines,
            ],
        )

    def build_runtime_system_prompt(self, context: DialogueContext) -> str:
        return self._build_system_prompt(context)

    def build_character_system_prompt(
        self,
        *,
        speaker: str,
        speaker_profile: CharacterDefinition | None,
    ) -> str:
        return self._build_system_prompt_for_values(
            speaker=speaker,
            speaker_profile=speaker_profile,
        )

    def build_runtime_init_payload(self, context: DialogueContext) -> str:
        return self._build_init_payload(context)

    def build_runtime_turn_payload(self, context: DialogueContext) -> str:
        return self._build_turn_payload(context)

    def build_runtime_prompt_trace(
        self,
        context: DialogueContext,
        *,
        init_messages: list[dict[str, object]] | None = None,
        turn_payload: str | None = None,
    ) -> dict[str, object]:
        system_prompt = self._build_system_prompt(context)
        init_payload = self._build_init_payload(context)
        resolved_turn_payload = turn_payload if turn_payload is not None else self._build_turn_payload(context)
        base_messages = list(init_messages) if init_messages is not None else self._build_generation_messages(context)
        messages = [self._normalize_prompt_message(item) for item in base_messages]
        if not messages or messages[-1].get("content") != resolved_turn_payload:
            messages.append({"role": "user", "content": resolved_turn_payload})
        return {
            "schema_version": "character_prompt_v2",
            "speaker": context.speaker,
            "system_prompt": system_prompt,
            "init_payload": init_payload,
            "turn_payload": resolved_turn_payload,
            "messages": messages,
        }

    def build_runtime_prompt_call(
        self,
        context: DialogueContext,
        *,
        stage: str = "普通回合",
    ) -> dict[str, object]:
        director_config = normalize_world_director_config(
            context.world_profile.director_config if context.world_profile is not None else {}
        )
        variables = self._template_variables(context)
        modules: list[PromptModule] = [
            *self._prompt_runtime.prompt_modules_for_presets(
                director_config=director_config,
                target="character",
                variables=variables,
            )
        ]
        long_prompt = self._prompt_runtime.render_template(
            str(context.speaker_profile.background_prompt if context.speaker_profile else ""),
            variables,
        )
        if long_prompt.strip():
            modules.append(
                PromptModule(
                    name="角色长期提示词",
                    source="角色设计 / 角色长期提示词",
                    content=long_prompt,
                    editable=True,
                )
            )
        init_payload = json.loads(self._build_init_payload(context))
        turn_payload = json.loads(self._build_turn_payload(context))
        character_payload = init_payload.get("basic_setting", {}).get("character", {}) if isinstance(init_payload, dict) else {}
        world_payload = init_payload.get("basic_setting", {}).get("world", {}) if isinstance(init_payload, dict) else {}
        modules.extend(
            [
                PromptModule("客观世界资料", "世界配置与会话", self._prompt_runtime.objective_json(world_payload), False),
                PromptModule("客观角色资料", "角色配置与运行时", self._prompt_runtime.objective_json(character_payload), False),
                PromptModule("当前状态", "运行时状态", self._prompt_runtime.objective_json(turn_payload.get("current_state", {})), False),
                PromptModule("聊天记录", "会话记录与记忆", self._prompt_runtime.objective_json(turn_payload.get("dialogue_history", {})), False),
            ]
        )
        return self._prompt_runtime.build_prompt_call(
            recipient_type="character",
            recipient_name=context.speaker,
            stage=stage,
            purpose="生成该角色本轮发言和动作",
            modules=modules,
            raw_debug={"init_payload": init_payload, "turn_payload": turn_payload},
        )

    def apply_return_processing(self, *, director_config: dict[str, object], raw_text: str):
        return self._prompt_runtime.apply_return_rules(
            director_config=director_config,
            target="character",
            raw_text=raw_text,
        )

    def attach_prompt_call_result(
        self,
        prompt_call: dict[str, object],
        *,
        raw_model_return: str | None,
        return_processing,
        processed_model_return: object,
        written_result: object,
    ) -> dict[str, object]:
        return self._prompt_runtime.attach_result(
            prompt_call,
            raw_model_return=raw_model_return,
            return_processing=return_processing,
            processed_model_return=processed_model_return,
            written_result=written_result,
        )

    def parse_runtime_payload(
        self,
        *,
        context: DialogueContext,
        payload: dict[str, object],
        debug_lines: list[str] | None = None,
    ) -> DialogueResult:
        debug_lines = debug_lines or []
        payload = self._normalize_dialogue_payload(payload)
        content = self._clean_dialogue_text(payload.get("content", ""))
        if not content:
            raise ValueError("閺傚洦婀板Ο鈥崇€烽張顏囩箲閸ョ偞婀侀弫鍫濆絺鐟封偓閸愬懎顔愰妴?")

        intent = self._clean_dialogue_text(payload.get("intent") or "advance_objective") or "advance_objective"
        emotion = self._clean_dialogue_text(payload.get("emotion") or "focused") or "focused"
        speaker = self._clean_dialogue_text(payload.get("speaker") or context.speaker) or context.speaker

        return DialogueResult(
            speaker=speaker,
            content=content,
            intent=intent,
            emotion=emotion,
            reasoning=None,
            prompt_trace=None,
            debug_lines=[
                f"DialoguePipeline speaker={speaker}",
                f"DialoguePipeline intent={intent}",
                f"DialoguePipeline emotion={emotion}",
                f"DialoguePipeline recalled_history={len(context.speaker_memories)}",
                *debug_lines,
            ],
        )

    def _normalize_dialogue_payload(self, payload: dict[str, object]) -> dict[str, object]:
        normalized = dict(payload)
        for key in ("content", "message", "text", "reply"):
            embedded = self._parse_embedded_json(normalized.get(key))
            if isinstance(embedded, dict):
                for field_name in ("speaker", "content", "intent", "emotion"):
                    if field_name in embedded and field_name not in normalized:
                        normalized[field_name] = embedded[field_name]
                if "content" in embedded:
                    normalized["content"] = embedded["content"]
                break
        return normalized

    def _clean_dialogue_text(self, value: object) -> str:
        current = value
        for _ in range(4):
            if isinstance(current, dict):
                if "content" in current:
                    current = current.get("content")
                    continue
                return json.dumps(current, ensure_ascii=False)
            if isinstance(current, list):
                return "\n".join(self._clean_dialogue_text(item) for item in current if item is not None).strip()

            text = str(current or "").strip()
            embedded = self._parse_embedded_json(text)
            if embedded is not None:
                current = embedded
                continue
            return self._strip_dialogue_field_artifacts(text)
        return self._strip_dialogue_field_artifacts(str(current or "").strip())

    def _parse_embedded_json(self, value: object) -> object | None:
        if not isinstance(value, str):
            return None
        text = value.strip()
        if not text:
            return None
        if text.startswith("```"):
            lines = text.splitlines()
            if len(lines) >= 3:
                text = "\n".join(lines[1:-1]).strip()
        candidates = [text]
        start_index = text.find("{")
        end_index = text.rfind("}")
        if start_index != -1 and end_index > start_index:
            candidates.append(text[start_index : end_index + 1])
        for candidate in candidates:
            try:
                return json.loads(candidate)
            except json.JSONDecodeError:
                continue
        return None

    def _strip_dialogue_field_artifacts(self, text: str) -> str:
        stripped = text.strip()
        match = re.search(r"(?is)(?:^|\n)\s*['\"]?content['\"]?\s*[:=]\s*(.+)$", stripped)
        if match and re.search(r"(?is)(?:^|\n)\s*['\"]?speaker['\"]?\s*[:=]", stripped):
            stripped = match.group(1).strip()
        stripped = re.sub(r"(?im)^\s*['\"]?(speaker|content|intent|emotion)['\"]?\s*[:=]\s*,?\s*$", "", stripped)
        if len(stripped) >= 2 and stripped[0] == stripped[-1] and stripped[0] in {"'", '"'}:
            stripped = stripped[1:-1].strip()
        return stripped.replace("\\n", "\n").replace("\\t", "\t").strip()

    def _build_generation_messages(self, context: DialogueContext) -> list[dict[str, object]]:
        messages: list[dict[str, object]] = []
        system_prompt = self._build_system_prompt(context)
        if system_prompt.strip():
            messages.append({"role": "system", "content": system_prompt})
        init_payload = self._build_init_payload(context)
        if init_payload.strip():
            messages.append({"role": "system", "content": init_payload})
        messages.append({"role": "user", "content": self._with_media_parts(self._build_turn_payload(context), context.player_media_parts)})
        return messages

    def _build_system_prompt(self, context: DialogueContext) -> str:
        return self._build_system_prompt_for_values(
            speaker=context.speaker,
            speaker_profile=context.speaker_profile,
        )

    def _build_init_payload(self, context: DialogueContext) -> str:
        speaker_profile = context.speaker_profile
        world_profile = context.world_profile
        return json.dumps(
            {
                "basic_setting": {
                    "character": {
                        "name": context.speaker,
                        "role": speaker_profile.role if speaker_profile else "",
                        "attributes": list(speaker_profile.attributes) if speaker_profile else [],
                        "custom_tabs": dict(speaker_profile.custom_tabs) if speaker_profile else {},
                    },
                    "world": {
                        "world_name": context.session.world_name,
                        "genre": world_profile.genre if world_profile else "",
                        "background_prompt": world_profile.background_prompt if world_profile else "",
                        "summary": world_profile.summary if world_profile else "",
                        "opening_scene": world_profile.opening_scene if world_profile else "",
                        "time_system": world_profile.time_system if world_profile else "",
                        "map_nodes": list(world_profile.map_nodes) if world_profile else [],
                        "triggers": list(world_profile.triggers) if world_profile else [],
                        "custom_tabs": dict(world_profile.custom_tabs) if world_profile else {},
                    },
                },
            },
            ensure_ascii=False,
            indent=2,
        )

    def _build_user_prompt(self, context: DialogueContext) -> str:
        return self._build_turn_payload(context)

    def _build_turn_payload(self, context: DialogueContext) -> str:
        scene_state = context.scene_state or self._fallback_scene_state(context)
        return json.dumps(
            {
                "dialogue_history": {
                    "recent_dialogue": self._build_recent_dialogue(
                        context.recent_messages,
                        player_character_name=context.session.player_character_name,
                    ),
                    "memory_dialogue": self._build_memory_dialogue(
                        context.speaker_memories,
                        player_character_name=context.session.player_character_name,
                    ),
                },
                "current_state": {
                    "requested_speaker": context.speaker,
                    "player_character_name": context.session.player_character_name,
                    "player_input": context.player_input,
                    "scene_state": self._build_scene_state_payload(scene_state),
                },
            },
            ensure_ascii=False,
            indent=2,
        )

    def _fallback_scene_state(self, context: DialogueContext) -> SceneState:
        public_attributes = [
            ContextAttributeRecord(
                key=item.schema.key,
                value=item.value.value,
                owner_type=item.value.owner_type,
                owner_relation="public",
            )
            for item in context.session_attributes
            if item.value.owner_type == "session"
        ]
        public_items = [
            ContextInventoryRecord(
                item_id=item.item_id,
                name=item.name,
                category=item.category,
                quantity=item.quantity,
                description=item.description,
                tags=list(item.tags),
                owner_type=item.owner_type,
                knowledge_scope="public",
            )
            for item in context.session.inventory_items
            if item.visibility == "public"
        ]
        discovered_locations = [
            node.label
            for node in context.session.map_graph_nodes
            if node.discovered and node.label.strip()
        ]
        present_characters = self._present_characters_for_session(context.session)
        return SceneState(
            world_name=context.session.world_name,
            location=context.session.location,
            time_label=context.session.time_label,
            scene_name=context.session.scene.name,
            scene_tags=list(context.session.scene.temporary_tags),
            present_characters=list(present_characters),
            discovered_locations=discovered_locations,
            public_attributes=public_attributes,
            public_items=public_items,
        )

    def _build_scene_state_payload(self, state: SceneState) -> dict[str, object]:
        return {
            "location": state.location,
            "time_label": state.time_label,
            "scene_name": state.scene_name,
            "scene_tags": [tag for tag in state.scene_tags if tag],
            "present_characters": [name for name in state.present_characters if name],
            "public_attributes": [
                {
                    "key": item.key,
                    "value": item.value,
                }
                for item in state.public_attributes
            ],
            "public_items": [
                {
                    "name": item.name,
                    "category": item.category,
                    "quantity": item.quantity,
                    "description": item.description,
                    "tags": list(item.tags),
                }
                for item in state.public_items
            ],
        }

    def _build_recent_dialogue(
        self,
        recent_messages: list[ChatMessage],
        *,
        player_character_name: str | None,
    ) -> list[dict[str, object]]:
        return [
            {
                "role": message.role,
                "speaker": self._resolve_dialogue_speaker(
                    role=message.role,
                    speaker=message.speaker,
                    player_character_name=player_character_name,
                ),
                "content": message.content,
            }
            for message in recent_messages
            if message.content.strip()
        ]

    def _build_memory_dialogue(
        self,
        speaker_memories: list[MemoryEntry],
        *,
        player_character_name: str | None,
    ) -> list[dict[str, object]]:
        dialogue_entries = [
            entry
            for entry in speaker_memories
            if entry.content.strip() and (entry.memory_type == "dialogue" or entry.role in {"player", "agent"})
        ]
        return [
            {
                "created_at": entry.created_at,
                "speaker": self._resolve_dialogue_speaker(
                    role=entry.role,
                    speaker=entry.speaker,
                    player_character_name=player_character_name,
                ),
                "role": entry.role,
                "location": entry.location,
                "content": entry.content,
            }
            for entry in dialogue_entries[:8]
        ]

    def _resolve_dialogue_speaker(
        self,
        *,
        role: str | None,
        speaker: str | None,
        player_character_name: str | None,
    ) -> str | None:
        normalized_speaker = str(speaker or "").strip()
        normalized_role = str(role or "").strip()
        if normalized_speaker and not (normalized_role == "player" and normalized_speaker == "player"):
            return normalized_speaker
        if normalized_role == "player":
            return str(player_character_name or "").strip() or "鐜╁"
        return normalized_speaker or None

    def _present_characters_for_session(self, session: SessionSnapshot) -> list[str]:
        names = [
            name.strip()
            for name in (session.scene.present_characters or session.visible_characters)
            if name and name.strip()
        ]
        if session.player_character_name and session.player_character_name.strip():
            names.append(session.player_character_name.strip())
        return list(dict.fromkeys(names))

    def _build_system_prompt_for_values(
        self,
        *,
        speaker: str,
        speaker_profile: CharacterDefinition | None,
    ) -> str:
        if speaker_profile is None:
            return ""
        return str(speaker_profile.background_prompt or "").strip()

    def _template_variables(self, context: DialogueContext) -> dict[str, str]:
        return {
            "user": str(context.session.player_character_name or "").strip() or "鐜╁",
            "char": context.speaker,
            "world": str(context.world_profile.name if context.world_profile else context.session.world_name),
            "scene": str(context.session.scene.name or context.session.location or ""),
            "time": str(context.session.time_label or ""),
        }

    def _normalize_prompt_message(self, message: dict[str, object]) -> dict[str, object]:
        content = message.get("content")
        return {
            "role": str(message.get("role") or "user"),
            "content": content if isinstance(content, list) else str(content or ""),
        }

    def _with_media_parts(self, text: str, media_parts: list[ContentPart]) -> str | list[ContentPart]:
        if not media_parts:
            return text
        return [{"type": "text", "text": text}, *media_parts]
