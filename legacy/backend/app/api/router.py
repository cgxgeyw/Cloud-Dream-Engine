from fastapi import APIRouter

from backend.app.api.routes.assets import router as assets_router
from backend.app.api.routes.attributes import router as attributes_router
from backend.app.api.routes.characters import router as characters_router
from backend.app.api.routes.debug import router as debug_router
from backend.app.api.routes.health import router as health_router
from backend.app.api.routes.memories import router as memories_router
from backend.app.api.routes.mcp_tools import router as mcp_tools_router
from backend.app.api.routes.model_configs import router as model_configs_router
from backend.app.api.routes.plugins import router as plugins_router
from backend.app.api.routes.saves import router as saves_router
from backend.app.api.routes.sessions import router as sessions_router
from backend.app.api.routes.settings import router as settings_router
from backend.app.api.routes.uploads import router as uploads_router
from backend.app.api.routes.worlds import router as worlds_router
from backend.app.api.ws.sessions import router as session_ws_router

api_router = APIRouter()
api_router.include_router(health_router)
api_router.include_router(assets_router)
api_router.include_router(attributes_router)
api_router.include_router(debug_router)
api_router.include_router(memories_router)
api_router.include_router(mcp_tools_router)
api_router.include_router(model_configs_router)
api_router.include_router(uploads_router)
api_router.include_router(worlds_router)
api_router.include_router(characters_router)
api_router.include_router(saves_router)
api_router.include_router(sessions_router)
api_router.include_router(settings_router)
api_router.include_router(plugins_router)
api_router.include_router(session_ws_router)
