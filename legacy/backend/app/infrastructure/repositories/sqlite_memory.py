from backend.app.domain.models.memory import MemoryEntry
from backend.app.infrastructure.sqlite_store import SqliteStore, row_to_memory


class SqliteMemoryRepository:
    def __init__(self, store: SqliteStore) -> None:
        self._store = store

    def list_for_character(
        self,
        world_id: str,
        character_id: str,
        session_id: str | None = None,
        conversation_id: str | None = None,
        scene_id: str | None = None,
        event_id: str | None = None,
        item_id: str | None = None,
        layers: list[str] | None = None,
        importance_min: float | None = None,
        importance_max: float | None = None,
        memory_types: list[str] | None = None,
        limit: int = 8,
    ) -> list[MemoryEntry]:
        with self._store.connect() as connection:
            query = """
                SELECT * FROM memories
                WHERE world_id = ? AND character_id = ?
            """
            params: list[object] = [world_id, character_id]
            if session_id:
                query += " AND session_id = ?"
                params.append(session_id)
            if conversation_id:
                query += " AND conversation_id = ?"
                params.append(conversation_id)
            if scene_id:
                query += " AND scene_id = ?"
                params.append(scene_id)
            if event_id:
                query += " AND event_id = ?"
                params.append(event_id)
            if item_id:
                query += " AND item_id = ?"
                params.append(item_id)
            if layers:
                placeholders = ", ".join("?" for _ in layers)
                query += f" AND layer IN ({placeholders})"
                params.extend(layers)
            if importance_min is not None:
                query += " AND importance >= ?"
                params.append(importance_min)
            if importance_max is not None:
                query += " AND importance <= ?"
                params.append(importance_max)
            if memory_types:
                placeholders = ", ".join("?" for _ in memory_types)
                query += f" AND memory_type IN ({placeholders})"
                params.extend(memory_types)
            query += " ORDER BY created_at DESC, id DESC LIMIT ?"
            params.append(limit)
            rows = connection.execute(query, tuple(params)).fetchall()
        return [row_to_memory(row) for row in rows]

    def append_entries(self, entries: list[MemoryEntry]) -> list[MemoryEntry]:
        if not entries:
            return []

        with self._store.connect() as connection:
            for entry in entries:
                self._store.insert_memory(connection, entry)
        return entries
