from __future__ import annotations

from dataclasses import dataclass, field
import json
import re
from typing import Any


@dataclass(frozen=True)
class PromptModule:
    name: str
    source: str
    content: str
    editable: bool
    sent: bool = True

    def to_dict(self) -> dict[str, object]:
        return {
            "name": self.name,
            "source": self.source,
            "content": self.content,
            "editable": self.editable,
            "sent": self.sent,
        }


@dataclass(frozen=True)
class ReturnProcessingResult:
    before: str
    after: str
    applied_rules: list[dict[str, object]] = field(default_factory=list)

    def to_dict(self) -> dict[str, object]:
        return {
            "before": self.before,
            "after": self.after,
            "applied_rules": list(self.applied_rules),
        }


class PromptRuntimeService:
    DIRECTOR_SCOPES = {"director", "both"}
    CHARACTER_SCOPES = {"character", "both"}

    def build_messages(self, modules: list[PromptModule]) -> list[dict[str, object]]:
        prompt_text = self._join_modules([module for module in modules if module.editable and module.sent])
        objective_text = self._join_modules([module for module in modules if not module.editable and module.sent])
        messages: list[dict[str, object]] = []
        if prompt_text.strip():
            messages.append({"role": "system", "content": prompt_text})
        if objective_text.strip():
            messages.append({"role": "user", "content": objective_text})
        return messages

    def build_prompt_call(
        self,
        *,
        recipient_type: str,
        recipient_name: str,
        stage: str,
        purpose: str,
        modules: list[PromptModule],
        raw_debug: dict[str, object] | None = None,
    ) -> dict[str, object]:
        messages = self.build_messages(modules)
        return {
            "schema_version": "prompt_call_v1",
            "recipient_type": recipient_type,
            "recipient_name": recipient_name,
            "stage": stage,
            "purpose": purpose,
            "modules": [module.to_dict() for module in modules],
            "messages": messages,
            "final_sent_content": self._format_final_sent_content(modules),
            "raw_model_return": None,
            "return_processing": None,
            "processed_model_return": None,
            "written_result": None,
            "raw_debug": raw_debug or {},
        }

    def attach_result(
        self,
        prompt_call: dict[str, object],
        *,
        raw_model_return: str | None,
        return_processing: ReturnProcessingResult | None,
        processed_model_return: object,
        written_result: object,
    ) -> dict[str, object]:
        updated = dict(prompt_call)
        updated["raw_model_return"] = raw_model_return
        updated["return_processing"] = return_processing.to_dict() if return_processing is not None else None
        updated["processed_model_return"] = processed_model_return
        updated["written_result"] = written_result
        return updated

    def prompt_modules_for_presets(
        self,
        *,
        director_config: dict[str, object],
        target: str,
        variables: dict[str, str],
    ) -> list[PromptModule]:
        scopes = self.DIRECTOR_SCOPES if target == "director" else self.CHARACTER_SCOPES
        presets = director_config.get("prompt_presets", [])
        if not isinstance(presets, list):
            return []
        modules: list[PromptModule] = []
        normalized = [
            item
            for item in presets
            if isinstance(item, dict)
            and bool(item.get("enabled", True))
            and str(item.get("scope") or "both").strip() in scopes
        ]
        normalized.sort(key=lambda item: int(item.get("order") or 0))
        for item in normalized:
            content = self.render_template(str(item.get("content") or ""), variables)
            if not content.strip():
                continue
            modules.append(
                PromptModule(
                    name=f"提示词预设：{str(item.get('name') or '未命名预设')}",
                    source="世界设计 / 提示词预设",
                    content=content,
                    editable=True,
                )
            )
        return modules

    def render_template(self, text: str, variables: dict[str, str]) -> str:
        rendered = text
        for key, value in variables.items():
            rendered = rendered.replace("{{" + key + "}}", value)
        return rendered

    def apply_return_rules(
        self,
        *,
        director_config: dict[str, object],
        target: str,
        raw_text: str,
    ) -> ReturnProcessingResult:
        scopes = self.DIRECTOR_SCOPES if target == "director" else self.CHARACTER_SCOPES
        rules = director_config.get("return_processing_rules", [])
        if not isinstance(rules, list):
            return ReturnProcessingResult(before=raw_text, after=raw_text)
        text = raw_text
        applied: list[dict[str, object]] = []
        normalized = [
            item
            for item in rules
            if isinstance(item, dict)
            and bool(item.get("enabled", True))
            and str(item.get("scope") or "both").strip() in scopes
            and str(item.get("pattern") or "")
        ]
        normalized.sort(key=lambda item: int(item.get("order") or 0))
        for item in normalized:
            pattern = str(item.get("pattern") or "")
            replacement = str(item.get("replacement") or "")
            try:
                next_text, count = re.subn(pattern, replacement, text)
            except re.error as exc:
                applied.append(
                    {
                        "name": str(item.get("name") or "未命名规则"),
                        "pattern": pattern,
                        "error": str(exc),
                        "count": 0,
                    }
                )
                continue
            if count:
                applied.append(
                    {
                        "name": str(item.get("name") or "未命名规则"),
                        "pattern": pattern,
                        "replacement": replacement,
                        "count": count,
                    }
                )
            text = next_text
        return ReturnProcessingResult(before=raw_text, after=text, applied_rules=applied)

    def objective_json(self, value: object) -> str:
        return json.dumps(value, ensure_ascii=False, indent=2)

    def _join_modules(self, modules: list[PromptModule]) -> str:
        return "\n\n".join(
            f"## {module.name}\n{module.content}".strip()
            for module in modules
            if module.sent and module.content.strip()
        )

    def _format_final_sent_content(self, modules: list[PromptModule]) -> str:
        return self._join_modules([module for module in modules if module.sent])
