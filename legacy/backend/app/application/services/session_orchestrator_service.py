from dataclasses import replace
import re
import threading

from backend.app.application.services.agent_conversation_runtime_service import AgentConversationRuntimeService
from backend.app.application.services.agent_runtime_manager_service import AgentRuntimeManagerService
from backend.app.application.services.attribute_runtime_service import AttributeRuntimeService, RuntimeAttributeItem
from backend.app.application.services.attribute_service import AttributeCommandService, AttributeQueryService
from backend.app.application.services.asset_resolver_service import AssetResolverService
from backend.app.application.services.catalog_service import CatalogCommandService, CatalogQueryService
from backend.app.application.services.character_runtime_service import CharacterRuntimeService
from backend.app.application.services.inventory_runtime_service import InventoryRuntimeService
from backend.app.application.services.memory_service import MemoryCommandService, MemoryQueryService
from backend.app.application.services.narration_service import NarrationService
from backend.app.application.services.runtime_visibility_service import RuntimeVisibilityService
from backend.app.application.services.rule_engine_service import RuleAttributeUpdate, RuleEngineService
from backend.app.application.services.scene_runtime_manager_service import SceneRuntimeManagerService
from backend.app.application.services.state_engine_service import StateEngineService
from backend.app.application.services.trigger_engine_service import TriggerAttributeUpdate, TriggerEngineService, TriggerEvaluation
from backend.app.application.services.world_director_service import (
    CharacterVisualDirective,
    DirectorDecision,
    GeneratedCharacterDraft,
    SwitchCharacterProposal,
    WorldDirectorService,
)
from backend.app.domain.models.attribute import AttributeValue
from backend.app.domain.models.asset import AssetSelection, CharacterVisualState
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.session import (
    ChatMessage,
    ContentPart,
    MessageInput,
    SessionMapEdge,
    SessionMapNode,
    SessionSnapshot,
    extract_media_parts,
    extract_message_text,
)
from backend.app.domain.models.state import SessionState
from backend.app.domain.models.world import WorldDefinition
from backend.app.domain.repositories.session import SessionRepository


