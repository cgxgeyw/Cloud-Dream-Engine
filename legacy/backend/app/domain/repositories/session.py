from typing import Protocol

from backend.app.domain.models.asset import AssetSelection
from backend.app.domain.models.attribute import AttributeValue
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.save import SaveSummary
from backend.app.domain.models.session import ChatMessage, SessionSnapshot
from backend.app.domain.models.state import SessionState


class SessionRepository(Protocol):
    def list_saves(self) -> list[SaveSummary]: ...

    def get_session(self, session_id: str) -> SessionSnapshot | None: ...

    def create_session(self, world_id: str, player_character_id: str | None = None) -> SessionSnapshot: ...

    def branch_save(self, save_id: str, branch_label: str | None = None) -> SaveSummary | None: ...

    def delete_save(self, save_id: str) -> bool: ...

    def delete_all_saves(self) -> int: ...

    def update_session_assets(
        self,
        session_id: str,
        assets: AssetSelection,
    ) -> SessionSnapshot | None: ...

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
    ) -> SessionSnapshot | None: ...

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
    ) -> SessionSnapshot | None: ...

    def rollback_to_turn_snapshot(
        self,
        *,
        session: SessionSnapshot,
        runtime_attribute_values: list[AttributeValue],
        from_turn_index: int,
        delete_character_ids: list[str] | None = None,
    ) -> SessionSnapshot: ...

    def publish_transient_snapshot(self, session: SessionSnapshot) -> None: ...
