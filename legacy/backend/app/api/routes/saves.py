from fastapi import APIRouter, Depends, HTTPException

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.saves import SaveResponse
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/saves", tags=["saves"])


@router.get("", response_model=list[SaveResponse])
def list_saves(container: AppContainer = Depends(get_app_container)):
    return [SaveResponse.from_domain(item) for item in container.session_queries.list_saves()]


@router.post("/{save_id}/branch", response_model=SaveResponse)
def branch_save(save_id: str, container: AppContainer = Depends(get_app_container)):
    branched = container.session_commands.branch_save(save_id)
    if branched is None:
        raise HTTPException(status_code=404, detail="Save not found")
    return SaveResponse.from_domain(branched)


@router.delete("/{save_id}")
def delete_save(save_id: str, container: AppContainer = Depends(get_app_container)):
    deleted = container.session_commands.delete_save(save_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="Save not found")
    return {"ok": True}


@router.delete("")
def delete_all_saves(container: AppContainer = Depends(get_app_container)):
    deleted_count = container.session_commands.delete_all_saves()
    return {"ok": True, "deleted_count": deleted_count}
