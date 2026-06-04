from dataclasses import dataclass, field

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.memory_runtime_models import MemoryEventContext
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.session import SessionSnapshot


@dataclass(frozen=True)
class TriggerAttributeUpdate:
    owner_type: str
    owner_id: str
    schema_key: str
    value: object
    source: str = "trigger"


@dataclass(frozen=True)
class TriggerEvaluation:
    system_messages: list[str] = field(default_factory=list)
    attribute_updates: list[TriggerAttributeUpdate] = field(default_factory=list)
    memory_events: list[MemoryEventContext] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class TriggerEngineService:
    def evaluate_turn(
        self,
        session: SessionSnapshot,
        player_input: str,
        director_decision: DirectorDecision,
        session_attributes: list[RuntimeAttributeItem],
    ) -> TriggerEvaluation:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        world_tension = self._as_number(attr_map.get("world_tension"), 0)
        weather_state = str(attr_map.get("weather_state", "clear"))

        system_messages: list[str] = []
        debug_lines: list[str] = []
        attribute_updates: list[TriggerAttributeUpdate] = []
        memory_events: list[MemoryEventContext] = []

        if director_decision.next_location and director_decision.next_location != session.location:
            message = f"触发器：已进入 {director_decision.next_location}"
            system_messages.append(message)
            attribute_updates.append(
                TriggerAttributeUpdate(
                    owner_type="session",
                    owner_id=session.id,
                    schema_key="active_objective",
                    value=f"调查 {director_decision.next_location}",
                )
            )
            memory_events.append(
                MemoryEventContext(
                    event_id="trigger:scene_enter",
                    content=message,
                    source="trigger_engine",
                    importance=0.42,
                    location=director_decision.next_location,
                )
            )
            debug_lines.append(f"TriggerEngine scene_enter -> {director_decision.next_location}")

        if director_decision.generated_characters:
            debug_lines.append(
                "TriggerEngine generated_character -> "
                + ", ".join(item.name for item in director_decision.generated_characters)
            )

        if world_tension >= 50 and weather_state == "storm":
            message = "触发器：封锁等级提升，场景压力继续上升。"
            system_messages.append(message)
            attribute_updates.append(
                TriggerAttributeUpdate(
                    owner_type="session",
                    owner_id=session.id,
                    schema_key="active_objective",
                    value="尽快找到可通行的安全路线",
                )
            )
            memory_events.append(
                MemoryEventContext(
                    event_id="trigger:storm_tension_threshold",
                    content=message,
                    source="trigger_engine",
                    importance=0.58,
                    location=director_decision.next_location or session.location,
                )
            )
            debug_lines.append("TriggerEngine threshold -> world_tension >= 50 and weather=storm")

        if "\u89c2\u5bdf" in player_input and session.location == director_decision.next_location:
            message = "触发器：观察行为命中，当前场景出现新的细节线索。"
            system_messages.append(message)
            memory_events.append(
                MemoryEventContext(
                    event_id="trigger:observe_hit",
                    content=message,
                    source="trigger_engine",
                    importance=0.48,
                    location=director_decision.next_location or session.location,
                )
            )
            debug_lines.append("TriggerEngine keyword -> observe")

        return TriggerEvaluation(
            system_messages=system_messages,
            attribute_updates=attribute_updates,
            memory_events=memory_events,
            debug_lines=debug_lines,
        )

    def _as_number(self, value: object, fallback: float) -> float:
        if isinstance(value, (int, float)):
            return float(value)
        return fallback
