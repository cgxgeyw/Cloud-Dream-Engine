from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.runtime_context_models import (
    ContextAttributeRecord,
    ContextInventoryRecord,
    SceneState,
)
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.state import SessionState


class RuntimeVisibilityService:
    PLAYER_OWNER_TYPE = "player"
    PLAYER_OWNER_ID = "player"

    def build_player_session_view(self, session: SessionSnapshot) -> SessionSnapshot:
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
            inventory_items=self.filter_inventory_for_player(session.inventory_items),
            system_log=session.system_log,
            scene=session.scene,
            assets=session.assets,
            state=session.state,
        )

    def build_character_session_view(
        self,
        session: SessionSnapshot,
        character_id: str | None,
        character_name: str,
    ) -> SessionSnapshot:
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
            player_stats=[],
            map_graph_nodes=session.map_graph_nodes,
            map_graph_edges=session.map_graph_edges,
            inventory_items=self.filter_inventory_for_character(
                inventory_items=session.inventory_items,
                character_id=character_id,
                character_name=character_name,
            ),
            system_log=[],
            scene=session.scene,
            assets=session.assets,
            state=SessionState(),
        )

    def build_scene_state(
        self,
        *,
        session: SessionSnapshot,
        visible_attributes: list[RuntimeAttributeItem],
    ) -> SceneState:
        public_attributes = [
            self._to_attribute_record(item=item, owner_relation="public")
            for item in visible_attributes
            if item.value.owner_type == "session"
        ]
        public_items = [
            self._to_inventory_record(item=item, knowledge_scope="public")
            for item in session.inventory_items
            if item.visibility == "public"
        ]
        discovered_locations = [
            node.label
            for node in session.map_graph_nodes
            if node.discovered and node.label.strip()
        ]
        present_characters = self._scene_present_characters(session)
        return SceneState(
            world_name=session.world_name,
            location=session.location,
            time_label=session.time_label,
            scene_name=session.scene.name,
            scene_tags=list(session.scene.temporary_tags),
            present_characters=list(present_characters),
            discovered_locations=discovered_locations,
            public_attributes=public_attributes,
            public_items=public_items,
        )

    def build_public_world_state(
        self,
        *,
        session: SessionSnapshot,
        visible_attributes: list[RuntimeAttributeItem],
    ) -> SceneState:
        return self.build_scene_state(session=session, visible_attributes=visible_attributes)

    def filter_inventory_for_player(self, inventory_items: list[InventoryItem]) -> list[InventoryItem]:
        return [
            item
            for item in inventory_items
            if self._can_view_item(
                item=item,
                viewer_type=self.PLAYER_OWNER_TYPE,
                viewer_id=self.PLAYER_OWNER_ID,
                viewer_label=self.PLAYER_OWNER_ID,
            )
        ]

    def filter_inventory_for_character(
        self,
        inventory_items: list[InventoryItem],
        character_id: str | None,
        character_name: str,
    ) -> list[InventoryItem]:
        if character_id is None:
            return [
                item
                for item in inventory_items
                if item.visibility == "public" or character_name in item.disclosed_to
            ]

        return [
            item
            for item in inventory_items
            if self._can_view_item(
                item=item,
                viewer_type="character",
                viewer_id=character_id,
                viewer_label=character_name,
            )
        ]

    def _can_view_item(
        self,
        item: InventoryItem,
        viewer_type: str,
        viewer_id: str,
        viewer_label: str,
    ) -> bool:
        if item.owner_type == viewer_type and item.owner_id == viewer_id:
            return True
        if item.visibility == "public":
            return True
        if viewer_id in item.disclosed_to or viewer_label in item.disclosed_to:
            return True
        return False

    def _to_attribute_record(
        self,
        *,
        item: RuntimeAttributeItem,
        owner_relation: str,
    ) -> ContextAttributeRecord:
        return ContextAttributeRecord(
            key=item.schema.key,
            value=item.value.value,
            owner_type=item.value.owner_type,
            owner_relation=owner_relation,
        )

    def _to_inventory_record(
        self,
        *,
        item: InventoryItem,
        knowledge_scope: str,
    ) -> ContextInventoryRecord:
        return ContextInventoryRecord(
            item_id=item.item_id,
            name=item.name,
            category=item.category,
            quantity=item.quantity,
            description=item.description,
            tags=list(item.tags),
            owner_type=item.owner_type,
            knowledge_scope=knowledge_scope,
        )

    def _scene_present_characters(self, session: SessionSnapshot) -> list[str]:
        names = [
            name.strip()
            for name in (session.scene.present_characters or session.visible_characters)
            if name and name.strip()
        ]
        if session.player_character_name and session.player_character_name.strip():
            names.append(session.player_character_name.strip())
        return list(dict.fromkeys(names))
