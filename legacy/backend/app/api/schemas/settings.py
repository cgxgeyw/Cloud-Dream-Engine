from pydantic import BaseModel

from backend.app.domain.models.settings import AppSettingsSnapshot


class SettingsResponse(BaseModel):
    text_model_provider: str
    default_text_model: str
    image_model_provider: str
    default_image_workflow: str
    home_background_strategy: str
    export_directory: str

    @classmethod
    def from_domain(cls, settings: AppSettingsSnapshot) -> "SettingsResponse":
        return cls(
            text_model_provider=settings.text_model_provider,
            default_text_model=settings.default_text_model,
            image_model_provider=settings.image_model_provider,
            default_image_workflow=settings.default_image_workflow,
            home_background_strategy=settings.home_background_strategy,
            export_directory=settings.export_directory,
        )


class SettingsUpdateRequest(BaseModel):
    text_model_provider: str
    default_text_model: str
    image_model_provider: str
    default_image_workflow: str
    home_background_strategy: str
    export_directory: str
