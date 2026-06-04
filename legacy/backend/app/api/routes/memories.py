from fastapi import APIRouter, Depends, Query

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.memories import MemoryEntryResponse
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/memories", tags=["memories"])


@router.get("", response_model=list[MemoryEntryResponse])
def list_memories(
    world_id: str = Query(...),
    character_id: str = Query(...),
    session_id: str | None = Query(default=None),
    conversation_id: str | None = Query(default=None),
    scene_id: str | None = Query(default=None),
    event_id: str | None = Query(default=None),
    item_id: str | None = Query(default=None),
    limit: int = Query(default=8, ge=1, le=50),
    container: AppContainer = Depends(get_app_container),
):
    memories = container.memory_queries.list_for_character(
        world_id=world_id,
        character_id=character_id,
        session_id=session_id,
        conversation_id=conversation_id,
        scene_id=scene_id,
        event_id=event_id,
        item_id=item_id,
        limit=limit,
    )
    return [MemoryEntryResponse.from_domain(item) for item in memories]
