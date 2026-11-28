use rusqlite::{params, Connection};
use tauri::State;

use crate::models::oag::{OagActor, OagQueryContext, OagQueryRequest, OagQueryResponse};
use crate::services::oag::canon;
use crate::services::oag::query;
use crate::state::AppState;

#[tauri::command]
pub async fn debug_oag_query(
    state: State<'_, AppState>,
    session_id: String,
    actor: OagActor,
    as_of_turn: Option<i32>,
    max_nodes: Option<usize>,
    request: OagQueryRequest,
) -> Result<OagQueryResponse, String> {
    let db = state.db.lock().await;
    query_impl(
        db.conn(),
        &session_id,
        actor,
        as_of_turn,
        max_nodes,
        request,
    )
}

fn query_impl(
    conn: &Connection,
    session_id: &str,
    actor: OagActor,
    as_of_turn: Option<i32>,
    max_nodes: Option<usize>,
    request: OagQueryRequest,
) -> Result<OagQueryResponse, String> {
    let (world_id, current_turn) = query_session_scope(conn, session_id)?;
    let ontology = canon::load_active_ontology(conn, &world_id)?
        .ok_or_else(|| "当前世界尚未导入 OAG 本体".to_string())?;
    let context = OagQueryContext {
        world_id,
        session_id: session_id.to_string(),
        actor,
        as_of_turn: as_of_turn.unwrap_or(current_turn),
        max_nodes: max_nodes.unwrap_or(200),
    };
    query::query(conn, context, ontology, request)
}

fn query_session_scope(conn: &Connection, session_id: &str) -> Result<(String, i32), String> {
    if let Ok((world_id, turn_index)) = conn.query_row(
        "SELECT world_id, turn_index FROM game_turn_states
         WHERE session_id = ?1 AND world_id <> ''",
        params![session_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
    ) {
        return Ok((world_id, turn_index.max(0)));
    }
    let (world_id, turn_index) = conn
        .query_row(
            "SELECT m.world_id, COALESCE(MAX(j.turn_index), 0)
             FROM memories m
             LEFT JOIN turn_journal j ON j.session_id = m.session_id
             WHERE m.session_id = ?1 AND m.world_id <> ''
             GROUP BY m.world_id
             ORDER BY MAX(j.turn_index) DESC, m.world_id LIMIT 1",
            params![session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
        )
        .map_err(|_| "无法确定会话对应的世界，请先完成一次回合写入".to_string())?;
    Ok((world_id, turn_index.max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{oag_schema, schema};

    #[test]
    fn session_scope_prefers_game_state_and_rejects_missing_world() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn.execute(
            "INSERT INTO game_turn_states
             (session_id, world_id, phase, active_actor, turn_index, sequence, terminal,
              last_handler, updated_at)
             VALUES ('session', 'world', 'idle', '', 7, 0, 0, '', '')",
            [],
        )
        .unwrap();
        assert_eq!(
            query_session_scope(&conn, "session").unwrap(),
            ("world".to_string(), 7)
        );
        assert!(query_session_scope(&conn, "missing").is_err());
    }
}
