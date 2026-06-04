from typing import Any
import json
import sqlite3
from pathlib import Path

from backend.app.domain.models.attribute import AttributeSchema, AttributeValue
from backend.app.domain.models.agent_runtime import AgentCheckpoint, AgentSession, TurnJournalEntry
from backend.app.domain.models.asset import AssetSelection, CharacterVisualState
from backend.app.domain.models.character import CharacterDefinition
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.memory import MemoryEntry
from backend.app.domain.models.model_config import ModelConfig
from backend.app.domain.models.plugin import PluginDefinition
from backend.app.domain.models.rule import RuleDefinition
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.save import SaveSummary
from backend.app.domain.models.session import ChatMessage, SessionSnapshot
from backend.app.domain.models.session import SessionMapEdge, SessionMapNode
from backend.app.domain.models.state import SessionState
from backend.app.domain.models.settings import AppSettingsSnapshot
from backend.app.domain.models.world import (
    WorldDefinition,
    WorldOpeningMessage,
    normalize_world_director_config,
)
from backend.app.infrastructure.repositories import seed_data


class SqliteStore:
    def __init__(self, db_path: str) -> None:
        self._db_path = Path(db_path)
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self.initialize()

    @property
    def db_path(self) -> Path:
        return self._db_path

    def connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self._db_path)
        connection.row_factory = sqlite3.Row
        return connection

    def initialize(self) -> None:
        with self.connect() as connection:
            connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS worlds (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    genre TEXT NOT NULL,
                    background_prompt TEXT NOT NULL,
                    opening_scene TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    time_system TEXT NOT NULL,
                    map_nodes_json TEXT NOT NULL,
                    triggers_json TEXT NOT NULL,
                    custom_tabs_json TEXT NOT NULL DEFAULT '{}',
                    time_config_json TEXT NOT NULL DEFAULT '{}',
                    director_config_json TEXT NOT NULL DEFAULT '{}',
                    ui_theme_config_json TEXT NOT NULL DEFAULT '{}',
                    opening_messages_json TEXT NOT NULL DEFAULT '[]',
                    opening_character_ids_json TEXT NOT NULL DEFAULT '[]',
                    player_character_id TEXT
                );

                CREATE TABLE IF NOT EXISTS characters (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    world_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    background_prompt TEXT NOT NULL,
                    model TEXT NOT NULL,
                    memory_strategy TEXT NOT NULL,
                    recent_dialogue_rounds INTEGER NOT NULL DEFAULT 2,
                    attributes_json TEXT NOT NULL,
                    portrait_assets_json TEXT NOT NULL DEFAULT '[]',
                    custom_tabs_json TEXT NOT NULL DEFAULT '{}'
                );

                CREATE TABLE IF NOT EXISTS settings (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    text_model_provider TEXT NOT NULL,
                    default_text_model TEXT NOT NULL,
                    image_model_provider TEXT NOT NULL,
                    default_image_workflow TEXT NOT NULL,
                    home_background_strategy TEXT NOT NULL,
                    export_directory TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS attribute_schemas (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    key TEXT NOT NULL,
                    label TEXT NOT NULL,
                    value_type TEXT NOT NULL,
                    description TEXT NOT NULL,
                    default_value_json TEXT NOT NULL,
                    enum_options_json TEXT NOT NULL,
                    display_policy_json TEXT NOT NULL,
                    access_policy_json TEXT NOT NULL,
                    mutation_policy_json TEXT NOT NULL,
                    influence_policy_json TEXT NOT NULL,
                    projection_policy_json TEXT NOT NULL,
                    UNIQUE(scope, key)
                );

                CREATE TABLE IF NOT EXISTS attribute_values (
                    id TEXT PRIMARY KEY,
                    schema_id TEXT NOT NULL,
                    owner_type TEXT NOT NULL,
                    owner_id TEXT NOT NULL,
                    value_json TEXT NOT NULL,
                    source TEXT NOT NULL,
                    UNIQUE(schema_id, owner_type, owner_id)
                );

                CREATE TABLE IF NOT EXISTS plugins (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    description TEXT NOT NULL,
                    hooks_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS mcp_tools (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    server_name TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    exposure_policy TEXT NOT NULL DEFAULT 'on-demand',
                    risk_level TEXT NOT NULL DEFAULT 'low',
                    trigger_keywords_json TEXT NOT NULL DEFAULT '[]'
                );

                CREATE TABLE IF NOT EXISTS rules (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    name TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    priority INTEGER NOT NULL,
                    description TEXT NOT NULL,
                    condition_json TEXT NOT NULL,
                    effects_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    world_name TEXT NOT NULL,
                    location TEXT NOT NULL,
                    time_label TEXT NOT NULL,
                    current_speaker TEXT NOT NULL,
                    current_line TEXT NOT NULL,
                    player_character_id TEXT,
                    player_character_name TEXT,
                    visible_characters_json TEXT NOT NULL,
                    messages_json TEXT NOT NULL,
                    player_stats_json TEXT NOT NULL,
                    map_graph_json TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[]}',
                    inventory_items_json TEXT NOT NULL,
                    system_log_json TEXT NOT NULL,
                    scene_json TEXT NOT NULL DEFAULT '{"scene_id":"default","name":"default","background_hint":"default","temporary_tags":[],"present_characters":[]}',
                    assets_json TEXT NOT NULL DEFAULT '{"background_hint":"default","active_speaker_portrait":"default","visible_character_portraits":[]}',
                    state_json TEXT NOT NULL DEFAULT '{"metrics":{},"tags":[],"phase":"idle"}'
                );

                CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY,
                    world_id TEXT,
                    session_id TEXT NOT NULL,
                    turn_index INTEGER NOT NULL DEFAULT 0,
                    conversation_id TEXT,
                    character_id TEXT NOT NULL,
                    event_id TEXT,
                    item_id TEXT,
                    scene_id TEXT,
                    layer TEXT NOT NULL,
                    content TEXT NOT NULL,
                    source TEXT NOT NULL,
                    importance REAL NOT NULL,
                    created_at TEXT NOT NULL,
                    memory_type TEXT NOT NULL DEFAULT 'dialogue',
                    speaker TEXT,
                    role TEXT,
                    location TEXT,
                    participants_json TEXT NOT NULL DEFAULT '[]',
                    keywords_json TEXT NOT NULL DEFAULT '[]'
                );

                CREATE TABLE IF NOT EXISTS agent_sessions (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    agent_type TEXT NOT NULL,
                    character_id TEXT,
                    character_name TEXT,
                    status TEXT NOT NULL,
                    connection_state TEXT NOT NULL,
                    scene_presence_state TEXT NOT NULL,
                    checkpoint_id TEXT,
                    last_active_turn INTEGER NOT NULL DEFAULT 0,
                    last_ack_message_index INTEGER NOT NULL DEFAULT 0,
                    prompt_version TEXT NOT NULL DEFAULT 'v1',
                    runtime_key TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE(session_id, runtime_key)
                );

                CREATE TABLE IF NOT EXISTS agent_checkpoints (
                    id TEXT PRIMARY KEY,
                    agent_session_id TEXT NOT NULL,
                    turn_index INTEGER NOT NULL,
                    checkpoint_type TEXT NOT NULL,
                    payload_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS turn_journal (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    turn_index INTEGER NOT NULL,
                    step TEXT NOT NULL,
                    status TEXT NOT NULL,
                    payload_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS saves (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    world_name TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    progress TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    player_character_name TEXT,
                    parent_save_id TEXT,
                    branch_root_save_id TEXT,
                    branch_label TEXT
                );

                CREATE TABLE IF NOT EXISTS model_configs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    model_type TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    is_default INTEGER NOT NULL DEFAULT 0
                );
                """
            )

            self._rebuild_sessions_table_without_legacy_map_nodes(connection)

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="background_prompt",
                column_definition="""TEXT NOT NULL DEFAULT ''""",
            )

            self._ensure_column(
                connection=connection,
                table_name="characters",
                column_name="background_prompt",
                column_definition="""TEXT NOT NULL DEFAULT ''""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="custom_tabs_json",
                column_definition="""TEXT NOT NULL DEFAULT '{}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="director_config_json",
                column_definition="""TEXT NOT NULL DEFAULT '{}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="player_character_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="ui_theme_config_json",
                column_definition="""TEXT NOT NULL DEFAULT '{}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="opening_messages_json",
                column_definition="""TEXT NOT NULL DEFAULT '[]'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="opening_character_ids_json",
                column_definition="""TEXT NOT NULL DEFAULT '[]'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="worlds",
                column_name="time_config_json",
                column_definition="""TEXT NOT NULL DEFAULT '{}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="characters",
                column_name="custom_tabs_json",
                column_definition="""TEXT NOT NULL DEFAULT '{}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="characters",
                column_name="recent_dialogue_rounds",
                column_definition="""INTEGER NOT NULL DEFAULT 2""",
            )

            self._ensure_column(
                connection=connection,
                table_name="characters",
                column_name="portrait_assets_json",
                column_definition="""TEXT NOT NULL DEFAULT '[]'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="scene_json",
                column_definition="""TEXT NOT NULL DEFAULT '{"scene_id":"default","name":"default","background_hint":"default","temporary_tags":[],"present_characters":[]}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="assets_json",
                column_definition="""TEXT NOT NULL DEFAULT '{"background_hint":"default","active_speaker_portrait":"default","visible_character_portraits":[]}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="state_json",
                column_definition="""TEXT NOT NULL DEFAULT '{"metrics":{},"tags":[],"phase":"idle"}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="player_character_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="player_character_name",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="saves",
                column_name="player_character_name",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="saves",
                column_name="parent_save_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="saves",
                column_name="branch_root_save_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="saves",
                column_name="branch_label",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="world_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="turn_index",
                column_definition="""INTEGER NOT NULL DEFAULT 0""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="memory_type",
                column_definition="""TEXT NOT NULL DEFAULT 'dialogue'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="conversation_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="event_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="item_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="scene_id",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="speaker",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="role",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="location",
                column_definition="""TEXT""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="participants_json",
                column_definition="""TEXT NOT NULL DEFAULT '[]'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="sessions",
                column_name="map_graph_json",
                column_definition="""TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[]}'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="memories",
                column_name="keywords_json",
                column_definition="""TEXT NOT NULL DEFAULT '[]'""",
            )

            self._ensure_column(
                connection=connection,
                table_name="agent_sessions",
                column_name="initialized_at",
                column_definition="""TEXT""",
            )

            self._backfill_seed_world_configs(connection)
            self._ensure_builtin_mcp_tools(connection)

            if connection.execute("SELECT COUNT(*) FROM worlds").fetchone()[0] == 0:
                for world in seed_data.default_worlds():
                    self.insert_world(connection, world)

            if connection.execute("SELECT COUNT(*) FROM characters").fetchone()[0] == 0:
                for character in seed_data.default_characters():
                    self.insert_character(connection, character)

            if connection.execute("SELECT COUNT(*) FROM settings").fetchone()[0] == 0:
                self.upsert_settings(connection, seed_data.default_settings())

            if connection.execute("SELECT COUNT(*) FROM attribute_schemas").fetchone()[0] == 0:
                for schema in seed_data.default_attribute_schemas():
                    self.insert_attribute_schema(connection, schema)

            if connection.execute("SELECT COUNT(*) FROM plugins").fetchone()[0] == 0:
                for plugin in seed_data.default_plugins():
                    self.insert_plugin(connection, plugin)

            if connection.execute("SELECT COUNT(*) FROM rules").fetchone()[0] == 0:
                for rule in seed_data.default_rules():
                    self.insert_rule(connection, rule)

            if connection.execute("SELECT COUNT(*) FROM model_configs").fetchone()[0] == 0:
                for model in seed_data.default_model_configs():
                    self.insert_model_config(connection, model)

    def _ensure_column(
        self,
        connection: sqlite3.Connection,
        table_name: str,
        column_name: str,
        column_definition: str,
    ) -> None:
        if not _has_column(connection=connection, table_name=table_name, column_name=column_name):
            _add_column(
                connection=connection,
                table_name=table_name,
                column_name=column_name,
                column_definition=column_definition,
            )

    def _backfill_seed_world_configs(self, connection: sqlite3.Connection) -> None:
        for world in seed_data.default_worlds():
            connection.execute(
                """
                UPDATE worlds
                SET time_config_json = CASE
                    WHEN COALESCE(NULLIF(TRIM(time_config_json), ''), '{}') = '{}'
                        THEN ?
                    ELSE time_config_json
                END,
                    director_config_json = CASE
                    WHEN COALESCE(NULLIF(TRIM(director_config_json), ''), '{}') = '{}'
                        THEN ?
                    ELSE director_config_json
                END
                WHERE id = ?
                """,
                (
                    json.dumps(world.time_config, ensure_ascii=False),
                    json.dumps(world.director_config, ensure_ascii=False),
                    world.id,
                ),
            )

            connection.execute(
                """
                UPDATE memories
                SET world_id = (
                    SELECT worlds.id
                    FROM sessions
                    JOIN worlds ON worlds.name = sessions.world_name
                    WHERE sessions.id = memories.session_id
                )
                WHERE COALESCE(TRIM(world_id), '') = ''
                """
            )
            connection.execute(
                """
                UPDATE memories
                SET conversation_id = session_id
                WHERE conversation_id IS NULL OR TRIM(conversation_id) = ''
                """
            )
            connection.execute(
                """
                UPDATE memories
                SET scene_id = (
                    SELECT json_extract(sessions.scene_json, '$.scene_id')
                    FROM sessions
                    WHERE sessions.id = memories.session_id
                )
                WHERE COALESCE(TRIM(scene_id), '') = ''
                """
            )

    def _rebuild_sessions_table_without_legacy_map_nodes(self, connection: sqlite3.Connection) -> None:
        if not _has_column(connection=connection, table_name="sessions", column_name="map_nodes_json"):
            return

        connection.execute("DROP TABLE IF EXISTS sessions")
        connection.execute("DELETE FROM saves")
        connection.execute(
            """
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                world_name TEXT NOT NULL,
                location TEXT NOT NULL,
                time_label TEXT NOT NULL,
                current_speaker TEXT NOT NULL,
                current_line TEXT NOT NULL,
                player_character_id TEXT,
                player_character_name TEXT,
                visible_characters_json TEXT NOT NULL,
                messages_json TEXT NOT NULL,
                player_stats_json TEXT NOT NULL,
                map_graph_json TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[]}',
                inventory_items_json TEXT NOT NULL,
                system_log_json TEXT NOT NULL,
                scene_json TEXT NOT NULL DEFAULT '{"scene_id":"default","name":"default","background_hint":"default","temporary_tags":[],"present_characters":[]}',
                assets_json TEXT NOT NULL DEFAULT '{"background_hint":"default","active_speaker_portrait":"default","visible_character_portraits":[]}',
                state_json TEXT NOT NULL DEFAULT '{"metrics":{},"tags":[],"phase":"idle"}'
            )
            """
        )

    def insert_world(self, connection: sqlite3.Connection, world: WorldDefinition) -> None:
        connection.execute(
            """
            INSERT INTO worlds (
                id, name, genre, background_prompt, opening_scene, summary, time_system,
                map_nodes_json, triggers_json, custom_tabs_json, time_config_json, director_config_json, ui_theme_config_json,
                opening_messages_json, opening_character_ids_json, player_character_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                world.id,
                world.name,
                world.genre,
                world.background_prompt,
                world.opening_scene,
                world.summary,
                world.time_system,
                json.dumps(world.map_nodes, ensure_ascii=False),
                json.dumps(world.triggers, ensure_ascii=False),
                json.dumps(world.custom_tabs, ensure_ascii=False),
                json.dumps(world.time_config, ensure_ascii=False),
                json.dumps(world.director_config, ensure_ascii=False),
                json.dumps(world.ui_theme_config, ensure_ascii=False),
                json.dumps(
                    [{"role": item.role, "content": item.content, "speaker": item.speaker} for item in world.opening_messages],
                    ensure_ascii=False,
                ),
                json.dumps(world.opening_character_ids, ensure_ascii=False),
                world.player_character_id,
            ),
        )

    def insert_character(self, connection: sqlite3.Connection, character: CharacterDefinition) -> None:
        connection.execute(
            """
            INSERT INTO characters (id, name, world_id, role, background_prompt, model, memory_strategy, recent_dialogue_rounds, attributes_json, portrait_assets_json, custom_tabs_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                character.id,
                character.name,
                character.world_id,
                character.role,
                character.background_prompt,
                character.model,
                character.memory_strategy,
                character.recent_dialogue_rounds,
                json.dumps(character.attributes, ensure_ascii=False),
                json.dumps(character.portrait_assets, ensure_ascii=False),
                json.dumps(character.custom_tabs, ensure_ascii=False),
            ),
        )

    def insert_plugin(self, connection: sqlite3.Connection, plugin: PluginDefinition) -> None:
        connection.execute(
            """
            INSERT INTO plugins (id, name, enabled, description, hooks_json)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                plugin.id,
                plugin.name,
                1 if plugin.enabled else 0,
                plugin.description,
                json.dumps(plugin.hooks, ensure_ascii=False),
            ),
        )

    def insert_rule(self, connection: sqlite3.Connection, rule: RuleDefinition) -> None:
        connection.execute(
            """
            INSERT INTO rules (id, scope, name, enabled, priority, description, condition_json, effects_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                rule.id,
                rule.scope,
                rule.name,
                1 if rule.enabled else 0,
                rule.priority,
                rule.description,
                json.dumps(rule.condition, ensure_ascii=False),
                json.dumps(rule.effects, ensure_ascii=False),
            ),
        )

    def upsert_rule(self, connection: sqlite3.Connection, rule: RuleDefinition) -> None:
        connection.execute(
            """
            INSERT INTO rules (id, scope, name, enabled, priority, description, condition_json, effects_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                scope = excluded.scope,
                name = excluded.name,
                enabled = excluded.enabled,
                priority = excluded.priority,
                description = excluded.description,
                condition_json = excluded.condition_json,
                effects_json = excluded.effects_json
            """,
            (
                rule.id,
                rule.scope,
                rule.name,
                1 if rule.enabled else 0,
                rule.priority,
                rule.description,
                json.dumps(rule.condition, ensure_ascii=False),
                json.dumps(rule.effects, ensure_ascii=False),
            ),
        )

    def upsert_settings(self, connection: sqlite3.Connection, settings: AppSettingsSnapshot) -> None:
        connection.execute(
            """
            INSERT INTO settings (id, text_model_provider, default_text_model, image_model_provider, default_image_workflow, home_background_strategy, export_directory)
            VALUES (1, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                text_model_provider = excluded.text_model_provider,
                default_text_model = excluded.default_text_model,
                image_model_provider = excluded.image_model_provider,
                default_image_workflow = excluded.default_image_workflow,
                home_background_strategy = excluded.home_background_strategy,
                export_directory = excluded.export_directory
            """,
            (
                settings.text_model_provider,
                settings.default_text_model,
                settings.image_model_provider,
                settings.default_image_workflow,
                settings.home_background_strategy,
                settings.export_directory,
            ),
        )

    def insert_attribute_schema(self, connection: sqlite3.Connection, schema: AttributeSchema) -> None:
        connection.execute(
            """
            INSERT INTO attribute_schemas (
                id, scope, key, label, value_type, description, default_value_json,
                enum_options_json, display_policy_json, access_policy_json,
                mutation_policy_json, influence_policy_json, projection_policy_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                schema.id,
                schema.scope,
                schema.key,
                schema.label,
                schema.value_type,
                schema.description,
                json.dumps(schema.default_value, ensure_ascii=False),
                json.dumps(schema.enum_options, ensure_ascii=False),
                json.dumps(schema.display_policy, ensure_ascii=False),
                json.dumps(schema.access_policy, ensure_ascii=False),
                json.dumps(schema.mutation_policy, ensure_ascii=False),
                json.dumps(schema.influence_policy, ensure_ascii=False),
                json.dumps(schema.projection_policy, ensure_ascii=False),
            ),
        )

    def upsert_attribute_schema(self, connection: sqlite3.Connection, schema: AttributeSchema) -> None:
        connection.execute(
            """
            INSERT INTO attribute_schemas (
                id, scope, key, label, value_type, description, default_value_json,
                enum_options_json, display_policy_json, access_policy_json,
                mutation_policy_json, influence_policy_json, projection_policy_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                scope = excluded.scope,
                key = excluded.key,
                label = excluded.label,
                value_type = excluded.value_type,
                description = excluded.description,
                default_value_json = excluded.default_value_json,
                enum_options_json = excluded.enum_options_json,
                display_policy_json = excluded.display_policy_json,
                access_policy_json = excluded.access_policy_json,
                mutation_policy_json = excluded.mutation_policy_json,
                influence_policy_json = excluded.influence_policy_json,
                projection_policy_json = excluded.projection_policy_json
            """,
            (
                schema.id,
                schema.scope,
                schema.key,
                schema.label,
                schema.value_type,
                schema.description,
                json.dumps(schema.default_value, ensure_ascii=False),
                json.dumps(schema.enum_options, ensure_ascii=False),
                json.dumps(schema.display_policy, ensure_ascii=False),
                json.dumps(schema.access_policy, ensure_ascii=False),
                json.dumps(schema.mutation_policy, ensure_ascii=False),
                json.dumps(schema.influence_policy, ensure_ascii=False),
                json.dumps(schema.projection_policy, ensure_ascii=False),
            ),
        )

    def upsert_attribute_value(self, connection: sqlite3.Connection, value: AttributeValue) -> None:
        connection.execute(
            """
            INSERT INTO attribute_values (id, schema_id, owner_type, owner_id, value_json, source)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(schema_id, owner_type, owner_id) DO UPDATE SET
                value_json = excluded.value_json,
                source = excluded.source
            """,
            (
                value.id,
                value.schema_id,
                value.owner_type,
                value.owner_id,
                json.dumps(value.value, ensure_ascii=False),
                value.source,
            ),
        )

    def upsert_session(self, connection: sqlite3.Connection, session: SessionSnapshot) -> None:
        connection.execute(
            """
            INSERT INTO sessions (
                id, world_name, location, time_label, current_speaker, current_line, player_character_id,
                player_character_name,
                visible_characters_json, messages_json, player_stats_json, map_graph_json,
                inventory_items_json, system_log_json, scene_json, assets_json, state_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                world_name = excluded.world_name,
                location = excluded.location,
                time_label = excluded.time_label,
                current_speaker = excluded.current_speaker,
                current_line = excluded.current_line,
                player_character_id = excluded.player_character_id,
                player_character_name = excluded.player_character_name,
                visible_characters_json = excluded.visible_characters_json,
                messages_json = excluded.messages_json,
                player_stats_json = excluded.player_stats_json,
                map_graph_json = excluded.map_graph_json,
                inventory_items_json = excluded.inventory_items_json,
                system_log_json = excluded.system_log_json,
                scene_json = excluded.scene_json,
                assets_json = excluded.assets_json,
                state_json = excluded.state_json
            """,
            (
                session.id,
                session.world_name,
                session.location,
                session.time_label,
                session.current_speaker,
                session.current_line,
                session.player_character_id,
                session.player_character_name,
                json.dumps(session.visible_characters, ensure_ascii=False),
                json.dumps(
                    [
                        {
                            "role": item.role,
                            "content": item.content,
                            "speaker": item.speaker,
                            **({"metadata": item.metadata} if item.metadata else {}),
                        }
                        for item in session.messages
                    ],
                    ensure_ascii=False,
                ),
                json.dumps(session.player_stats, ensure_ascii=False),
                json.dumps(
                    {
                        "nodes": [
                            {
                                "node_id": item.node_id,
                                "label": item.label,
                                "discovered": item.discovered,
                                "current": item.current,
                            }
                            for item in session.map_graph_nodes
                        ],
                        "edges": [
                            {
                                "edge_id": item.edge_id,
                                "source_node_id": item.source_node_id,
                                "target_node_id": item.target_node_id,
                            }
                            for item in session.map_graph_edges
                        ],
                    },
                    ensure_ascii=False,
                ),
                json.dumps(
                    [
                        {
                            "item_id": item.item_id,
                            "name": item.name,
                            "category": item.category,
                            "quantity": item.quantity,
                            "description": item.description,
                            "tags": item.tags,
                            "owner_type": item.owner_type,
                            "owner_id": item.owner_id,
                            "visibility": item.visibility,
                            "disclosed_to": item.disclosed_to,
                        }
                        for item in session.inventory_items
                    ],
                    ensure_ascii=False,
                ),
                json.dumps(session.system_log, ensure_ascii=False),
                json.dumps(
                    {
                        "scene_id": session.scene.scene_id,
                        "name": session.scene.name,
                        "background_hint": session.scene.background_hint,
                        "temporary_tags": session.scene.temporary_tags,
                        "present_characters": session.scene.present_characters,
                    },
                    ensure_ascii=False,
                ),
                json.dumps(
                    {
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
                    ensure_ascii=False,
                ),
                json.dumps(
                    {
                        "metrics": session.state.metrics,
                        "tags": session.state.tags,
                        "phase": session.state.phase,
                    },
                    ensure_ascii=False,
                ),
            ),
        )

    def upsert_save(self, connection: sqlite3.Connection, save: SaveSummary) -> None:
        connection.execute(
            """
            INSERT INTO saves (
                id, session_id, title, world_name, updated_at, progress, summary,
                player_character_name, parent_save_id, branch_root_save_id, branch_label
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                title = excluded.title,
                world_name = excluded.world_name,
                updated_at = excluded.updated_at,
                progress = excluded.progress,
                summary = excluded.summary,
                player_character_name = excluded.player_character_name,
                parent_save_id = excluded.parent_save_id,
                branch_root_save_id = excluded.branch_root_save_id,
                branch_label = excluded.branch_label
            """,
            (
                save.id,
                save.session_id,
                save.title,
                save.world_name,
                save.updated_at,
                save.progress,
                save.summary,
                save.player_character_name,
                save.parent_save_id,
                save.branch_root_save_id,
                save.branch_label,
            ),
        )

    def insert_memory(self, connection: sqlite3.Connection, entry: MemoryEntry) -> None:
        connection.execute(
            """
            INSERT INTO memories (
                id, world_id, session_id, turn_index, conversation_id, character_id, event_id, item_id, scene_id, layer, content, source, importance, created_at,
                memory_type, speaker, role, location, participants_json, keywords_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                entry.id,
                entry.world_id,
                entry.session_id,
                entry.turn_index,
                entry.conversation_id,
                entry.character_id,
                entry.event_id,
                entry.item_id,
                entry.scene_id,
                entry.layer,
                entry.content,
                entry.source,
                entry.importance,
                entry.created_at,
                entry.memory_type,
                entry.speaker,
                entry.role,
                entry.location,
                json.dumps(entry.participants, ensure_ascii=False),
                json.dumps(entry.keywords, ensure_ascii=False),
            ),
        )

    def upsert_agent_session(self, connection: sqlite3.Connection, session: AgentSession) -> None:
        connection.execute(
            """
            INSERT INTO agent_sessions (
                id, session_id, agent_type, character_id, character_name, status, connection_state, scene_presence_state,
                checkpoint_id, last_active_turn, last_ack_message_index, prompt_version, runtime_key,
                initialized_at, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                agent_type = excluded.agent_type,
                character_id = excluded.character_id,
                character_name = excluded.character_name,
                status = excluded.status,
                connection_state = excluded.connection_state,
                scene_presence_state = excluded.scene_presence_state,
                checkpoint_id = excluded.checkpoint_id,
                last_active_turn = excluded.last_active_turn,
                last_ack_message_index = excluded.last_ack_message_index,
                prompt_version = excluded.prompt_version,
                runtime_key = excluded.runtime_key,
                initialized_at = excluded.initialized_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            """,
            (
                session.id,
                session.session_id,
                session.agent_type,
                session.character_id,
                session.character_name,
                session.status,
                session.connection_state,
                session.scene_presence_state,
                session.checkpoint_id,
                session.last_active_turn,
                session.last_ack_message_index,
                session.prompt_version,
                session.runtime_key,
                session.initialized_at,
                session.created_at,
                session.updated_at,
            ),
        )

    def insert_agent_checkpoint(self, connection: sqlite3.Connection, checkpoint: AgentCheckpoint) -> None:
        connection.execute(
            """
            INSERT INTO agent_checkpoints (id, agent_session_id, turn_index, checkpoint_type, payload_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                checkpoint.id,
                checkpoint.agent_session_id,
                checkpoint.turn_index,
                checkpoint.checkpoint_type,
                json.dumps(checkpoint.payload, ensure_ascii=False),
                checkpoint.created_at,
            ),
        )

    def insert_turn_journal_entry(self, connection: sqlite3.Connection, entry: TurnJournalEntry) -> None:
        connection.execute(
            """
            INSERT INTO turn_journal (id, session_id, turn_index, step, status, payload_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                entry.id,
                entry.session_id,
                entry.turn_index,
                entry.step,
                entry.status,
                json.dumps(entry.payload, ensure_ascii=False),
                entry.created_at,
            ),
        )

    def insert_model_config(self, connection: sqlite3.Connection, model: ModelConfig) -> None:
        connection.execute(
            """
            INSERT INTO model_configs (id, name, model_type, provider, model_id, base_url, api_key, is_default)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                model.id,
                model.name,
                model.model_type,
                model.provider,
                model.model_id,
                model.base_url,
                model.api_key,
                1 if model.is_default else 0,
            ),
        )

    def upsert_model_config(self, connection: sqlite3.Connection, model: ModelConfig) -> None:
        connection.execute(
            """
            INSERT INTO model_configs (id, name, model_type, provider, model_id, base_url, api_key, is_default)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                model_type = excluded.model_type,
                provider = excluded.provider,
                model_id = excluded.model_id,
                base_url = excluded.base_url,
                api_key = excluded.api_key,
                is_default = excluded.is_default
            """,
            (
                model.id,
                model.name,
                model.model_type,
                model.provider,
                model.model_id,
                model.base_url,
                model.api_key,
                1 if model.is_default else 0,
            ),
        )

    def clear_default_models(self, connection: sqlite3.Connection, model_type: str) -> None:
        connection.execute(
            "UPDATE model_configs SET is_default = 0 WHERE model_type = ? AND is_default = 1",
            (model_type,),
        )

    def _ensure_builtin_mcp_tools(self, connection: sqlite3.Connection) -> None:
        builtin_tools = [
            ("mcp-tool-image-generation", "文生图", "根据世界主控给出的视觉提示词生成背景图或人物立绘，并把生成资产写入会话存档。", "builtin-image-generation", "generate_image", 1, "on-demand", "medium", ["背景图", "立绘", "文生图", "生成图片", "场景图"]),
            ("mcp-tool-list-scenes", "查询场景", "查询当前世界已有场景、地图节点和当前会话场景，供世界主控切换场景前参考。", "builtin-world-director", "list_scenes", 1, "on-demand", "low", ["场景", "地图", "地点", "查询场景"]),
            ("mcp-tool-list-characters", "查询角色", "查询当前世界已有角色、角色身份和在场状态，供世界主控安排 NPC 前参考。", "builtin-world-director", "list_characters", 1, "on-demand", "low", ["角色", "人物", "NPC", "查询角色"]),
            ("mcp-tool-change-scene", "切换场景", "由世界主控显式切换场景、填写场景描述、新增人物、在场人物和玩家操控人物。", "builtin-world-director", "change_scene", 1, "on-demand", "medium", ["切换场景", "转世", "换身份", "进入场景", "新增人物"]),
        ]
        connection.executemany(
            """
            INSERT OR IGNORE INTO mcp_tools (
                id, name, description, server_name, tool_name, enabled, exposure_policy, risk_level, trigger_keywords_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [(*item[:8], json.dumps(item[8], ensure_ascii=False)) for item in builtin_tools],
        )


def row_to_world(row: sqlite3.Row) -> WorldDefinition:
    return WorldDefinition(
        id=row["id"],
        name=row["name"],
        genre=row["genre"],
        background_prompt=row["background_prompt"] if "background_prompt" in row.keys() else "",
        opening_scene=row["opening_scene"],
        summary=row["summary"],
        time_system=row["time_system"],
        map_nodes=json.loads(row["map_nodes_json"]),
        triggers=json.loads(row["triggers_json"]),
        custom_tabs=json.loads(row["custom_tabs_json"]) if "custom_tabs_json" in row.keys() else {},
        time_config=json.loads(row["time_config_json"]) if "time_config_json" in row.keys() else {},
        director_config=normalize_world_director_config(
            json.loads(row["director_config_json"]) if "director_config_json" in row.keys() else {}
        ),
        ui_theme_config=json.loads(row["ui_theme_config_json"]) if "ui_theme_config_json" in row.keys() else {},
        opening_messages=[
            WorldOpeningMessage(
                role=str(item.get("role", "system")),
                content=str(item.get("content", "")),
                speaker=item.get("speaker"),
            )
            for item in (
                json.loads(row["opening_messages_json"])
                if "opening_messages_json" in row.keys()
                else []
            )
            if str(item.get("content", "")).strip()
        ],
        opening_character_ids=[
            str(item).strip()
            for item in (
                json.loads(row["opening_character_ids_json"])
                if "opening_character_ids_json" in row.keys()
                else []
            )
            if str(item).strip()
        ],
        player_character_id=row["player_character_id"] if "player_character_id" in row.keys() else None,
    )


def row_to_attribute_schema(row: sqlite3.Row) -> AttributeSchema:
    return AttributeSchema(
        id=row["id"],
        scope=row["scope"],
        key=row["key"],
        label=row["label"],
        value_type=row["value_type"],
        description=row["description"],
        default_value=json.loads(row["default_value_json"]),
        enum_options=json.loads(row["enum_options_json"]),
        display_policy=json.loads(row["display_policy_json"]),
        access_policy=json.loads(row["access_policy_json"]),
        mutation_policy=json.loads(row["mutation_policy_json"]),
        influence_policy=json.loads(row["influence_policy_json"]),
        projection_policy=json.loads(row["projection_policy_json"]),
    )


def row_to_attribute_value(row: sqlite3.Row) -> AttributeValue:
    return AttributeValue(
        id=row["id"],
        schema_id=row["schema_id"],
        owner_type=row["owner_type"],
        owner_id=row["owner_id"],
        value=json.loads(row["value_json"]),
        source=row["source"],
    )


def row_to_character(row: sqlite3.Row) -> CharacterDefinition:
    return CharacterDefinition(
        id=row["id"],
        name=row["name"],
        world_id=row["world_id"],
        role=row["role"],
        background_prompt=row["background_prompt"] if "background_prompt" in row.keys() else "",
        model=row["model"],
        memory_strategy=row["memory_strategy"],
        recent_dialogue_rounds=int(row["recent_dialogue_rounds"]) if "recent_dialogue_rounds" in row.keys() else 2,
        attributes=json.loads(row["attributes_json"]),
        portrait_assets=json.loads(row["portrait_assets_json"]) if "portrait_assets_json" in row.keys() else [],
        custom_tabs=json.loads(row["custom_tabs_json"]) if "custom_tabs_json" in row.keys() else {},
    )


def row_to_settings(row: sqlite3.Row) -> AppSettingsSnapshot:
    return AppSettingsSnapshot(
        text_model_provider=row["text_model_provider"],
        default_text_model=row["default_text_model"],
        image_model_provider=row["image_model_provider"],
        default_image_workflow=row["default_image_workflow"],
        home_background_strategy=row["home_background_strategy"],
        export_directory=row["export_directory"],
    )


def row_to_plugin(row: sqlite3.Row) -> PluginDefinition:
    return PluginDefinition(
        id=row["id"],
        name=row["name"],
        enabled=bool(row["enabled"]),
        description=row["description"],
        hooks=json.loads(row["hooks_json"]),
    )


def row_to_rule(row: sqlite3.Row) -> RuleDefinition:
    return RuleDefinition(
        id=row["id"],
        scope=row["scope"],
        name=row["name"],
        enabled=bool(row["enabled"]),
        priority=int(row["priority"]),
        description=row["description"],
        condition=json.loads(row["condition_json"]),
        effects=json.loads(row["effects_json"]),
    )


def row_to_session(row: sqlite3.Row) -> SessionSnapshot:
    raw_messages = json.loads(row["messages_json"])
    raw_scene = (
        json.loads(row["scene_json"])
        if "scene_json" in row.keys()
        else {
            "scene_id": "default",
            "name": row["location"],
            "background_hint": "default",
            "temporary_tags": [],
            "present_characters": json.loads(row["visible_characters_json"]),
        }
    )
    raw_assets = (
        json.loads(row["assets_json"])
        if "assets_json" in row.keys()
        else {
            "background_hint": "default",
            "active_speaker_portrait": "default",
            "visible_character_portraits": [],
        }
    )
    raw_state = json.loads(row["state_json"]) if "state_json" in row.keys() else {"metrics": {}, "tags": [], "phase": "idle"}
    raw_map_graph = json.loads(row["map_graph_json"]) if "map_graph_json" in row.keys() else {"nodes": [], "edges": []}
    return SessionSnapshot(
        id=row["id"],
        world_name=row["world_name"],
        location=row["location"],
        time_label=row["time_label"],
        current_speaker=row["current_speaker"],
        current_line=row["current_line"],
        player_character_id=row["player_character_id"] if "player_character_id" in row.keys() else None,
        player_character_name=row["player_character_name"] if "player_character_name" in row.keys() else None,
        visible_characters=json.loads(row["visible_characters_json"]),
        messages=[ChatMessage(role=item["role"], content=item["content"], speaker=item.get("speaker"), metadata=item.get("metadata")) for item in raw_messages],
        player_stats=json.loads(row["player_stats_json"]),
        map_graph_nodes=[
            SessionMapNode(
                node_id=item.get("node_id", ""),
                label=item.get("label", ""),
                discovered=bool(item.get("discovered", True)),
                current=bool(item.get("current", False)),
            )
            for item in raw_map_graph.get("nodes", [])
        ],
        map_graph_edges=[
            SessionMapEdge(
                edge_id=item.get("edge_id", ""),
                source_node_id=item.get("source_node_id", ""),
                target_node_id=item.get("target_node_id", ""),
            )
            for item in raw_map_graph.get("edges", [])
        ],
        inventory_items=[
            InventoryItem(
                item_id=item.get("item_id", "item-unknown"),
                name=item.get("name", "unknown"),
                category=item.get("category", "misc"),
                quantity=int(item.get("quantity", 1)),
                description=item.get("description", ""),
                tags=item.get("tags", []),
                owner_type=item.get("owner_type", "player"),
                owner_id=item.get("owner_id", "player"),
                visibility=item.get("visibility", "private"),
                disclosed_to=item.get("disclosed_to", []),
            )
            for item in json.loads(row["inventory_items_json"])
        ],
        system_log=json.loads(row["system_log_json"]),
        scene=SceneRuntime(
            scene_id=raw_scene.get("scene_id", "default"),
            name=raw_scene.get("name", row["location"]),
            background_hint=raw_scene.get("background_hint", "default"),
            temporary_tags=raw_scene.get("temporary_tags", []),
            present_characters=raw_scene.get("present_characters", []),
        ),
        assets=AssetSelection(
            background_hint=raw_assets.get("background_hint", "default"),
            active_speaker_portrait=raw_assets.get("active_speaker_portrait", "default"),
            background_asset_path=raw_assets.get("background_asset_path"),
            active_speaker_portrait_path=raw_assets.get("active_speaker_portrait_path"),
            background_generation_prompt=raw_assets.get("background_generation_prompt"),
            active_speaker_generation_prompt=raw_assets.get("active_speaker_generation_prompt"),
            visible_character_portraits=[
                CharacterVisualState(
                    character_name=item.get("character_name", ""),
                    portrait_hint=item.get("portrait_hint", "default"),
                    portrait_asset_path=item.get("portrait_asset_path"),
                    generation_prompt=item.get("generation_prompt"),
                )
                for item in raw_assets.get("visible_character_portraits", [])
            ],
        ),
        state=SessionState(
            metrics=raw_state.get("metrics", {}),
            tags=raw_state.get("tags", []),
            phase=raw_state.get("phase", "idle"),
        ),
    )


def row_to_save(row: sqlite3.Row) -> SaveSummary:
    return SaveSummary(
        id=row["id"],
        session_id=row["session_id"],
        title=row["title"],
        world_name=row["world_name"],
        updated_at=row["updated_at"],
        progress=row["progress"],
        summary=row["summary"],
        player_character_name=row["player_character_name"] if "player_character_name" in row.keys() else None,
        parent_save_id=row["parent_save_id"] if "parent_save_id" in row.keys() else None,
        branch_root_save_id=row["branch_root_save_id"] if "branch_root_save_id" in row.keys() else None,
        branch_label=row["branch_label"] if "branch_label" in row.keys() else None,
    )


def row_to_memory(row: sqlite3.Row) -> MemoryEntry:
    return MemoryEntry(
        id=row["id"],
        world_id=row["world_id"] if "world_id" in row.keys() else "",
        session_id=row["session_id"],
        turn_index=int(row["turn_index"]) if "turn_index" in row.keys() else 0,
        conversation_id=row["conversation_id"] if "conversation_id" in row.keys() else row["session_id"],
        character_id=row["character_id"],
        event_id=row["event_id"] if "event_id" in row.keys() else None,
        item_id=row["item_id"] if "item_id" in row.keys() else None,
        scene_id=row["scene_id"] if "scene_id" in row.keys() else None,
        layer=row["layer"],
        content=row["content"],
        source=row["source"],
        importance=float(row["importance"]),
        created_at=row["created_at"],
        memory_type=row["memory_type"] if "memory_type" in row.keys() else "dialogue",
        speaker=row["speaker"] if "speaker" in row.keys() else None,
        role=row["role"] if "role" in row.keys() else None,
        location=row["location"] if "location" in row.keys() else None,
        participants=json.loads(row["participants_json"]) if "participants_json" in row.keys() else [],
        keywords=json.loads(row["keywords_json"]) if "keywords_json" in row.keys() else [],
    )


def row_to_agent_session(row: sqlite3.Row) -> AgentSession:
    return AgentSession(
        id=row["id"],
        session_id=row["session_id"],
        agent_type=row["agent_type"],
        character_id=row["character_id"] if "character_id" in row.keys() else None,
        character_name=row["character_name"] if "character_name" in row.keys() else None,
        status=row["status"],
        connection_state=row["connection_state"],
        scene_presence_state=row["scene_presence_state"],
        checkpoint_id=row["checkpoint_id"] if "checkpoint_id" in row.keys() else None,
        last_active_turn=int(row["last_active_turn"]) if "last_active_turn" in row.keys() else 0,
        last_ack_message_index=int(row["last_ack_message_index"]) if "last_ack_message_index" in row.keys() else 0,
        prompt_version=row["prompt_version"] if "prompt_version" in row.keys() else "v1",
        runtime_key=row["runtime_key"] if "runtime_key" in row.keys() else None,
        initialized_at=row["initialized_at"] if "initialized_at" in row.keys() else None,
        created_at=row["created_at"] if "created_at" in row.keys() else "",
        updated_at=row["updated_at"] if "updated_at" in row.keys() else "",
    )


def row_to_agent_checkpoint(row: sqlite3.Row) -> AgentCheckpoint:
    return AgentCheckpoint(
        id=row["id"],
        agent_session_id=row["agent_session_id"],
        turn_index=int(row["turn_index"]),
        checkpoint_type=row["checkpoint_type"],
        payload=json.loads(row["payload_json"]) if "payload_json" in row.keys() else {},
        created_at=row["created_at"] if "created_at" in row.keys() else "",
    )


def row_to_turn_journal_entry(row: sqlite3.Row) -> TurnJournalEntry:
    return TurnJournalEntry(
        id=row["id"],
        session_id=row["session_id"],
        turn_index=int(row["turn_index"]),
        step=row["step"],
        status=row["status"],
        payload=json.loads(row["payload_json"]) if "payload_json" in row.keys() else {},
        created_at=row["created_at"] if "created_at" in row.keys() else "",
    )


def row_to_model_config(row: sqlite3.Row) -> ModelConfig:
    return ModelConfig(
        id=row["id"],
        name=row["name"],
        model_type=row["model_type"],
        provider=row["provider"],
        model_id=row["model_id"],
        base_url=row["base_url"],
        api_key=row["api_key"],
        is_default=bool(row["is_default"]),
    )


def _has_column(connection: sqlite3.Connection, table_name: str, column_name: str) -> bool:
    rows = connection.execute(f"PRAGMA table_info({table_name})").fetchall()
    return any(row["name"] == column_name for row in rows)


def _add_column(connection: sqlite3.Connection, table_name: str, column_name: str, column_definition: str) -> None:
    connection.execute(f"ALTER TABLE {table_name} ADD COLUMN {column_name} {column_definition}")
