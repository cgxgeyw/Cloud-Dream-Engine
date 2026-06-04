from pydantic import BaseModel, Field

from backend.app.domain.models.attribute import AttributeSchema, AttributeValue


class AttributeSchemaResponse(BaseModel):
    id: str
    scope: str
    key: str
    label: str
    value_type: str
    description: str
    default_value: object | None
    enum_options: list[str]
    display_policy: dict[str, object]
    access_policy: dict[str, object]
    mutation_policy: dict[str, object]
    influence_policy: dict[str, object]
    projection_policy: dict[str, object]

    @classmethod
    def from_domain(cls, schema: AttributeSchema) -> "AttributeSchemaResponse":
        return cls(
            id=schema.id,
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


class AttributeSchemaUpsertRequest(BaseModel):
    scope: str = Field(min_length=1)
    key: str = Field(min_length=1)
    label: str = Field(min_length=1)
    value_type: str = Field(min_length=1)
    description: str = ""
    default_value: object | None = None
    enum_options: list[str] = Field(default_factory=list)
    display_policy: dict[str, object] = Field(default_factory=dict)
    access_policy: dict[str, object] = Field(default_factory=dict)
    mutation_policy: dict[str, object] = Field(default_factory=dict)
    influence_policy: dict[str, object] = Field(default_factory=dict)
    projection_policy: dict[str, object] = Field(default_factory=dict)


class AttributeValueResponse(BaseModel):
    id: str
    schema_id: str
    owner_type: str
    owner_id: str
    value: object | None
    source: str

    @classmethod
    def from_domain(cls, value: AttributeValue) -> "AttributeValueResponse":
        return cls(
            id=value.id,
            schema_id=value.schema_id,
            owner_type=value.owner_type,
            owner_id=value.owner_id,
            value=value.value,
            source=value.source,
        )


class AttributeValueUpsertRequest(BaseModel):
    schema_id: str = Field(min_length=1)
    owner_type: str = Field(min_length=1)
    owner_id: str = Field(min_length=1)
    value: object | None = None
    source: str = "manual"


class RuntimeAttributeItemResponse(BaseModel):
    schema_id: str
    key: str
    label: str
    value_type: str
    value: object | None
    source: str
    display_policy: dict[str, object]
    influence_policy: dict[str, object]


class RuntimeAttributeGroupResponse(BaseModel):
    owner_type: str
    owner_id: str
    owner_label: str
    items: list[RuntimeAttributeItemResponse]


class SessionRuntimeAttributesResponse(BaseModel):
    session_id: str
    session_attributes: list[RuntimeAttributeItemResponse]
    character_attributes: list[RuntimeAttributeGroupResponse]
