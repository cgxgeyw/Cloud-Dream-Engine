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

    /// M1: 发送失败不应被静默吞掉,否则 UI 收不到更新会显示过期状态而无任何线索。
    /// 此变体在失败时记录日志(debug 构建打到 stderr),供故障排查。
    pub fn emit_snapshot_logged(app: &AppHandle, session_id: &str, snapshot: &SessionSnapshot) {
        if let Err(err) = Self::emit_snapshot(app, session_id, snapshot) {
            #[cfg(debug_assertions)]
            eprintln!("[session_events] emit snapshot for {session_id} failed: {err}");
            #[cfg(not(debug_assertions))]
            let _ = err;
        }
    }
}
