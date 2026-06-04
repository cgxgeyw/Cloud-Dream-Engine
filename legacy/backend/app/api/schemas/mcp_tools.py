from pydantic import BaseModel, Field


class McpToolResponse(BaseModel):
    id: str
    name: str
    description: str
    server_name: str
    tool_name: str
    enabled: bool
    exposure_policy: str
    risk_level: str
    trigger_keywords: list[str]


class McpToolUpsertRequest(BaseModel):
    name: str = Field(min_length=1)
    description: str = ""
    server_name: str = Field(min_length=1)
    tool_name: str = Field(min_length=1)
    enabled: bool = True
    exposure_policy: str = "on-demand"
    risk_level: str = "low"
    trigger_keywords: list[str] = Field(default_factory=list)
