from dataclasses import dataclass, field

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.rule_engine_service import RuleEvaluation
from backend.app.application.services.trigger_engine_service import TriggerEvaluation
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.state import SessionState


@dataclass(frozen=True)
class StateTransitionResult:
    state: SessionState
    system_messages: list[str] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class StateEngineService:
    def evaluate_turn(
        self,
        session: SessionSnapshot,
        player_input: str,
        director_decision: DirectorDecision,
        trigger_evaluation: TriggerEvaluation,
        rule_evaluation: RuleEvaluation,
        session_attributes: list[RuntimeAttributeItem],
    ) -> StateTransitionResult:
        metrics = dict(session.state.metrics)
        tags = list(session.state.tags)
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        world_tension = self._as_number(attr_map.get("world_tension"), 0)

        metrics.setdefault("pressure", world_tension)
        metrics.setdefault("focus", 50.0)
        metrics.setdefault("stability", 100.0)

        metrics["pressure"] = max(metrics["pressure"], world_tension)
        metrics["stability"] = max(0.0, metrics["stability"] - 3.0)

        if any(keyword in player_input for keyword in ["观察", "查看", "调查"]):
            metrics["focus"] = min(100.0, metrics["focus"] + 4.0)
            self._ensure_tag(tags, "observing")

        if director_decision.next_location and director_decision.next_location != session.location:
            self._ensure_tag(tags, "traveling")
            metrics["focus"] = min(100.0, metrics["focus"] + 2.0)
        else:
            self._remove_tag(tags, "traveling")

        if any("封锁" in message for message in trigger_evaluation.system_messages):
            metrics["pressure"] += 6.0
            self._ensure_tag(tags, "under_lockdown")

        for metric, delta in rule_evaluation.metric_deltas.items():
            metrics[metric] = metrics.get(metric, 0.0) + delta

        for tag in rule_evaluation.add_tags:
            self._ensure_tag(tags, tag)

        for tag in rule_evaluation.remove_tags:
            self._remove_tag(tags, tag)

        phase = self._resolve_phase(world_phase=director_decision.world_phase, pressure=metrics["pressure"])
        if rule_evaluation.phase_override:
            phase = rule_evaluation.phase_override

        state = SessionState(
            metrics={
                "pressure": round(metrics["pressure"], 2),
                "focus": round(metrics["focus"], 2),
                "stability": round(metrics["stability"], 2),
            },
            tags=tags,
            phase=phase,
        )

        system_messages = [f"状态引擎：phase -> {phase}"]
        debug_lines = [
            f"StateEngine pressure={state.metrics['pressure']:.2f}",
            f"StateEngine focus={state.metrics['focus']:.2f}",
            f"StateEngine stability={state.metrics['stability']:.2f}",
            "StateEngine tags=" + (", ".join(state.tags) if state.tags else "none"),
        ]
        if rule_evaluation.hit_rules:
            debug_lines.append("StateEngine rules=" + ", ".join(rule_evaluation.hit_rules))

        return StateTransitionResult(state=state, system_messages=system_messages, debug_lines=debug_lines)

    def _resolve_phase(self, world_phase: str, pressure: float) -> str:
        if world_phase == "crisis" or pressure >= 70:
            return "combat-ready"
        if world_phase == "escalation" or pressure >= 35:
            return "alert"
        return "idle"

    def _ensure_tag(self, tags: list[str], tag: str) -> None:
        if tag not in tags:
            tags.append(tag)

    def _remove_tag(self, tags: list[str], tag: str) -> None:
        if tag in tags:
            tags.remove(tag)

    def _as_number(self, value: object, fallback: float) -> float:
        if isinstance(value, (int, float)):
            return float(value)
        return fallback
