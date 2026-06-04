from fastapi import APIRouter, Depends

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.plugins import PluginResponse
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/plugins", tags=["plugins"])


@router.get("", response_model=list[PluginResponse])
def list_plugins(container: AppContainer = Depends(get_app_container)):
    return [PluginResponse.from_domain(item) for item in container.catalog_queries.list_plugins()]
