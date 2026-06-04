import json
import re
import uuid

from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.model_config import ModelConfig
from backend.app.domain.models.plugin import PluginDefinition
from backend.app.domain.models.settings import AppSettingsSnapshot
from backend.app.domain.models.world import WorldDefinition
from backend.app.infrastructure.sqlite_store import (
    SqliteStore,
    row_to_character,
    row_to_model_config,
    row_to_plugin,
    row_to_settings,
    row_to_world,
)


class SqliteCatalogRepository:
    def __init__(self, store: SqliteStore) -> None:
        self._store = store

    def list_worlds(self) -> list[WorldDefinition]:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT * FROM worlds ORDER BY name").fetchall()
        return [row_to_world(row) for row in rows]

    def get_world(self, world_id: str) -> WorldDefinition | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM worlds WHERE id = ?", (world_id,)).fetchone()
        return row_to_world(row) if row else None

    def create_world(self, world: WorldDefinition) -> WorldDefinition:
        created = WorldDefinition(
            id=self._normalize_id(world.id, world.name, "world"),
            name=world.name,
            genre=world.genre,
            background_prompt=world.background_prompt,
            opening_scene=world.opening_scene,
            summary=world.summary,
            time_system=world.time_system,
            map_nodes=world.map_nodes,
            triggers=world.triggers,
            custom_tabs=world.custom_tabs,
            time_config=world.time_config,
            director_config=world.director_config,
            ui_theme_config=world.ui_theme_config,
            opening_messages=world.opening_messages,
            opening_character_ids=list(dict.fromkeys(world.opening_character_ids)),
            player_character_id=world.player_character_id,
        )
        with self._store.connect() as connection:
            self._store.insert_world(connection, created)
        return created

    def update_world(self, world_id: str, world: WorldDefinition) -> WorldDefinition | None:
        if self.get_world(world_id) is None:
            return None

        updated = WorldDefinition(
            id=world_id,
            name=world.name,
            genre=world.genre,
            background_prompt=world.background_prompt,
            opening_scene=world.opening_scene,
            summary=world.summary,
            time_system=world.time_system,
            map_nodes=world.map_nodes,
            triggers=world.triggers,
            custom_tabs=world.custom_tabs,
            time_config=world.time_config,
            director_config=world.director_config,
            ui_theme_config=world.ui_theme_config,
            opening_messages=world.opening_messages,
            opening_character_ids=list(dict.fromkeys(world.opening_character_ids)),
            player_character_id=world.player_character_id,
        )
        with self._store.connect() as connection:
            connection.execute(
                """
                UPDATE worlds
                SET name = ?, genre = ?, background_prompt = ?, opening_scene = ?, summary = ?, time_system = ?, map_nodes_json = ?, triggers_json = ?, custom_tabs_json = ?, time_config_json = ?, director_config_json = ?, ui_theme_config_json = ?, opening_messages_json = ?, opening_character_ids_json = ?, player_character_id = ?
                WHERE id = ?
                """,
                (
                    updated.name,
                    updated.genre,
                    updated.background_prompt,
                    updated.opening_scene,
                    updated.summary,
                    updated.time_system,
                    json.dumps(updated.map_nodes, ensure_ascii=False),
                    json.dumps(updated.triggers, ensure_ascii=False),
                    json.dumps(updated.custom_tabs, ensure_ascii=False),
                    json.dumps(updated.time_config, ensure_ascii=False),
                    json.dumps(updated.director_config, ensure_ascii=False),
                    json.dumps(updated.ui_theme_config, ensure_ascii=False),
                    json.dumps(
                        [
                            {"role": item.role, "content": item.content, "speaker": item.speaker}
                            for item in updated.opening_messages
                        ],
                        ensure_ascii=False,
                    ),
                    json.dumps(updated.opening_character_ids, ensure_ascii=False),
                    updated.player_character_id,
                    world_id,
                ),
            )
        return updated

    def duplicate_world(self, world_id: str) -> WorldDefinition | None:
        source_world = self.get_world(world_id)
        if source_world is None:
            return None

        source_characters = [item for item in self.list_characters() if item.world_id == world_id]
        duplicated_world = WorldDefinition(
            id="new",
            name=f"{source_world.name} 副本",
            genre=source_world.genre,
            background_prompt=source_world.background_prompt,
            opening_scene=source_world.opening_scene,
            summary=source_world.summary,
            time_system=source_world.time_system,
            map_nodes=list(source_world.map_nodes),
            triggers=list(source_world.triggers),
            custom_tabs=dict(source_world.custom_tabs),
            time_config=dict(source_world.time_config),
            director_config=dict(source_world.director_config),
            ui_theme_config=dict(source_world.ui_theme_config),
            opening_messages=list(source_world.opening_messages),
            opening_character_ids=[],
            player_character_id=None,
        )
        created_world = self.create_world(duplicated_world)

        character_id_map: dict[str, str] = {}
        player_character_id = None
        for character in source_characters:
            created_character = self.create_character(
                CharacterDefinition(
                    id="new",
                    name=character.name,
                    world_id=created_world.id,
                    role=character.role,
                    background_prompt=character.background_prompt,
                    model=character.model,
                    memory_strategy=character.memory_strategy,
                    recent_dialogue_rounds=character.recent_dialogue_rounds,
                    attributes=list(character.attributes),
                    portrait_assets=list(character.portrait_assets),
                    custom_tabs=dict(character.custom_tabs),
                )
            )
            character_id_map[character.id] = created_character.id
            if source_world.player_character_id == character.id:
                player_character_id = created_character.id

        opening_character_ids = [
            character_id_map[character_id]
            for character_id in source_world.opening_character_ids
            if character_id in character_id_map
        ]

        if player_character_id is not None or opening_character_ids:
            updated = self.update_world(
                created_world.id,
                WorldDefinition(
                    id=created_world.id,
                    name=created_world.name,
                    genre=created_world.genre,
                    background_prompt=created_world.background_prompt,
                    opening_scene=created_world.opening_scene,
                    summary=created_world.summary,
                    time_system=created_world.time_system,
                    map_nodes=created_world.map_nodes,
                    triggers=created_world.triggers,
                    custom_tabs=created_world.custom_tabs,
                    time_config=created_world.time_config,
                    director_config=created_world.director_config,
                    ui_theme_config=created_world.ui_theme_config,
                    opening_messages=created_world.opening_messages,
                    opening_character_ids=opening_character_ids,
                    player_character_id=player_character_id,
                ),
            )
            return updated
        return created_world

    def list_characters(self) -> list[CharacterDefinition]:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT * FROM characters ORDER BY name").fetchall()
        return [row_to_character(row) for row in rows]

    def list_characters_for_world(self, world_id: str) -> list[CharacterDefinition]:
        with self._store.connect() as connection:
            rows = connection.execute(
                "SELECT * FROM characters WHERE world_id = ? ORDER BY name",
                (world_id,),
            ).fetchall()
        return [row_to_character(row) for row in rows]

    def get_character(self, character_id: str) -> CharacterDefinition | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM characters WHERE id = ?", (character_id,)).fetchone()
        return row_to_character(row) if row else None

    def create_character(self, character: CharacterDefinition) -> CharacterDefinition:
        created = CharacterDefinition(
            id=self._normalize_id(character.id, character.name, "character"),
            name=character.name,
            world_id=character.world_id,
            role=character.role,
            background_prompt=character.background_prompt,
            model=character.model,
            memory_strategy=character.memory_strategy,
            recent_dialogue_rounds=character.recent_dialogue_rounds,
            attributes=list(character.attributes),
            portrait_assets=list(character.portrait_assets),
            custom_tabs=dict(character.custom_tabs),
        )
        with self._store.connect() as connection:
            self._ensure_unique_character_name(
                connection=connection,
                world_id=created.world_id,
                name=created.name,
            )
            self._store.insert_character(connection, created)
        return created

    def update_character(self, character_id: str, character: CharacterDefinition) -> CharacterDefinition | None:
        if self.get_character(character_id) is None:
            return None

        updated = CharacterDefinition(
            id=character_id,
            name=character.name,
            world_id=character.world_id,
            role=character.role,
            background_prompt=character.background_prompt,
            model=character.model,
            memory_strategy=character.memory_strategy,
            recent_dialogue_rounds=character.recent_dialogue_rounds,
            attributes=list(character.attributes),
            portrait_assets=list(character.portrait_assets),
            custom_tabs=dict(character.custom_tabs),
        )
        with self._store.connect() as connection:
            self._ensure_unique_character_name(
                connection=connection,
                world_id=updated.world_id,
                name=updated.name,
                exclude_character_id=character_id,
            )
            connection.execute(
                """
                UPDATE characters
                SET name = ?, world_id = ?, role = ?, background_prompt = ?, model = ?, memory_strategy = ?, recent_dialogue_rounds = ?, attributes_json = ?, portrait_assets_json = ?, custom_tabs_json = ?
                WHERE id = ?
                """,
                (
                    updated.name,
                    updated.world_id,
                    updated.role,
                    updated.background_prompt,
                    updated.model,
                    updated.memory_strategy,
                    updated.recent_dialogue_rounds,
                    json.dumps(updated.attributes, ensure_ascii=False),
                    json.dumps(updated.portrait_assets, ensure_ascii=False),
                    json.dumps(updated.custom_tabs, ensure_ascii=False),
                    character_id,
                ),
            )
        return updated

    def _ensure_unique_character_name(
        self,
        *,
        connection,
        world_id: str,
        name: str,
        exclude_character_id: str | None = None,
    ) -> None:
        normalized_name = name.strip()
        if not normalized_name:
            return

        row = connection.execute(
            "SELECT id FROM characters WHERE world_id = ? AND lower(trim(name)) = lower(trim(?))",
            (world_id, normalized_name),
        ).fetchone()
        if row is None:
            return
        if exclude_character_id is not None and row["id"] == exclude_character_id:
            return
        raise ValueError("Character name already exists in this world")

    def delete_world(self, world_id: str) -> bool:
        if self.get_world(world_id) is None:
            return False
        with self._store.connect() as connection:
            connection.execute("DELETE FROM attribute_values WHERE owner_type = 'world' AND owner_id = ?", (world_id,))
            connection.execute(
                "DELETE FROM attribute_values WHERE owner_type = 'character' AND owner_id IN (SELECT id FROM characters WHERE world_id = ?)",
                (world_id,),
            )
            connection.execute("DELETE FROM characters WHERE world_id = ?", (world_id,))
            connection.execute("DELETE FROM worlds WHERE id = ?", (world_id,))
        return True

    def delete_all_worlds(self) -> int:
        with self._store.connect() as connection:
            row = connection.execute("SELECT COUNT(*) AS count FROM worlds").fetchone()
            deleted_count = int(row["count"]) if row is not None else 0
            if deleted_count == 0:
                return 0
            connection.execute("DELETE FROM attribute_values WHERE owner_type = 'world' AND owner_id IN (SELECT id FROM worlds)")
            connection.execute(
                "DELETE FROM attribute_values WHERE owner_type = 'character' AND owner_id IN (SELECT id FROM characters WHERE world_id IN (SELECT id FROM worlds))"
            )
            connection.execute("DELETE FROM characters WHERE world_id IN (SELECT id FROM worlds)")
            connection.execute("DELETE FROM worlds")
        return deleted_count

    def delete_character(self, character_id: str) -> bool:
        if self.get_character(character_id) is None:
            return False
        with self._store.connect() as connection:
            connection.execute("DELETE FROM attribute_values WHERE owner_type = 'character' AND owner_id = ?", (character_id,))
            connection.execute("DELETE FROM characters WHERE id = ?", (character_id,))
        return True

    def get_settings(self) -> AppSettingsSnapshot:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM settings WHERE id = 1").fetchone()
        return row_to_settings(row)

    def update_settings(self, settings: AppSettingsSnapshot) -> AppSettingsSnapshot:
        with self._store.connect() as connection:
            self._store.upsert_settings(connection, settings)
        return settings

    def list_plugins(self) -> list[PluginDefinition]:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT * FROM plugins ORDER BY name").fetchall()
        return [row_to_plugin(row) for row in rows]

    # ─── Model Config CRUD ───

    def list_models(self) -> list[ModelConfig]:
        with self._store.connect() as connection:
            rows = connection.execute("SELECT * FROM model_configs ORDER BY model_type, name").fetchall()
        return [row_to_model_config(row) for row in rows]

    def get_model(self, model_id: str) -> ModelConfig | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM model_configs WHERE id = ?", (model_id,)).fetchone()
        return row_to_model_config(row) if row else None

    def create_model(self, model: ModelConfig) -> ModelConfig:
        created = ModelConfig(
            id=self._normalize_id(model.id, model.name, "model"),
            name=model.name,
            model_type=model.model_type,
            provider=model.provider,
            model_id=model.model_id,
            base_url=model.base_url,
            api_key=model.api_key,
            is_default=model.is_default,
        )
        with self._store.connect() as connection:
            if created.is_default:
                self._store.clear_default_models(connection, created.model_type)
            self._store.insert_model_config(connection, created)
        return created

    def update_model(self, model_id: str, model: ModelConfig) -> ModelConfig | None:
        if self.get_model(model_id) is None:
            return None
        updated = ModelConfig(
            id=model_id,
            name=model.name,
            model_type=model.model_type,
            provider=model.provider,
            model_id=model.model_id,
            base_url=model.base_url,
            api_key=model.api_key,
            is_default=model.is_default,
        )
        with self._store.connect() as connection:
            if updated.is_default:
                self._store.clear_default_models(connection, updated.model_type)
            self._store.upsert_model_config(connection, updated)
        return updated

    def delete_model(self, model_id: str) -> bool:
        if self.get_model(model_id) is None:
            return False
        with self._store.connect() as connection:
            connection.execute("DELETE FROM model_configs WHERE id = ?", (model_id,))
        return True

    def set_default_model(self, model_id: str, model_type: str) -> None:
        with self._store.connect() as connection:
            self._store.clear_default_models(connection, model_type)
            connection.execute("UPDATE model_configs SET is_default = 1 WHERE id = ?", (model_id,))

    def _normalize_id(self, requested_id: str, name: str, prefix: str) -> str:
        if requested_id and requested_id != "new":
            return requested_id

        slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
        candidate = f"{prefix}-{slug}" if slug and not slug.startswith(prefix) else (slug or f"{prefix}-{uuid.uuid4().hex[:8]}")

        with self._store.connect() as connection:
            while True:
                world_exists = connection.execute("SELECT 1 FROM worlds WHERE id = ?", (candidate,)).fetchone()
                character_exists = connection.execute("SELECT 1 FROM characters WHERE id = ?", (candidate,)).fetchone()
                model_exists = connection.execute("SELECT 1 FROM model_configs WHERE id = ?", (candidate,)).fetchone()
                if not world_exists and not character_exists and not model_exists:
                    return candidate
                candidate = f"{candidate}-{uuid.uuid4().hex[:4]}"
