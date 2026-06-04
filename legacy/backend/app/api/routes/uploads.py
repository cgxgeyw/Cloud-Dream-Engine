import os
import shutil
import uuid

from fastapi import APIRouter, HTTPException, UploadFile, File

from backend.app.core.config import Settings

router = APIRouter(prefix="/api/uploads", tags=["uploads"])


def _assets_dir() -> str:
    settings = Settings()
    assets_dir = os.path.join(os.path.dirname(settings.database_path), "assets")
    os.makedirs(assets_dir, exist_ok=True)
    return assets_dir


@router.post("")
async def upload_file(file: UploadFile = File(...)):
    ext = os.path.splitext(file.filename or "file")[1]
    file_id = uuid.uuid4().hex[:12]
    filename = f"{file_id}{ext}"
    file_path = os.path.join(_assets_dir(), filename)

    content = await file.read()
    if len(content) > 50 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="File too large (max 50MB)")

    with open(file_path, "wb") as f:
        f.write(content)

    return {"filename": filename, "url": f"/assets/{filename}"}


@router.delete("/{filename}")
def delete_uploaded_file(filename: str):
    file_path = os.path.join(_assets_dir(), filename)
    if os.path.exists(file_path):
        os.remove(file_path)
    return {"ok": True}
