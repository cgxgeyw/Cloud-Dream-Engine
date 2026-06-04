from dataclasses import dataclass, field

from backend.app.application.services.attribute_runtime_service import RuntimeAttributeItem
from backend.app.application.services.rule_engine_service import RuleEvaluation
from backend.app.application.services.scene_runtime_manager_service import SceneRuntimeResult
from backend.app.application.services.state_engine_service import StateTransitionResult
from backend.app.application.services.trigger_engine_service import TriggerEvaluation
from backend.app.application.services.world_director_service import DirectorDecision
from backend.app.domain.models.session import SessionSnapshot
from backend.app.domain.models.world import WorldDefinition


@dataclass(frozen=True)
class NarrationResult:
    messages: list[str] = field(default_factory=list)
    debug_lines: list[str] = field(default_factory=list)


class NarrationService:
    def compose_turn_narration(
        self,
        *,
        session: SessionSnapshot,
        world_profile: WorldDefinition | None,
        director_decision: DirectorDecision,
        scene_runtime: SceneRuntimeResult,
        trigger_evaluation: TriggerEvaluation,
        rule_evaluation: RuleEvaluation,
        state_transition: StateTransitionResult,
        session_attributes: list[RuntimeAttributeItem],
    ) -> NarrationResult:
        attr_map = {item.schema.key: item.value.value for item in session_attributes}
        weather_state = str(attr_map.get("weather_state", "clear"))
        active_objective = str(attr_map.get("active_objective", "")).strip()

        next_location = director_decision.next_location or session.location
        scene_changed = next_location != session.location
        phase_changed = state_transition.state.phase != session.state.phase
        generated_names = [item.name for item in director_decision.generated_characters]

        fragments: list[str] = []
        if scene_changed:
            fragments.append(self._describe_scene_change(next_location, weather_state, world_profile))
        elif weather_state in {"storm", "foggy"} and director_decision.world_phase in {"escalation", "crisis"}:
            fragments.append(self._describe_weather_pressure(weather_state, world_profile))

        if generated_names:
            fragments.append(self._describe_character_arrival(generated_names, weather_state, world_profile))

        if phase_changed:
            fragments.append(self._describe_phase_change(state_transition.state.phase, weather_state))
        elif rule_evaluation.hit_rules and not scene_changed:
            fragments.append(self._describe_rule_aftertaste(rule_evaluation.hit_rules, active_objective))
        elif trigger_evaluation.attribute_updates and not scene_changed and active_objective:
            fragments.append(f"新的线索仿佛正在收束，眼下最要紧的事只剩下{active_objective}。")

        messages = [" ".join(part.strip() for part in fragments if part.strip())] if fragments else []
        debug_lines = [
            f"Narration scene_changed={scene_changed}",
            f"Narration generated={', '.join(generated_names) if generated_names else 'none'}",
            f"Narration phase={state_transition.state.phase}",
        ]
        if messages:
            debug_lines.append(f"Narration emitted={messages[0]}")

        return NarrationResult(messages=messages, debug_lines=debug_lines)

    def _describe_scene_change(self, location: str, weather_state: str, world_profile: WorldDefinition | None) -> str:
        genre = world_profile.genre if world_profile else ""
        if weather_state == "storm":
            return f"你们穿过被风雨撕扯得发颤的通道，终于踏进了{location}，潮湿的回声贴着墙面一路蔓延开来。"
        if weather_state == "foggy":
            return f"雾气沿着地面无声漫开，{location}在朦胧中一点点显出轮廓，像是把人引进更深的局里。"
        if "末日" in genre:
            return f"一行人谨慎地摸进{location}，废墟里的细响被拉得极长，仿佛任何动静都会惊醒埋伏。"
        return f"脚步声渐渐收拢，你们来到{location}，周围的空气也随之换了一种更紧绷的质地。"

    def _describe_weather_pressure(self, weather_state: str, world_profile: WorldDefinition | None) -> str:
        genre = world_profile.genre if world_profile else ""
        if weather_state == "storm":
            if "都市" in genre:
                return "暴雨继续敲打着钢架与霓虹残片，整座城市像被一只看不见的手越攥越紧。"
            return "风雨声压得人几乎听不清彼此的呼吸，四周的局势也在这股躁动里缓慢收紧。"
        return "雾意没有散去，视线之外像藏着尚未露面的变数，连沉默都显得比平时更沉。"

    def _describe_character_arrival(
        self,
        generated_names: list[str],
        weather_state: str,
        world_profile: WorldDefinition | None,
    ) -> str:
        joined = "、".join(generated_names)
        if weather_state == "storm":
            return f"雨幕深处忽然有人影逼近，{joined}在昏暗光线里现身，让原本就紧绷的气氛又沉了一层。"
        if weather_state == "foggy":
            return f"浓雾稍稍翻涌，一道新身影自白茫中走出，{joined}的出现让众人的目光都短暂地停了停。"
        if world_profile and "修仙" in world_profile.genre:
            return f"不远处衣袂微动，{joined}无声现身，像是早已在暗处把局势看了个分明。"
        return f"就在众人迟疑的空当里，{joined}走入视线，原本单一的局面忽然多出了一层新的牵制。"

    def _describe_phase_change(self, phase: str, weather_state: str) -> str:
        if phase == "combat-ready":
            return "空气里的试探意味已经退开，取而代之的是一触即发的锋利紧张。"
        if phase == "alert":
            if weather_state == "storm":
                return "每个人都不自觉放轻了呼吸，像是在等下一次更实质的波动落下来。"
            return "局面还没有彻底失控，但所有人的神经都已经先一步绷紧。"
        return "表面的平静暂时维持住了，只是无人真的敢把它当成安稳。"

    def _describe_rule_aftertaste(self, hit_rules: list[str], active_objective: str) -> str:
        if active_objective:
            return f"局势的细微偏转已经有了方向，接下来的一举一动都绕不开{active_objective}。"
        if hit_rules:
            return "一些不易察觉的变化正在暗处接连生效，眼前的局面也因此悄悄改了走向。"
        return ""
