from backend.app.domain.models.agent_runtime import AgentCheckpoint, AgentSession, TurnJournalEntry
from backend.app.infrastructure.sqlite_store import (
    SqliteStore,
    row_to_agent_checkpoint,
    row_to_agent_session,
    row_to_turn_journal_entry,
)


class SqliteAgentRuntimeRepository:
    def __init__(self, store: SqliteStore) -> None:
        self._store = store

    def list_agent_sessions(self, session_id: str) -> list[AgentSession]:
        with self._store.connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM agent_sessions
                WHERE session_id = ?
                ORDER BY created_at ASC, id ASC
                """,
                (session_id,),
            ).fetchall()
        return [row_to_agent_session(row) for row in rows]

    def get_agent_session(self, agent_session_id: str) -> AgentSession | None:
        with self._store.connect() as connection:
            row = connection.execute(
                "SELECT * FROM agent_sessions WHERE id = ?",
                (agent_session_id,),
            ).fetchone()
        return row_to_agent_session(row) if row else None

    def get_agent_session_by_runtime_key(self, session_id: str, runtime_key: str) -> AgentSession | None:
        with self._store.connect() as connection:
            row = connection.execute(
                """
                SELECT * FROM agent_sessions
                WHERE session_id = ? AND runtime_key = ?
                """,
                (session_id, runtime_key),
            ).fetchone()
        return row_to_agent_session(row) if row else None

    def upsert_agent_session(self, session: AgentSession) -> AgentSession:
        with self._store.connect() as connection:
            self._store.upsert_agent_session(connection, session)
        return session

    def delete_agent_session(self, agent_session_id: str) -> None:
        with self._store.connect() as connection:
            connection.execute("DELETE FROM agent_sessions WHERE id = ?", (agent_session_id,))

    def list_checkpoints(self, agent_session_id: str, limit: int = 10) -> list[AgentCheckpoint]:
        with self._store.connect() as connection:
            rows = connection.execute(
                """
                SELECT * FROM agent_checkpoints
                WHERE agent_session_id = ?
                ORDER BY created_at DESC, id DESC
                LIMIT ?
                """,
                (agent_session_id, limit),
            ).fetchall()
        return [row_to_agent_checkpoint(row) for row in rows]

    def get_latest_checkpoint(self, agent_session_id: str) -> AgentCheckpoint | None:
        checkpoints = self.list_checkpoints(agent_session_id=agent_session_id, limit=1)
        return checkpoints[0] if checkpoints else None

    def append_checkpoint(self, checkpoint: AgentCheckpoint) -> AgentCheckpoint:
        with self._store.connect() as connection:
            self._store.insert_agent_checkpoint(connection, checkpoint)
        return checkpoint

    def list_turn_journal(self, session_id: str, turn_index: int | None = None) -> list[TurnJournalEntry]:
        with self._store.connect() as connection:
            if turn_index is None:
                rows = connection.execute(
                    """
                    SELECT * FROM turn_journal
                    WHERE session_id = ?
                    ORDER BY turn_index DESC, created_at DESC, id DESC
                    """,
                    (session_id,),
                ).fetchall()
            else:
                rows = connection.execute(
                    """
                    SELECT * FROM turn_journal
                    WHERE session_id = ? AND turn_index = ?
                    ORDER BY created_at ASC, id ASC
                    """,
                    (session_id, turn_index),
                ).fetchall()
        return [row_to_turn_journal_entry(row) for row in rows]

    def get_latest_turn_index(self, session_id: str) -> int:
        with self._store.connect() as connection:
            row = connection.execute(
                "SELECT MAX(turn_index) AS turn_index FROM turn_journal WHERE session_id = ?",
                (session_id,),
            ).fetchone()
        return int(row["turn_index"] or 0) if row else 0

    def append_turn_journal(self, entry: TurnJournalEntry) -> TurnJournalEntry:
        with self._store.connect() as connection:
            self._store.insert_turn_journal_entry(connection, entry)
        return entry
