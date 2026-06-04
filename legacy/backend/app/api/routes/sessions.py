import asyncio
import asyncio
import json
import threading

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import StreamingResponse

from backend.app.api.deps import get_app_container
from backend.app.api.schemas.attributes import (
    RuntimeAttributeGroupResponse,
    RuntimeAttributeItemResponse,
    SessionRuntimeAttributesResponse,
)
from backend.app.api.schemas.sessions import (
    PlayerActionRequest,
    SessionCreateRequest,
    SessionSnapshotResponse,
    SwitchPlayerCharacterRequest,
)
from backend.app.core.container import AppContainer

router = APIRouter(prefix="/api/sessions", tags=["sessions"])


@router.post("", response_model=SessionSnapshotResponse)
def create_session(payload: SessionCreateRequest, container: AppContainer = Depends(get_app_container)):
    try:
        session = container.session_orchestrator.create_session(
            world_id=payload.world_id,
            player_character_id=payload.player_character_id,
        )
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return SessionSnapshotResponse.from_domain(container.runtime_visibility.build_player_session_view(session))


@router.get("/{session_id}", response_model=SessionSnapshotResponse)
def get_session(session_id: str, container: AppContainer = Depends(get_app_container)):
    session = container.session_queries.get_session(session_id)
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    session = container.session_orchestrator.materialize_missing_session_assets(session)
    return SessionSnapshotResponse.from_domain(container.runtime_visibility.build_player_session_view(session))


@router.post("/{session_id}/actions", response_model=SessionSnapshotResponse)
def submit_player_action(
    session_id: str,
    payload: PlayerActionRequest,
    container: AppContainer = Depends(get_app_container),
):
    try:
        session = container.session_orchestrator.submit_player_action(
            session_id=session_id,
            content=payload.model_dump()["content"],
            resend_from_turn_index=payload.resend_from_turn_index,
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    return SessionSnapshotResponse.from_domain(container.runtime_visibility.build_player_session_view(session))


@router.post("/{session_id}/actions/stream")
async def stream_player_action(
    session_id: str,
    payload: PlayerActionRequest,
    container: AppContainer = Depends(get_app_container),
):
    session = container.session_queries.get_session(session_id)
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")

    queue = container.session_runtime.subscribe(session_id)
    result: dict[str, object] = {}
    completed = threading.Event()

    def run_action() -> None:
        try:
            result["session"] = container.session_orchestrator.submit_player_action(
                session_id=session_id,
                content=payload.model_dump()["content"],
                resend_from_turn_index=payload.resend_from_turn_index,
            )
        except ValueError as exc:
            result["error"] = str(exc)
        except Exception as exc:  # pragma: no cover - defensive stream fallback
            result["error"] = str(exc)
        finally:
            completed.set()

    threading.Thread(target=run_action, daemon=True).start()

    async def event_stream():
        last_snapshot_json: str | None = None

        def encode_event(event_name: str, body: dict[str, object]) -> str:
            payload_json = json.dumps(body, ensure_ascii=False)
            return f"event: {event_name}\ndata: {payload_json}\n\n"

        def build_snapshot_event(snapshot) -> str:
            nonlocal last_snapshot_json
            response_payload = SessionSnapshotResponse.from_domain(
                container.runtime_visibility.build_player_session_view(snapshot)
            ).model_dump()
            serialized = json.dumps(response_payload, ensure_ascii=False)
            last_snapshot_json = serialized
            return encode_event("session.snapshot", {"type": "session.snapshot", "payload": response_payload})

        try:
            while True:
                try:
                    snapshot = await asyncio.wait_for(queue.get(), timeout=0.1)
                    yield build_snapshot_event(snapshot)
                    continue
                except asyncio.TimeoutError:
                    pass

                if not completed.is_set():
                    continue

                while not queue.empty():
                    try:
                        snapshot = queue.get_nowait()
                    except asyncio.QueueEmpty:
                        break
                    yield build_snapshot_event(snapshot)

                error_detail = result.get("error")
                if isinstance(error_detail, str) and error_detail.strip():
                    yield encode_event("error", {"type": "error", "detail": error_detail})
                    break

                final_session = result.get("session")
                if final_session is not None:
                    response_payload = SessionSnapshotResponse.from_domain(
                        container.runtime_visibility.build_player_session_view(final_session)
                    ).model_dump()
                    serialized = json.dumps(response_payload, ensure_ascii=False)
                    if serialized != last_snapshot_json:
                        yield encode_event("session.snapshot", {"type": "session.snapshot", "payload": response_payload})

                yield encode_event("done", {"type": "done"})
                break
        finally:
            container.session_runtime.unsubscribe(session_id, queue)

    return StreamingResponse(event_stream(), media_type="text/event-stream")


@router.post("/{session_id}/switch-character", response_model=SessionSnapshotResponse)
def switch_player_character(
    session_id: str,
    payload: SwitchPlayerCharacterRequest,
    container: AppContainer = Depends(get_app_container),
):
    session = container.session_orchestrator.switch_player_character(
        session_id=session_id,
        player_character_id=payload.player_character_id,
        proposal_payload=payload.proposal.model_dump() if payload.proposal else None,
    )
    if session is None:
        raise HTTPException(status_code=404, detail="Session or character not found")
    return SessionSnapshotResponse.from_domain(container.runtime_visibility.build_player_session_view(session))


@router.get("/{session_id}/runtime-attributes", response_model=SessionRuntimeAttributesResponse)
def get_runtime_attributes(session_id: str, container: AppContainer = Depends(get_app_container)):
    session = container.session_queries.get_session(session_id)
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    session_items, character_groups = container.attribute_runtime.list_player_visible_attributes(session_id=session_id)

    return SessionRuntimeAttributesResponse(
        session_id=session_id,
        session_attributes=[
            RuntimeAttributeItemResponse(
                schema_id=item.schema.id,
                key=item.schema.key,
                label=item.schema.label,
                value_type=item.schema.value_type,
                value=item.value.value,
                source=item.value.source,
                display_policy=item.schema.display_policy,
                influence_policy=item.schema.influence_policy,
            )
            for item in session_items
        ],
        character_attributes=[
            RuntimeAttributeGroupResponse(
                owner_type=group.owner_type,
                owner_id=group.owner_id,
                owner_label=group.owner_label,
                items=[
                    RuntimeAttributeItemResponse(
                        schema_id=item.schema.id,
                        key=item.schema.key,
                        label=item.schema.label,
                        value_type=item.schema.value_type,
                        value=item.value.value,
                        source=item.value.source,
                        display_policy=item.schema.display_policy,
                        influence_policy=item.schema.influence_policy,
                    )
                    for item in group.items
                ],
            )
            for group in character_groups
        ],
    )
