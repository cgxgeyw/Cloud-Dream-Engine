from __future__ import annotations

import base64
import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from urllib import error, request

from backend.app.application.services.catalog_service import CatalogQueryService
from backend.app.core.config import Settings
from backend.app.domain.models.model_config import ModelConfig


@dataclass(frozen=True)
class GeneratedImageResult:
    asset_path: str | None
    model: ModelConfig | None
    debug_lines: list[str] = field(default_factory=list)


class ImageGenerationService:
    def __init__(self, catalog_queries: CatalogQueryService, settings: Settings) -> None:
        self._catalog_queries = catalog_queries
        self._settings = settings
        self._assets_dir = Path(settings.database_path).parent / "assets" / "generated"
        self._assets_dir.mkdir(parents=True, exist_ok=True)

    def generate_image(
        self,
        *,
        prompt: str,
        kind: str,
        preferred_model: str | None = None,
    ) -> GeneratedImageResult:
        model = self._resolve_image_model(preferred_model=preferred_model)
        if model is None:
            return GeneratedImageResult(asset_path=None, model=None, debug_lines=["ImageGeneration no_image_model_configured"])
        if not model.base_url.strip():
            return GeneratedImageResult(asset_path=None, model=model, debug_lines=[f"ImageGeneration missing_base_url model={model.model_id}"])

        cache_key = hashlib.sha1(f"{model.id}|{kind}|{prompt}".encode("utf-8")).hexdigest()
        target_path = self._assets_dir / f"{cache_key}.png"
        if target_path.exists():
            return GeneratedImageResult(
                asset_path=f"/assets/generated/{target_path.name}",
                model=model,
                debug_lines=[f"ImageGeneration cache_hit model={model.model_id}"],
            )

        lowered_provider = model.provider.lower()
        try:
            if "automatic1111" in lowered_provider or "a1111" in lowered_provider:
                image_bytes = self._generate_via_a1111(model=model, prompt=prompt, kind=kind)
            elif "openai" in lowered_provider:
                image_bytes = self._generate_via_openai_images(model=model, prompt=prompt, kind=kind)
            else:
                return GeneratedImageResult(
                    asset_path=None,
                    model=model,
                    debug_lines=[f"ImageGeneration unsupported_provider provider={model.provider}"],
                )
        except Exception as exc:
            return GeneratedImageResult(
                asset_path=None,
                model=model,
                debug_lines=[f"ImageGeneration failed model={model.model_id}", f"ImageGeneration error={exc}"],
            )

        target_path.write_bytes(image_bytes)
        return GeneratedImageResult(
            asset_path=f"/assets/generated/{target_path.name}",
            model=model,
            debug_lines=[f"ImageGeneration ok model={model.model_id}"],
        )

    def _resolve_image_model(self, preferred_model: str | None) -> ModelConfig | None:
        models = [model for model in self._catalog_queries.list_models() if model.model_type == "image"]
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
        return default_model or models[0]

    def _generate_via_a1111(self, *, model: ModelConfig, prompt: str, kind: str) -> bytes:
        width, height = (960, 540) if kind == "background" else (512, 768)
        response = self._post_json(
            endpoint=f"{model.base_url.rstrip('/')}/sdapi/v1/txt2img",
            payload={
                "prompt": prompt,
                "steps": 20,
                "width": width,
                "height": height,
                "sampler_name": "Euler a",
            },
            headers=self._headers(model),
        )
        images = response.get("images")
        if not isinstance(images, list) or not images:
            raise RuntimeError("a1111_empty_images")
        return base64.b64decode(str(images[0]))

    def _generate_via_openai_images(self, *, model: ModelConfig, prompt: str, kind: str) -> bytes:
        size = "1536x1024" if kind == "background" else "1024x1536"
        response = self._post_json(
            endpoint=f"{model.base_url.rstrip('/')}/images/generations",
            payload={
                "model": model.model_id,
                "prompt": prompt,
                "size": size,
            },
            headers=self._headers(model),
        )
        data = response.get("data")
        if not isinstance(data, list) or not data:
            raise RuntimeError("openai_empty_images")
        first = data[0]
        if isinstance(first, dict) and isinstance(first.get("b64_json"), str):
            return base64.b64decode(first["b64_json"])
        raise RuntimeError("openai_missing_b64")

    def _headers(self, model: ModelConfig) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if model.api_key.strip():
            headers["Authorization"] = f"Bearer {model.api_key.strip()}"
        return headers

    def _post_json(self, *, endpoint: str, payload: dict[str, object], headers: dict[str, str]) -> dict[str, object]:
        req = request.Request(
            endpoint,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        try:
            with request.urlopen(req, timeout=90) as response:
                return json.loads(response.read().decode("utf-8"))
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore")
            raise RuntimeError(f"http_{exc.code}: {detail}") from exc
        except error.URLError as exc:
            raise RuntimeError(f"url_error: {exc.reason}") from exc
