from pydantic import BaseModel

from backend.app.domain.models.save import SaveSummary


class SaveResponse(BaseModel):
    id: str
    session_id: str
    title: str
    world_name: str
    updated_at: str
    progress: str
    summary: str
    player_character_name: str | None = None
    parent_save_id: str | None = None
    branch_root_save_id: str | None = None
    branch_label: str | None = None

    @classmethod
    def from_domain(cls, save: SaveSummary) -> "SaveResponse":
        return cls(
            id=save.id,
            session_id=save.session_id,
            title=save.title,
            world_name=save.world_name,
            updated_at=save.updated_at,
            progress=save.progress,
            summary=save.summary,
            player_character_name=save.player_character_name,
            parent_save_id=save.parent_save_id,
            branch_root_save_id=save.branch_root_save_id,
            branch_label=save.branch_label,
        )
