use crate::models::session::SessionSnapshot;
use tauri::{AppHandle, Emitter};

pub struct SessionEventEmitter;

impl SessionEventEmitter {
    pub fn emit_snapshot(
        app: &AppHandle,
        session_id: &str,
        snapshot: &SessionSnapshot,
    ) -> Result<(), String> {
        app.emit(&format!("session:{}:snapshot", session_id), snapshot)
            .map_err(|e: tauri::Error| e.to_string())
    }
}
