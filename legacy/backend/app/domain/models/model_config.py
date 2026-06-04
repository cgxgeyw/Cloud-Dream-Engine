from dataclasses import dataclass, field


@dataclass(frozen=True)
class ModelConfig:
    id: str
    name: str
    model_type: str  # "text" | "image"
    provider: str
    model_id: str
    base_url: str
    api_key: str
    is_default: bool = False
