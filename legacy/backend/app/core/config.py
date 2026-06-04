import os
from pathlib import Path

from pydantic import BaseModel


class Settings(BaseModel):
    app_name: str = "Dream Narrative Engine Backend"
    app_version: str = "0.1.0"
    debug: bool = True
    database_path: str = os.getenv("DNE_DATABASE_PATH") or str(
        Path(os.getenv("DNE_DATA_DIR", "data")) / "app.db"
    )
