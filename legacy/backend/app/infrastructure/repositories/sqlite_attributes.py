import uuid

from backend.app.domain.models.attribute import AttributeSchema, AttributeValue
from backend.app.infrastructure.sqlite_store import (
    SqliteStore,
    row_to_attribute_schema,
    row_to_attribute_value,
)


class SqliteAttributeRepository:
    def __init__(self, store: SqliteStore) -> None:
        self._store = store

    def list_schemas(self, scope: str | None = None) -> list[AttributeSchema]:
        with self._store.connect() as connection:
            if scope:
                rows = connection.execute("SELECT * FROM attribute_schemas WHERE scope = ? ORDER BY label", (scope,)).fetchall()
            else:
                rows = connection.execute("SELECT * FROM attribute_schemas ORDER BY scope, label").fetchall()
        return [row_to_attribute_schema(row) for row in rows]

    def get_schema(self, schema_id: str) -> AttributeSchema | None:
        with self._store.connect() as connection:
            row = connection.execute("SELECT * FROM attribute_schemas WHERE id = ?", (schema_id,)).fetchone()
        return row_to_attribute_schema(row) if row else None

    def create_schema(self, schema: AttributeSchema) -> AttributeSchema:
        created = AttributeSchema(
            id=schema.id if schema.id and schema.id != "new" else f"attr-{uuid.uuid4().hex[:8]}",
            scope=schema.scope,
            key=schema.key,
            label=schema.label,
            value_type=schema.value_type,
            description=schema.description,
            default_value=schema.default_value,
            enum_options=schema.enum_options,
            display_policy=schema.display_policy,
            access_policy=schema.access_policy,
            mutation_policy=schema.mutation_policy,
            influence_policy=schema.influence_policy,
            projection_policy=schema.projection_policy,
        )
        with self._store.connect() as connection:
            self._store.insert_attribute_schema(connection, created)
        return created

    def update_schema(self, schema_id: str, schema: AttributeSchema) -> AttributeSchema | None:
        if self.get_schema(schema_id) is None:
            return None
        updated = AttributeSchema(
            id=schema_id,
            scope=schema.scope,
            key=schema.key,
            label=schema.label,
            value_type=schema.value_type,
            description=schema.description,
            default_value=schema.default_value,
            enum_options=schema.enum_options,
            display_policy=schema.display_policy,
            access_policy=schema.access_policy,
            mutation_policy=schema.mutation_policy,
            influence_policy=schema.influence_policy,
            projection_policy=schema.projection_policy,
        )
        with self._store.connect() as connection:
            self._store.upsert_attribute_schema(connection, updated)
        return updated

    def list_values(
        self,
        owner_type: str | None = None,
        owner_id: str | None = None,
        schema_id: str | None = None,
    ) -> list[AttributeValue]:
        clauses = []
        params: list[str] = []

        if owner_type:
            clauses.append("owner_type = ?")
            params.append(owner_type)
        if owner_id:
            clauses.append("owner_id = ?")
            params.append(owner_id)
        if schema_id:
            clauses.append("schema_id = ?")
            params.append(schema_id)

        query = "SELECT * FROM attribute_values"
        if clauses:
            query += " WHERE " + " AND ".join(clauses)
        query += " ORDER BY owner_type, owner_id, schema_id"

        with self._store.connect() as connection:
            rows = connection.execute(query, tuple(params)).fetchall()
        return [row_to_attribute_value(row) for row in rows]

    def upsert_value(self, value: AttributeValue) -> AttributeValue:
        created = AttributeValue(
            id=value.id if value.id and value.id != "new" else f"attrval-{uuid.uuid4().hex[:8]}",
            schema_id=value.schema_id,
            owner_type=value.owner_type,
            owner_id=value.owner_id,
            value=value.value,
            source=value.source,
        )
        with self._store.connect() as connection:
            self._store.upsert_attribute_value(connection, created)
        return created

    def project_session_attributes(
        self,
        session_id: str,
        world_id: str,
        character_ids: list[str],
    ) -> list[AttributeValue]:
        projected: list[AttributeValue] = []
        schemas = self.list_schemas()

        for schema in schemas:
            if not schema.projection_policy.get("inherit_to_session", False):
                continue

            if schema.scope == "world":
                owner_type = "world"
                owner_id = world_id
                runtime_owner_type = schema.projection_policy.get("session_owner_type", "session")
                runtime_owner_id = session_id
                projected.append(
                    self.upsert_value(
                        AttributeValue(
                            id="new",
                            schema_id=schema.id,
                            owner_type=runtime_owner_type,
                            owner_id=runtime_owner_id,
                            value=self._resolve_source_value(schema_id=schema.id, owner_type=owner_type, owner_id=owner_id, default_value=schema.default_value),
                            source="projection",
                        )
                    )
                )

            if schema.scope == "character":
                runtime_owner_type = schema.projection_policy.get("session_owner_type", "session_character")
                for character_id in character_ids:
                    projected.append(
                        self.upsert_value(
                            AttributeValue(
                                id="new",
                                schema_id=schema.id,
                                owner_type=runtime_owner_type,
                                owner_id=f"{session_id}:{character_id}",
                                value=self._resolve_source_value(
                                    schema_id=schema.id,
                                    owner_type="character",
                                    owner_id=character_id,
                                    default_value=schema.default_value,
                                ),
                                source="projection",
                            )
                        )
                    )

            if schema.scope == "session":
                runtime_owner_type = schema.projection_policy.get("session_owner_type", "session")
                projected.append(
                    self.upsert_value(
                        AttributeValue(
                            id="new",
                            schema_id=schema.id,
                            owner_type=runtime_owner_type,
                            owner_id=session_id,
                            value=schema.default_value,
                            source="projection",
                        )
                    )
                )

        return projected

    def apply_player_action_effects(self, session_id: str, content: str) -> list[AttributeValue]:
        updated_values: list[AttributeValue] = []
        session_values = self.list_values(owner_type="session", owner_id=session_id)
        session_character_values = [
            value
            for value in self.list_values(owner_type="session_character")
            if value.owner_id.startswith(f"{session_id}:")
        ]
        schemas = {schema.id: schema for schema in self.list_schemas()}

        for value in [*session_values, *session_character_values]:
            schema = schemas.get(value.schema_id)
            if schema is None:
                continue

            if not schema.mutation_policy.get("player_action_write", False):
                continue

            allowed_ops = schema.mutation_policy.get("allowed_ops", [])
            if schema.value_type == "number" and "increment" in allowed_ops and isinstance(value.value, (int, float)):
                updated_values.append(
                    self.upsert_value(
                        AttributeValue(
                            id=value.id,
                            schema_id=value.schema_id,
                            owner_type=value.owner_type,
                            owner_id=value.owner_id,
                            value=value.value + 1,
                            source="player_action",
                        )
                    )
                )
            elif schema.value_type == "string" and "set" in allowed_ops:
                updated_values.append(
                    self.upsert_value(
                        AttributeValue(
                            id=value.id,
                            schema_id=value.schema_id,
                            owner_type=value.owner_type,
                            owner_id=value.owner_id,
                            value=content,
                            source="player_action",
                        )
                    )
                )

        return updated_values

    def _resolve_source_value(self, schema_id: str, owner_type: str, owner_id: str, default_value):
        existing = next(
            (
                value
                for value in self.list_values(owner_type=owner_type, owner_id=owner_id, schema_id=schema_id)
            ),
            None,
        )
        return existing.value if existing else default_value
