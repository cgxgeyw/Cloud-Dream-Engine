from dataclasses import dataclass, field

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.memory_runtime_models import MemoryEventContext
from backend.app.application.services.trigger_engine_service import TriggerEvaluation
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.rule import RuleDefinition
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.state import SessionState
from backend.app.domain.repositories.rule import RuleRepository


@dataclass(frozen=True)
class RuleAttributeUpdate:
    owner_type: str
    owner_id: str
    schema_key: str
    value: object
    source: str = "rule"


@dataclass(frozen=True)
class RuleEvaluation:
    system_messages: list[str] = field(default_factory=list)
    attribute_updates: list[RuleAttributeUpdate] = field(default_factory=list)
    memory_events: list[MemoryEventContext] = field(default_factory=list)
    metric_deltas: dict[str, float] = field(default_factory=dict)
    add_tags: list[str] = field(default_factory=list)
    remove_tags: list[str] = field(default_factory=list)
    phase_override: str | None = None
    debug_lines: list[str] = field(default_factory=list)
    hit_rules: list[str] = field(default_factory=list)


class RuleEngineService:
    def __init__(self, rule_repository: RuleRepository) -> None:
        self._rule_repository = rule_repository

    def evaluate_turn(
        self,
        session: SessionSnapshot,
        player_input: str,
        director_decision: DirectorDecision,
        trigger_evaluation: TriggerEvaluation,
        session_attributes: list[RuntimeAttributeItem],
        current_state: SessionState,
    ) -> RuleEvaluation:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        context = {
            "player_input": player_input,
            "location": session.location,
            "next_location": director_decision.next_location,
            "world_phase": director_decision.world_phase,
            "state_phase": current_state.phase,
            "state_tags": current_state.tags,
            "attributes": attr_map,
            "trigger_messages": trigger_evaluation.system_messages,
        }

        rules = sorted(
            [rule for rule in self._rule_repository.list_rules(scope="session") if rule.enabled],
            key=lambda item: item.priority,
            reverse=True,
        )

        system_messages: list[str] = []
        attribute_updates: list[RuleAttributeUpdate] = []
        memory_events: list[MemoryEventContext] = []
        metric_deltas: dict[str, float] = {}
        add_tags: list[str] = []
        remove_tags: list[str] = []
        debug_lines: list[str] = []
        hit_rules: list[str] = []
        phase_override: str | None = None

        for rule in rules:
            if not self._matches(rule, context):
                continue

            hit_rules.append(rule.name)
            debug_lines.append(f"RuleEngine hit={rule.name}")
            rule_generated_memory = False

            for effect in rule.effects:
                effect_type = effect.get("type")
                if effect_type == "message":
                    text = str(effect.get("text", ""))
                    system_messages.append(text)
                    if text:
                        memory_events.append(
                            MemoryEventContext(
                                event_id=f"rule:{rule.id}",
                                content=text,
                                source="rule_engine",
                                importance=0.46,
                                location=director_decision.next_location or session.location,
                            )
                        )
                        rule_generated_memory = True
                elif effect_type == "attribute_set":
                    attribute_updates.append(
                        RuleAttributeUpdate(
                            owner_type=str(effect.get("owner_type", "session")),
                            owner_id=str(effect.get("owner_id", session.id)),
                            schema_key=str(effect.get("schema_key")),
                            value=effect.get("value"),
                        )
                    )
                elif effect_type == "metric_delta":
                    key = str(effect.get("metric"))
                    metric_deltas[key] = metric_deltas.get(key, 0.0) + float(effect.get("delta", 0.0))
                elif effect_type == "add_tag":
                    add_tags.append(str(effect.get("tag")))
                elif effect_type == "remove_tag":
                    remove_tags.append(str(effect.get("tag")))
                elif effect_type == "phase_override":
                    phase_override = str(effect.get("phase"))

            if not rule_generated_memory:
                memory_events.append(
                    MemoryEventContext(
                        event_id=f"rule:{rule.id}",
                        content=f"规则生效：{rule.name}",
                        source="rule_engine",
                        importance=0.4,
                        location=director_decision.next_location or session.location,
                    )
                )

        if hit_rules:
            debug_lines.append("RuleEngine matched=" + ", ".join(hit_rules))

        return RuleEvaluation(
            system_messages=system_messages,
            attribute_updates=attribute_updates,
            memory_events=memory_events,
            metric_deltas=metric_deltas,
            add_tags=add_tags,
            remove_tags=remove_tags,
            phase_override=phase_override,
            debug_lines=debug_lines,
            hit_rules=hit_rules,
        )

    def _matches(self, rule: RuleDefinition, context: dict[str, object]) -> bool:
        condition = rule.condition
        condition_type = condition.get("type")

        if condition_type == "attribute_threshold":
            key = str(condition.get("attribute_key"))
            operator = str(condition.get("operator", ">="))
            expected = condition.get("value")
            actual = context["attributes"].get(key) if isinstance(context.get("attributes"), dict) else None
            return self._compare(actual, expected, operator)

        if condition_type == "player_input_contains":
            needle = str(condition.get("value", ""))
            return needle in str(context.get("player_input", ""))

        if condition_type == "phase_equals":
            return str(context.get("world_phase")) == str(condition.get("value"))

        if condition_type == "scene_changed":
            return context.get("next_location") != context.get("location")

        if condition_type == "trigger_message_contains":
            needle = str(condition.get("value", ""))
            messages = context.get("trigger_messages", [])
            return isinstance(messages, list) and any(needle in str(item) for item in messages)

        return False

    def _compare(self, actual: object, expected: object, operator: str) -> bool:
        if operator == ">=":
            return actual is not None and actual >= expected
        if operator == ">":
            return actual is not None and actual > expected
        if operator == "<=":
            return actual is not None and actual <= expected
        if operator == "<":
            return actual is not None and actual < expected
        if operator == "==":
            return actual == expected
        return False
