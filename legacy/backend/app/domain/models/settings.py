from dataclasses import dataclass


@dataclass(frozen=True)
class AppSettingsSnapshot:
    text_model_provider: str
    default_text_model: str
    image_model_provider: str
    default_image_workflow: str
    home_background_strategy: str
    export_directory: str
