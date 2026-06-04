from backend.app.domain.repositories.catalog import CatalogRepository


class CatalogQueryService:
    def __init__(self, catalog_repository: CatalogRepository) -> None:
        self._catalog_repository = catalog_repository

    def list_worlds(self):
        return self._catalog_repository.list_worlds()

    def get_world(self, world_id: str):
        return self._catalog_repository.get_world(world_id)

    def list_characters(self):
        return self._catalog_repository.list_characters()

    def list_characters_for_world(self, world_id: str):
        return self._catalog_repository.list_characters_for_world(world_id)

    def get_character(self, character_id: str):
        return self._catalog_repository.get_character(character_id)

    def get_settings(self):
        return self._catalog_repository.get_settings()

    def list_plugins(self):
        return self._catalog_repository.list_plugins()

    def list_models(self):
        return self._catalog_repository.list_models()

    def get_model(self, model_id: str):
        return self._catalog_repository.get_model(model_id)


class CatalogCommandService:
    def __init__(self, catalog_repository: CatalogRepository) -> None:
        self._catalog_repository = catalog_repository

    def create_world(self, world):
        return self._catalog_repository.create_world(world)

    def update_world(self, world_id: str, world):
        return self._catalog_repository.update_world(world_id, world)

    def duplicate_world(self, world_id: str):
        return self._catalog_repository.duplicate_world(world_id)

    def create_character(self, character):
        return self._catalog_repository.create_character(character)

    def update_character(self, character_id: str, character):
        return self._catalog_repository.update_character(character_id, character)

    def delete_world(self, world_id: str) -> bool:
        return self._catalog_repository.delete_world(world_id)

    def delete_all_worlds(self) -> int:
        return self._catalog_repository.delete_all_worlds()

    def delete_character(self, character_id: str) -> bool:
        return self._catalog_repository.delete_character(character_id)

    def update_settings(self, settings):
        return self._catalog_repository.update_settings(settings)

    def create_model(self, model):
        return self._catalog_repository.create_model(model)

    def update_model(self, model_id: str, model):
        return self._catalog_repository.update_model(model_id, model)

    def delete_model(self, model_id: str) -> bool:
        return self._catalog_repository.delete_model(model_id)

    def set_default_model(self, model_id: str, model_type: str) -> None:
        return self._catalog_repository.set_default_model(model_id, model_type)
