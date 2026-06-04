from __future__ import annotations

from dataclasses import replace
from datetime import datetime
import uuid

from backend.app.domain.models.agent_runtime import (
    AgentCheckpoint,
    AgentSession,
    AgentSessionStatus,
    CheckpointType,
    ConnectionState,
    TurnJournalEntry,
)
from backend.app.domain.repositories.agent_runtime import AgentRuntimeRepository


class AgentRuntimeManagerService:
    def __init__(self, runtime_repository: AgentRuntimeRepository) -> None:
        self._runtime_repository = runtime_repository

    def list_agent_sessions(self, session_id: str) -> list[AgentSession]:
        return self._runtime_repository.list_agent_sessions(session_id)

    def list_active_agent_sessions(self, session_id: str) -> list[AgentSession]:
        return [
            agent
            for agent in self._runtime_repository.list_agent_sessions(session_id)
            if agent.status in {AgentSessionStatus.ACTIVE, AgentSessionStatus.PENDING_INIT, AgentSessionStatus.INACTIVE}
        ]

    def get_or_create_director_agent(
        self,
        *,
        session_id: str,
        prompt_version: str = "v1",
    ) -> AgentSession:
        runtime_key = "director"
        existing = self._runtime_repository.get_agent_session_by_runtime_key(session_id, runtime_key)
        if existing is not None:
            return self._maybe_reuse_or_reset(existing, prompt_version=prompt_version)

        now = self._now()
        session = AgentSession(
            id=self._new_id("agent"),
            session_id=session_id,
            agent_type="director",
            character_id=None,
            character_name=None,
            status="pending_init",
            connection_state="disconnected",
            scene_presence_state="present",
            checkpoint_id=None,
            last_active_turn=0,
            last_ack_message_index=0,
            prompt_version=prompt_version,
            runtime_key=runtime_key,
            initialized_at=None,
            created_at=now,
            updated_at=now,
        )
        return self._runtime_repository.upsert_agent_session(session)

    def get_or_create_character_agent(
        self,
        *,
        session_id: str,
        character_id: str,
        character_name: str,
        scene_presence_state: str = "present",
        prompt_version: str = "v1",
    ) -> AgentSession:
        runtime_key = f"character:{character_id}"
        existing = self._runtime_repository.get_agent_session_by_runtime_key(session_id, runtime_key)
        if existing is not None:
            if existing.status in {AgentSessionStatus.CLOSED, AgentSessionStatus.FAILED, AgentSessionStatus.EVICTED}:
                return self._maybe_reuse_or_reset(existing, prompt_version=prompt_version)
            if existing.character_name == character_name and existing.scene_presence_state == scene_presence_state:
                return existing
            return self._runtime_repository.upsert_agent_session(
                replace(
                    existing,
                    character_name=character_name,
                    scene_presence_state=scene_presence_state,
                    updated_at=self._now(),
                )
            )

        now = self._now()
        session = AgentSession(
            id=self._new_id("agent"),
            session_id=session_id,
            agent_type="character",
            character_id=character_id,
            character_name=character_name,
            status="pending_init",
            connection_state="disconnected",
            scene_presence_state=scene_presence_state,
            checkpoint_id=None,
            last_active_turn=0,
            last_ack_message_index=0,
            prompt_version=prompt_version,
            runtime_key=runtime_key,
            initialized_at=None,
            created_at=now,
            updated_at=now,
        )
        return self._runtime_repository.upsert_agent_session(session)

    def close_agent_session(self, agent_session_id: str) -> AgentSession | None:
        """Close an agent: mark as closed and disconnect, but keep metadata."""
        session = self._runtime_repository.get_agent_session(agent_session_id)
        if session is None:
            return None
        return self._runtime_repository.upsert_agent_session(
            replace(
                session,
                status=AgentSessionStatus.CLOSED,
                connection_state=ConnectionState.DISCONNECTED,
                updated_at=self._now(),
            )
        )

    def recover_agent_from_checkpoint(
        self,
        agent_session_id: str,
    ) -> AgentSession | None:
        """Recover an agent from its latest checkpoint: create recovery checkpoint and reset to active."""
        session = self._runtime_repository.get_agent_session(agent_session_id)
        if session is None:
            return None

        latest_checkpoint = self._runtime_repository.get_latest_checkpoint(agent_session_id)
        if latest_checkpoint is None:
            return None

        # Create a recovery checkpoint from the latest state
        recovery_checkpoint = AgentCheckpoint(
            id=self._new_id("checkpoint"),
            agent_session_id=agent_session_id,
            turn_index=latest_checkpoint.turn_index,
            checkpoint_type=CheckpointType.RECOVERY,
            payload=latest_checkpoint.payload,
            created_at=self._now(),
        )
        stored = self._runtime_repository.append_checkpoint(recovery_checkpoint)

        return self._runtime_repository.upsert_agent_session(
            replace(
                session,
                status=AgentSessionStatus.ACTIVE,
                connection_state=ConnectionState.CONNECTED,
                checkpoint_id=stored.id,
                updated_at=self._now(),
            )
        )

    def recover_all_agents_for_session(self, session_id: str) -> list[AgentSession]:
        """Recover all non-closed agents for a session after system restart."""
        recovered: list[AgentSession] = []
        for agent in self._runtime_repository.list_agent_sessions(session_id):
            if agent.status == AgentSessionStatus.CLOSED:
                continue
            # If agent was connected or active, reset to recovering state
            if agent.connection_state == ConnectionState.CONNECTED or agent.status in {
                AgentSessionStatus.ACTIVE,
                AgentSessionStatus.PENDING_INIT,
            }:
                latest_checkpoint = self._runtime_repository.get_latest_checkpoint(agent.id)
                recovered_session = self._runtime_repository.upsert_agent_session(
                    replace(
                        agent,
                        status=AgentSessionStatus.PENDING_INIT,
                        connection_state=ConnectionState.RECOVERING if latest_checkpoint is not None else ConnectionState.DISCONNECTED,
                        updated_at=self._now(),
                    )
                )
                recovered.append(recovered_session)
        return recovered

    def update_agent_state(
        self,
        *,
        agent_session_id: str,
        status: str | None = None,
        connection_state: str | None = None,
        scene_presence_state: str | None = None,
        checkpoint_id: str | None = None,
        last_active_turn: int | None = None,
        last_ack_message_index: int | None = None,
        initialized_at: str | None = ...,
    ) -> AgentSession | None:
        session = self._runtime_repository.get_agent_session(agent_session_id)
        if session is None:
            return None

        updated = replace(
            session,
            status=status if status is not None else session.status,
            connection_state=connection_state if connection_state is not None else session.connection_state,
            scene_presence_state=scene_presence_state
            if scene_presence_state is not None
            else session.scene_presence_state,
            checkpoint_id=checkpoint_id if checkpoint_id is not None else session.checkpoint_id,
            last_active_turn=last_active_turn if last_active_turn is not None else session.last_active_turn,
            last_ack_message_index=(
                last_ack_message_index
                if last_ack_message_index is not None
                else session.last_ack_message_index
            ),
            initialized_at=initialized_at if initialized_at is not ... else session.initialized_at,
            updated_at=self._now(),
        )
        return self._runtime_repository.upsert_agent_session(updated)

    def sync_scene_presence(
        self,
        *,
        session_id: str,
        active_character_ids: set[str],
    ) -> list[AgentSession]:
        updated_sessions: list[AgentSession] = []
        for agent_session in self._runtime_repository.list_agent_sessions(session_id):
            if agent_session.agent_type != "character" or not agent_session.character_id:
                continue
            next_presence = "present" if agent_session.character_id in active_character_ids else "offstage"
            next_status = agent_session.status
            if next_presence == "offstage" and agent_session.status == AgentSessionStatus.ACTIVE:
                # Deactivate but don't close - keep metadata for potential return
                next_status = AgentSessionStatus.INACTIVE
            updated_sessions.append(
                self._runtime_repository.upsert_agent_session(
                    replace(
                        agent_session,
                        scene_presence_state=next_presence,
                        status=next_status,
                        connection_state=ConnectionState.DISCONNECTED if next_presence == "offstage" else agent_session.connection_state,
                        updated_at=self._now(),
                    )
                )
            )
        return updated_sessions

    def append_checkpoint(
        self,
        *,
        agent_session_id: str,
        turn_index: int,
        checkpoint_type: str,
        payload: dict[str, object],
    ) -> AgentCheckpoint:
        checkpoint = AgentCheckpoint(
            id=self._new_id("checkpoint"),
            agent_session_id=agent_session_id,
            turn_index=turn_index,
            checkpoint_type=checkpoint_type,
            payload=payload,
            created_at=self._now(),
        )
        stored = self._runtime_repository.append_checkpoint(checkpoint)
        self.update_agent_state(agent_session_id=agent_session_id, checkpoint_id=stored.id)
        return stored

    def get_latest_checkpoint(self, agent_session_id: str) -> AgentCheckpoint | None:
        return self._runtime_repository.get_latest_checkpoint(agent_session_id)

    def list_checkpoints(self, agent_session_id: str, limit: int = 10) -> list[AgentCheckpoint]:
        return self._runtime_repository.list_checkpoints(agent_session_id, limit=limit)

    def append_turn_journal(
        self,
        *,
        session_id: str,
        turn_index: int,
        step: str,
        status: str,
        payload: dict[str, object] | None = None,
    ) -> TurnJournalEntry:
        entry = TurnJournalEntry(
            id=self._new_id("turn"),
            session_id=session_id,
            turn_index=turn_index,
            step=step,
            status=status,
            payload=payload or {},
            created_at=self._now(),
        )
        return self._runtime_repository.append_turn_journal(entry)

    def list_turn_journal(
        self,
        *,
        session_id: str,
        turn_index: int | None = None,
    ) -> list[TurnJournalEntry]:
        return self._runtime_repository.list_turn_journal(session_id=session_id, turn_index=turn_index)

    def next_turn_index(self, session_id: str) -> int:
        return self._runtime_repository.get_latest_turn_index(session_id) + 1

    def get_latest_turn_index(self, session_id: str) -> int:
        return self._runtime_repository.get_latest_turn_index(session_id)

    def _maybe_reuse_or_reset(
        self,
        existing: AgentSession,
        *,
        prompt_version: str = "v1",
    ) -> AgentSession:
        """Reset a failed/evicted/closed agent back to pending_init for reuse."""
        if existing.status not in {AgentSessionStatus.FAILED, AgentSessionStatus.EVICTED, AgentSessionStatus.CLOSED}:
            return existing
        return self._runtime_repository.upsert_agent_session(
            replace(
                existing,
                status=AgentSessionStatus.PENDING_INIT,
                connection_state=ConnectionState.DISCONNECTED,
                prompt_version=prompt_version,
                checkpoint_id=None,
                last_active_turn=0,
                last_ack_message_index=0,
                initialized_at=None,
                updated_at=self._now(),
            )
        )

    def _new_id(self, prefix: str) -> str:
        return f"{prefix}-{uuid.uuid4().hex[:12]}"

    def _now(self) -> str:
        return datetime.now().strftime("%Y-%m-%d %H:%M:%S")
