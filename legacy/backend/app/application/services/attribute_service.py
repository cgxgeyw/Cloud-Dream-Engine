from backend.app.domain.models.attribute import AttributeSchema, AttributeValue
from backend.app.domain.repositories.attribute import AttributeRepository


class AttributeQueryService:
    def __init__(self, attribute_repository: AttributeRepository) -> None:
        self._attribute_repository = attribute_repository

    def list_schemas(self, scope: str | None = None):
        return self._attribute_repository.list_schemas(scope=scope)

    def get_schema(self, schema_id: str):
        return self._attribute_repository.get_schema(schema_id)

    def list_values(
        self,
        owner_type: str | None = None,
        owner_id: str | None = None,
        schema_id: str | None = None,
    ):
        return self._attribute_repository.list_values(
            owner_type=owner_type,
            owner_id=owner_id,
            schema_id=schema_id,
        )


class AttributeCommandService:
    def __init__(self, attribute_repository: AttributeRepository) -> None:
        self._attribute_repository = attribute_repository

    def create_schema(self, schema: AttributeSchema):
        return self._attribute_repository.create_schema(schema)

    def update_schema(self, schema_id: str, schema: AttributeSchema):
        return self._attribute_repository.update_schema(schema_id, schema)

    def upsert_value(self, value: AttributeValue):
        return self._attribute_repository.upsert_value(value)

    def project_session_attributes(self, session_id: str, world_id: str, character_ids: list[str]):
        return self._attribute_repository.project_session_attributes(
            session_id=session_id,
            world_id=world_id,
            character_ids=character_ids,
        )

    def apply_player_action_effects(self, session_id: str, content: str):
        return self._attribute_repository.apply_player_action_effects(session_id=session_id, content=content)
