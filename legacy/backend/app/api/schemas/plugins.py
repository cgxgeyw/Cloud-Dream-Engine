from pydantic import BaseModel

from backend.app.domain.models.plugin import PluginDefinition


class PluginResponse(BaseModel):
    id: str
    name: str
    enabled: bool
    description: str
    hooks: list[str]

    @classmethod
    def from_domain(cls, plugin: PluginDefinition) -> "PluginResponse":
        return cls(
            id=plugin.id,
            name=plugin.name,
            enabled=plugin.enabled,
            description=plugin.description,
            hooks=plugin.hooks,
        )
