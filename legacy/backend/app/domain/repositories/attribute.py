from typing import Protocol

from backend.app.domain.models.attribute import AttributeSchema, AttributeValue


class AttributeRepository(Protocol):
    def list_schemas(self, scope: str | None = None) -> list[AttributeSchema]: ...

    def get_schema(self, schema_id: str) -> AttributeSchema | None: ...

    def create_schema(self, schema: AttributeSchema) -> AttributeSchema: ...

    def update_schema(self, schema_id: str, schema: AttributeSchema) -> AttributeSchema | None: ...

    def list_values(
        self,
        owner_type: str | None = None,
        owner_id: str | None = None,
        schema_id: str | None = None,
    ) -> list[AttributeValue]: ...

    def upsert_value(self, value: AttributeValue) -> AttributeValue: ...

    def project_session_attributes(
        self,
        session_id: str,
        world_id: str,
        character_ids: list[str],
    ) -> list[AttributeValue]: ...

    def apply_player_action_effects(self, session_id: str, content: str) -> list[AttributeValue]: ...
