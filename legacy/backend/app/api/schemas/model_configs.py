from pydantic import BaseModel, Field


class ModelConfigResponse(BaseModel):
    id: str
    name: str
    model_type: str
    provider: str
    model_id: str
    base_url: str
    api_key: str
    is_default: bool

    @classmethod
    def from_domain(cls, m) -> "ModelConfigResponse":
        return cls(
            id=m.id,
            name=m.name,
            model_type=m.model_type,
            provider=m.provider,
            model_id=m.model_id,
            base_url=m.base_url,
            api_key=m.api_key,
            is_default=m.is_default,
        )


class ModelConfigUpsertRequest(BaseModel):
    name: str = Field(min_length=1)
    model_type: str = Field(min_length=1)
    provider: str = ""
    model_id: str = ""
    base_url: str = ""
    api_key: str = ""
    is_default: bool = False


class ModelConfigTestResponse(BaseModel):
    ok: bool
    detail: str
    debug_lines: list[str]


class ImageModelTestRequest(BaseModel):
    prompt: str = Field(min_length=1)


class ImageModelTestResponse(BaseModel):
    ok: bool
    detail: str
    debug_lines: list[str]
    asset_path: str | None = None
    image_url: str | None = None
    seed: int | None = None


class ModelEndpointDiscoveryRequest(BaseModel):
    provider: str = ""
    base_url: str = ""
    api_key: str = ""


class ModelEndpointDiscoveryResponse(BaseModel):
    ok: bool
    detail: str
    model_ids: list[str]
    debug_lines: list[str]