class SessionOrchestratorService:
    SWITCH_SCENE_HINT_PATTERN = re.compile(
        r"[\u4e00-\u9fffA-Za-z0-9]{2,24}"
        r"(?:入口|出口|正殿|偏殿|前殿|后殿|中殿|内殿|外殿|山下|峰下|崖下|谷口|谷中|湖畔|河畔|江畔|海边|门外|门内|"
        r"府中|府内|府外|宫中|宫内|宫外|城中|城内|城外|院中|院内|院外|阁中|楼中|殿中|境内|界中|桥头|渡口|营地|"
        r"村口|镇上|街口|巷口|坊间|山门|关口|码头|客栈|茶馆|书房|卧房|厢房|后园|花园|祠堂|大殿|偏厅|大厅|秘境|"
        r"幻境|仙府|洞府|山谷|谷地|山岭|高台|驿站|站台|广场|前厅|后厅|正门|侧门|地宫|墓室|祖宅|旧宅|内宅|外宅|"
        r"王府|行宫|皇宫|寝宫|宗门|学宫|衙门|军营|牢房|囚室|祭坛|神殿|天宫|龙宫|冥府|府|宫|殿|阁|楼|台|轩|寺|"
        r"观|庙|院|堂|庄|寨|营|门|关|谷|峰|山|岭|崖|洞|渊|泽|河|江|海|湖|岛|洲|原|林|苑|园|桥|巷|街|坊|馆|"
        r"厅|塔|站|港|渡|道|路|境|界|乡|县|郡|州|京|城|村|镇)"
    )
    SWITCH_SCENE_PREFIXES = (
        "前往",
        "去往",
        "去到",
        "进入",
        "来到",
        "回到",
        "转入",
        "转到",
        "抵达",
        "身在",
        "位于",
        "身处",
        "置身",
        "落在",
        "在",
        "到",
        "去",
        "赴",
        "往",
        "向",
        "从",
    )
    SWITCH_SCENE_STOPWORDS = {
        "当前",
        "这里",
        "那里",
        "此处",
        "原地",
        "场景",
        "地点",
        "地图",
        "剧情线",
        "人间线",
        "身份线",
        "视角",
        "主控",
        "玩家",
    }
    PLAYER_VIEW_SWITCH_PATTERN = re.compile(r"^(.+?)的视角已启用")

    def __init__(
        self,
        session_repository: SessionRepository,
        catalog_queries: CatalogQueryService,
        catalog_commands: CatalogCommandService,
        attribute_queries: AttributeQueryService,
        attribute_commands: AttributeCommandService,
        agent_runtime_manager: AgentRuntimeManagerService,
        agent_conversation_runtime: AgentConversationRuntimeService,
        attribute_runtime: AttributeRuntimeService,
        asset_resolver: AssetResolverService,
        inventory_runtime: InventoryRuntimeService,
        memory_queries: MemoryQueryService,
        memory_commands: MemoryCommandService,
        runtime_visibility: RuntimeVisibilityService,
        world_director: WorldDirectorService,
        scene_runtime_manager: SceneRuntimeManagerService,
        trigger_engine: TriggerEngineService,
        rule_engine: RuleEngineService,
        state_engine: StateEngineService,
        narration_service: NarrationService,
        character_runtime: CharacterRuntimeService,
    ) -> None:
        self._session_repository = session_repository
        self._catalog_queries = catalog_queries
        self._catalog_commands = catalog_commands
        self._attribute_queries = attribute_queries
        self._attribute_commands = attribute_commands
        self._agent_runtime_manager = agent_runtime_manager
        self._agent_conversation_runtime = agent_conversation_runtime
        self._attribute_runtime = attribute_runtime
        self._asset_resolver = asset_resolver
        self._inventory_runtime = inventory_runtime
        self._memory_queries = memory_queries
        self._memory_commands = memory_commands
        self._runtime_visibility = runtime_visibility
        self._world_director = world_director
        self._scene_runtime_manager = scene_runtime_manager
        self._trigger_engine = trigger_engine
        self._rule_engine = rule_engine
        self._state_engine = state_engine
        self._narration_service = narration_service
        self._character_runtime = character_runtime

    def create_session(self, world_id: str, player_character_id: str | None = None):
        session = self._session_repository.create_session(world_id, player_character_id=player_character_id)
        session = self._apply_default_existing_assets(session, world_id=world_id)
        self._bootstrap_agent_runtime(session)
        return session

    def _apply_default_existing_assets(self, session: SessionSnapshot, *, world_id: str) -> SessionSnapshot:
        current_world = self._catalog_queries.get_world(world_id)
        if current_world is None:
            return session
        session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session.id)
        assets = self._asset_resolver.resolve(
            session=session,
            scene=session.scene,
            state=session.state,
            current_speaker=session.current_speaker,
            session_attributes=session_attributes,
            world_profile=current_world,
            allow_generation=False,
        )
        if assets == session.assets:
            return session
        return self._session_repository.update_session_assets(session.id, assets) or session

    def submit_player_action(
        self,
        session_id: str,
        content: MessageInput,
        resend_from_turn_index: int | None = None,
    ):
        session = self._session_repository.get_session(session_id)
        if session is None:
            return None
        content_text = extract_message_text(content)
        media_parts = extract_media_parts(content)

        if resend_from_turn_index is not None:
            return self._resend_player_action(
                session=session,
                content=content_text,
                resend_from_turn_index=resend_from_turn_index,
            )

        return self._execute_turn(session=session, content=content_text, media_parts=media_parts)

    def switch_player_character(
        self,
        session_id: str,
        player_character_id: str,
        proposal_payload: dict[str, object] | None = None,
    ) -> SessionSnapshot | None:
        """Switch the player's bound character mid-session (e.g. reincarnation, transmigration)."""
        session = self._session_repository.get_session(session_id)
        if session is None:
            return None

        worlds = self._catalog_queries.list_worlds()
        current_world = next((item for item in worlds if item.name == session.world_name), None)
        if current_world is None:
            return None

        characters = self._catalog_queries.list_characters_for_world(current_world.id)
        target_character = next((item for item in characters if item.id == player_character_id), None)
        if target_character is None:
            return None
        if target_character.id == session.player_character_id:
            return session

        updated_session = self._switch_player_character_with_proposal(
            session=session,
            current_world=current_world,
            target_character=target_character,
            proposal_payload=proposal_payload or {},
        )
        if updated_session is None:
            return None

        # Re-sync agent runtime: old player char becomes visible NPC, new player char leaves NPC pool
        self._sync_character_agent_runtime(
            session_id=session_id,
            world_id=current_world.id,
            visible_character_names=updated_session.visible_characters,
        )

        # Create an agent for the new player character so it has a runtime checkpoint
        self._agent_runtime_manager.get_or_create_character_agent(
            session_id=session_id,
            character_id=player_character_id,
            character_name=target_character.name,
            scene_presence_state="present",
        )

        return updated_session

    def _resend_player_action(
        self,
        *,
        session: SessionSnapshot,
        content: str,
        resend_from_turn_index: int,
    ) -> SessionSnapshot | None:
        target_turn_index = max(1, int(resend_from_turn_index))
        target_turn_journal = self._agent_runtime_manager.list_turn_journal(
            session_id=session.id,
            turn_index=target_turn_index,
        )
        snapshot_payload = next(
            (
                entry.payload
                for entry in target_turn_journal
                if entry.step == "snapshot_created" and entry.status == "completed"
            ),
            None,
        )
        if not isinstance(snapshot_payload, dict):
            raise ValueError("该回合缺少可回滚快照，暂时无法重发")

        restored_session = self._deserialize_session_snapshot(
            snapshot_payload.get("session_snapshot"),
        )
        if restored_session is None or restored_session.id != session.id:
            raise ValueError("回滚快照损坏，无法重发该回合")

        runtime_attribute_values = self._deserialize_attribute_values(
            snapshot_payload.get("attribute_values"),
        )
        created_character_ids = self._collect_created_character_ids_from_turn(
            session_id=session.id,
            from_turn_index=target_turn_index,
        )
        rolled_back_session = self._session_repository.rollback_to_turn_snapshot(
            session=restored_session,
            runtime_attribute_values=runtime_attribute_values,
            from_turn_index=target_turn_index,
            delete_character_ids=created_character_ids,
        )
        return self._execute_turn(
            session=rolled_back_session,
            content=content,
            turn_index=target_turn_index,
            recovery_journal=[],
        )

    def resume_last_incomplete_turn(self, session_id: str):
        session = self._session_repository.get_session(session_id)
        if session is None:
            return None

        latest_turn_index = self._agent_runtime_manager.get_latest_turn_index(session_id)
        if latest_turn_index <= 0:
            return session

        recovery_journal = self._agent_runtime_manager.list_turn_journal(
            session_id=session_id,
            turn_index=latest_turn_index,
        )
        if not recovery_journal:
            return session
        if any(entry.step == "finished" and entry.status == "completed" for entry in recovery_journal):
            return session

        created_entry = next(
            (entry for entry in recovery_journal if entry.step == "created" and entry.status == "completed"),
            None,
        )
        if created_entry is None:
            return session
        player_input = str(created_entry.payload.get("player_input") or "").strip()
        if not player_input:
            return session

        return self._execute_turn(
            session=session,
            content=player_input,
            turn_index=latest_turn_index,
            recovery_journal=recovery_journal,
        )

    def materialize_missing_session_assets(self, session: SessionSnapshot) -> SessionSnapshot:
        if not self._has_missing_materialized_assets(session.assets):
            return session

        current_world = next(
            (item for item in self._catalog_queries.list_worlds() if item.name == session.world_name),
            None,
        )
        if not self._world_allows_mcp_tool(current_world, "mcp-tool-image-generation"):
            return session
        session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session.id)
        assets = self._asset_resolver.resolve(
            session=session,
            scene=session.scene,
            state=session.state,
            current_speaker=session.current_speaker,
            session_attributes=session_attributes,
            world_profile=current_world,
            allow_generation=True,
        )
        if assets == session.assets:
            return session
        return self._session_repository.update_session_assets(session.id, assets) or session

    def _has_missing_materialized_assets(self, assets: AssetSelection) -> bool:
        if assets.background_generation_prompt and assets.background_asset_path is None:
            return True
        if assets.active_speaker_generation_prompt and assets.active_speaker_portrait_path is None:
            return True
        return any(
            item.generation_prompt and item.portrait_asset_path is None
            for item in assets.visible_character_portraits
        )

    def _execute_turn(
        self,
        *,
        session: SessionSnapshot,
        content: str,
        media_parts: list[ContentPart] | None = None,
        turn_index: int | None = None,
        recovery_journal: list[object] | None = None,
    ):
        session_id = session.id
        recovery_journal = recovery_journal or []
        if turn_index is None:
            turn_index = self._agent_runtime_manager.next_turn_index(session_id)
        try:
            return self._execute_turn_impl(
                session=session,
                content=content,
                media_parts=list(media_parts or []),
                turn_index=turn_index,
                recovery_journal=recovery_journal,
            )
        except Exception as exc:
            payload = {"error": str(exc)}
            debug_lines = getattr(exc, "debug_lines", None)
            if isinstance(debug_lines, list) and debug_lines:
                payload["debug_lines"] = [str(line) for line in debug_lines if str(line).strip()]
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="aborted",
                status="failed",
                payload=payload,
            )
            raise

    def _execute_turn_impl(
        self,
        *,
        session: SessionSnapshot,
        content: str,
        media_parts: list[ContentPart],
        turn_index: int,
        recovery_journal: list[object],
    ):
        session_id = session.id

        if not self._journal_has_completed_step(recovery_journal, "created"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="created",
                status="completed",
                payload={"player_input": content},
            )
        if not self._journal_has_completed_step(recovery_journal, "snapshot_created"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="snapshot_created",
                status="completed",
                payload={
                    "session_snapshot": self._serialize_session_snapshot(session),
                    "attribute_values": self._serialize_attribute_values(
                        self._collect_runtime_attribute_values(session_id)
                    ),
                },
            )
        self._bootstrap_agent_runtime(session)

        session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session_id)
        worlds = self._catalog_queries.list_worlds()
        current_world = next((item for item in worlds if item.name == session.world_name), None)
        if current_world is None:
            return None
        starting_characters = self._catalog_queries.list_characters_for_world(current_world.id)
        preexisting_character_names = {
            item.name.strip()
            for item in starting_characters
            if item.name.strip()
        }
        recovered_director = self._journal_payload(recovery_journal, "director_completed")
        if recovered_director is not None:
            director_decision = DirectorDecision(
                world_phase=str(recovered_director.get("world_phase") or "opening"),
                next_location=recovered_director.get("next_location"),
                next_scene_name=str(recovered_director.get("next_scene_name") or "").strip() or None,
                next_scene_background_hint=str(recovered_director.get("next_scene_background_hint") or "").strip() or None,
                background_asset_name=str(recovered_director.get("background_asset_name") or "").strip() or None,
                background_asset_path=str(recovered_director.get("background_asset_path") or "").strip() or None,
                background_generation_prompt=str(recovered_director.get("background_generation_prompt") or "").strip() or None,
                next_scene_tags=[
                    str(item).strip()
                    for item in recovered_director.get("next_scene_tags", [])
                    if str(item).strip()
                ] if isinstance(recovered_director.get("next_scene_tags"), list) else [],
                next_time_label=str(recovered_director.get("next_time_label") or "").strip() or None,
                generated_characters=[],
                character_visual_directives=[
                    CharacterVisualDirective(
                        character_name=str(item.get("character_name") or "").strip(),
                        portrait_hint=str(item.get("portrait_hint") or "").strip(),
                        portrait_asset_name=str(item.get("portrait_asset_name") or "").strip() or None,
                        portrait_asset_path=str(item.get("portrait_asset_path") or "").strip() or None,
                        generation_prompt=str(item.get("generation_prompt") or "").strip() or None,
                    )
                    for item in recovered_director.get("character_visual_directives", [])
                    if isinstance(item, dict) and str(item.get("character_name") or "").strip()
                ] if isinstance(recovered_director.get("character_visual_directives"), list) else [],
                scene_visible_characters=[
                    str(item).strip()
                    for item in recovered_director.get("scene_visible_characters", [])
                    if str(item).strip()
                ] if isinstance(recovered_director.get("scene_visible_characters"), list) else None,
                planned_speakers=[
                    str(item).strip()
                    for item in recovered_director.get("planned_speakers", [])
                    if str(item).strip()
                ],
            )
        else:
            director_agent = self._agent_runtime_manager.get_or_create_director_agent(session_id=session_id)
            def handle_streaming_director_content(streaming_text: str) -> None:
                if not streaming_text.strip():
                    return
                transient_director_decision = replace(
                    DirectorDecision(
                        world_phase="opening",
                    ),
                    raw_model_response=streaming_text,
                    next_time_label=session.time_label,
                )
                transient_system_messages = self._build_director_trace_messages(
                    director_decision=transient_director_decision,
                )
                self._publish_turn_progress_snapshot(
                    runtime_session=session,
                    player_input=content,
                    agent_messages=[],
                    turn_index=turn_index,
                    next_time_label=session.time_label,
                    system_messages=transient_system_messages,
                )
            director_decision = self._agent_conversation_runtime.plan_director_turn(
                agent_session=director_agent,
                turn_index=turn_index,
                session=session,
                world_profile=current_world,
                world_character_names=[
                    item.name.strip()
                    for item in starting_characters
                    if item.name.strip()
                ],
                world_characters=starting_characters,
                player_input=content,
                session_attributes=session_attributes,
                recent_dialogue_rounds=self._world_director_history_rounds(current_world),
                on_stream_full_text=handle_streaming_director_content,
            )
        journal_visible_characters = self._visible_characters_from_journal(recovery_journal)
        created_generated_characters: list[CharacterDefinition] = []
        if journal_visible_characters:
            visible_character_names = journal_visible_characters
        else:
            visible_character_names, created_generated_characters = self._apply_generated_characters(
                session_id=session_id,
                session_world_name=session.world_name,
                decision=director_decision,
                visible_character_names=session.visible_characters,
                desired_visible_character_names=director_decision.scene_visible_characters,
                player_character_name=session.player_character_name,
                exclude_scene_character_names=self._scene_hidden_generated_character_names(
                    director_decision=director_decision,
                    player_character_name=session.player_character_name,
                    current_location=session.location,
                    current_scene_name=session.scene.name,
                ),
            )
        if director_decision.scene_change is not None:
            target_player_name = str(director_decision.scene_change.player_character_name or "").strip()
            if target_player_name and target_player_name != session.player_character_name:
                characters_after_generation = self._catalog_queries.list_characters_for_world(current_world.id)
                target_player = next((item for item in characters_after_generation if item.name == target_player_name), None)
                if target_player is not None:
                    switched_session = self._switch_player_character_with_proposal(
                        session=session,
                        current_world=current_world,
                        target_character=target_player,
                        proposal_payload={
                            "location": director_decision.next_location,
                            "scene_name": director_decision.next_scene_name,
                            "scene_background_hint": director_decision.next_scene_background_hint,
                            "scene_tags": director_decision.next_scene_tags,
                            "visible_characters": visible_character_names,
                        },
                    )
                    if switched_session is not None:
                        session = switched_session
                        visible_character_names = list(switched_session.visible_characters)
        if not self._journal_has_completed_step(recovery_journal, "director_completed"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="director_completed",
                status="completed",
                payload={
                    "world_phase": director_decision.world_phase,
                    "next_location": director_decision.next_location,
                    "next_scene_name": director_decision.next_scene_name,
                    "next_scene_background_hint": director_decision.next_scene_background_hint,
                    "background_asset_name": director_decision.background_asset_name,
                    "background_asset_path": director_decision.background_asset_path,
                    "background_generation_prompt": director_decision.background_generation_prompt,
                    "next_scene_tags": director_decision.next_scene_tags,
                    "next_time_label": director_decision.next_time_label,
                    "scene_visible_characters": director_decision.scene_visible_characters or [],
                    "planned_speakers": director_decision.planned_speakers,
                    "character_visual_directives": [
                        {
                            "character_name": item.character_name,
                            "portrait_hint": item.portrait_hint,
                            "portrait_asset_name": item.portrait_asset_name,
                            "portrait_asset_path": item.portrait_asset_path,
                            "generation_prompt": item.generation_prompt,
                        }
                        for item in director_decision.character_visual_directives
                    ],
                    "generated_characters": [item.name for item in director_decision.generated_characters],
                    "scene_change": {
                        "scene_name": director_decision.scene_change.scene_name,
                        "scene_description": director_decision.scene_change.scene_description,
                        "all_characters": director_decision.scene_change.all_characters,
                        "player_character_name": director_decision.scene_change.player_character_name,
                    } if director_decision.scene_change is not None else None,
                    "prompt_trace": director_decision.prompt_trace,
                    "llm_output": director_decision.raw_model_response,
                },
            )
        scene_runtime = self._scene_runtime_manager.refresh_scene(
            session=session,
            director_decision=director_decision,
            visible_character_names=visible_character_names,
            session_attributes=session_attributes,
        )

        trigger_evaluation = self._trigger_engine.evaluate_turn(
            session=session,
            player_input=content,
            director_decision=director_decision,
            session_attributes=session_attributes,
        )
        self._apply_trigger_attribute_updates(trigger_evaluation.attribute_updates)

        refreshed_session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session_id)
        rule_evaluation = self._rule_engine.evaluate_turn(
            session=session,
            player_input=content,
            director_decision=director_decision,
            trigger_evaluation=trigger_evaluation,
            session_attributes=refreshed_session_attributes,
            current_state=session.state,
        )
        self._apply_rule_attribute_updates(rule_evaluation.attribute_updates)

        refreshed_session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session_id)
        inventory_runtime = self._inventory_runtime.evaluate_turn(
            session=session,
            player_input=content,
            location_override=director_decision.next_location,
        )
        self._sync_character_agent_runtime(
            session_id=session_id,
            world_id=current_world.id,
            visible_character_names=visible_character_names,
        )
        if not self._journal_has_completed_step(recovery_journal, "scene_applied"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="scene_applied",
                status="completed",
                payload={
                    "scene_id": scene_runtime.scene.scene_id,
                    "scene_name": scene_runtime.scene.name,
                    "visible_characters": visible_character_names,
                },
            )
        planned_speakers, speaker_plan_debug_lines = self._resolve_turn_speakers(
            session_id=session_id,
            visible_character_names=visible_character_names,
            player_input=content,
            director_decision=director_decision,
            player_character_name=session.player_character_name,
        )

        characters = self._catalog_queries.list_characters_for_world(current_world.id)
        director_decision, characters, created_switch_scene_characters = self._materialize_switch_character_proposal(
            session_id=session_id,
            current_world=current_world,
            session=session,
            director_decision=director_decision,
            characters=characters,
        )
        character_name_map = {item.name: item.id for item in characters}
        character_definition_map = {item.name: item for item in characters}
        switch_target_character_name = (
            director_decision.switch_character_proposal.target_character_name.strip()
            if director_decision.switch_character_proposal is not None
            else ""
        )
        switch_created_character_names = {
            item.name.strip()
            for item in created_switch_scene_characters
            if item.name.strip()
        }
        switch_target_created_in_turn = bool(
            switch_target_character_name
            and (
                switch_target_character_name in switch_created_character_names
                or (
                    switch_target_character_name not in preexisting_character_names
                    and switch_target_character_name in character_definition_map
                )
            )
        )
        created_character_ids_this_turn = [
            item.id
            for item in [*created_generated_characters, *created_switch_scene_characters]
            if item.id.strip()
        ]
        if created_character_ids_this_turn and not self._journal_has_completed_step(
            recovery_journal,
            "characters_created",
        ):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="characters_created",
                status="completed",
                payload={"character_ids": list(dict.fromkeys(created_character_ids_this_turn))},
            )
        current_event_ids = [
            *[event.event_id for event in trigger_evaluation.memory_events if event.event_id],
            *[event.event_id for event in rule_evaluation.memory_events if event.event_id],
        ]
        current_item_ids = [operation.item_id for operation in inventory_runtime.operations if operation.item_id]
        runtime_session = self._build_runtime_session_for_response(
            session=session,
            location_override=director_decision.next_location,
            visible_character_names=visible_character_names,
            inventory_items=inventory_runtime.inventory_items,
            scene=scene_runtime.scene,
        )
        director_action_messages = self._build_director_action_messages(
            director_decision=director_decision,
            character_definition_map=character_definition_map,
            created_characters_in_turn=created_switch_scene_characters,
            switch_target_created_in_turn=switch_target_created_in_turn,
            current_player_character_name=session.player_character_name,
        )
        self._publish_turn_progress_snapshot(
            runtime_session=runtime_session,
            player_input=content,
            agent_messages=[],
            turn_index=turn_index,
            next_time_label=director_decision.next_time_label,
            system_messages=director_action_messages,
        )
        character_responses = []
        agent_messages: list[ChatMessage] = []
        runtime_agents = {
            agent.character_name: agent
            for agent in self._agent_runtime_manager.list_agent_sessions(session_id)
            if agent.agent_type == "character" and agent.character_name
        }
        completed_speaker_steps = self._completed_speaker_steps(recovery_journal)
        for speaker_index, speaker_name in enumerate(planned_speakers, start=1):
            if speaker_index in completed_speaker_steps:
                continue
            speaker_id = character_name_map.get(speaker_name)
            speaker_profile = character_definition_map.get(speaker_name)
            speaker_memories = (
                self._memory_queries.recall_for_character(
                    world_id=current_world.id,
                    session_id=session_id,
                    character_id=speaker_id,
                    query_text=content,
                    location=director_decision.next_location or session.location,
                    scene_id=scene_runtime.scene.scene_id,
                    participants=self._build_turn_participants(
                        visible_character_names=visible_character_names,
                        player_character_name=session.player_character_name,
                    ),
                    current_event_ids=current_event_ids,
                    current_item_ids=current_item_ids,
                    layers=["archive"],
                    memory_types=["dialogue"],
                    limit=6,
                )
                if speaker_id
                else []
            )
            speaker_runtime_session = self._with_turn_messages(
                session=runtime_session,
                player_input=content,
                agent_messages=agent_messages,
                turn_index=turn_index,
            )
            speaker_session_view = self._runtime_visibility.build_character_session_view(
                session=speaker_runtime_session,
                character_id=speaker_id,
                character_name=speaker_name,
            )
            speaker_visible_attributes = self._attribute_runtime.list_character_visible_attributes(
                session_id=session_id,
                character_id=speaker_id,
            )
            scene_state = self._runtime_visibility.build_scene_state(
                session=speaker_runtime_session,
                visible_attributes=speaker_visible_attributes,
            )
            recent_dialogue = self._slice_recent_dialogue_rounds(
                messages=speaker_runtime_session.messages,
                previous_rounds=speaker_profile.recent_dialogue_rounds if speaker_profile else 2,
                current_player_name=speaker_runtime_session.player_character_name,
            )
            speaker_agent = runtime_agents.get(speaker_name)
            if speaker_agent is None and speaker_id:
                speaker_agent = self._agent_runtime_manager.get_or_create_character_agent(
                    session_id=session_id,
                    character_id=speaker_id,
                    character_name=speaker_name,
                    scene_presence_state="present",
                )
            streaming_character_content = ""
            streaming_character_reasoning = ""

            def publish_streaming_character_snapshot() -> None:
                if not streaming_character_content.strip() and not streaming_character_reasoning.strip():
                    return
                transient_agent_messages = [
                    *agent_messages,
                    ChatMessage(
                        role="agent",
                        speaker=speaker_name,
                        content=streaming_character_content.strip(),
                        metadata={
                            "streaming": True,
                            "reasoning": streaming_character_reasoning.strip(),
                        },
                    ),
                ]
                self._publish_turn_progress_snapshot(
                    runtime_session=runtime_session,
                    player_input=content,
                    player_media_parts=media_parts,
                    agent_messages=transient_agent_messages,
                    turn_index=turn_index,
                    next_time_label=director_decision.next_time_label,
                    system_messages=director_action_messages,
                )

            def handle_streaming_character_content(streaming_content: str) -> None:
                nonlocal streaming_character_content
                streaming_text = streaming_content.strip()
                if not streaming_text:
                    return
                streaming_character_content = self._character_runtime.clean_streaming_content(streaming_text)
                publish_streaming_character_snapshot()

            def handle_streaming_character_reasoning(streaming_reasoning: str) -> None:
                nonlocal streaming_character_reasoning
                reasoning_text = streaming_reasoning.strip()
                if not reasoning_text:
                    return
                streaming_character_reasoning = reasoning_text
                publish_streaming_character_snapshot()

            character_response = self._character_runtime.generate_response(
                session=speaker_session_view,
                speaker=speaker_name,
                speaker_profile=speaker_profile,
                world_profile=current_world,
                player_input=content,
                player_media_parts=media_parts,
                session_attributes=speaker_visible_attributes,
                speaker_memories=speaker_memories,
                recent_dialogue=recent_dialogue,
                scene_state=scene_state,
                director_decision=director_decision,
                trigger_evaluation=trigger_evaluation,
                agent_session=speaker_agent,
                turn_index=turn_index,
                on_stream_text=handle_streaming_character_content,
                on_stream_reasoning=handle_streaming_character_reasoning,
            )
            character_responses.append(character_response)
            agent_messages.append(
                ChatMessage(
                    role="agent",
                    speaker=character_response.speaker,
                    content=character_response.content,
                    metadata={"reasoning": character_response.reasoning} if character_response.reasoning else None,
                )
            )
            self._publish_turn_progress_snapshot(
                runtime_session=runtime_session,
                player_input=content,
                player_media_parts=media_parts,
                agent_messages=agent_messages,
                turn_index=turn_index,
                next_time_label=director_decision.next_time_label,
                system_messages=director_action_messages,
            )
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step=f"speaker_{speaker_index}_completed",
                status="completed",
                payload={
                    "speaker": character_response.speaker,
                    "intent": character_response.intent,
                    "emotion": character_response.emotion,
                    "prompt_trace": character_response.prompt_trace,
                    "llm_output": {
                        "speaker": character_response.speaker,
                        "content": character_response.content,
                        "intent": character_response.intent,
                        "emotion": character_response.emotion,
                        "reasoning": character_response.reasoning,
                    },
                },
            )

        state_transition = self._state_engine.evaluate_turn(
            session=session,
            player_input=content,
            director_decision=director_decision,
            trigger_evaluation=trigger_evaluation,
            rule_evaluation=rule_evaluation,
            session_attributes=refreshed_session_attributes,
        )
        narration = self._narration_service.compose_turn_narration(
            session=session,
            world_profile=current_world,
            director_decision=director_decision,
            scene_runtime=scene_runtime,
            trigger_evaluation=trigger_evaluation,
            rule_evaluation=rule_evaluation,
            state_transition=state_transition,
            session_attributes=refreshed_session_attributes,
        )
        assets = self._asset_resolver.resolve(
            session=session,
            scene=scene_runtime.scene,
            state=state_transition.state,
            current_speaker=character_responses[-1].speaker if character_responses else session.current_speaker,
            session_attributes=refreshed_session_attributes,
            world_profile=current_world,
            director_decision=director_decision,
            allow_generation=self._world_allows_mcp_tool(current_world, "mcp-tool-image-generation"),
        )

        debug_lines = [
            *scene_runtime.debug_lines,
            *trigger_evaluation.debug_lines,
            *rule_evaluation.debug_lines,
            *speaker_plan_debug_lines,
            *[line for response in character_responses for line in response.debug_lines],
            *state_transition.debug_lines,
            *inventory_runtime.debug_lines,
            *narration.debug_lines,
        ]

        updated_session = self._session_repository.submit_player_action(
            session_id,
            content,
            player_media_parts=media_parts,
            turn_index=turn_index,
            time_label_override=director_decision.next_time_label,
            agent_messages=agent_messages,
            debug_lines=debug_lines,
            location_override=director_decision.next_location,
            visible_characters_override=visible_character_names,
            narration_messages=narration.messages,
            switch_proposal_messages=director_action_messages,
            log_messages=[
                *scene_runtime.system_messages,
                *trigger_evaluation.system_messages,
                *rule_evaluation.system_messages,
                *state_transition.system_messages,
                *inventory_runtime.system_messages,
            ],
            inventory_override=inventory_runtime.inventory_items,
            scene_override=scene_runtime.scene,
            assets_override=assets,
            state_override=state_transition.state,
        )

        if updated_session is None:
            return None

        visible_characters = [
            (character_name_map[name], name)
            for name in visible_character_names
            if name in character_name_map
        ]
        if session.player_character_id and session.player_character_name:
            visible_characters.append((session.player_character_id, session.player_character_name))
        memory_entries = self._memory_commands.build_turn_entries(
            world_id=current_world.id,
            session_id=session_id,
            turn_index=turn_index,
            visible_characters=visible_characters,
            player_character_name=session.player_character_name,
            speaker_responses=[
                (character_name_map.get(response.speaker), response.speaker, response.content)
                for response in character_responses
            ],
            player_input=content,
            observed_facts=narration.messages,
            location=director_decision.next_location or session.location,
            scene_id=scene_runtime.scene.scene_id,
            memory_events=[*trigger_evaluation.memory_events, *rule_evaluation.memory_events],
            inventory_operations=inventory_runtime.operations,
        )
        self._memory_commands.append_entries(memory_entries)
        if not self._journal_has_completed_step(recovery_journal, "memory_committed"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="memory_committed",
                status="completed",
                payload={"memory_count": len(memory_entries)},
            )
        self._schedule_asset_refresh(
            session_id=session_id,
            session=updated_session,
            world_profile=current_world,
            session_attributes=refreshed_session_attributes,
            current_speaker=character_responses[-1].speaker if character_responses else session.current_speaker,
            expected_assets=assets,
        )
        if not self._journal_has_completed_step(recovery_journal, "finished"):
            self._agent_runtime_manager.append_turn_journal(
                session_id=session_id,
                turn_index=turn_index,
                step="finished",
                status="completed",
                payload={
                    "current_speaker": updated_session.current_speaker,
                    "message_count": len(updated_session.messages),
                },
            )

        return updated_session

    def _bootstrap_agent_runtime(self, session: SessionSnapshot) -> None:
        self._agent_runtime_manager.get_or_create_director_agent(session_id=session.id)
        if not session.visible_characters:
            return

        world = next((item for item in self._catalog_queries.list_worlds() if item.name == session.world_name), None)
        if world is None:
            return

        self._sync_character_agent_runtime(
            session_id=session.id,
            world_id=world.id,
            visible_character_names=session.visible_characters,
        )

    def _sync_character_agent_runtime(
        self,
        *,
        session_id: str,
        world_id: str,
        visible_character_names: list[str],
    ) -> None:
        characters = self._catalog_queries.list_characters_for_world(world_id)
        active_character_ids: set[str] = set()
        for character in characters:
            if character.name not in visible_character_names:
                continue
            active_character_ids.add(character.id)
            self._agent_runtime_manager.get_or_create_character_agent(
                session_id=session_id,
                character_id=character.id,
                character_name=character.name,
                scene_presence_state="present",
            )
        self._agent_runtime_manager.sync_scene_presence(
            session_id=session_id,
            active_character_ids=active_character_ids,
        )

    def _build_runtime_session_for_response(
        self,
        session: SessionSnapshot,
        location_override: str | None,
        visible_character_names: list[str],
        inventory_items: list[InventoryItem],
        scene: SceneRuntime,
    ) -> SessionSnapshot:
        return SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=location_override or session.location,
            time_label=session.time_label,
            current_speaker=session.current_speaker,
            current_line=session.current_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=visible_character_names,
            messages=session.messages,
            player_stats=session.player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=inventory_items,
            system_log=session.system_log,
            scene=scene,
            assets=session.assets,
            state=session.state,
        )

    def _with_turn_messages(
        self,
        *,
        session: SessionSnapshot,
        player_input: str,
        agent_messages: list[ChatMessage],
        turn_index: int,
        system_messages: list[ChatMessage] | None = None,
    ) -> SessionSnapshot:
        system_messages = list(system_messages or [])
        resolved_turn_index = max(1, int(turn_index or 1))
        latest_agent_message = agent_messages[-1] if agent_messages else None
        if latest_agent_message and latest_agent_message.speaker:
            current_speaker = latest_agent_message.speaker
        elif system_messages:
            current_speaker = "系统"
        else:
            current_speaker = session.current_speaker
        if latest_agent_message:
            current_line = latest_agent_message.content
        elif system_messages:
            current_line = system_messages[-1].content
        else:
            current_line = session.current_line
        return SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            current_speaker=current_speaker,
            current_line=current_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=session.visible_characters,
            messages=[
                *session.messages,
                ChatMessage(
                    role="player",
                    content=player_input,
                    speaker=self._resolved_player_speaker(session.player_character_name),
                    metadata={
                        "turn_index": resolved_turn_index,
                        "message_kind": "player_action",
                    },
                ),
                *[
                    ChatMessage(
                        role=item.role,
                        content=item.content,
                        speaker=item.speaker,
                        metadata=self._normalize_transient_message_metadata(
                            item.metadata,
                            role=item.role,
                            turn_index=resolved_turn_index,
                        ),
                    )
                    for item in system_messages
                ],
                *[
                    ChatMessage(
                        role=item.role,
                        content=item.content,
                        speaker=item.speaker,
                        metadata=self._normalize_transient_message_metadata(
                            item.metadata,
                            role=item.role,
                            turn_index=resolved_turn_index,
                        ),
                    )
                    for item in agent_messages
                ],
            ],
            player_stats=session.player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=session.system_log,
            scene=session.scene,
            assets=session.assets,
            state=session.state,
        )

    def _publish_turn_progress_snapshot(
        self,
        *,
        runtime_session: SessionSnapshot,
        player_input: str,
        agent_messages: list[ChatMessage],
        turn_index: int,
        next_time_label: str | None,
        system_messages: list[ChatMessage] | None = None,
    ) -> None:
        if not agent_messages and not system_messages:
            return

        transient_session = self._with_turn_messages(
            session=runtime_session,
            player_input=player_input,
            agent_messages=agent_messages,
            turn_index=turn_index,
            system_messages=system_messages,
        )
        if next_time_label:
            transient_session = replace(transient_session, time_label=next_time_label)
        self._session_repository.publish_transient_snapshot(transient_session)

    def _normalize_transient_message_metadata(
        self,
        metadata: dict[str, object] | None,
        *,
        role: str,
        turn_index: int,
    ) -> dict[str, object]:
        merged = dict(metadata or {})
        merged["turn_index"] = turn_index
        existing_kind = str(merged.get("message_kind") or "").strip()
        if existing_kind:
            return merged
        if role == "agent":
            merged["message_kind"] = "agent_response"
            return merged
        action_type = str(merged.get("action_type") or "").strip()
        if action_type == "director_trace":
            merged["message_kind"] = "director_trace"
        else:
            merged["message_kind"] = "system_action"
        return merged

    def _resolve_turn_speakers(
        self,
        *,
        session_id: str,
        visible_character_names: list[str],
        player_input: str,
        director_decision: DirectorDecision,
        player_character_name: str | None = None,
    ) -> tuple[list[str], list[str]]:
        planned_speakers = list(
            dict.fromkeys(
                name for name in director_decision.planned_speakers
                if name in visible_character_names and name != player_character_name
            )
        )
        if planned_speakers:
            return planned_speakers, [f"TurnSpeakers planned={planned_speakers}"]

        selection = self._attribute_runtime.select_turn_speakers(
            session_id=session_id,
            visible_character_names=[
                name for name in visible_character_names if name != player_character_name
            ],
            player_input=player_input,
        )
        return selection.speakers, selection.debug_lines

    def _slice_recent_dialogue_rounds(
        self,
        *,
        messages: list[ChatMessage],
        previous_rounds: int,
        current_player_name: str | None = None,
    ) -> list[ChatMessage]:
        max_previous_rounds = max(0, previous_rounds)
        normalized_messages = self._annotate_player_message_speakers(
            messages=messages,
            current_player_name=current_player_name,
        )
        dialogue_messages = [
            message
            for message in normalized_messages
            if message.role in {"player", "agent"} and message.content.strip()
        ]
        if not dialogue_messages:
            return []

        selected: list[ChatMessage] = []
        player_messages_seen = 0
        for message in reversed(dialogue_messages):
            selected.append(message)
            if message.role == "player":
                player_messages_seen += 1
                if player_messages_seen > max_previous_rounds:
                    break

        return list(reversed(selected))

    def _build_turn_participants(
        self,
        *,
        visible_character_names: list[str],
        player_character_name: str | None,
    ) -> list[str]:
        names = [name.strip() for name in visible_character_names if name.strip()]
        if player_character_name and player_character_name.strip():
            names.append(player_character_name.strip())
        return list(dict.fromkeys(names))

    def _annotate_player_message_speakers(
        self,
        *,
        messages: list[ChatMessage],
        current_player_name: str | None,
    ) -> list[ChatMessage]:
        has_switch_marker = any(
            message.role == "system" and self.PLAYER_VIEW_SWITCH_PATTERN.match(message.content.strip())
            for message in messages
        )
        resolved_player_speaker = (
            self._resolved_player_speaker(current_player_name)
            if current_player_name and not has_switch_marker
            else "玩家"
        )
        annotated: list[ChatMessage] = []
        for message in messages:
            if message.role == "system":
                match = self.PLAYER_VIEW_SWITCH_PATTERN.match(message.content.strip())
                if match and match.group(1).strip():
                    resolved_player_speaker = match.group(1).strip()
                annotated.append(message)
                continue
            if message.role == "player":
                raw_speaker = str(message.speaker or "").strip()
                speaker = raw_speaker if raw_speaker and raw_speaker != "player" else resolved_player_speaker
                annotated.append(
                    ChatMessage(
                        role=message.role,
                        content=message.content,
                        speaker=speaker,
                        metadata=message.metadata,
                    )
                )
                continue
            annotated.append(message)
        return annotated

    def _resolved_player_speaker(self, player_character_name: str | None) -> str:
        return str(player_character_name or "").strip() or "玩家"

    def _apply_generated_characters(
        self,
        session_id: str,
        session_world_name: str,
        decision: DirectorDecision,
        visible_character_names: list[str],
        desired_visible_character_names: list[str] | None = None,
        player_character_name: str | None = None,
        exclude_scene_character_names: set[str] | None = None,
    ) -> tuple[list[str], list[CharacterDefinition]]:
        world = next((item for item in self._catalog_queries.list_worlds() if item.name == session_world_name), None)
        world_characters = self._catalog_queries.list_characters_for_world(world.id) if world is not None else []
        existing_names = {item.name: item for item in world_characters}
        created_characters: list[CharacterDefinition] = []
        excluded_names = exclude_scene_character_names or set()
        requested_visible = (
            [
                name
                for name in (
                    str(item).strip()
                    for item in (desired_visible_character_names or [])
                )
                if name and name != player_character_name and name not in excluded_names
            ]
            if desired_visible_character_names is not None
            else []
        )
        current_visible = (
            []
            if desired_visible_character_names is not None
            else [
                name
                for name in (str(item).strip() for item in visible_character_names)
                if name and name != player_character_name and name not in excluded_names
            ]
        )

        for draft in decision.generated_characters:
            character = existing_names.get(draft.name)
            if character is None:
                if world is None:
                    continue
                character = self._catalog_commands.create_character(self._to_character_definition(world.id, draft))
                created_characters.append(character)
                existing_names[character.name] = character

            if (
                desired_visible_character_names is None
                and character.name not in current_visible
                and character.name not in excluded_names
            ):
                current_visible.append(character.name)

        if desired_visible_character_names is not None:
            for name in requested_visible:
                if name not in existing_names or name in current_visible:
                    continue
                current_visible.append(name)

        if world is not None:
            character_name_map = {item.name: item.id for item in [*world_characters, *created_characters]}
            projected_character_names = list(
                dict.fromkeys(
                    [
                        *current_visible,
                        *[item.name for item in created_characters if item.name.strip()],
                    ]
                )
            )
            self._attribute_commands.project_session_attributes(
                session_id=session_id,
                world_id=world.id,
                character_ids=[
                    character_name_map[name]
                    for name in projected_character_names
                    if name in character_name_map
                ],
            )

        return list(dict.fromkeys(current_visible)), created_characters

    def _scene_hidden_generated_character_names(
        self,
        *,
        director_decision: DirectorDecision,
        player_character_name: str | None,
        current_location: str,
        current_scene_name: str,
    ) -> set[str]:
        proposal = director_decision.switch_character_proposal
        if proposal is None:
            return set()
        hidden_names = {
            name
            for name in (
                proposal.target_character_name.strip(),
                *[item.strip() for item in proposal.visible_characters],
            )
            if name and name != player_character_name
        }
        next_location = str(proposal.next_location or "").strip()
        scene_name = str(proposal.scene_name or "").strip()
        scene_changes = (
            bool(next_location and next_location != current_location)
            or bool(scene_name and scene_name != current_scene_name)
        )
        if scene_changes or not proposal.visible_characters:
            hidden_names.update(
                draft.name.strip()
                for draft in director_decision.generated_characters
                if draft.name.strip() and draft.name.strip() != player_character_name
            )
        return hidden_names

    def _apply_trigger_attribute_updates(self, updates: list[TriggerAttributeUpdate]) -> None:
        self._apply_attribute_updates(updates)

    def _apply_rule_attribute_updates(self, updates: list[RuleAttributeUpdate]) -> None:
        self._apply_attribute_updates(updates)

    def _apply_attribute_updates(self, updates: list[object]) -> None:
        schemas = self._attribute_queries.list_schemas()
        schema_map = {(schema.scope, schema.key): schema for schema in schemas}

        for update in updates:
            scope = "session" if update.owner_type == "session" else "character"
            schema = schema_map.get((scope, update.schema_key))
            if schema is None:
                continue

            self._attribute_commands.upsert_value(
                AttributeValue(
                    id="new",
                    schema_id=schema.id,
                    owner_type=update.owner_type,
                    owner_id=update.owner_id,
                    value=update.value,
                    source=update.source,
                )
            )

    def _to_character_definition(self, world_id: str, draft: GeneratedCharacterDraft) -> CharacterDefinition:
        return CharacterDefinition(
            id="new",
            name=draft.name,
            world_id=world_id,
            role=draft.role,
            background_prompt=draft.background_prompt,
            model=draft.model,
            memory_strategy=draft.memory_strategy,
            attributes=draft.attributes,
        )

    def _build_switch_target_character_definition(
        self,
        *,
        world_id: str,
        session: SessionSnapshot,
        characters: list[CharacterDefinition],
        director_decision: DirectorDecision,
        target_character_name: str,
    ) -> CharacterDefinition:
        player_profile = next(
            (
                item
                for item in characters
                if item.id == session.player_character_id or item.name == session.player_character_name
            ),
            None,
        )
        proposal = director_decision.switch_character_proposal
        matched_draft = next(
            (
                item
                for item in director_decision.generated_characters
                if item.name.strip() == target_character_name
            ),
            None,
        )
        if matched_draft is not None:
            return self._to_character_definition(world_id, matched_draft)

        fallback_reason = proposal.reason.strip() if proposal is not None else ""
        return CharacterDefinition(
            id="new",
            name=target_character_name,
            world_id=world_id,
            role="",
            background_prompt=fallback_reason,
            model=player_profile.model if player_profile is not None else "",
            memory_strategy=player_profile.memory_strategy if player_profile is not None else "",
            recent_dialogue_rounds=player_profile.recent_dialogue_rounds if player_profile is not None else 2,
            attributes=[],
            portrait_assets=[],
            custom_tabs={},
        )

    def _build_switch_scene_companion_character_definition(
        self,
        *,
        world_id: str,
        session: SessionSnapshot,
        characters: list[CharacterDefinition],
        director_decision: DirectorDecision,
        scene_character_name: str,
        target_character_name: str,
    ) -> CharacterDefinition:
        player_profile = next(
            (
                item
                for item in characters
                if item.id == session.player_character_id or item.name == session.player_character_name
            ),
            None,
        )
        proposal = director_decision.switch_character_proposal
        matched_draft = next(
            (
                item
                for item in director_decision.generated_characters
                if item.name.strip() == scene_character_name
            ),
            None,
        )
        if matched_draft is not None:
            return self._to_character_definition(world_id, matched_draft)

        fallback_background_prompt = proposal.reason.strip() if proposal is not None and proposal.reason.strip() else ""

        return CharacterDefinition(
            id="new",
            name=scene_character_name,
            world_id=world_id,
            role="",
            background_prompt=fallback_background_prompt,
            model=player_profile.model if player_profile is not None else "",
            memory_strategy=player_profile.memory_strategy if player_profile is not None else "",
            recent_dialogue_rounds=player_profile.recent_dialogue_rounds if player_profile is not None else 2,
            attributes=[],
            portrait_assets=[],
            custom_tabs={},
        )

    def _infer_switch_scene_location(
        self,
        *,
        current_world: WorldDefinition,
        session: SessionSnapshot,
        proposal: SwitchCharacterProposal,
        target_character: CharacterDefinition,
    ) -> str | None:
        explicit_location = str(proposal.next_location or "").strip()
        if explicit_location:
            return explicit_location

        scene_name = str(proposal.scene_name or "").strip()
        reference_texts = self._switch_scene_reference_texts(
            current_world=current_world,
            session=session,
        )
        candidate_locations = self._candidate_switch_scene_locations(
            current_world=current_world,
            session=session,
            reference_texts=reference_texts,
        )
        matched_scene_name = self._match_switch_scene_location(
            candidate_text=scene_name,
            candidate_locations=candidate_locations,
        )
        if matched_scene_name:
            return matched_scene_name

        situation_sources = [
            scene_name,
            proposal.reason,
            target_character.background_prompt,
            target_character.role,
        ]
        primary_situation_hints = self._extract_switch_scene_hints(
            scene_name,
            proposal.reason,
            target_character.background_prompt,
        )
        for source in situation_sources:
            matched_location = self._match_switch_scene_location(
                candidate_text=source,
                candidate_locations=candidate_locations,
            )
            if matched_location and matched_location not in {session.location, session.scene.name}:
                return matched_location

        supported_hint = self._select_switch_scene_hint(
            candidate_hints=primary_situation_hints,
            supported_hints=candidate_locations,
            current_location=session.location,
            current_scene_name=session.scene.name,
        )
        if supported_hint:
            return supported_hint

        if scene_name and scene_name not in {session.location, session.scene.name}:
            return scene_name

        fallback_hint = self._select_switch_scene_hint(
            candidate_hints=primary_situation_hints,
            supported_hints=[],
            current_location=session.location,
            current_scene_name=session.scene.name,
        )
        if fallback_hint:
            return fallback_hint

        return None

    def _candidate_switch_scene_locations(
        self,
        *,
        current_world: WorldDefinition,
        session: SessionSnapshot,
        reference_texts: list[str],
    ) -> list[str]:
        candidate_locations: list[str] = []
        raw_locations = [
            current_world.opening_scene,
            *current_world.map_nodes,
            *[node.label for node in session.map_graph_nodes],
            session.scene.name,
            session.location,
            *self._extract_switch_scene_hints(*reference_texts),
        ]
        for raw_location in raw_locations:
            location = self._normalize_switch_scene_hint(str(raw_location or "").strip())
            if location and location not in candidate_locations:
                candidate_locations.append(location)
        return candidate_locations

    def _match_switch_scene_location(
        self,
        *,
        candidate_text: str | None,
        candidate_locations: list[str],
    ) -> str | None:
        normalized_text = str(candidate_text or "").strip()
        if not normalized_text:
            return None

        exact_match = next(
            (location for location in candidate_locations if location == normalized_text),
            None,
        )
        if exact_match:
            return exact_match

        fuzzy_matches = [
            location
            for location in candidate_locations
            if location and (location in normalized_text or normalized_text in location)
        ]
        if not fuzzy_matches:
            return None
        return sorted(fuzzy_matches, key=len, reverse=True)[0]

    def _switch_scene_reference_texts(
        self,
        *,
        current_world: WorldDefinition,
        session: SessionSnapshot,
    ) -> list[str]:
        trigger_texts = [str(item or "").strip() for item in current_world.triggers]
        custom_tab_texts = [
            str(item).strip()
            for item in (
                *current_world.custom_tabs.keys(),
                *current_world.custom_tabs.values(),
            )
            if str(item).strip()
        ]
        opening_message_texts = [
            item.content.strip()
            for item in current_world.opening_messages
            if item.content.strip()
        ]
        return [
            current_world.opening_scene,
            current_world.background_prompt,
            current_world.summary,
            *current_world.map_nodes,
            *trigger_texts,
            *custom_tab_texts,
            *opening_message_texts,
            session.location,
            session.scene.name,
            *[node.label for node in session.map_graph_nodes if node.label.strip()],
        ]

    def _extract_switch_scene_hints(self, *texts: str | None) -> list[str]:
        hints: list[str] = []
        for raw_text in texts:
            text = str(raw_text or "").strip()
            if not text:
                continue
            for match in self.SWITCH_SCENE_HINT_PATTERN.finditer(text):
                hint = self._normalize_switch_scene_hint(match.group(0))
                if hint and hint not in hints:
                    hints.append(hint)
        return hints

    def _normalize_switch_scene_hint(self, value: str) -> str:
        hint = re.sub(r"\s+", "", str(value or "").strip())
        hint = hint.strip("，。！？；：、“”‘’（）()【】[]《》<>")
        anchored_match = self.SWITCH_SCENE_HINT_PATTERN.match(hint)
        if anchored_match is not None:
            hint = anchored_match.group(0)
        while True:
            stripped = False
            for prefix in sorted(self.SWITCH_SCENE_PREFIXES, key=len, reverse=True):
                if hint.startswith(prefix) and len(hint) - len(prefix) >= 2:
                    hint = hint[len(prefix) :]
                    stripped = True
            if not stripped:
                break
        for prefix in sorted(self.SWITCH_SCENE_PREFIXES, key=len, reverse=True):
            if prefix not in hint:
                continue
            tail = hint.rsplit(prefix, 1)[-1].strip("，。！？；：、“”‘’（）()【】[]《》<>")
            if len(tail) < 2:
                continue
            tail_match = self.SWITCH_SCENE_HINT_PATTERN.match(tail)
            hint = tail_match.group(0) if tail_match is not None else tail
            break
        anchored_match = self.SWITCH_SCENE_HINT_PATTERN.match(hint)
        if anchored_match is not None:
            hint = anchored_match.group(0)
        hint = hint.strip("，。！？；：、“”‘’（）()【】[]《》<>")
        if len(hint) < 2 or hint in self.SWITCH_SCENE_STOPWORDS:
            return ""
        return hint

    def _select_switch_scene_hint(
        self,
        *,
        candidate_hints: list[str],
        supported_hints: list[str],
        current_location: str,
        current_scene_name: str,
    ) -> str | None:
        normalized_supported = [
            self._normalize_switch_scene_hint(item)
            for item in supported_hints
        ]
        normalized_supported = [item for item in normalized_supported if item]
        fallback_candidates: list[str] = []
        for hint in candidate_hints:
            normalized_hint = self._normalize_switch_scene_hint(hint)
            if not normalized_hint or normalized_hint in {current_location, current_scene_name}:
                continue
            if any(
                normalized_hint == supported
                or normalized_hint in supported
                or supported in normalized_hint
                for supported in normalized_supported
            ):
                return normalized_hint
            fallback_candidates.append(normalized_hint)
        if fallback_candidates:
            return sorted(fallback_candidates, key=len, reverse=True)[0]
        return None

    def _resolve_switch_scene_character_names(
        self,
        *,
        director_decision: DirectorDecision,
        proposal: SwitchCharacterProposal,
        player_character_name: str | None,
    ) -> list[str]:
        resolved_names: list[str] = []
        for raw_name in proposal.visible_characters:
            name = raw_name.strip()
            if not name or name == player_character_name or name == proposal.target_character_name:
                continue
            if name not in resolved_names:
                resolved_names.append(name)

        for draft in director_decision.generated_characters:
            name = draft.name.strip()
            if not name or name == player_character_name or name == proposal.target_character_name:
                continue
            if name not in resolved_names:
                resolved_names.append(name)

        return resolved_names

    def _infer_existing_switch_scene_character_names(
        self,
        *,
        characters: list[CharacterDefinition],
        target_character_name: str,
        player_character_name: str | None,
        resolved_next_location: str | None,
        resolved_scene_name: str | None,
    ) -> list[str]:
        scene_terms = list(
            dict.fromkeys(
                term
                for term in (
                    str(resolved_scene_name or "").strip(),
                    str(resolved_next_location or "").strip(),
                )
                if term
            )
        )
        if not scene_terms:
            return []

        inferred_names: list[str] = []
        for character in characters:
            character_name = character.name.strip()
            if (
                not character_name
                or character_name == target_character_name
                or character_name == player_character_name
            ):
                continue

            scene_hints = "\n".join(
                part.strip()
                for part in (character.role, character.background_prompt)
                if part and part.strip()
            )
            if not scene_hints:
                continue
            if not any(term in scene_hints for term in scene_terms):
                continue
            if character_name not in inferred_names:
                inferred_names.append(character_name)

        return inferred_names[:4]

    def _build_director_action_messages(
        self,
        *,
        director_decision: DirectorDecision,
        character_definition_map: dict[str, CharacterDefinition],
        created_characters_in_turn: list[CharacterDefinition],
        switch_target_created_in_turn: bool,
        current_player_character_name: str | None,
    ) -> list[ChatMessage]:
        messages: list[ChatMessage] = []
        messages.extend(self._build_director_trace_messages(director_decision=director_decision))

        proposal = director_decision.switch_character_proposal
        if proposal is None:
            return messages

        target_character = character_definition_map.get(proposal.target_character_name)
        if (
            target_character is None
            or proposal.target_character_name == current_player_character_name
        ):
            return messages
        created_character_name_set = set()
        for created_character in created_characters_in_turn:
            created_character_name = created_character.name.strip()
            if not created_character_name or created_character_name in created_character_name_set:
                continue
            created_character_name_set.add(created_character_name)
            messages.append(
                ChatMessage(
                    role="system",
                    content=f"世界主控已创建角色：{created_character.name}",
                    metadata={
                        "action_type": "character_created",
                        "character_name": created_character.name,
                        "character_id": created_character.id,
                        "character_role": created_character.role,
                        "character_background_prompt": created_character.background_prompt,
                        "for_switch_character": created_character.name == proposal.target_character_name,
                    },
                )
            )

        messages.append(
            ChatMessage(
                role="system",
                content=proposal.reason,
                metadata={
                    "action_type": "switch_character",
                    "target_character_name": proposal.target_character_name,
                    "target_character_id": target_character.id,
                    "target_role": target_character.role,
                    "target_background_prompt": target_character.background_prompt,
                    "target_created_in_turn": switch_target_created_in_turn,
                    "location": proposal.next_location,
                    "scene_name": proposal.scene_name,
                    "scene_background_hint": proposal.scene_background_hint,
                    "scene_tags": proposal.scene_tags,
                    "visible_characters": proposal.visible_characters,
                },
            )
        )
        return messages

    def _build_director_trace_messages(
        self,
        *,
        director_decision: DirectorDecision,
    ) -> list[ChatMessage]:
        raw_trace = (director_decision.raw_model_response or "").strip()
        if not raw_trace:
            return []
        trace_lines = raw_trace.splitlines()
        content = raw_trace
        return [
            ChatMessage(
                role="system",
                content=content,
                metadata={
                    "action_type": "director_trace",
                    "message_kind": "director_trace",
                    "trace_lines": trace_lines,
                    "trace_text": content,
                    "trace_source": "world_director_model_raw",
                    "world_phase": director_decision.world_phase,
                    "next_location": director_decision.next_location,
                    "next_scene_name": director_decision.next_scene_name,
                    "next_time_label": director_decision.next_time_label,
                    "planned_speakers": list(director_decision.planned_speakers),
                },
            )
        ]

    def _schedule_asset_refresh(
        self,
        *,
        session_id: str,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        session_attributes: list[RuntimeAttributeItem],
        current_speaker: str,
        expected_assets: AssetSelection,
    ) -> None:
        if not self._should_schedule_asset_refresh(
            current_assets=session.assets,
            expected_assets=expected_assets,
            world_profile=world_profile,
        ):
            return

        worker = threading.Thread(
            target=self._refresh_assets_async,
            kwargs={
                "session_id": session_id,
                "session": session,
                "world_profile": world_profile,
                "session_attributes": session_attributes,
                "current_speaker": current_speaker,
                "expected_assets": expected_assets,
            },
            daemon=True,
        )
        worker.start()

    def build_opening_prompt_preview(
        self,
        *,
        world_id: str,
        player_character_id: str | None = None,
        player_input: str = "继续",
    ) -> dict[str, object]:
        current_world = self._catalog_queries.get_world(world_id)
        if current_world is None:
            raise ValueError("World not found")

        session = self._session_repository.build_preview_session(
            world_id=world_id,
            player_character_id=player_character_id,
        )
        characters = self._catalog_queries.list_characters_for_world(current_world.id)
        character_map = {item.name.strip(): item for item in characters if item.name.strip()}
        session_attributes: list[RuntimeAttributeItem] = []
        director_fallback, director_config = self._world_director.build_heuristic_decision(
            session=session,
            world_profile=current_world,
            player_input=player_input,
            session_attributes=session_attributes,
        )
        director_prompt_trace = self._world_director.build_runtime_prompt_call(
            session=session,
            world_profile=current_world,
            player_input=player_input,
            session_attributes=session_attributes,
            fallback=director_fallback,
            director_config=director_config,
            character_profiles=characters,
            stage="玩家第一次输入",
        )

        visible_character_names = [
            name for name in session.visible_characters if name.strip() and name != session.player_character_name
        ]
        planned_speakers = list(
            dict.fromkeys(
                name
                for name in director_fallback.planned_speakers
                if name in visible_character_names and name != session.player_character_name
            )
        )
        if not planned_speakers and visible_character_names:
            planned_speakers = visible_character_names[:1]

        director_preview_decision = DirectorDecision(
            world_phase=director_fallback.world_phase,
            next_location=director_fallback.next_location,
            next_scene_name=director_fallback.next_scene_name,
            next_scene_background_hint=director_fallback.next_scene_background_hint,
            background_asset_name=director_fallback.background_asset_name,
            background_asset_path=director_fallback.background_asset_path,
            background_generation_prompt=director_fallback.background_generation_prompt,
            next_scene_tags=list(director_fallback.next_scene_tags),
            next_time_label=director_fallback.next_time_label,
            generated_characters=list(director_fallback.generated_characters),
            character_visual_directives=list(director_fallback.character_visual_directives),
            scene_visible_characters=list(session.visible_characters),
            planned_speakers=list(planned_speakers),
            switch_character_proposal=director_fallback.switch_character_proposal,
            raw_model_response=None,
            prompt_trace=director_prompt_trace,
        )

        character_prompt_traces: list[dict[str, object]] = []
        trigger_evaluation = TriggerEvaluation()
        for speaker_name in planned_speakers:
            speaker_profile = character_map.get(speaker_name)
            speaker_session_view = self._runtime_visibility.build_character_session_view(
                session=session,
                character_id=speaker_profile.id if speaker_profile is not None else None,
                character_name=speaker_name,
            )
            speaker_scene_state = self._runtime_visibility.build_scene_state(
                session=speaker_session_view,
                visible_attributes=[],
            )
            prompt_trace = self._character_runtime.build_prompt_trace_preview(
                session=speaker_session_view,
                speaker=speaker_name,
                speaker_profile=speaker_profile,
                world_profile=current_world,
                player_input=player_input,
                session_attributes=[],
                speaker_memories=[],
                recent_dialogue=list(session.messages),
                scene_state=speaker_scene_state,
                director_decision=director_preview_decision,
                trigger_evaluation=trigger_evaluation,
            )
            character_prompt_traces.append({"speaker": speaker_name, "prompt_trace": prompt_trace})

        return {
            "opening_calls_llm": False,
            "opening_messages": [
                {
                    "role": item.role,
                    "content": item.content,
                    "speaker": item.speaker,
                }
                for item in session.messages
            ],
            "sample_player_input": player_input,
            "planned_speakers": planned_speakers,
            "world_director_prompt_trace": director_prompt_trace,
            "character_prompt_traces": character_prompt_traces,
            "notes": [
                "新建会话本身不会调用 LLM；开场聊天消息直接来自世界的 opening_messages。",
                "下面展示的是玩家发出第一条输入后，首轮真正会发给世界主控和角色模型的完整 Prompt 预览。",
                "该预览使用当前世界配置、开场场景角色和示例玩家输入生成，不包含运行后的动态记忆与已写回属性。",
            ],
        }

    def _serialize_session_snapshot(self, session: SessionSnapshot) -> dict[str, object]:
        return {
            "id": session.id,
            "world_name": session.world_name,
            "location": session.location,
            "time_label": session.time_label,
            "current_speaker": session.current_speaker,
            "current_line": session.current_line,
            "player_character_id": session.player_character_id,
            "player_character_name": session.player_character_name,
            "visible_characters": list(session.visible_characters),
            "messages": [
                {
                    "role": message.role,
                    "content": message.content,
                    "speaker": message.speaker,
                    "metadata": dict(message.metadata or {}) if isinstance(message.metadata, dict) else None,
                }
                for message in session.messages
            ],
            "player_stats": list(session.player_stats),
            "map_graph_nodes": [
                {
                    "node_id": node.node_id,
                    "label": node.label,
                    "discovered": node.discovered,
                    "current": node.current,
                }
                for node in session.map_graph_nodes
            ],
            "map_graph_edges": [
                {
                    "edge_id": edge.edge_id,
                    "source_node_id": edge.source_node_id,
                    "target_node_id": edge.target_node_id,
                }
                for edge in session.map_graph_edges
            ],
            "inventory_items": [
                {
                    "item_id": item.item_id,
                    "name": item.name,
                    "category": item.category,
                    "quantity": item.quantity,
                    "description": item.description,
                    "tags": list(item.tags),
                    "owner_type": item.owner_type,
                    "owner_id": item.owner_id,
                    "visibility": item.visibility,
                    "disclosed_to": list(item.disclosed_to),
                }
                for item in session.inventory_items
            ],
            "system_log": list(session.system_log),
            "scene": {
                "scene_id": session.scene.scene_id,
                "name": session.scene.name,
                "background_hint": session.scene.background_hint,
                "temporary_tags": list(session.scene.temporary_tags),
                "present_characters": list(session.scene.present_characters),
            },
            "assets": {
                "background_hint": session.assets.background_hint,
                "active_speaker_portrait": session.assets.active_speaker_portrait,
                "background_asset_path": session.assets.background_asset_path,
                "active_speaker_portrait_path": session.assets.active_speaker_portrait_path,
                "background_generation_prompt": session.assets.background_generation_prompt,
                "active_speaker_generation_prompt": session.assets.active_speaker_generation_prompt,
                "visible_character_portraits": [
                    {
                        "character_name": item.character_name,
                        "portrait_hint": item.portrait_hint,
                        "portrait_asset_path": item.portrait_asset_path,
                        "generation_prompt": item.generation_prompt,
                    }
                    for item in session.assets.visible_character_portraits
                ],
            },
            "state": {
                "metrics": dict(session.state.metrics),
                "tags": list(session.state.tags),
                "phase": session.state.phase,
            },
        }

    def _deserialize_session_snapshot(self, raw: object) -> SessionSnapshot | None:
        if not isinstance(raw, dict):
            return None

        def optional_text(value: object) -> str | None:
            text = str(value or "").strip()
            return text or None

        def string_list(value: object) -> list[str]:
            if not isinstance(value, list):
                return []
            return [str(item).strip() for item in value if str(item).strip()]

        session_id = optional_text(raw.get("id"))
        world_name = optional_text(raw.get("world_name"))
        location = optional_text(raw.get("location"))
        time_label = optional_text(raw.get("time_label"))
        current_speaker = optional_text(raw.get("current_speaker"))
        if not session_id or not world_name or not location or not time_label or not current_speaker:
            return None

        raw_messages = raw.get("messages", [])
        raw_scene = raw.get("scene")
        raw_assets = raw.get("assets")
        raw_state = raw.get("state")
        raw_nodes = raw.get("map_graph_nodes", [])
        raw_edges = raw.get("map_graph_edges", [])
        raw_inventory_items = raw.get("inventory_items", [])

        metrics: dict[str, float] = {}
        if isinstance(raw_state, dict) and isinstance(raw_state.get("metrics"), dict):
            for key, value in raw_state.get("metrics", {}).items():
                if isinstance(value, (int, float)):
                    metrics[str(key)] = float(value)

        return SessionSnapshot(
            id=session_id,
            world_name=world_name,
            location=location,
            time_label=time_label,
            current_speaker=current_speaker,
            current_line=str(raw.get("current_line") or ""),
            player_character_id=optional_text(raw.get("player_character_id")),
            player_character_name=optional_text(raw.get("player_character_name")),
            visible_characters=string_list(raw.get("visible_characters")),
            messages=[
                ChatMessage(
                    role=str(item.get("role") or "system"),
                    content=str(item.get("content") or ""),
                    speaker=optional_text(item.get("speaker")),
                    metadata=dict(item.get("metadata")) if isinstance(item.get("metadata"), dict) else None,
                )
                for item in raw_messages
                if isinstance(item, dict) and str(item.get("content") or "").strip()
            ],
            player_stats=string_list(raw.get("player_stats")),
            map_graph_nodes=[
                SessionMapNode(
                    node_id=str(item.get("node_id") or ""),
                    label=str(item.get("label") or ""),
                    discovered=bool(item.get("discovered", True)),
                    current=bool(item.get("current", False)),
                )
                for item in raw_nodes
                if isinstance(item, dict) and str(item.get("node_id") or "").strip()
            ],
            map_graph_edges=[
                SessionMapEdge(
                    edge_id=str(item.get("edge_id") or ""),
                    source_node_id=str(item.get("source_node_id") or ""),
                    target_node_id=str(item.get("target_node_id") or ""),
                )
                for item in raw_edges
                if isinstance(item, dict)
                and str(item.get("edge_id") or "").strip()
                and str(item.get("source_node_id") or "").strip()
                and str(item.get("target_node_id") or "").strip()
            ],
            inventory_items=[
                InventoryItem(
                    item_id=str(item.get("item_id") or "item-unknown"),
                    name=str(item.get("name") or "unknown"),
                    category=str(item.get("category") or "misc"),
                    quantity=int(item.get("quantity", 1) or 1),
                    description=str(item.get("description") or ""),
                    tags=string_list(item.get("tags")),
                    owner_type=str(item.get("owner_type") or "player"),
                    owner_id=str(item.get("owner_id") or "player"),
                    visibility=str(item.get("visibility") or "private"),
                    disclosed_to=string_list(item.get("disclosed_to")),
                )
                for item in raw_inventory_items
                if isinstance(item, dict)
            ],
            system_log=string_list(raw.get("system_log")),
            scene=SceneRuntime(
                scene_id=str(raw_scene.get("scene_id") or "default") if isinstance(raw_scene, dict) else "default",
                name=str(raw_scene.get("name") or location) if isinstance(raw_scene, dict) else location,
                background_hint=(
                    str(raw_scene.get("background_hint") or "default")
                    if isinstance(raw_scene, dict)
                    else "default"
                ),
                temporary_tags=string_list(raw_scene.get("temporary_tags")) if isinstance(raw_scene, dict) else [],
                present_characters=(
                    string_list(raw_scene.get("present_characters"))
                    if isinstance(raw_scene, dict)
                    else string_list(raw.get("visible_characters"))
                ),
            ),
            assets=AssetSelection(
                background_hint=(
                    str(raw_assets.get("background_hint") or "default")
                    if isinstance(raw_assets, dict)
                    else "default"
                ),
                active_speaker_portrait=(
                    str(raw_assets.get("active_speaker_portrait") or "default")
                    if isinstance(raw_assets, dict)
                    else "default"
                ),
                background_asset_path=(
                    optional_text(raw_assets.get("background_asset_path"))
                    if isinstance(raw_assets, dict)
                    else None
                ),
                active_speaker_portrait_path=(
                    optional_text(raw_assets.get("active_speaker_portrait_path"))
                    if isinstance(raw_assets, dict)
                    else None
                ),
                background_generation_prompt=(
                    optional_text(raw_assets.get("background_generation_prompt"))
                    if isinstance(raw_assets, dict)
                    else None
                ),
                active_speaker_generation_prompt=(
                    optional_text(raw_assets.get("active_speaker_generation_prompt"))
                    if isinstance(raw_assets, dict)
                    else None
                ),
                visible_character_portraits=[
                    CharacterVisualState(
                        character_name=str(item.get("character_name") or ""),
                        portrait_hint=str(item.get("portrait_hint") or "default"),
                        portrait_asset_path=optional_text(item.get("portrait_asset_path")),
                        generation_prompt=optional_text(item.get("generation_prompt")),
                    )
                    for item in (
                        raw_assets.get("visible_character_portraits", []) if isinstance(raw_assets, dict) else []
                    )
                    if isinstance(item, dict) and str(item.get("character_name") or "").strip()
                ],
            ),
            state=SessionState(
                metrics=metrics,
                tags=string_list(raw_state.get("tags")) if isinstance(raw_state, dict) else [],
                phase=str(raw_state.get("phase") or "idle") if isinstance(raw_state, dict) else "idle",
            ),
        )

    def _serialize_attribute_values(self, values: list[AttributeValue]) -> list[dict[str, object]]:
        return [
            {
                "id": value.id,
                "schema_id": value.schema_id,
                "owner_type": value.owner_type,
                "owner_id": value.owner_id,
                "value": value.value,
                "source": value.source,
            }
            for value in values
        ]

    def _deserialize_attribute_values(self, raw: object) -> list[AttributeValue]:
        if not isinstance(raw, list):
            return []

        restored_values: list[AttributeValue] = []
        for item in raw:
            if not isinstance(item, dict):
                continue
            schema_id = str(item.get("schema_id") or "").strip()
            owner_type = str(item.get("owner_type") or "").strip()
            owner_id = str(item.get("owner_id") or "").strip()
            value_id = str(item.get("id") or "").strip()
            if not schema_id or not owner_type or not owner_id or not value_id:
                continue
            restored_values.append(
                AttributeValue(
                    id=value_id,
                    schema_id=schema_id,
                    owner_type=owner_type,
                    owner_id=owner_id,
                    value=item.get("value"),
                    source=str(item.get("source") or "system"),
                )
            )
        return restored_values

    def _collect_runtime_attribute_values(self, session_id: str) -> list[AttributeValue]:
        session_values = list(
            self._attribute_queries.list_values(owner_type="session", owner_id=session_id)
        )
        character_values = [
            value
            for value in self._attribute_queries.list_values(owner_type="session_character")
            if value.owner_id.startswith(f"{session_id}:")
        ]
        return sorted(
            [*session_values, *character_values],
            key=lambda item: (item.owner_type, item.owner_id, item.schema_id, item.id),
        )

    def _collect_created_character_ids_from_turn(self, session_id: str, from_turn_index: int) -> list[str]:
        created_character_ids: list[str] = []
        seen: set[str] = set()
        journal_entries = sorted(
            self._agent_runtime_manager.list_turn_journal(session_id=session_id),
            key=lambda entry: (
                int(getattr(entry, "turn_index", 0) or 0),
                str(getattr(entry, "created_at", "")),
                str(getattr(entry, "id", "")),
            ),
        )
        for entry in journal_entries:
            if getattr(entry, "status", None) != "completed":
                continue
            if getattr(entry, "step", None) != "characters_created":
                continue
            if int(getattr(entry, "turn_index", 0) or 0) < from_turn_index:
                continue
            payload = getattr(entry, "payload", None)
            if not isinstance(payload, dict):
                continue
            raw_character_ids = payload.get("character_ids", [])
            if not isinstance(raw_character_ids, list):
                continue
            for item in raw_character_ids:
                character_id = str(item).strip()
                if not character_id or character_id in seen:
                    continue
                seen.add(character_id)
                created_character_ids.append(character_id)
        return created_character_ids

    def _journal_has_completed_step(self, recovery_journal: list[object], step: str) -> bool:
        return any(getattr(entry, "step", None) == step and getattr(entry, "status", None) == "completed" for entry in recovery_journal)

    def _journal_payload(self, recovery_journal: list[object], step: str) -> dict[str, object] | None:
        for entry in recovery_journal:
            if getattr(entry, "step", None) == step and getattr(entry, "status", None) == "completed":
                payload = getattr(entry, "payload", None)
                return payload if isinstance(payload, dict) else {}
        return None

    def _visible_characters_from_journal(self, recovery_journal: list[object]) -> list[str]:
        payload = self._journal_payload(recovery_journal, "scene_applied")
        if payload is None:
            return []
        return [str(item).strip() for item in payload.get("visible_characters", []) if str(item).strip()]

    def _completed_speaker_steps(self, recovery_journal: list[object]) -> set[int]:
        completed: set[int] = set()
        for entry in recovery_journal:
            step = getattr(entry, "step", "")
            status = getattr(entry, "status", "")
            if status != "completed" or not isinstance(step, str):
                continue
            if not step.startswith("speaker_") or not step.endswith("_completed"):
                continue
            numeric_part = step[len("speaker_") : -len("_completed")]
            if numeric_part.isdigit():
                completed.add(int(numeric_part))
        return completed

    def _refresh_assets_async(
        self,
        *,
        session_id: str,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        session_attributes: list[RuntimeAttributeItem],
        current_speaker: str,
        expected_assets: AssetSelection,
    ) -> None:
        try:
            generated_assets = self._asset_resolver.resolve(
                session=session,
                scene=session.scene,
                state=session.state,
                current_speaker=current_speaker,
                session_attributes=session_attributes,
                world_profile=world_profile,
                allow_generation=True,
            )
        except Exception:
            return

        if generated_assets == expected_assets:
            return

        latest_session = self._session_repository.get_session(session_id)
        if latest_session is None:
            return
        if self._asset_refresh_became_stale(latest_session.assets, expected_assets):
            return
        if latest_session.assets == generated_assets:
            return

        self._session_repository.update_session_assets(session_id, generated_assets)

    def _should_schedule_asset_refresh(
        self,
        *,
        current_assets: AssetSelection,
        expected_assets: AssetSelection,
        world_profile: WorldDefinition | None,
    ) -> bool:
        if not self._world_allows_mcp_tool(world_profile, "mcp-tool-image-generation"):
            return False

        if expected_assets.background_generation_prompt and expected_assets.background_asset_path is None:
            return True

        if expected_assets.active_speaker_generation_prompt and expected_assets.active_speaker_portrait_path is None:
            return True

        current_visible = {
            item.character_name: (
                item.portrait_hint,
                item.portrait_asset_path,
                item.generation_prompt,
            )
            for item in current_assets.visible_character_portraits
        }
        for portrait in expected_assets.visible_character_portraits:
            previous_hint, previous_path, previous_prompt = current_visible.get(
                portrait.character_name,
                ("", None, None),
            )
            if portrait.generation_prompt and (
                portrait.portrait_asset_path is None
                or portrait.portrait_hint != previous_hint
                or portrait.generation_prompt != previous_prompt
                or previous_path is None
            ):
                return True

        return False

    def _world_allows_mcp_tool(self, world_profile: WorldDefinition | None, tool_id: str) -> bool:
        if world_profile is None:
            return False
        allowed_tool_ids = world_profile.director_config.get("allowed_mcp_tool_ids", [])
        if not isinstance(allowed_tool_ids, list):
            return False
        return tool_id in {str(item).strip() for item in allowed_tool_ids if str(item).strip()}

    def _asset_refresh_became_stale(
        self,
        current_assets: AssetSelection,
        expected_assets: AssetSelection,
    ) -> bool:
        if current_assets.background_hint != expected_assets.background_hint:
            return True
        if current_assets.background_generation_prompt != expected_assets.background_generation_prompt:
            return True
        if current_assets.active_speaker_portrait != expected_assets.active_speaker_portrait:
            return True
        if current_assets.active_speaker_generation_prompt != expected_assets.active_speaker_generation_prompt:
            return True

        current_visible = {
            item.character_name: (item.portrait_hint, item.generation_prompt)
            for item in current_assets.visible_character_portraits
        }
        expected_visible = {
            item.character_name: (item.portrait_hint, item.generation_prompt)
            for item in expected_assets.visible_character_portraits
        }
        return current_visible != expected_visible

    def _materialize_switch_character_proposal(
        self,
        *,
        session_id: str,
        current_world: WorldDefinition,
        session: SessionSnapshot,
        director_decision: DirectorDecision,
        characters: list[CharacterDefinition],
    ) -> tuple[DirectorDecision, list[CharacterDefinition], list[CharacterDefinition]]:
        proposal = director_decision.switch_character_proposal
        if proposal is None:
            return director_decision, characters, []

        target_character_name = proposal.target_character_name.strip()
        if not target_character_name or target_character_name == session.player_character_name:
            return (
                replace(
                    director_decision,
                    switch_character_proposal=None,
                ),
                characters,
                [],
            )

        character_map = {item.name.strip(): item for item in characters if item.name.strip()}
        created_characters: list[CharacterDefinition] = []
        if target_character_name not in character_map:
            created_character = self._catalog_commands.create_character(
                self._build_switch_target_character_definition(
                    world_id=current_world.id,
                    session=session,
                    characters=characters,
                    director_decision=director_decision,
                    target_character_name=target_character_name,
                )
            )
            characters = [*characters, created_character]
            character_map[target_character_name] = created_character
            created_characters.append(created_character)

        target_character = character_map[target_character_name]
        resolved_next_location = self._infer_switch_scene_location(
            current_world=current_world,
            session=session,
            proposal=proposal,
            target_character=target_character,
        )
        resolved_scene_name = str(proposal.scene_name or "").strip() or resolved_next_location
        resolved_scene_character_names = self._resolve_switch_scene_character_names(
            director_decision=director_decision,
            proposal=proposal,
            player_character_name=session.player_character_name,
        )
        if not resolved_scene_character_names:
            resolved_scene_character_names = self._infer_existing_switch_scene_character_names(
                characters=characters,
                target_character_name=target_character_name,
                player_character_name=session.player_character_name,
                resolved_next_location=resolved_next_location,
                resolved_scene_name=resolved_scene_name,
            )

        for scene_character_name in resolved_scene_character_names:
            if scene_character_name in character_map:
                continue
            created_character = self._catalog_commands.create_character(
                self._build_switch_scene_companion_character_definition(
                    world_id=current_world.id,
                    session=session,
                    characters=characters,
                    director_decision=director_decision,
                    scene_character_name=scene_character_name,
                    target_character_name=target_character_name,
                )
            )
            characters = [*characters, created_character]
            character_map[scene_character_name] = created_character
            created_characters.append(created_character)

        if created_characters:
            self._attribute_commands.project_session_attributes(
                session_id=session_id,
                world_id=current_world.id,
                character_ids=[item.id for item in created_characters],
            )

        sanitized_visible_characters = list(
            dict.fromkeys(
                name
                for name in resolved_scene_character_names
                if name
                and name in character_map
                and name != target_character_name
                and name != session.player_character_name
            )
        )

        updated_proposal = replace(
            proposal,
            next_location=resolved_next_location,
            scene_name=resolved_scene_name,
            visible_characters=sanitized_visible_characters,
        )

        return (
            replace(
                director_decision,
                switch_character_proposal=updated_proposal,
            ),
            characters,
            created_characters,
        )

    def _switch_player_character_with_proposal(
        self,
        *,
        session: SessionSnapshot,
        current_world: WorldDefinition,
        target_character: CharacterDefinition,
        proposal_payload: dict[str, object],
    ) -> SessionSnapshot | None:
        location_override = str(proposal_payload.get("location") or "").strip() or None
        scene_name = str(proposal_payload.get("scene_name") or "").strip() or None
        scene_background_hint = str(proposal_payload.get("scene_background_hint") or "").strip() or None
        scene_tags = [
            str(item).strip()
            for item in proposal_payload.get("scene_tags", [])
            if str(item).strip()
        ] if isinstance(proposal_payload.get("scene_tags"), list) else []

        visible_characters_override = None
        if isinstance(proposal_payload.get("visible_characters"), list):
            visible_characters_override = [
                str(item).strip()
                for item in proposal_payload.get("visible_characters", [])
                if str(item).strip() and str(item).strip() != target_character.name
            ]

        next_location = location_override or session.location
        next_scene_name = scene_name or next_location or session.scene.name
        next_visible_characters = (
            list(visible_characters_override)
            if visible_characters_override is not None
            else list(session.visible_characters)
        )
        if visible_characters_override is None and session.player_character_name and session.player_character_name != target_character.name:
            if session.player_character_name not in next_visible_characters:
                next_visible_characters.append(session.player_character_name)
        next_visible_characters = [
            name
            for name in (item.strip() for item in next_visible_characters)
            if name and name != target_character.name
        ]
        next_visible_characters = list(dict.fromkeys(next_visible_characters))

        scene_override = SceneRuntime(
            scene_id=self._slugify_scene_id(next_scene_name),
            name=next_scene_name,
            background_hint=scene_background_hint or session.scene.background_hint or f"{next_scene_name}:switch",
            temporary_tags=(
                list(dict.fromkeys(scene_tags))
                if isinstance(proposal_payload.get("scene_tags"), list)
                else list(dict.fromkeys(session.scene.temporary_tags))
            ),
            present_characters=self._build_turn_participants(
                visible_character_names=next_visible_characters,
                player_character_name=target_character.name,
            ),
        )
        switch_line = (
            f"{target_character.name}的视角已启用，当前地点：{next_location}。"
            if next_location
            else f"{target_character.name}的视角已启用。"
        )

        updated_session = self._session_repository.update_player_character(
            session_id=session.id,
            player_character_id=target_character.id,
            player_character_name=target_character.name,
            location_override=location_override,
            visible_characters_override=next_visible_characters,
            scene_override=scene_override,
            current_speaker=target_character.name,
            current_line=switch_line,
            system_messages=[switch_line],
        )
        if updated_session is None:
            return None

        session_attributes, _ = self._attribute_runtime.list_game_visible_attributes(session_id=session.id)
        assets = self._asset_resolver.resolve(
            session=updated_session,
            scene=updated_session.scene,
            state=updated_session.state,
            current_speaker=updated_session.current_speaker,
            session_attributes=session_attributes,
            world_profile=current_world,
            allow_generation=False,
        )
        return self._session_repository.update_player_character(
            session_id=session.id,
            player_character_id=target_character.id,
            player_character_name=target_character.name,
            location_override=updated_session.location,
            visible_characters_override=updated_session.visible_characters,
            scene_override=updated_session.scene,
            assets_override=assets,
            current_speaker=updated_session.current_speaker,
            current_line=updated_session.current_line,
        ) or updated_session

    def _world_director_history_rounds(self, world_profile: WorldDefinition | None) -> int:
        director_config = self._world_director._resolve_director_config(world_profile)
        raw_value = director_config.get("history_dialogue_rounds", 6)
        if isinstance(raw_value, int):
            return max(0, min(raw_value, 20))
        return 6

    def _slugify_scene_id(self, value: str) -> str:
        normalized = "".join(char.lower() if char.isalnum() else "-" for char in value).strip("-")
        while "--" in normalized:
            normalized = normalized.replace("--", "-")
        return normalized or "scene-switch"
