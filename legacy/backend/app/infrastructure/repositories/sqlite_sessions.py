import asyncio
from collections import defaultdict
from datetime import datetime
import uuid
from typing import Any

from backend.app.domain.models.attribute import AttributeValue
from backend.app.domain.models.asset import AssetSelection, CharacterVisualState
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.save import SaveSummary
from backend.app.domain.models.session import ChatMessage, SessionMapEdge, SessionMapNode, SessionSnapshot
from backend.app.domain.models.state import SessionState
from backend.app.domain.models.world import WorldDefinition
from backend.app.infrastructure.repositories.sqlite_attributes import SqliteAttributeRepository
from backend.app.infrastructure.repositories.sqlite_catalog import SqliteCatalogRepository
from backend.app.infrastructure.sqlite_store import SqliteStore, row_to_save, row_to_session


class SqliteSessionRepository:
    def __init__(
        self,
        store: SqliteStore,
        catalog_repository: SqliteCatalogRepository,
        attribute_repository: SqliteAttributeRepository,
    ) -> None:
        self._store = store
        self._catalog_repository = catalog_repository
        self._attribute_repository = attribute_repository
        self._listeners: dict[str, list[asyncio.Queue[SessionSnapshot]]] = defaultdict(list)

    def list_saves(self) -> list[SaveSummary]:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT * FROM saves ORDER BY updated_at DESC, id DESC").fetchall()
        return [row_to_save(row) for row in rows]

    def get_session(self, session_id: str) -> SessionSnapshot | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM sessions WHERE id = ?", (session_id,)).fetchone()
        return row_to_session(row) if row else None

    def create_session(self, world_id: str, player_character_id: str | None = None) -> SessionSnapshot:
        world = self._catalog_repository.get_world(world_id)
        if world is None:
            raise ValueError("World not found")

        session_id = f"session-{uuid.uuid4().hex[:8]}"
        session = self._build_session_from_world(
            world=world,
            session_id=session_id,
            player_character_id=player_character_id,
        )
        projected_attributes = self._attribute_repository.project_session_attributes(
            session_id=session_id,
            world_id=world.id,
            character_ids=[item.id for item in self._characters_for_world(world.id)],
        )
        if projected_attributes:
            session = self._with_attribute_log(
                session=session,
                projected=projected_attributes,
                prefix="属性已投影到新会话",
            )

        save = self._build_save_summary(session=session)
        with self._store.connect() as connection:
            self._store.upsert_session(connection, session)
            self._store.upsert_save(connection, save)
        self._publish_snapshot(session)
        return session

    def build_preview_session(self, world_id: str, player_character_id: str | None = None) -> SessionSnapshot:
        world = self._catalog_repository.get_world(world_id)
        if world is None:
            raise ValueError("World not found")
        return self._build_session_from_world(
            world=world,
            session_id="session-preview",
            player_character_id=player_character_id,
        )

    def submit_player_action(
        self,
        session_id: str,
        content: str,
        turn_index: int | None = None,
        time_label_override: str | None = None,
        agent_messages: list[ChatMessage] | None = None,
        responder: str | None = None,
        response_text: str | None = None,
        debug_lines: list[str] | None = None,
        location_override: str | None = None,
        visible_characters_override: list[str] | None = None,
        narration_messages: list[str] | None = None,
        log_messages: list[str] | None = None,
        inventory_override: list[InventoryItem] | None = None,
        scene_override: SceneRuntime | None = None,
        assets_override: AssetSelection | None = None,
        state_override: SessionState | None = None,
        switch_proposal_messages: list[ChatMessage] | None = None,
    ) -> SessionSnapshot | None:
        session = self.get_session(session_id)
        if session is None:
            return None

        next_location = location_override or session.location
        next_visible_characters = (
            visible_characters_override if visible_characters_override is not None else session.visible_characters
        )
        next_map_graph_nodes, next_map_graph_edges = self._expand_map_graph(
            existing_nodes=session.map_graph_nodes,
            existing_edges=session.map_graph_edges,
            previous_location=session.location,
            next_location=next_location,
        )
        next_inventory = inventory_override if inventory_override is not None else session.inventory_items
        next_scene = scene_override or session.scene
        next_assets = assets_override or session.assets
        next_state = state_override or session.state
        resolved_turn_index = max(1, int(turn_index or 1))
        normalized_agent_messages = [
            ChatMessage(
                role="agent",
                speaker=message.speaker,
                content=message.content.strip(),
                metadata=self._merge_message_metadata(
                    message.metadata,
                    turn_index=resolved_turn_index,
                    message_kind="agent_response",
                ),
            )
            for message in (agent_messages or [])
            if message.role == "agent" and message.content.strip()
        ]
        if normalized_agent_messages:
            selected_responder = normalized_agent_messages[-1].speaker or responder or "系统"
            resolved_response_text = normalized_agent_messages[-1].content
        else:
            fallback_response_text = (response_text or "").strip()
            selected_responder = responder or (session.visible_characters[0] if session.visible_characters else "系统")
            resolved_response_text = fallback_response_text or session.current_line
            normalized_agent_messages = (
                [
                    ChatMessage(
                        role="agent",
                        speaker=selected_responder,
                        content=resolved_response_text,
                        metadata=self._merge_message_metadata(
                            None,
                            turn_index=resolved_turn_index,
                            message_kind="agent_response",
                        ),
                    )
                ]
                if fallback_response_text
                else []
            )
        updated_attributes = self._attribute_repository.apply_player_action_effects(
            session_id=session_id,
            content=content,
        )
        narration_messages = self._dedupe_narration_messages(
            session.messages,
            narration_messages or [],
        )
        log_messages = log_messages or []

        updated_session = SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=next_location,
            time_label=time_label_override or session.time_label,
            current_speaker=normalized_agent_messages[-1].speaker if normalized_agent_messages else session.current_speaker,
            current_line=normalized_agent_messages[-1].content if normalized_agent_messages else session.current_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=next_visible_characters,
            messages=[
                *session.messages,
                ChatMessage(
                    role="player",
                    content=content,
                    speaker=self._resolved_player_speaker(session.player_character_name),
                    metadata=self._merge_message_metadata(
                        None,
                        turn_index=resolved_turn_index,
                        message_kind="player_action",
                    ),
                ),
                *[
                    ChatMessage(
                        role="system",
                        content=item,
                        metadata=self._merge_message_metadata(
                            None,
                            turn_index=resolved_turn_index,
                            message_kind="narration",
                        ),
                    )
                    for item in narration_messages
                ],
                *normalized_agent_messages,
                *[
                    ChatMessage(
                        role=item.role,
                        content=item.content,
                        speaker=item.speaker,
                        metadata=self._merge_message_metadata(
                            item.metadata,
                            turn_index=resolved_turn_index,
                            message_kind=self._resolve_system_message_kind(item.metadata),
                        ),
                    )
                    for item in (switch_proposal_messages or [])
                ],
            ],
            player_stats=[
                *([f"当前身份：{session.player_character_name}"] if session.player_character_name else []),
                f"当前场景可见人物：{len(next_visible_characters)}",
            ],
            map_graph_nodes=next_map_graph_nodes,
            map_graph_edges=next_map_graph_edges,
            inventory_items=next_inventory,
            system_log=[
                *log_messages,
                *(debug_lines or []),
                *self._attribute_log_lines(updated_attributes),
                *session.system_log,
            ][:6],
            scene=next_scene,
            assets=next_assets,
            state=next_state,
        )

        save = self._build_save_summary(session=updated_session)
        with self._store.connect() as connection:
            self._store.upsert_session(connection, updated_session)
            self._store.upsert_save(connection, save)
        self._publish_snapshot(updated_session)
        return updated_session

    def _dedupe_narration_messages(
        self,
        existing_messages: list[ChatMessage],
        narration_messages: list[str],
    ) -> list[str]:
        latest_system_content = next(
            (
                message.content.strip()
                for message in reversed(existing_messages)
                if message.role == "system" and message.content.strip()
            ),
            "",
        )
        deduped: list[str] = []
        previous_content = latest_system_content
        for raw_message in narration_messages:
            content = raw_message.strip()
            if not content:
                continue
            if content == previous_content:
                continue
            deduped.append(content)
            previous_content = content
        return deduped

    def rollback_to_turn_snapshot(
        self,
        *,
        session: SessionSnapshot,
        runtime_attribute_values: list[AttributeValue],
        from_turn_index: int,
        delete_character_ids: list[str] | None = None,
    ) -> SessionSnapshot:
        delete_character_ids = [
            character_id
            for character_id in dict.fromkeys(delete_character_ids or [])
            if character_id.strip()
        ]
        with self._store.connect() as connection:
            self._delete_runtime_attribute_values(connection, session_id=session.id)
            for value in runtime_attribute_values:
                self._store.upsert_attribute_value(connection, value)

            connection.execute(
                "DELETE FROM memories WHERE session_id = ? AND turn_index >= ?",
                (session.id, from_turn_index),
            )

            agent_session_rows = connection.execute(
                "SELECT id FROM agent_sessions WHERE session_id = ?",
                (session.id,),
            ).fetchall()
            agent_session_ids = [row["id"] for row in agent_session_rows]
            if agent_session_ids:
                placeholders = ", ".join("?" for _ in agent_session_ids)
                connection.execute(
                    f"DELETE FROM agent_checkpoints WHERE agent_session_id IN ({placeholders})",
                    tuple(agent_session_ids),
                )
            connection.execute("DELETE FROM agent_sessions WHERE session_id = ?", (session.id,))
            connection.execute(
                "DELETE FROM turn_journal WHERE session_id = ? AND turn_index >= ?",
                (session.id, from_turn_index),
            )

            for character_id in delete_character_ids:
                connection.execute(
                    "DELETE FROM attribute_values WHERE owner_type = 'character' AND owner_id = ?",
                    (character_id,),
                )
                connection.execute("DELETE FROM characters WHERE id = ?", (character_id,))

            self._store.upsert_session(connection, session)
            self._store.upsert_save(connection, self._build_save_summary(session=session))
        self._publish_snapshot(session)
        return session

    def _merge_message_metadata(
        self,
        metadata: dict[str, object] | None,
        *,
        turn_index: int,
        message_kind: str,
    ) -> dict[str, object]:
        merged = dict(metadata or {})
        merged["turn_index"] = turn_index
        if message_kind:
            merged["message_kind"] = message_kind
        return merged

    def _resolve_system_message_kind(self, metadata: dict[str, object] | None) -> str:
        merged = dict(metadata or {})
        existing_kind = str(merged.get("message_kind") or "").strip()
        if existing_kind:
            return existing_kind
        action_type = str(merged.get("action_type") or "").strip()
        if action_type == "director_trace":
            return "director_trace"
        return "system_action"

    def _delete_runtime_attribute_values(self, connection, *, session_id: str) -> None:
        connection.execute(
            "DELETE FROM attribute_values WHERE owner_type = 'session' AND owner_id = ?",
            (session_id,),
        )
        connection.execute(
            "DELETE FROM attribute_values WHERE owner_type = 'session_character' AND owner_id LIKE ?",
            (f"{session_id}:%",),
        )

    def delete_save(self, save_id: str) -> bool:
        with self._store.connect() as connection:
            row = connection.execute("SELECT session_id FROM saves WHERE id = ?", (save_id,)).fetchone()
            if row is None:
                return False
            session_id = row["session_id"]
            self._delete_save_records(connection, save_id=save_id, session_id=session_id)
        return True

    def delete_all_saves(self) -> int:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT id, session_id FROM saves").fetchall()
            for row in rows:
                self._delete_save_records(connection, save_id=row["id"], session_id=row["session_id"])
        return len(rows)

    def update_session_assets(
        self,
        session_id: str,
        assets: AssetSelection,
    ) -> SessionSnapshot | None:
        session = self.get_session(session_id)
        if session is None:
            return None

        updated_session = SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            current_speaker=session.current_speaker,
            current_line=session.current_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=session.visible_characters,
            messages=session.messages,
            player_stats=session.player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=session.system_log,
            scene=session.scene,
            assets=assets,
            state=session.state,
        )

        with self._store.connect() as connection:
            self._store.upsert_session(connection, updated_session)
        self._publish_snapshot(updated_session)
        return updated_session

    def _update_player_character_legacy(
        self,
        session_id: str,
        player_character_id: str | None,
        player_character_name: str | None,
    ) -> SessionSnapshot | None:
        session = self.get_session(session_id)
        if session is None:
            return None

        # Rebuild visible_characters: exclude old player character, exclude new player character
        old_player_id = session.player_character_id
        new_player_id = player_character_id
        world_characters = self._characters_for_world(
            next((w.id for w in self._catalog_repository.list_worlds() if w.name == session.world_name), "")
        )

        # All world characters except the new player character
        visible_names = [
            item.name for item in world_characters
            if item.id != new_player_id
        ]

        # If the old player character was not an NPC in visible_characters, add it back
        if old_player_id and old_player_id != new_player_id:
            old_name = next((item.name for item in world_characters if item.id == old_player_id), None)
            if old_name and old_name not in visible_names:
                visible_names.append(old_name)

        updated_player_stats = [
            *([f"当前身份：{player_character_name}"] if player_character_name else []),
            f"当前场景可见人物：{len(visible_names)}",
        ]

        updated_session = SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            current_speaker=session.current_speaker,
            current_line=session.current_line,
            player_character_id=player_character_id,
            player_character_name=player_character_name,
            visible_characters=visible_names or session.visible_characters,
            messages=session.messages,
            player_stats=updated_player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=[
                f"身份变更：{player_character_name or '无绑定角色'}",
                *session.system_log,
            ][:6],
            scene=session.scene,
            assets=session.assets,
            state=session.state,
        )

        with self._store.connect() as connection:
            self._store.upsert_session(connection, updated_session)
        self._publish_snapshot(updated_session)
        return updated_session

    def branch_save(self, save_id: str, branch_label: str | None = None) -> SaveSummary | None:
        source_save = next((item for item in self.list_saves() if item.id == save_id), None)
        if source_save is None:
            return None

        source_session = self.get_session(source_save.session_id)
        if source_session is None:
            return None

        branched_session_id = f"session-{uuid.uuid4().hex[:8]}"
        branched_save_id = f"save-{uuid.uuid4().hex[:8]}"
        resolved_branch_label = branch_label.strip() if branch_label and branch_label.strip() else "新分支"
        branch_title = f"{source_save.title} / {resolved_branch_label}"

        branched_session = SessionSnapshot(
            id=branched_session_id,
            world_name=source_session.world_name,
            location=source_session.location,
            time_label=source_session.time_label,
            current_speaker=source_session.current_speaker,
            current_line=source_session.current_line,
            player_character_id=source_session.player_character_id,
            player_character_name=source_session.player_character_name,
            visible_characters=list(source_session.visible_characters),
            messages=list(source_session.messages),
            player_stats=list(source_session.player_stats),
            map_graph_nodes=list(source_session.map_graph_nodes),
            map_graph_edges=list(source_session.map_graph_edges),
            inventory_items=list(source_session.inventory_items),
            system_log=list(source_session.system_log),
            scene=source_session.scene,
            assets=source_session.assets,
            state=source_session.state,
        )

        branched_save = SaveSummary(
            id=branched_save_id,
            session_id=branched_session_id,
            title=branch_title,
            world_name=source_save.world_name,
            updated_at=datetime.now().strftime("%Y-%m-%d %H:%M"),
            progress=source_save.progress,
            summary=source_save.summary,
            player_character_name=source_save.player_character_name,
            parent_save_id=source_save.id,
            branch_root_save_id=source_save.branch_root_save_id or source_save.id,
            branch_label=resolved_branch_label,
        )

        with self._store.connect() as connection:
            self._store.upsert_session(connection, branched_session)
            self._copy_branch_memories(connection, source_session_id=source_session.id, branched_session_id=branched_session_id)
            self._copy_branch_attributes(connection, source_session_id=source_session.id, branched_session_id=branched_session_id)
            self._store.upsert_save(connection, branched_save)

        self._publish_snapshot(branched_session)
        return branched_save

    def subscribe(self, session_id: str) -> asyncio.Queue[SessionSnapshot]:
        queue: asyncio.Queue[SessionSnapshot] = asyncio.Queue()
        self._listeners[session_id].append(queue)
        return queue

    def unsubscribe(self, session_id: str, queue: asyncio.Queue[SessionSnapshot]) -> None:
        listeners = self._listeners.get(session_id, [])
        if queue in listeners:
            listeners.remove(queue)
        if not listeners and session_id in self._listeners:
            del self._listeners[session_id]

    def publish_transient_snapshot(self, session: SessionSnapshot) -> None:
        self._publish_snapshot(session)

    def _publish_snapshot(self, session: SessionSnapshot) -> None:
        for queue in list(self._listeners.get(session.id, [])):
            queue.put_nowait(session)

    def _delete_save_records(self, connection, *, save_id: str, session_id: str) -> None:
        connection.execute("DELETE FROM memories WHERE session_id = ?", (session_id,))
        connection.execute("DELETE FROM attribute_values WHERE owner_type = 'session' AND owner_id = ?", (session_id,))
        connection.execute(
            "DELETE FROM attribute_values WHERE owner_type = 'session_character' AND owner_id LIKE ?",
            (f"{session_id}:%",),
        )
        connection.execute("DELETE FROM sessions WHERE id = ?", (session_id,))
        connection.execute("DELETE FROM saves WHERE id = ?", (save_id,))
        self._listeners.pop(session_id, None)

    def _build_session_from_world(
        self,
        world: WorldDefinition,
        session_id: str,
        player_character_id: str | None = None,
    ) -> SessionSnapshot:
        world_characters = self._characters_for_world(world.id)
        selected_player_character = self._resolve_player_character(
            world=world,
            world_characters=world_characters,
            requested_player_character_id=player_character_id,
        )
        player_character_name = selected_player_character.name if selected_player_character is not None else None
        scene_visible_characters = self._resolve_opening_visible_characters(
            world=world,
            world_characters=world_characters,
            player_character_name=player_character_name,
        )
        scene_present_characters = self._build_scene_present_characters(
            visible_character_names=scene_visible_characters,
            player_character_name=player_character_name,
        )
        visible_characters = scene_visible_characters or (["待添加角色"] if player_character_name is None else [])
        opening_line = f"{world.name} 的新会话已经创建，当前场景是 {world.opening_scene}。"
        opening_speaker = scene_visible_characters[0] if scene_visible_characters else (player_character_name or "系统")
        initial_messages = self._build_opening_messages(world=world, player_character_name=player_character_name)
        opening_speaker, opening_line = self._resolve_opening_focus(
            messages=initial_messages,
            player_character_name=player_character_name,
        )
        initial_map_nodes, initial_map_edges = self._create_initial_map_graph(world.opening_scene)
        return self._apply_opening_session_state(
            SessionSnapshot(
            id=session_id,
            world_name=world.name,
            location=world.opening_scene,
            time_label=self._initial_time_label(world),
            current_speaker=opening_speaker,
            current_line=opening_line,
            player_character_id=selected_player_character.id if selected_player_character is not None else None,
            player_character_name=player_character_name,
            visible_characters=visible_characters,
            messages=[
                ChatMessage(role="system", content=f"已进入 {world.name}。"),
                ChatMessage(role="agent", speaker=opening_speaker, content=opening_line),
            ],
            player_stats=[
                *([f"当前身份：{player_character_name}"] if player_character_name else []),
                f"当前场景可见人物：{len(scene_visible_characters)}",
            ],
            map_graph_nodes=initial_map_nodes,
            map_graph_edges=initial_map_edges,
            inventory_items=[],
            system_log=["会话已创建", f"世界已装载：{world.name}", f"当前已发现区域：{world.opening_scene}"],
            scene=SceneRuntime(
                scene_id="opening-scene",
                name=world.opening_scene,
                background_hint="default:opening",
                temporary_tags=["scene-entered", "phase:opening"],
                present_characters=scene_present_characters,
            ),
            assets=AssetSelection(
                background_hint="default:opening",
                active_speaker_portrait=f"{opening_speaker}:clear:idle:speaking",
                visible_character_portraits=[
                    CharacterVisualState(
                        character_name=name,
                        portrait_hint=f"{name}:clear:idle:idle",
                    )
                    for name in scene_present_characters
                ],
            ),
            state=SessionState(
                metrics={"pressure": 10.0, "focus": 50.0, "stability": 100.0},
                tags=[],
                phase="idle",
            ),
            ),
            messages=initial_messages,
            opening_speaker=opening_speaker,
            opening_line=opening_line,
        )

    def _build_opening_messages(
        self,
        *,
        world: WorldDefinition,
        player_character_name: str | None,
    ) -> list[ChatMessage]:
        if world.opening_messages:
            messages = [
                ChatMessage(
                    role="system" if item.role == "system" else "agent",
                    content=item.content,
                    speaker=item.speaker,
                )
                for item in world.opening_messages
                if item.content.strip()
                and not (
                    player_character_name
                    and item.role != "system"
                    and str(item.speaker or "").strip() == player_character_name
                )
            ]
            if messages:
                return messages

        opening_scene = world.opening_scene.strip() or "未知地点"
        return [
            ChatMessage(
                role="system",
                content=f"已进入 {world.name}。当前场景：{opening_scene}。",
            )
        ]

    def _resolve_opening_visible_characters(
        self,
        *,
        world: WorldDefinition,
        world_characters: list[CharacterDefinition],
        player_character_name: str | None,
    ) -> list[str]:
        character_by_id = {item.id: item for item in world_characters}
        opening_visible: list[str] = []

        for character_id in world.opening_character_ids:
            character = character_by_id.get(character_id)
            if character is None:
                continue
            name = character.name.strip()
            if not name or name == player_character_name or name in opening_visible:
                continue
            opening_visible.append(name)

        known_names = {item.name.strip() for item in world_characters if item.name.strip()}
        for message in world.opening_messages:
            if message.role == "system":
                continue
            speaker = str(message.speaker or "").strip()
            if not speaker or speaker == player_character_name or speaker not in known_names:
                continue
            if speaker not in opening_visible:
                opening_visible.append(speaker)

        return opening_visible

    def _apply_opening_session_state(
        self,
        session: SessionSnapshot,
        *,
        messages: list[ChatMessage],
        opening_speaker: str,
        opening_line: str,
    ) -> SessionSnapshot:
        return SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            current_speaker=opening_speaker,
            current_line=opening_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=session.visible_characters,
            messages=messages,
            player_stats=session.player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=session.system_log,
            scene=session.scene,
            assets=session.assets,
            state=session.state,
        )

    def _resolve_opening_focus(
        self,
        *,
        messages: list[ChatMessage],
        player_character_name: str | None,
    ) -> tuple[str, str]:
        if not messages:
            return "系统", "新会话已创建。"

        last_message = messages[-1]
        if last_message.role == "agent" and last_message.speaker:
            return last_message.speaker, last_message.content
        return last_message.speaker or "系统", last_message.content

    def _build_save_summary(self, session: SessionSnapshot) -> SaveSummary:
        existing = next((item for item in self.list_saves() if item.session_id == session.id), None)
        save_id = existing.id if existing else f"save-{uuid.uuid4().hex[:8]}"
        return SaveSummary(
            id=save_id,
            session_id=session.id,
            title=f"{session.world_name} / {session.location}",
            world_name=session.world_name,
            updated_at=datetime.now().strftime("%Y-%m-%d %H:%M"),
            progress=f"共 {len(session.messages)} 条消息",
            summary=session.current_line[:40],
            player_character_name=session.player_character_name,
        )

    def _resolve_player_character(
        self,
        *,
        world: WorldDefinition,
        world_characters: list[CharacterDefinition],
        requested_player_character_id: str | None,
    ) -> CharacterDefinition | None:
        selected_id = requested_player_character_id or world.player_character_id
        if not selected_id:
            return None
        return next((item for item in world_characters if item.id == selected_id), None)

    def _resolved_player_speaker(self, player_character_name: str | None) -> str:
        return str(player_character_name or "").strip() or "玩家"

    def _build_scene_present_characters(
        self,
        *,
        visible_character_names: list[str],
        player_character_name: str | None,
    ) -> list[str]:
        names = [name.strip() for name in visible_character_names if name.strip()]
        if player_character_name and player_character_name.strip():
            names.append(player_character_name.strip())
        return list(dict.fromkeys(names))

    def _create_initial_map_graph(self, opening_scene: str) -> tuple[list[SessionMapNode], list[SessionMapEdge]]:
        if not opening_scene:
            return [], []
        return (
            [SessionMapNode(node_id=self._map_node_id(opening_scene), label=opening_scene, discovered=True, current=True)],
            [],
        )

    def _expand_map_graph(
        self,
        *,
        existing_nodes: list[SessionMapNode],
        existing_edges: list[SessionMapEdge],
        previous_location: str,
        next_location: str,
    ) -> tuple[list[SessionMapNode], list[SessionMapEdge]]:
        if not next_location:
            return existing_nodes, existing_edges

        nodes_by_id = {
            item.node_id: SessionMapNode(
                node_id=item.node_id,
                label=item.label,
                discovered=item.discovered,
                current=False,
            )
            for item in existing_nodes
        }

        next_node_id = self._map_node_id(next_location)
        if next_node_id not in nodes_by_id:
            nodes_by_id[next_node_id] = SessionMapNode(
                node_id=next_node_id,
                label=next_location,
                discovered=True,
                current=False,
            )

        ordered_nodes = [
            SessionMapNode(
                node_id=item.node_id,
                label=item.label,
                discovered=item.discovered,
                current=item.node_id == next_node_id,
            )
            for item in nodes_by_id.values()
        ]

        edge_map = {item.edge_id: item for item in existing_edges}
        previous_node_id = self._map_node_id(previous_location) if previous_location else ""
        if previous_node_id and previous_node_id != next_node_id:
            edge_id = self._map_edge_id(previous_node_id, next_node_id)
            reverse_edge_id = self._map_edge_id(next_node_id, previous_node_id)
            if edge_id not in edge_map and reverse_edge_id not in edge_map:
                edge_map[edge_id] = SessionMapEdge(
                    edge_id=edge_id,
                    source_node_id=previous_node_id,
                    target_node_id=next_node_id,
                )

        return ordered_nodes, list(edge_map.values())

    def _map_node_id(self, label: str) -> str:
        slug = "".join(char.lower() if char.isalnum() else "-" for char in label).strip("-")
        return f"node-{slug or 'unknown'}"

    def _map_edge_id(self, source_node_id: str, target_node_id: str) -> str:
        return f"edge-{source_node_id}-{target_node_id}"

    def _copy_branch_memories(self, connection, *, source_session_id: str, branched_session_id: str) -> None:
        rows = connection.execute("SELECT * FROM memories WHERE session_id = ?", (source_session_id,)).fetchall()
        for row in rows:
            connection.execute(
                """
                INSERT INTO memories (
                    id, world_id, session_id, conversation_id, character_id, event_id, item_id, scene_id, layer, content, source, importance, created_at,
                    memory_type, speaker, role, location, participants_json, keywords_json
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    f"memory-{uuid.uuid4().hex[:8]}",
                    row["world_id"],
                    branched_session_id,
                    branched_session_id,
                    row["character_id"],
                    row["event_id"],
                    row["item_id"],
                    row["scene_id"],
                    row["layer"],
                    row["content"],
                    row["source"],
                    row["importance"],
                    row["created_at"],
                    row["memory_type"],
                    row["speaker"],
                    row["role"],
                    row["location"],
                    row["participants_json"],
                    row["keywords_json"],
                ),
            )

    def _copy_branch_attributes(self, connection, *, source_session_id: str, branched_session_id: str) -> None:
        rows = connection.execute(
            """
            SELECT * FROM attribute_values
            WHERE (owner_type = 'session' AND owner_id = ?)
               OR (owner_type = 'session_character' AND owner_id LIKE ?)
            """,
            (source_session_id, f"{source_session_id}:%"),
        ).fetchall()
        for row in rows:
            owner_type = row["owner_type"]
            owner_id = row["owner_id"]
            if owner_type == "session":
                next_owner_id = branched_session_id
            else:
                _, character_id = owner_id.split(":", 1)
                next_owner_id = f"{branched_session_id}:{character_id}"
            connection.execute(
                """
                INSERT INTO attribute_values (id, schema_id, owner_type, owner_id, value_json, source)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (
                    f"attrval-{uuid.uuid4().hex[:8]}",
                    row["schema_id"],
                    owner_type,
                    next_owner_id,
                    row["value_json"],
                    row["source"],
                ),
            )

    def _characters_for_world(self, world_id: str) -> list[CharacterDefinition]:
        return [item for item in self._catalog_repository.list_characters() if item.world_id == world_id]

    def _world_by_name(self, world_name: str) -> WorldDefinition | None:
        return next((item for item in self._catalog_repository.list_worlds() if item.name == world_name), None)

    def _initial_time_label(self, world: WorldDefinition) -> str:
        config = self._normalize_time_config(world.time_config)
        if config["mode"] == "24h":
            return str(config["start_time"])
        return str(config["start_label"])

    def _normalize_time_config(self, raw: dict[str, Any] | None) -> dict[str, Any]:
        base = {
            "mode": "labels",
            "labels": ["清晨", "中午", "晚上"],
            "slots": [
                {"label": "清晨", "clock": "06:00"},
                {"label": "中午", "clock": "12:00"},
                {"label": "晚上", "clock": "20:00"},
            ],
            "start_label": "清晨",
            "start_time": "08:00",
        }
        if not isinstance(raw, dict):
            return base

        slot_labels: list[str] = []
        slots = raw.get("slots")
        if isinstance(slots, list):
            normalized_slots: list[dict[str, str]] = []
            for item in slots:
                if not isinstance(item, dict):
                    continue
                label = str(item.get("label", "")).strip()
                clock = str(item.get("clock", "")).strip()
                if not label and not clock:
                    continue
                normalized_slots.append({"label": label, "clock": clock})

            if normalized_slots:
                base["slots"] = normalized_slots
                slot_labels = [item["label"] for item in normalized_slots if item["label"]]
                if slot_labels:
                    base["labels"] = slot_labels
                    base["start_label"] = slot_labels[0]

        labels = raw.get("labels")
        if isinstance(labels, list) and not slot_labels:
            normalized_labels = [str(item).strip() for item in labels if str(item).strip()]
            if normalized_labels:
                base["labels"] = normalized_labels
                base["slots"] = [{"label": item, "clock": ""} for item in normalized_labels]
                base["start_label"] = normalized_labels[0]

        if raw.get("mode") == "24h":
            base["mode"] = "24h"

        start_label = raw.get("start_label")
        if isinstance(start_label, str) and start_label.strip():
            base["start_label"] = start_label.strip()

        start_time = raw.get("start_time")
        if isinstance(start_time, str) and self._parse_clock_minutes(start_time) is not None:
            base["start_time"] = start_time

        return base

    def _parse_clock_minutes(self, value: str) -> int | None:
        parts = value.split(":", 1)
        if len(parts) != 2:
            return None
        try:
            hour = int(parts[0])
            minute = int(parts[1])
        except ValueError:
            return None
        if hour < 0 or hour > 23 or minute < 0 or minute > 59:
            return None
        return hour * 60 + minute

    def _attribute_log_lines(self, values: list[AttributeValue]) -> list[str]:
        if not values:
            return []

        schema_map = {schema.id: schema for schema in self._attribute_repository.list_schemas()}
        lines = []
        for value in values:
            schema = schema_map.get(value.schema_id)
            if schema is None:
                continue
            lines.append(f"属性更新：{schema.label} -> {value.value}")
        return lines

    def _with_attribute_log(
        self,
        session: SessionSnapshot,
        projected: list[AttributeValue],
        prefix: str,
    ) -> SessionSnapshot:
        lines = self._attribute_log_lines(projected)
        return SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            current_speaker=session.current_speaker,
            current_line=session.current_line,
            player_character_id=session.player_character_id,
            player_character_name=session.player_character_name,
            visible_characters=session.visible_characters,
            messages=session.messages,
            player_stats=session.player_stats,
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=[prefix, *lines, *session.system_log][:8],
            scene=session.scene,
            assets=session.assets,
            state=session.state,
        )

    def update_player_character(
        self,
        session_id: str,
        player_character_id: str | None,
        player_character_name: str | None,
        location_override: str | None = None,
        visible_characters_override: list[str] | None = None,
        scene_override: SceneRuntime | None = None,
        assets_override: AssetSelection | None = None,
        current_speaker: str | None = None,
        current_line: str | None = None,
        system_messages: list[str] | None = None,
    ) -> SessionSnapshot | None:
        session = self.get_session(session_id)
        if session is None:
            return None

        old_player_id = session.player_character_id
        world_characters = self._characters_for_world(
            next((w.id for w in self._catalog_repository.list_worlds() if w.name == session.world_name), "")
        )
        next_location = location_override or session.location

        if visible_characters_override is not None:
            visible_names = [
                name
                for name in (str(item).strip() for item in visible_characters_override)
                if name and name != player_character_name
            ]
        else:
            visible_names = [
                name
                for name in (str(item).strip() for item in session.visible_characters)
                if name and name != player_character_name
            ]
            if old_player_id and old_player_id != player_character_id:
                old_name = next((item.name for item in world_characters if item.id == old_player_id), None)
                if old_name and old_name != player_character_name and old_name not in visible_names:
                    visible_names.append(old_name)
        visible_names = list(dict.fromkeys(visible_names))

        next_map_graph_nodes, next_map_graph_edges = self._expand_map_graph(
            existing_nodes=session.map_graph_nodes,
            existing_edges=session.map_graph_edges,
            previous_location=session.location,
            next_location=next_location,
        )
        next_scene = scene_override or session.scene
        next_assets = assets_override or session.assets
        appended_system_messages = [
            ChatMessage(role="system", content=content.strip())
            for content in (system_messages or [])
            if content.strip()
        ]

        updated_player_stats = [
            *([f"当前身份：{player_character_name}"] if player_character_name else []),
            f"当前场景可见人物：{len(visible_names)}",
        ]

        updated_session = SessionSnapshot(
            id=session.id,
            world_name=session.world_name,
            location=next_location,
            time_label=session.time_label,
            current_speaker=current_speaker or session.current_speaker,
            current_line=current_line or session.current_line,
            player_character_id=player_character_id,
            player_character_name=player_character_name,
            visible_characters=visible_names,
            messages=[*session.messages, *appended_system_messages],
            player_stats=updated_player_stats,
            map_graph_nodes=next_map_graph_nodes,
            map_graph_edges=next_map_graph_edges,
            inventory_items=session.inventory_items,
            system_log=[
                f"身份变更：{player_character_name or '无绑定角色'}",
                *[message.content for message in appended_system_messages],
                *session.system_log,
            ][:6],
            scene=next_scene,
            assets=next_assets,
            state=session.state,
        )

        save = self._build_save_summary(session=updated_session)
        with self._store.connect() as connection:
            self._store.upsert_session(connection, updated_session)
            self._store.upsert_save(connection, save)
        self._publish_snapshot(updated_session)
        return updated_session
