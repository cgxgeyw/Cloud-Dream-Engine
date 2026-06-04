from fastapi import APIRouter, Depends, HTTPException, Query

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.attributes import (
    AttributeSchemaResponse,
    AttributeSchemaUpsertRequest,
    AttributeValueResponse,
    AttributeValueUpsertRequest,
)
from backend.app.core.container import AppContainer
from backend.app.domain.models.attribute import AttributeSchema, AttributeValue

router = APIRouter(prefix="/api/attributes", tags=["attributes"])


@router.get("/schemas", response_model=list[AttributeSchemaResponse])
def list_attribute_schemas(
    scope: str | None = Query(default=None),
    container: AppContainer = Depends(get_app_container),
):
    return [AttributeSchemaResponse.from_domain(item) for item in container.attribute_queries.list_schemas(scope=scope)]


@router.post("/schemas", response_model=AttributeSchemaResponse)
def create_attribute_schema(
    payload: AttributeSchemaUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    created = container.attribute_commands.create_schema(
        AttributeSchema(
            id="new",
            scope=payload.scope,
            key=payload.key,
            label=payload.label,
            value_type=payload.value_type,
            description=payload.description,
            default_value=payload.default_value,
            enum_options=payload.enum_options,
            display_policy=payload.display_policy,
            access_policy=payload.access_policy,
            mutation_policy=payload.mutation_policy,
            influence_policy=payload.influence_policy,
            projection_policy=payload.projection_policy,
        )
    )
    return AttributeSchemaResponse.from_domain(created)


@router.put("/schemas/{schema_id}", response_model=AttributeSchemaResponse)
def update_attribute_schema(
    schema_id: str,
    payload: AttributeSchemaUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    updated = container.attribute_commands.update_schema(
        schema_id,
        AttributeSchema(
            id=schema_id,
            scope=payload.scope,
            key=payload.key,
            label=payload.label,
            value_type=payload.value_type,
            description=payload.description,
            default_value=payload.default_value,
            enum_options=payload.enum_options,
            display_policy=payload.display_policy,
            access_policy=payload.access_policy,
            mutation_policy=payload.mutation_policy,
            influence_policy=payload.influence_policy,
            projection_policy=payload.projection_policy,
        ),
    )
    if updated is None:
        raise HTTPException(status_code=404, detail="Attribute schema not found")
    return AttributeSchemaResponse.from_domain(updated)


@router.get("/values", response_model=list[AttributeValueResponse])
def list_attribute_values(
    owner_type: str | None = Query(default=None),
    owner_id: str | None = Query(default=None),
    schema_id: str | None = Query(default=None),
    container: AppContainer = Depends(get_app_container),
):
    return [
        AttributeValueResponse.from_domain(item)
        for item in container.attribute_queries.list_values(
            owner_type=owner_type,
            owner_id=owner_id,
            schema_id=schema_id,
        )
    ]


@router.put("/values", response_model=AttributeValueResponse)
def upsert_attribute_value(
    payload: AttributeValueUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    value = container.attribute_commands.upsert_value(
        AttributeValue(
            id="new",
            schema_id=payload.schema_id,
            owner_type=payload.owner_type,
            owner_id=payload.owner_id,
            value=payload.value,
            source=payload.source,
        )
    )
    return AttributeValueResponse.from_domain(value)
