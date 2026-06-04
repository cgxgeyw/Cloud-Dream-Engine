Legacy baseline imported from `E:\code\gpt54\legacy`.

Purpose:

- Preserve the Python/FastAPI behavior reference inside the active migration workspace.
- Keep the old API, service, domain, and test code available while the Rust/Tauri core reaches parity.

Rules:

- Treat `legacy/backend` as a reference implementation, not the target runtime.
- Do not add new product features here unless they are strictly needed to clarify migration behavior.
- Exclude caches, local build artifacts, and generated metadata from this baseline copy.

Current status:

- `frontend/` in this workspace already matches the Tauri web frontend line from `E:\code\gpt54\desktop\frontend`.
- `legacy/backend/` is the imported Python baseline from the archived FastAPI stack.
- `src-tauri/` remains the target replacement implementation.
