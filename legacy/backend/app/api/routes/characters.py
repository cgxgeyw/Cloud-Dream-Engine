from fastapi import APIRouter, Depends, HTTPException

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.characters import CharacterResponse
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/characters", tags=["characters"])


def _to_character_response(container: AppContainer, character) -> CharacterResponse:
    return CharacterResponse.from_domain(
        character,
        runtime_system_prompt=container.dialogue_pipeline.build_character_system_prompt(
            speaker=character.name,
            speaker_profile=character,
        ),
    )


@router.get("", response_model=list[CharacterResponse])
def list_characters(container: AppContainer = Depends(get_app_container)):
    return [_to_character_response(container, item) for item in container.catalog_queries.list_characters()]


@router.get("/{character_id}", response_model=CharacterResponse)
def get_character(character_id: str, container: AppContainer = Depends(get_app_container)):
    character = container.catalog_queries.get_character(character_id)
    if character is None:
        raise HTTPException(status_code=404, detail="Character not found")
    return _to_character_response(container, character)
