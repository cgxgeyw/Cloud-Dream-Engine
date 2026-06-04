import os

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse

from backend.app.core.config import Settings

router = APIRouter(tags=["assets"])


@router.get("/assets/{filename:path}")
def serve_asset(filename: str):
    settings = Settings()
    assets_dir = os.path.join(os.path.dirname(settings.database_path), "assets")
    file_path = os.path.join(assets_dir, filename)

    if not os.path.exists(file_path) or not os.path.isfile(file_path):
        raise HTTPException(status_code=404, detail="Asset not found")

    return FileResponse(file_path)
