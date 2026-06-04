# Dream Narrative Engine Backend

FastAPI backend scaffold matching the layered architecture in `系统架构终版.md`.

## Structure

- `app/api`: REST and WebSocket entrypoints
- `app/application`: use-case and query services
- `app/domain`: core entities and repository protocols
- `app/infrastructure`: in-memory repository implementations and wiring
- `app/plugins`: plugin host placeholder
- `app/tests`: backend tests

## Run

```bash
python -m uvicorn backend.app.main:app --reload --app-dir .
```
