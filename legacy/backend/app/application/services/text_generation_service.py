from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Callable
from urllib import error, request

from backend.app.application.services.catalog_service import CatalogQueryService
from backend.app.domain.models.model_config import ModelConfig


class TextGenerationUnavailableError(ValueError):
    def __init__(self, message: str, *, debug_lines: list[str] | None = None) -> None:
        super().__init__(message)
        self.debug_lines = list(debug_lines or [])


@dataclass(frozen=True)
class TextGenerationResult:
    payload: dict[str, Any] | None
    model: ModelConfig | None
    raw_content: str | None = None
    raw_reasoning: str | None = None
    debug_lines: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class TextGenerationProbeResult:
    ok: bool
    detail: str
    model: ModelConfig | None
    debug_lines: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class TextModelDiscoveryResult:
    ok: bool
    detail: str
    model_ids: list[str]
    debug_lines: list[str] = field(default_factory=list)


class TextGenerationService:
    ANTHROPIC_VERSION = "2023-06-01"

    def __init__(self, catalog_queries: CatalogQueryService) -> None:
        self._catalog_queries = catalog_queries

    def generate_json(
        self,
        *,
        system_prompt: str,
        user_prompt: str,
        preferred_model: str | None = None,
        temperature: float = 0.4,
    ) -> TextGenerationResult:
        messages: list[dict[str, Any]] = []
        if system_prompt.strip():
            messages.append({"role": "system", "content": system_prompt})
        messages.append({"role": "user", "content": user_prompt})
        return self.generate_json_messages(
            messages=messages,
            preferred_model=preferred_model,
            temperature=temperature,
        )

    def generate_json_messages(
        self,
        *,
        messages: list[dict[str, Any]],
        preferred_model: str | None = None,
        temperature: float = 0.4,
        on_stream_text: Callable[[str], None] | None = None,
        on_stream_reasoning: Callable[[str], None] | None = None,
        on_stream_full_text: Callable[[str], None] | None = None,
    ) -> TextGenerationResult:
        model = self._resolve_text_model(preferred_model=preferred_model)
        if model is None:
            return TextGenerationResult(
                payload=None,
                model=None,
                raw_content=None,
                raw_reasoning=None,
                debug_lines=["TextGeneration no_text_model_configured"],
            )

        if not model.base_url.strip():
            return TextGenerationResult(
                payload=None,
                model=model,
                raw_content=None,
                raw_reasoning=None,
                debug_lines=[f"TextGeneration missing_base_url model={model.model_id}"],
            )

        if on_stream_text is not None or on_stream_full_text is not None:
            stream_result = self._generate_json_messages_stream(
                model=model,
                messages=messages,
                temperature=temperature,
                on_stream_text=on_stream_text,
                on_stream_reasoning=on_stream_reasoning,
                on_stream_full_text=on_stream_full_text,
            )
            if stream_result is not None:
                return stream_result

        if self._is_anthropic_model(model):
            body = self._build_anthropic_messages_body(
                model=model,
                messages=messages,
                temperature=temperature,
                max_tokens=4096,
            )
            headers = self._headers(model=model, request_kind="anthropic")
            endpoint = self._anthropic_endpoint(model.base_url, "messages")
            try:
                response_data = self._post_json(endpoint=endpoint, payload=body, headers=headers)
                request_debug = [f"TextGeneration request_mode=anthropic_messages model={model.model_id}"]
            except Exception as exc:
                return TextGenerationResult(
                    payload=None,
                    model=model,
                    raw_content=None,
                    raw_reasoning=None,
                    debug_lines=[
                        f"TextGeneration request_failed model={model.model_id}",
                        f"TextGeneration error={exc}",
                    ],
                )
        else:
            body = {
                "model": model.model_id,
                "messages": messages,
                "temperature": temperature,
            }
            body["response_format"] = {"type": "json_object"}
            headers = self._headers(model=model, request_kind="openai")
            endpoint = f"{model.base_url.rstrip('/')}/chat/completions"

            try:
                response_data = self._post_json(endpoint=endpoint, payload=body, headers=headers)
                request_debug = [f"TextGeneration request_mode=json_format model={model.model_id}"]
            except Exception:
                relaxed_body = {
                    "model": model.model_id,
                    "messages": body["messages"],
                    "temperature": temperature,
                }
                try:
                    response_data = self._post_json(endpoint=endpoint, payload=relaxed_body, headers=headers)
                    request_debug = [f"TextGeneration request_mode=plain_chat model={model.model_id}"]
                except Exception as exc:  # pragma: no cover - fallback path depends on local model availability
                    return TextGenerationResult(
                        payload=None,
                        model=model,
                        raw_content=None,
                        raw_reasoning=None,
                        debug_lines=[
                            f"TextGeneration request_failed model={model.model_id}",
                            f"TextGeneration error={exc}",
                        ],
                    )

        content = self._extract_content(response_data=response_data, model=model)
        reasoning = self._extract_reasoning(response_data=response_data, model=model)
        if not content:
            return TextGenerationResult(
                payload=None,
                model=model,
                raw_content=None,
                raw_reasoning=reasoning or None,
                debug_lines=[
                    f"TextGeneration empty_content model={model.model_id}",
                ],
            )

        parsed = self._parse_json_payload(content)
        if parsed is None:
            return TextGenerationResult(
                payload=None,
                model=model,
                raw_content=content,
                raw_reasoning=reasoning or None,
                debug_lines=[
                    f"TextGeneration invalid_json model={model.model_id}",
                ],
            )

        return TextGenerationResult(
            payload=parsed,
            model=model,
            raw_content=content,
            raw_reasoning=reasoning or None,
            debug_lines=[
                *request_debug,
                f"TextGeneration ok model={model.model_id}",
            ],
        )

    def test_connection(self, *, preferred_model: str | None = None) -> TextGenerationProbeResult:
        model = self._resolve_text_model(preferred_model=preferred_model)
        if model is None:
            return TextGenerationProbeResult(
                ok=False,
                detail="未找到可用的文本模型配置。",
                model=None,
                debug_lines=["TextGenerationProbe no_text_model_configured"],
            )

        if not model.model_id.strip():
            return TextGenerationProbeResult(
                ok=False,
                detail="模型 ID 为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe missing_model_id id={model.id}"],
            )

        if not model.base_url.strip():
            return TextGenerationProbeResult(
                ok=False,
                detail="Base URL 为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe missing_base_url model={model.model_id}"],
            )

        if self._is_anthropic_model(model):
            headers = self._headers(model=model, request_kind="anthropic")
            body = self._build_anthropic_messages_body(
                model=model,
                messages=[{"role": "user", "content": "Reply with the single word OK."}],
                temperature=0,
                max_tokens=64,
            )
            endpoint = self._anthropic_endpoint(model.base_url, "messages")
        else:
            headers = self._headers(model=model, request_kind="openai")
            body = {
                "model": model.model_id,
                "messages": [{"role": "user", "content": "Reply with the single word OK."}],
                "temperature": 0,
            }
            endpoint = f"{model.base_url.rstrip('/')}/chat/completions"

        try:
            response_data = self._post_json(endpoint=endpoint, payload=body, headers=headers)
        except Exception as exc:
            return TextGenerationProbeResult(
                ok=False,
                detail=f"请求失败：{exc}",
                model=model,
                debug_lines=[
                    f"TextGenerationProbe request_failed model={model.model_id}",
                    f"TextGenerationProbe error={exc}",
                ],
            )

        content = self._extract_content(response_data=response_data, model=model).strip()
        if not content:
            return TextGenerationProbeResult(
                ok=False,
                detail="模型返回内容为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe empty_content model={model.model_id}"],
            )

        preview = " ".join(content.split())
        if len(preview) > 80:
            preview = f"{preview[:77]}..."

        return TextGenerationProbeResult(
            ok=True,
            detail=f"调用成功，模型返回：{preview}",
            model=model,
            debug_lines=[f"TextGenerationProbe ok model={model.model_id}"],
        )

    def test_connection(self, *, preferred_model: str | None = None) -> TextGenerationProbeResult:
        model = self._resolve_text_model(preferred_model=preferred_model)
        if model is None:
            return TextGenerationProbeResult(
                ok=False,
                detail="未找到可用的文本模型配置。",
                model=None,
                debug_lines=["TextGenerationProbe no_text_model_configured"],
            )

        if not model.model_id.strip():
            return TextGenerationProbeResult(
                ok=False,
                detail="模型 ID 为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe missing_model_id id={model.id}"],
            )

        if not model.base_url.strip():
            return TextGenerationProbeResult(
                ok=False,
                detail="Base URL 为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe missing_base_url model={model.model_id}"],
            )

        probe_messages = [
            {
                "role": "system",
                "content": (
                    "Return only one JSON object with string fields: "
                    "speaker, content, intent, emotion. Do not use markdown."
                ),
            },
            {"role": "user", "content": "Generate a short sample response JSON now."},
        ]

        if self._is_anthropic_model(model):
            headers = self._headers(model=model, request_kind="anthropic")
            body = self._build_anthropic_messages_body(
                model=model,
                messages=probe_messages,
                temperature=0,
                max_tokens=256,
            )
            endpoint = self._anthropic_endpoint(model.base_url, "messages")
            request_mode = "anthropic_messages"
        else:
            headers = self._headers(model=model, request_kind="openai")
            body = {
                "model": model.model_id,
                "messages": probe_messages,
                "temperature": 0,
                "response_format": {"type": "json_object"},
            }
            endpoint = f"{model.base_url.rstrip('/')}/chat/completions"
            request_mode = "json_format"

        try:
            response_data = self._post_json(endpoint=endpoint, payload=body, headers=headers)
        except Exception as exc:
            if self._is_anthropic_model(model):
                return TextGenerationProbeResult(
                    ok=False,
                    detail=f"请求失败：{exc}",
                    model=model,
                    debug_lines=[
                        f"TextGenerationProbe request_failed model={model.model_id} mode={request_mode}",
                        f"TextGenerationProbe error={exc}",
                    ],
                )

            fallback_body = {
                "model": model.model_id,
                "messages": probe_messages,
                "temperature": 0,
            }
            try:
                response_data = self._post_json(endpoint=endpoint, payload=fallback_body, headers=headers)
                request_mode = "plain_chat"
            except Exception as fallback_exc:
                return TextGenerationProbeResult(
                    ok=False,
                    detail=f"请求失败：{fallback_exc}",
                    model=model,
                    debug_lines=[
                        f"TextGenerationProbe request_failed model={model.model_id} mode={request_mode}",
                        f"TextGenerationProbe error={fallback_exc}",
                    ],
                )

        content = self._extract_content(response_data=response_data, model=model).strip()
        if not content:
            return TextGenerationProbeResult(
                ok=False,
                detail="模型返回内容为空。",
                model=model,
                debug_lines=[f"TextGenerationProbe empty_content model={model.model_id} mode={request_mode}"],
            )

        parsed = self._parse_json_payload(content)
        if parsed is None:
            return TextGenerationProbeResult(
                ok=False,
                detail="模型返回不是可解析的 JSON 对象。",
                model=model,
                debug_lines=[f"TextGenerationProbe invalid_json model={model.model_id} mode={request_mode}"],
            )

        required_fields = ("speaker", "content", "intent", "emotion")
        missing_fields = [
            field_name
            for field_name in required_fields
            if not isinstance(parsed.get(field_name), str) or not str(parsed.get(field_name)).strip()
        ]
        if missing_fields:
            return TextGenerationProbeResult(
                ok=False,
                detail=f"模型返回缺少必填字段：{', '.join(missing_fields)}",
                model=model,
                debug_lines=[
                    f"TextGenerationProbe invalid_payload model={model.model_id} mode={request_mode}",
                    f"TextGenerationProbe missing_fields={','.join(missing_fields)}",
                ],
            )

        preview = " ".join(str(parsed.get("content", "")).split())
        if len(preview) > 80:
            preview = f"{preview[:77]}..."

        return TextGenerationProbeResult(
            ok=True,
            detail=f"调用成功，对话协议校验已通过：{preview}",
            model=model,
            debug_lines=[f"TextGenerationProbe ok model={model.model_id} mode={request_mode}"],
        )

    def discover_models(
        self,
        *,
        base_url: str,
        api_key: str = "",
        provider: str = "",
    ) -> TextModelDiscoveryResult:
        normalized_base_url = base_url.strip()
        if not normalized_base_url:
            return TextModelDiscoveryResult(
                ok=False,
                detail="Base URL 为空。",
                model_ids=[],
                debug_lines=["TextModelDiscovery missing_base_url"],
            )

        endpoints = self._build_model_discovery_endpoints(
            base_url=normalized_base_url,
            provider=provider,
        )
        headers = self._discovery_headers(
            provider=provider,
            base_url=normalized_base_url,
            api_key=api_key,
        )

        debug_lines: list[str] = []
        last_error: Exception | None = None

        for endpoint in endpoints:
            debug_lines.append(f"TextModelDiscovery try endpoint={endpoint}")
            try:
                payload = self._get_json(endpoint=endpoint, headers=headers)
                model_ids = self._extract_model_ids(payload)
                if model_ids:
                    return TextModelDiscoveryResult(
                        ok=True,
                        detail=f"已从端点拉取到 {len(model_ids)} 个模型。",
                        model_ids=model_ids,
                        debug_lines=[
                            *debug_lines,
                            f"TextModelDiscovery ok endpoint={endpoint} count={len(model_ids)}",
                        ],
                    )
                last_error = RuntimeError("empty_model_list")
                debug_lines.append(f"TextModelDiscovery empty endpoint={endpoint}")
            except Exception as exc:
                last_error = exc
                debug_lines.append(f"TextModelDiscovery failed endpoint={endpoint} error={exc}")

        detail = "端点没有返回可用模型列表。"
        if last_error is not None and str(last_error) != "empty_model_list":
            detail = f"拉取失败：{last_error}"

        return TextModelDiscoveryResult(
            ok=False,
            detail=detail,
            model_ids=[],
            debug_lines=debug_lines,
        )

    def _resolve_text_model(self, preferred_model: str | None) -> ModelConfig | None:
        models = [model for model in self._catalog_queries.list_models() if model.model_type == "text"]
        if not models:
            return None

        if preferred_model:
            matched = next(
                (
                    model
                    for model in models
                    if model.id == preferred_model or model.model_id == preferred_model or model.name == preferred_model
                ),
                None,
            )
            if matched is not None:
                return matched

        default_model = next((model for model in models if model.is_default), None)
        if default_model is not None:
            return default_model

        settings = self._catalog_queries.get_settings()
        matched_from_settings = next(
            (
                model
                for model in models
                if model.id == settings.default_text_model
                or model.model_id == settings.default_text_model
                or model.name == settings.default_text_model
            ),
            None,
        )
        if matched_from_settings is not None:
            return matched_from_settings

        return models[0]

    def _build_model_discovery_endpoints(self, *, base_url: str, provider: str) -> list[str]:
        normalized = base_url.rstrip("/")
        candidates: list[str] = []

        def push(candidate: str) -> None:
            if candidate and candidate not in candidates:
                candidates.append(candidate)

        if self._looks_like_anthropic_provider(provider, normalized):
            if normalized.endswith("/models"):
                push(normalized)
            elif normalized.endswith("/v1"):
                push(f"{normalized}/models")
            else:
                push(f"{normalized}/v1/models")
        else:
            if normalized.endswith("/models"):
                push(normalized)
            else:
                push(f"{normalized}/models")
            if not normalized.endswith("/v1") and not normalized.endswith("/models"):
                push(f"{normalized}/v1/models")

        lowered_provider = provider.lower()
        if "ollama" in lowered_provider:
            ollama_root = normalized[:-3] if normalized.endswith("/v1") else normalized
            push(f"{ollama_root.rstrip('/')}/api/tags")

        return candidates

    def _extract_model_ids(self, payload: Any) -> list[str]:
        candidates: list[str] = []

        def append_from_items(items: Any) -> None:
            if not isinstance(items, list):
                return
            for item in items:
                if not isinstance(item, dict):
                    continue
                for key in ("id", "name", "model"):
                    value = item.get(key)
                    if isinstance(value, str) and value.strip():
                        candidates.append(value.strip())
                        break

        if isinstance(payload, dict):
            append_from_items(payload.get("data"))
            append_from_items(payload.get("models"))
        elif isinstance(payload, list):
            append_from_items(payload)

        deduped: list[str] = []
        for model_id in candidates:
            if model_id not in deduped:
                deduped.append(model_id)
        return deduped

    def _is_anthropic_model(self, model: ModelConfig) -> bool:
        return self._looks_like_anthropic_provider(model.provider, model.base_url)

    def _looks_like_anthropic_provider(self, provider: str, base_url: str) -> bool:
        lowered_provider = provider.lower()
        lowered_base_url = base_url.lower()
        return any(token in lowered_provider for token in ("anthropic", "claude")) or "anthropic" in lowered_base_url

    def _discovery_headers(self, *, provider: str, base_url: str, api_key: str) -> dict[str, str]:
        if self._looks_like_anthropic_provider(provider, base_url):
            headers = {"anthropic-version": self.ANTHROPIC_VERSION}
            if api_key.strip():
                headers["x-api-key"] = api_key.strip()
            return headers

        headers: dict[str, str] = {}
        if api_key.strip():
            headers["Authorization"] = f"Bearer {api_key.strip()}"
        return headers

    def _headers(self, *, model: ModelConfig, request_kind: str) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if request_kind == "anthropic":
            headers["anthropic-version"] = self.ANTHROPIC_VERSION
            if model.api_key.strip():
                headers["x-api-key"] = model.api_key.strip()
            return headers

        if model.api_key.strip():
            headers["Authorization"] = f"Bearer {model.api_key.strip()}"
        return headers

    def _anthropic_endpoint(self, base_url: str, path: str) -> str:
        normalized = base_url.rstrip("/")
        if normalized.endswith(f"/v1/{path}") or normalized.endswith(f"/{path}"):
            return normalized
        if normalized.endswith("/v1"):
            return f"{normalized}/{path}"
        return f"{normalized}/v1/{path}"

    def _build_anthropic_messages_body(
        self,
        *,
        model: ModelConfig,
        messages: list[dict[str, Any]],
        temperature: float,
        max_tokens: int,
    ) -> dict[str, Any]:
        system_prompt, anthropic_messages = self._split_anthropic_messages(messages)
        body: dict[str, Any] = {
            "model": model.model_id,
            "messages": anthropic_messages or [{"role": "user", "content": ""}],
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        if system_prompt:
            body["system"] = system_prompt
        return body

    def _split_anthropic_messages(self, messages: list[dict[str, Any]]) -> tuple[str, list[dict[str, str]]]:
        system_parts: list[str] = []
        anthropic_messages: list[dict[str, str]] = []
        for message in messages:
            if not isinstance(message, dict):
                continue
            role = str(message.get("role") or "user").strip().lower()
            content = self._coerce_message_content(message.get("content")).strip()
            if not content:
                continue
            if role == "system":
                system_parts.append(content)
                continue
            if role not in {"user", "assistant"}:
                role = "user"
            anthropic_messages.append({"role": role, "content": content})
        return "\n\n".join(system_parts).strip(), anthropic_messages

    def _generate_json_messages_stream(
        self,
        *,
        model: ModelConfig,
        messages: list[dict[str, Any]],
        temperature: float,
        on_stream_text: Callable[[str], None] | None,
        on_stream_reasoning: Callable[[str], None] | None,
        on_stream_full_text: Callable[[str], None] | None = None,
    ) -> TextGenerationResult | None:
        if self._is_anthropic_model(model):
            body = self._build_anthropic_messages_body(
                model=model,
                messages=messages,
                temperature=temperature,
                max_tokens=4096,
            )
            body["stream"] = True
            headers = self._headers(model=model, request_kind="anthropic")
            endpoint = self._anthropic_endpoint(model.base_url, "messages")
            try:
                content, reasoning = self._post_sse_collect(
                    endpoint=endpoint,
                    payload=body,
                    headers=headers,
                    provider="anthropic",
                    on_stream_text=on_stream_text,
                    on_stream_reasoning=on_stream_reasoning,
                    on_stream_full_text=on_stream_full_text,
                )
                request_debug = [f"TextGeneration request_mode=anthropic_stream model={model.model_id}"]
            except Exception:
                return None
        else:
            body = {
                "model": model.model_id,
                "messages": messages,
                "temperature": temperature,
                "stream": True,
                "response_format": {"type": "json_object"},
            }
            headers = self._headers(model=model, request_kind="openai")
            endpoint = f"{model.base_url.rstrip('/')}/chat/completions"
            try:
                content, reasoning = self._post_sse_collect(
                    endpoint=endpoint,
                    payload=body,
                    headers=headers,
                    provider="openai",
                    on_stream_text=on_stream_text,
                    on_stream_reasoning=on_stream_reasoning,
                    on_stream_full_text=on_stream_full_text,
                )
                request_debug = [f"TextGeneration request_mode=json_stream model={model.model_id}"]
            except Exception:
                relaxed_body = {
                    "model": model.model_id,
                    "messages": messages,
                    "temperature": temperature,
                    "stream": True,
                }
                try:
                    content, reasoning = self._post_sse_collect(
                        endpoint=endpoint,
                        payload=relaxed_body,
                        headers=headers,
                        provider="openai",
                        on_stream_text=on_stream_text,
                        on_stream_reasoning=on_stream_reasoning,
                        on_stream_full_text=on_stream_full_text,
                    )
                    request_debug = [f"TextGeneration request_mode=plain_stream model={model.model_id}"]
                except Exception:
                    return None

        if not content.strip():
            return None

        parsed = self._parse_json_payload(content)
        if parsed is None:
            return TextGenerationResult(
                payload=None,
                model=model,
                raw_content=content,
                raw_reasoning=reasoning or None,
                debug_lines=[
                    *request_debug,
                    f"TextGeneration invalid_json model={model.model_id}",
                ],
            )

        return TextGenerationResult(
            payload=parsed,
            model=model,
            raw_content=content,
            raw_reasoning=reasoning or None,
            debug_lines=[
                *request_debug,
                f"TextGeneration stream_ok model={model.model_id}",
            ],
        )

    def _post_sse_collect(
        self,
        *,
        endpoint: str,
        payload: dict[str, Any],
        headers: dict[str, str],
        provider: str,
        on_stream_text: Callable[[str], None] | None,
        on_stream_reasoning: Callable[[str], None] | None,
        on_stream_full_text: Callable[[str], None] | None = None,
    ) -> tuple[str, str]:
        req = request.Request(
            endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        try:
            with request.urlopen(req, timeout=60) as response:
                accumulated = ""
                accumulated_reasoning = ""
                last_preview = ""
                last_reasoning = ""
                last_full_text = ""
                current_event = ""
                data_lines: list[str] = []

                def flush_event() -> None:
                    nonlocal accumulated, accumulated_reasoning, last_preview, last_reasoning, last_full_text, current_event, data_lines
                    if not data_lines:
                        current_event = ""
                        return
                    content_delta, reasoning_delta = self._extract_sse_delta(
                        provider=provider,
                        event_name=current_event,
                        data_payload="\n".join(data_lines),
                    )
                    data_lines = []
                    current_event = ""
                    if not content_delta and not reasoning_delta:
                        return
                    if reasoning_delta:
                        accumulated_reasoning += reasoning_delta
                    if content_delta:
                        accumulated += content_delta
                    if on_stream_reasoning is not None:
                        reasoning_preview = accumulated_reasoning.strip()
                        if reasoning_preview and reasoning_preview != last_reasoning:
                            last_reasoning = reasoning_preview
                            try:
                                on_stream_reasoning(reasoning_preview)
                            except Exception:
                                return
                    full_text = self._compose_stream_full_text(
                        reasoning=accumulated_reasoning,
                        content=accumulated,
                    )
                    if on_stream_full_text is not None and full_text and full_text != last_full_text:
                        last_full_text = full_text
                        try:
                            on_stream_full_text(full_text)
                        except Exception:
                            return
                    if on_stream_text is not None:
                        preview = self._extract_stream_content_preview(accumulated)
                        if not preview or preview == last_preview:
                            return
                        last_preview = preview
                        try:
                            on_stream_text(preview)
                        except Exception:
                            return

                while True:
                    raw_line = response.readline()
                    if not raw_line:
                        break
                    line = raw_line.decode("utf-8", errors="ignore").strip("\r\n")
                    if not line:
                        flush_event()
                        continue
                    if line.startswith("event:"):
                        current_event = line[6:].strip()
                        continue
                    if line.startswith("data:"):
                        data_lines.append(line[5:].strip())
                        continue

                flush_event()
                return accumulated, accumulated_reasoning
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore")
            raise RuntimeError(f"http_{exc.code}: {detail}") from exc
        except error.URLError as exc:
            raise RuntimeError(f"url_error: {exc.reason}") from exc

    def _extract_sse_delta(
        self,
        *,
        provider: str,
        event_name: str,
        data_payload: str,
    ) -> tuple[str, str]:
        if not data_payload or data_payload == "[DONE]":
            return "", ""
        try:
            payload = json.loads(data_payload)
        except json.JSONDecodeError:
            return "", ""

        if not isinstance(payload, dict):
            return "", ""

        if provider == "anthropic":
            return self._extract_anthropic_sse_delta(payload, event_name=event_name)
        return self._extract_openai_sse_delta(payload)

    def _extract_openai_sse_delta(self, payload: dict[str, Any]) -> tuple[str, str]:
        choices = payload.get("choices")
        if not isinstance(choices, list) or not choices:
            return "", ""
        first = choices[0]
        if not isinstance(first, dict):
            return "", ""

        delta = first.get("delta")
        content_text = ""
        reasoning_text = ""
        if isinstance(delta, dict):
            content = delta.get("content")
            if isinstance(content, str):
                content_text = content
            elif isinstance(content, list):
                text_parts: list[str] = []
                for item in content:
                    if isinstance(item, dict) and item.get("type") == "text" and isinstance(item.get("text"), str):
                        text_parts.append(item["text"])
                    elif isinstance(item, dict) and item.get("type") in {"reasoning", "thinking"}:
                        nested = self._coerce_reasoning_field(item)
                        if nested:
                            reasoning_text += nested
                content_text = "".join(text_parts)
            reasoning_text += self._coerce_reasoning_field(delta.get("reasoning_content"))
            reasoning_text += self._coerce_reasoning_field(delta.get("reasoning"))
            reasoning_text += self._coerce_reasoning_field(delta.get("thinking"))
        return content_text, reasoning_text

    def _extract_anthropic_sse_delta(self, payload: dict[str, Any], *, event_name: str) -> tuple[str, str]:
        payload_type = str(payload.get("type") or event_name or "").strip()
        if payload_type == "content_block_delta":
            delta = payload.get("delta")
            if isinstance(delta, dict):
                text = delta.get("text")
                if isinstance(text, str):
                    return text, ""
                thinking = delta.get("thinking")
                if isinstance(thinking, str):
                    return "", thinking
        if payload_type == "content_block_start":
            content_block = payload.get("content_block")
            if isinstance(content_block, dict):
                block_type = str(content_block.get("type") or "").strip()
                text = content_block.get("text")
                if isinstance(text, str):
                    return ("", text) if block_type == "thinking" else (text, "")
                thinking = content_block.get("thinking")
                if isinstance(thinking, str):
                    return "", thinking
        return "", ""

    def _compose_stream_full_text(self, *, reasoning: str, content: str) -> str:
        parts: list[str] = []
        if reasoning.strip():
            parts.append(reasoning.strip())
        if content.strip():
            parts.append(content.strip())
        return "\n\n".join(parts).strip()

    def _extract_stream_content_preview(self, accumulated: str) -> str:
        accumulated = accumulated.strip()
        if not accumulated:
            return ""
        marker_index = accumulated.find('"content"')
        if marker_index == -1:
            if accumulated.startswith("{") or accumulated.startswith("```"):
                return ""
            return accumulated
        colon_index = accumulated.find(":", marker_index + len('"content"'))
        if colon_index == -1:
            return ""
        cursor = colon_index + 1
        while cursor < len(accumulated) and accumulated[cursor] in {" ", "\n", "\r", "\t"}:
            cursor += 1
        if cursor >= len(accumulated) or accumulated[cursor] != '"':
            return ""
        cursor += 1

        chars: list[str] = []
        escaping = False
        while cursor < len(accumulated):
            ch = accumulated[cursor]
            if escaping:
                if ch == "n":
                    chars.append("\n")
                elif ch == "r":
                    chars.append("\r")
                elif ch == "t":
                    chars.append("\t")
                elif ch == "u":
                    hex_value = accumulated[cursor + 1:cursor + 5]
                    if len(hex_value) == 4:
                        try:
                            chars.append(chr(int(hex_value, 16)))
                            cursor += 4
                        except ValueError:
                            pass
                else:
                    chars.append(ch)
                escaping = False
            else:
                if ch == "\\":
                    escaping = True
                elif ch == '"':
                    break
                else:
                    chars.append(ch)
            cursor += 1
        return "".join(chars).strip()

    def _post_json(self, endpoint: str, payload: dict[str, Any], headers: dict[str, str]) -> dict[str, Any]:
        req = request.Request(
            endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        try:
            with request.urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore")
            raise RuntimeError(f"http_{exc.code}: {detail}") from exc
        except error.URLError as exc:
            raise RuntimeError(f"url_error: {exc.reason}") from exc

    def _get_json(self, *, endpoint: str, headers: dict[str, str]) -> dict[str, Any] | list[Any]:
        req = request.Request(endpoint, headers=headers, method="GET")
        try:
            with request.urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore")
            raise RuntimeError(f"http_{exc.code}: {detail}") from exc
        except error.URLError as exc:
            raise RuntimeError(f"url_error: {exc.reason}") from exc

    def _extract_content(self, *, response_data: dict[str, Any], model: ModelConfig) -> str:
        if self._is_anthropic_model(model):
            content = response_data.get("content")
            if not isinstance(content, list):
                return ""
            text_parts: list[str] = []
            for item in content:
                if isinstance(item, dict) and item.get("type") == "text" and isinstance(item.get("text"), str):
                    text_parts.append(item["text"])
            return "\n".join(text_parts).strip()

        choices = response_data.get("choices")
        if not isinstance(choices, list) or not choices:
            return ""

        first = choices[0]
        if not isinstance(first, dict):
            return ""

        message = first.get("message")
        if not isinstance(message, dict):
            return ""

        content = message.get("content")
        if isinstance(content, str):
            return content

        if isinstance(content, list):
            text_parts = []
            for item in content:
                if isinstance(item, dict) and item.get("type") == "text" and isinstance(item.get("text"), str):
                    text_parts.append(item["text"])
            return "\n".join(text_parts)

        return ""

    def _extract_reasoning(self, *, response_data: dict[str, Any], model: ModelConfig) -> str:
        if self._is_anthropic_model(model):
            content = response_data.get("content")
            if not isinstance(content, list):
                return ""
            reasoning_parts: list[str] = []
            for item in content:
                if not isinstance(item, dict):
                    continue
                item_type = str(item.get("type") or "").strip()
                if item_type == "thinking":
                    thinking = item.get("thinking")
                    if isinstance(thinking, str) and thinking.strip():
                        reasoning_parts.append(thinking.strip())
                    elif isinstance(item.get("text"), str) and str(item.get("text")).strip():
                        reasoning_parts.append(str(item.get("text")).strip())
            return "\n\n".join(reasoning_parts).strip()

        choices = response_data.get("choices")
        if not isinstance(choices, list) or not choices:
            return ""

        first = choices[0]
        if not isinstance(first, dict):
            return ""

        message = first.get("message")
        if not isinstance(message, dict):
            return ""

        reasoning_parts: list[str] = []
        for key in ("reasoning_content", "reasoning", "thinking"):
            text = self._coerce_reasoning_field(message.get(key))
            if text:
                reasoning_parts.append(text)

        content = message.get("content")
        if isinstance(content, list):
            for item in content:
                if not isinstance(item, dict):
                    continue
                item_type = str(item.get("type") or "").strip()
                if item_type in {"reasoning", "thinking"}:
                    text = self._coerce_reasoning_field(item.get("text") or item.get("thinking") or item)
                    if text:
                        reasoning_parts.append(text)

        return "\n\n".join(part for part in reasoning_parts if part).strip()

    def _coerce_reasoning_field(self, value: Any) -> str:
        if isinstance(value, str):
            return value.strip()
        if isinstance(value, list):
            parts: list[str] = []
            for item in value:
                if isinstance(item, str) and item.strip():
                    parts.append(item.strip())
                elif isinstance(item, dict):
                    text = item.get("text")
                    thinking = item.get("thinking")
                    if isinstance(text, str) and text.strip():
                        parts.append(text.strip())
                    elif isinstance(thinking, str) and thinking.strip():
                        parts.append(thinking.strip())
            return "\n".join(parts).strip()
        if isinstance(value, dict):
            for key in ("text", "thinking", "content"):
                nested = value.get(key)
                text = self._coerce_reasoning_field(nested)
                if text:
                    return text
        return ""

    def _coerce_message_content(self, content: Any) -> str:
        if isinstance(content, str):
            return content
        if isinstance(content, list):
            text_parts: list[str] = []
            for item in content:
                if isinstance(item, dict) and item.get("type") == "text" and isinstance(item.get("text"), str):
                    text_parts.append(item["text"])
            return "\n".join(text_parts)
        return str(content) if content is not None else ""

    def _parse_json_payload(self, content: str) -> dict[str, Any] | None:
        stripped = content.strip()
        if stripped.startswith("```"):
            lines = stripped.splitlines()
            if len(lines) >= 3:
                stripped = "\n".join(lines[1:-1]).strip()

        try:
            parsed = json.loads(stripped)
        except json.JSONDecodeError:
            start_index = stripped.find("{")
            end_index = stripped.rfind("}")
            if start_index == -1 or end_index == -1 or end_index <= start_index:
                return None
            candidate = stripped[start_index : end_index + 1]
            try:
                parsed = json.loads(candidate)
            except json.JSONDecodeError:
                return None

        return parsed if isinstance(parsed, dict) else None
