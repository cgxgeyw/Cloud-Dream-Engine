from fastapi import APIRouter, Depends, HTTPException

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.model_configs import (
    ImageModelTestRequest,
    ImageModelTestResponse,
    ModelConfigResponse,
    ModelConfigTestResponse,
    ModelConfigUpsertRequest,
    ModelEndpointDiscoveryRequest,
    ModelEndpointDiscoveryResponse,
)
from backend.app.application.services.image_generation_service import ImageGenerationService
from backend.app.application.services.text_generation_service import TextGenerationService
from backend.app.core.config import Settings
from backend.app.core.container import AppContainer
from backend.app.domain.models.model_config import ModelConfig
from backend.app.domain.models.settings import AppSettingsSnapshot

router = APIRouter(prefix="/api/models", tags=["models"])


@router.get("", response_model=list[ModelConfigResponse])
def list_models(
    model_type: str | None = None,
    container: AppContainer = Depends(get_app_container),
):
    items = container.catalog_queries.list_models()
    if model_type:
        items = [m for m in items if m.model_type == model_type]
    return [ModelConfigResponse.from_domain(item) for item in items]


@router.get("/{model_id}", response_model=ModelConfigResponse)
def get_model(model_id: str, container: AppContainer = Depends(get_app_container)):
    model = container.catalog_queries.get_model(model_id)
    if model is None:
        raise HTTPException(status_code=404, detail="Model not found")
    return ModelConfigResponse.from_domain(model)


@router.post("", response_model=ModelConfigResponse)
def create_model(payload: ModelConfigUpsertRequest, container: AppContainer = Depends(get_app_container)):
    created = container.catalog_commands.create_model(
        ModelConfig(
            id="new",
            name=payload.name,
            model_type=payload.model_type,
            provider=payload.provider,
            model_id=payload.model_id,
            base_url=payload.base_url,
            api_key=payload.api_key,
            is_default=payload.is_default,
        )
    )
    return ModelConfigResponse.from_domain(created)


@router.put("/{model_id}", response_model=ModelConfigResponse)
def update_model(
    model_id: str,
    payload: ModelConfigUpsertRequest,
    container: AppContainer = Depends(get_app_container),
):
    updated = container.catalog_commands.update_model(
        model_id,
        ModelConfig(
            id=model_id,
            name=payload.name,
            model_type=payload.model_type,
            provider=payload.provider,
            model_id=payload.model_id,
            base_url=payload.base_url,
            api_key=payload.api_key,
            is_default=payload.is_default,
        ),
    )
    if updated is None:
        raise HTTPException(status_code=404, detail="Model not found")
    return ModelConfigResponse.from_domain(updated)


@router.delete("/{model_id}")
def delete_model(model_id: str, container: AppContainer = Depends(get_app_container)):
    deleted = container.catalog_commands.delete_model(model_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="Model not found")
    return {"ok": True}


@router.post("/{model_id}/set-default")
def set_default_model(model_id: str, container: AppContainer = Depends(get_app_container)):
    model = container.catalog_queries.get_model(model_id)
    if model is None:
        raise HTTPException(status_code=404, detail="Model not found")
    container.catalog_commands.set_default_model(model_id, model.model_type)
    if model.model_type == "text":
        settings = container.catalog_queries.get_settings()
        container.catalog_commands.update_settings(
            AppSettingsSnapshot(
                text_model_provider=model.provider or settings.text_model_provider,
                default_text_model=model.model_id,
                image_model_provider=settings.image_model_provider,
                default_image_workflow=settings.default_image_workflow,
                home_background_strategy=settings.home_background_strategy,
                export_directory=settings.export_directory,
            )
        )
    elif model.model_type == "image":
        settings = container.catalog_queries.get_settings()
        container.catalog_commands.update_settings(
            AppSettingsSnapshot(
                text_model_provider=settings.text_model_provider,
                default_text_model=settings.default_text_model,
                image_model_provider=model.provider or settings.image_model_provider,
                default_image_workflow=model.model_id,
                home_background_strategy=settings.home_background_strategy,
                export_directory=settings.export_directory,
            )
        )
    return {"ok": True}


@router.post("/{model_id}/test", response_model=ModelConfigTestResponse)
def test_model(model_id: str, container: AppContainer = Depends(get_app_container)):
    model = container.catalog_queries.get_model(model_id)
    if model is None:
        raise HTTPException(status_code=404, detail="Model not found")
    if model.model_type != "text":
        return ModelConfigTestResponse(
            ok=False,
            detail="当前页面只支持测试文本模型。",
            debug_lines=[f"ModelConfigTest unsupported_model_type type={model.model_type}"],
        )

    result = TextGenerationService(catalog_queries=container.catalog_queries).test_connection(preferred_model=model_id)
    return ModelConfigTestResponse(
        ok=result.ok,
        detail=result.detail,
        debug_lines=result.debug_lines,
    )


@router.post("/{model_id}/test-image", response_model=ImageModelTestResponse)
def test_image_model(
    model_id: str,
    payload: ImageModelTestRequest,
    container: AppContainer = Depends(get_app_container),
):
    model = container.catalog_queries.get_model(model_id)
    if model is None:
        raise HTTPException(status_code=404, detail="Model not found")
    if model.model_type != "image":
        return ImageModelTestResponse(
            ok=False,
            detail="Only image models support image test generation.",
            debug_lines=[f"ModelImageTest unsupported_model_type type={model.model_type}"],
        )

    result = ImageGenerationService(
        catalog_queries=container.catalog_queries,
        settings=Settings(),
    ).generate_image(
        prompt=payload.prompt,
        kind="background",
        preferred_model=model_id,
    )

    image_url = result.asset_path
    return ImageModelTestResponse(
        ok=result.asset_path is not None,
        detail=(
            f"Image generated by {result.model.model_id}."
            if result.asset_path and result.model is not None
            else "Image generation failed."
        ),
        debug_lines=result.debug_lines,
        asset_path=result.asset_path,
        image_url=image_url,
        seed=None,
    )


@router.post("/discover", response_model=ModelEndpointDiscoveryResponse)
def discover_models(payload: ModelEndpointDiscoveryRequest, container: AppContainer = Depends(get_app_container)):
    result = TextGenerationService(catalog_queries=container.catalog_queries).discover_models(
        base_url=payload.base_url,
        api_key=payload.api_key,
        provider=payload.provider,
    )
    return ModelEndpointDiscoveryResponse(
        ok=result.ok,
        detail=result.detail,
        model_ids=result.model_ids,
        debug_lines=result.debug_lines,
    )
