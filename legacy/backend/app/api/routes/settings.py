from fastapi import APIRouter, Depends

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.settings import SettingsResponse, SettingsUpdateRequest
from backend.app.core.container import AppContainer
from backend.app.domain.models.settings import AppSettingsSnapshot

router = APIRouter(prefix="/api/settings", tags=["settings"])


@router.get("", response_model=SettingsResponse)
def get_settings(container: AppContainer = Depends(get_app_container)):
    return SettingsResponse.from_domain(container.catalog_queries.get_settings())


@router.put("", response_model=SettingsResponse)
def update_settings(payload: SettingsUpdateRequest, container: AppContainer = Depends(get_app_container)):
    updated = container.catalog_commands.update_settings(
        AppSettingsSnapshot(
            text_model_provider=payload.text_model_provider,
            default_text_model=payload.default_text_model,
            image_model_provider=payload.image_model_provider,
            default_image_workflow=payload.default_image_workflow,
            home_background_strategy=payload.home_background_strategy,
            export_directory=payload.export_directory,
        )
    )
    return SettingsResponse.from_domain(updated)
