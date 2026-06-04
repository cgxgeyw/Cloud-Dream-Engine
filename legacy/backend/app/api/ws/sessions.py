from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from backend.app.api.schemas.sessions import SessionSnapshotResponse
from backend.app.core.container import get_container

router = APIRouter(tags=["ws"])


@router.websocket("/ws/sessions/{session_id}")
async def session_stream(websocket: WebSocket, session_id: str):
    container = get_container()
    await websocket.accept()

    session = container.session_queries.get_session(session_id)
    if session is None:
        await websocket.send_json({"type": "error", "detail": "Session not found"})
        await websocket.close()
        return

    queue = container.session_runtime.subscribe(session_id)

    await websocket.send_json(
        {
            "type": "session.snapshot",
            "payload": SessionSnapshotResponse.from_domain(
                container.runtime_visibility.build_player_session_view(session)
            ).model_dump(),
        }
    )

    try:
        while True:
            snapshot = await queue.get()
            await websocket.send_json(
                {
                    "type": "session.snapshot",
                    "payload": SessionSnapshotResponse.from_domain(
                        container.runtime_visibility.build_player_session_view(snapshot)
                    ).model_dump(),
                }
            )
    except WebSocketDisconnect:
        pass
    finally:
        container.session_runtime.unsubscribe(session_id, queue)
