from fastapi import APIRouter, Depends, HTTPException

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.debug import SessionDebugResponse
from backend.app.application.services.debug_service import DebugReadService
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/debug", tags=["debug"])


@router.get("/sessions/{session_id}", response_model=SessionDebugResponse)
def get_session_debug(session_id: str, container: AppContainer = Depends(get_app_container)):
    debug_service = DebugReadService(
        session_queries=container.session_queries,
        catalog_queries=container.catalog_queries,
        attribute_runtime=container.attribute_runtime,
        memory_queries=container.memory_queries,
        agent_runtime_manager=container.agent_runtime_manager,
    )
    debug_model = debug_service.get_session_debug(session_id)
    if debug_model is None:
        raise HTTPException(status_code=404, detail="Session not found")
    return SessionDebugResponse.from_domain(debug_model)
