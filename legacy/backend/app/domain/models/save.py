from dataclasses import dataclass


@dataclass(frozen=True)
class SaveSummary:
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
