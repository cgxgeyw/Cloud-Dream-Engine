//! Host-side world logic. UI and character tools share the package's JavaScript
//! rules and commit session KV writes atomically under the database lock.
use crate::models::world::WorldDefinition;
use crate::services::llm::client::ChatToolDefinition;
use rquickjs::{Context, Runtime};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

const MAX_HANDLER_NAME_BYTES: usize = 128;
const MAX_WRITES_PER_CALL: usize = 256;
const MAX_TRACE_PAYLOAD_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameTurnTransition {
    pub known: bool,
    pub command_id: String,
    pub actor: String,
    pub previous_actor: Option<String>,
    pub next_actor: Option<String>,
    pub phase: Option<String>,
    pub turn_index: i64,
    pub sequence: i64,
    pub continue_action: bool,
    pub turn_ended: bool,
    pub game_over: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameCommandResponse {
    pub result: Value,
    pub transition: GameTurnTransition,
}

/// Unified event boundary for world UI and rule workflows.
///
/// Narrative player messages are handled by the session orchestrator; all
/// deterministic world actions use this boundary and return the same
/// transition/status contract.
pub fn dispatch_event(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    event: crate::services::workflow::WorldEvent,
) -> Result<crate::services::workflow::WorldEventResult, String> {
    let Some((handler, input)) = event.handler_input() else {
        return Err("This world event is handled by the session workflow".to_string());
    };
    let actor = event.actor();
    let command = resolve_action(
        conn,
        world,
        session_id,
        handler,
        input.clone(),
        actor,
        None,
    )?;
    Ok(crate::services::workflow::WorldEventResult::from_command(
        event,
        command.result,
        command.transition,
    ))
}

pub fn tool_definitions(world: &WorldDefinition) -> Vec<ChatToolDefinition> {
    let logic = &world.ui_theme_config["logic"];
    if logic["execution"] != "host" {
        return Vec::new();
    }
    logic["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| serde_json::from_value(tool.clone()).ok())
        .collect()
}

pub fn tool_handler(world: &WorldDefinition, name: &str, speaker: &str) -> Option<String> {
    if world.ui_theme_config["logic"]["execution"] != "host" {
        return None;
    }
    world.ui_theme_config["logic"]["tools"]
        .as_array()?
        .iter()
        .find_map(|tool| {
            if tool["name"] != name || !tool["speakers"].as_array()?.iter().any(|v| v == speaker) {
                return None;
            }
            tool["handler"].as_str().map(str::to_owned)
        })
}

fn handler_is_declared(world: &WorldDefinition, handler: &str) -> bool {
    if handler.is_empty()
        || handler.len() > MAX_HANDLER_NAME_BYTES
        || !handler
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        return false;
    }
    let logic = &world.ui_theme_config["logic"];
    let listed = |key: &str| {
        logic[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|value| value == handler)
    };
    listed("ui_handlers")
        || logic["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|tool| tool["handler"].as_str() == Some(handler))
        || logic["events"]
            .as_object()
            .into_iter()
            .flatten()
            .any(|(_, value)| value.as_str() == Some(handler))
        || logic["agent_turn"]["view_handler"].as_str() == Some(handler)
}

fn turn_protocol(world: &WorldDefinition) -> Option<&serde_json::Map<String, Value>> {
    world.ui_theme_config["logic"]["turn_protocol"].as_object()
}

fn command_actor(world: &WorldDefinition, handler: &str) -> Option<String> {
    turn_protocol(world)?
        .get("command_handlers")?
        .as_object()?
        .get(handler)
        .and_then(Value::as_str)
        .filter(|actor| matches!(*actor, "player" | "agent" | "system"))
        .map(str::to_owned)
}

fn protocol_handler_list_contains(world: &WorldDefinition, key: &str, handler: &str) -> bool {
    turn_protocol(world)
        .and_then(|protocol| protocol.get(key))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|value| value.as_str() == Some(handler))
}

fn protocol_state_handler(world: &WorldDefinition) -> Option<&str> {
    turn_protocol(world)?
        .get("state_handler")
        .and_then(Value::as_str)
        .filter(|handler| handler_is_declared(world, handler))
}

fn canonical_actor(world: &WorldDefinition, raw: &str) -> Option<String> {
    let protocol = turn_protocol(world)?;
    if let Some(values) = protocol.get("actor_values").and_then(Value::as_object) {
        for (actor, value) in values {
            if value.as_str() == Some(raw) {
                return Some(actor.clone());
            }
        }
    }
    match raw {
        "player" => Some("player".to_string()),
        "agent" | "devil" | "npc" => Some("agent".to_string()),
        _ => None,
    }
}

fn state_field<'a>(protocol: &'a serde_json::Map<String, Value>, key: &str, fallback: &'a str) -> &'a str {
    protocol.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn derive_turn_from_view(
    world: &WorldDefinition,
    view: &Value,
    actor: &str,
    previous: Option<&GameTurnState>,
    command_id: String,
) -> GameTurnTransition {
    let Some(protocol) = turn_protocol(world) else {
        return GameTurnTransition {
            known: false,
            command_id,
            actor: actor.to_string(),
            previous_actor: previous.map(|state| state.active_actor.clone()),
            next_actor: None,
            phase: None,
            turn_index: previous.map(|state| state.turn_index).unwrap_or(0),
            sequence: previous.map(|state| state.sequence).unwrap_or(0),
            continue_action: false,
            turn_ended: false,
            game_over: false,
        };
    };
    let phase_name = state_field(protocol, "phase_field", "phase");
    let actor_name = state_field(protocol, "active_actor_field", "turn");
    let phase = view.get(phase_name).and_then(Value::as_str).map(str::to_owned);
    let raw_actor = view.get(actor_name).and_then(Value::as_str);
    let next_actor = raw_actor.and_then(|raw| canonical_actor(world, raw));
    let terminal_values = protocol
        .get("terminal_phase_values")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![Value::String("over".to_string()), Value::String("ended".to_string())]);
    let game_over = phase
        .as_deref()
        .map(|value| terminal_values.iter().any(|item| item.as_str() == Some(value)))
        .unwrap_or(false);
    let turn_ended = game_over || next_actor.as_deref().is_some_and(|next| next != actor);
    GameTurnTransition {
        known: next_actor.is_some() || phase.is_some(),
        command_id,
        actor: actor.to_string(),
        previous_actor: previous.map(|state| state.active_actor.clone()),
        next_actor: next_actor.clone(),
        phase,
        turn_index: previous.map(|state| state.turn_index + i64::from(turn_ended)).unwrap_or(0),
        sequence: previous.map(|state| state.sequence + 1).unwrap_or(1),
        continue_action: !turn_ended && !game_over,
        turn_ended,
        game_over,
    }
}

#[derive(Debug, Clone)]
struct GameTurnState {
    active_actor: String,
    turn_index: i64,
    sequence: i64,
    terminal: bool,
}

fn load_turn_state(conn: &Connection, session_id: &str) -> Result<Option<GameTurnState>, String> {
    conn.query_row(
        "SELECT world_id, phase, active_actor, turn_index, sequence, terminal
         FROM game_turn_states WHERE session_id = ?1",
        rusqlite::params![session_id],
        |row| {
            Ok(GameTurnState {
                active_actor: row.get(2)?,
                turn_index: row.get(3)?,
                sequence: row.get(4)?,
                terminal: row.get::<_, i64>(5)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn persist_turn_state(
    conn: &Connection,
    session_id: &str,
    world: &WorldDefinition,
    transition: &GameTurnTransition,
    handler: &str,
) -> Result<(), String> {
    let Some(next_actor) = transition.next_actor.as_deref() else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO game_turn_states
         (session_id, world_id, phase, active_actor, turn_index, sequence, terminal, last_handler, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(session_id) DO UPDATE SET
           world_id = excluded.world_id, phase = excluded.phase, active_actor = excluded.active_actor,
           turn_index = excluded.turn_index, sequence = excluded.sequence, terminal = excluded.terminal,
           last_handler = excluded.last_handler, updated_at = excluded.updated_at",
        rusqlite::params![
            session_id,
            world.id,
            transition.phase.as_deref().unwrap_or_default(),
            next_actor,
            transition.turn_index,
            transition.sequence,
            i64::from(transition.game_over),
            handler,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn bounded_json(value: &Value) -> String {
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    if encoded.len() <= MAX_TRACE_PAYLOAD_BYTES {
        return encoded;
    }
    // 调试记录有固定上限，避免世界脚本把 trace 表当作无限日志。
    let limit = MAX_TRACE_PAYLOAD_BYTES.saturating_sub(32);
    let end = encoded
        .char_indices()
        .take_while(|(index, _)| *index < limit)
        .map(|(index, _)| index)
        .last()
        .unwrap_or(0);
    let truncated = &encoded[..end];
    format!("{}...<truncated>", truncated)
}

fn record_trace(
    conn: &Connection,
    trace_id: &str,
    request_id: &str,
    world: &WorldDefinition,
    session_id: &str,
    actor: &str,
    handler: &str,
    started_at: std::time::Instant,
    result: &Result<Value, String>,
    input: &Value,
) {
    let (status, result_json, error) = match result {
        Ok(value) => ("completed", bounded_json(value), String::new()),
        Err(error) => ("failed", "null".to_string(), error.chars().take(1024).collect()),
    };
    // 测试和旧的嵌入式调用者可能使用精简 schema；缺少调试表时不应改变业务结果。
    let _ = conn.execute(
        "INSERT INTO world_logic_traces
         (trace_id, request_id, world_id, session_id, actor, handler, status, latency_ms,
          input_json, result_json, error, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            trace_id,
            request_id,
            world.id,
            session_id,
            actor,
            handler,
            status,
            started_at.elapsed().as_millis().min(i64::MAX as u128) as i64,
            bounded_json(input),
            result_json,
            error,
            chrono::Utc::now().to_rfc3339(),
        ],
    );
}

/// 统一的宿主边界：所有 UI 和 agent 调用都生成 request/trace id，并记录成功或失败。
pub fn invoke(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    handler: &str,
    input: Value,
    actor: &str,
) -> Result<Value, String> {
    let trace_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let started_at = std::time::Instant::now();
    let result = invoke_untraced(conn, world, session_id, handler, input.clone(), actor);
    record_trace(
        conn,
        &trace_id,
        &request_id,
        world,
        session_id,
        actor,
        handler,
        started_at,
        &result,
        &input,
    );
    result
}

/// Framework command boundary. World logic owns rule calculation; this layer owns
/// actor validation, turn ownership, transition persistence, and the normalized result.
pub fn resolve_action(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    handler: &str,
    input: Value,
    actor: &str,
    actor_name: Option<&str>,
) -> Result<GameCommandResponse, String> {
    let expected_actor = command_actor(world, handler);
    let initializes_state = protocol_handler_list_contains(world, "initialize_handlers", handler);
    if initializes_state && actor != "player" && actor != "system" {
        return Err("Only the player or system can initialize a game turn state".into());
    }
    if let Some(expected_actor) = expected_actor.as_deref() {
        if expected_actor != actor {
            return Err(format!(
                "This game command belongs to `{expected_actor}`, not `{actor}`"
            ));
        }
        if actor == "agent" {
            let speaker = actor_name.ok_or("Agent game commands require a speaker")?;
            if !world.ui_theme_config["logic"]["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|tool| {
                    tool["handler"].as_str() == Some(handler)
                        && tool["speakers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|value| value.as_str() == Some(speaker))
                })
            {
                return Err("Agent is not authorized for this game command".into());
            }
        }
    }
    let previous = load_turn_state(conn, session_id)?;
    if let Some(expected_actor) = expected_actor.as_deref() {
        let current = if previous.is_some() {
            previous.clone()
        } else if let Some(state_handler) = protocol_state_handler(world) {
            let view = invoke(conn, world, session_id, state_handler, json!({}), "system")?;
            let initial = derive_turn_from_view(world, &view, actor, None, String::new());
            initial.next_actor.map(|active_actor| GameTurnState {
                active_actor,
                turn_index: 0,
                sequence: 0,
                terminal: initial.game_over,
            })
        } else {
            None
        };
        if let Some(current) = current {
            if !current.terminal && current.active_actor != expected_actor {
                return Err(format!(
                    "It is `{}`'s turn; `{expected_actor}` cannot act now",
                    current.active_actor
                ));
            }
        }
    }

    let result = invoke(conn, world, session_id, handler, input, actor)?;
    let command_id = uuid::Uuid::new_v4().to_string();
    let mut transition = if let Some(state_handler) = protocol_state_handler(world) {
        let view = invoke(conn, world, session_id, state_handler, json!({}), "system")?;
        derive_turn_from_view(
            world,
            &view,
            actor,
            previous.as_ref(),
            command_id,
        )
    } else {
        derive_turn_from_view(world, &result, actor, previous.as_ref(), command_id)
    };
    if let Some(turn_ended) = result.get("turn_ended").and_then(Value::as_bool) {
        transition.turn_ended = transition.turn_ended || turn_ended;
        transition.continue_action = !transition.turn_ended && !transition.game_over;
    }
    if (expected_actor.is_some() || initializes_state) && transition.known {
        persist_turn_state(conn, session_id, world, &transition, handler)?;
    }
    Ok(GameCommandResponse { result, transition })
}

/// A world-owned turn must finish through tools before the character can return dialogue.
pub fn apply_agent_turn_tool_choice(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    speaker: &str,
    request: &mut crate::services::llm::client::ChatRequest,
) -> Result<(), String> {
    if !request.tools.as_ref().is_some_and(|tools| tools.iter().any(|tool| {
        tool_handler(world, &tool.name, speaker).is_some()
    })) {
        return Ok(());
    }
    let trigger = &world.ui_theme_config["logic"]["agent_turn"];
    let required = if let Some(state_handler) = protocol_state_handler(world) {
        let view = invoke(conn, world, session_id, state_handler, json!({}), "system")?;
        let transition = derive_turn_from_view(world, &view, "agent", None, String::new());
        transition.next_actor.as_deref() == Some("agent") && transition.phase.as_deref() != Some("over")
    } else {
        let (Some(handler), Some(field)) = (trigger["view_handler"].as_str(), trigger["when_field"].as_str()) else {
            return Ok(());
        };
        let view = invoke(conn, world, session_id, handler, json!({}), "system")?;
        view[field] == true
    };
    request.tool_choice = Some(if required {
        crate::services::llm::client::ChatToolChoice::Required
    } else {
        crate::services::llm::client::ChatToolChoice::None
    });
    Ok(())
}

fn invoke_untraced(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    handler: &str,
    input: Value,
    actor: &str,
) -> Result<Value, String> {
    let logic = &world.ui_theme_config["logic"];
    if !matches!(actor, "player" | "agent" | "system") {
        return Err("Unsupported world logic actor".into());
    }
    if !handler_is_declared(world, handler) {
        return Err("World logic handler is not declared by this world".into());
    }
    if input.to_string().len() > 256 * 1024 {
        return Err("World logic input too large".into());
    }
    if logic["runtime"] != "sandbox-js-v1" || logic["execution"] != "host" {
        return Err("World has no host logic".into());
    }
    if actor == "player"
        && !logic["ui_handlers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v == handler)
    {
        return Err("World logic handler is not exposed to the UI".into());
    }
    let belongs: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1 AND world_name = ?2)",
            rusqlite::params![session_id, world.name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !belongs {
        return Err("Session does not belong to this world".into());
    }
    let namespaces = world.ui_theme_config["storage"]["kv_namespaces"]
        .as_array()
        .ok_or("Host logic requires declared KV namespaces")?;
    let repo = crate::db::repositories::world_kv_repo::WorldKvRepository::new(conn);
    let mut snapshot = serde_json::Map::new();
    for ns in namespaces.iter().filter_map(Value::as_str) {
        let entries = repo.list("session", session_id, ns)?;
        snapshot.insert(
            ns.into(),
            entries
                .into_iter()
                .map(|e| (e.key, e.value))
                .collect::<serde_json::Map<_, _>>()
                .into(),
        );
    }
    let source = logic["source"]
        .as_str()
        .ok_or("Missing world logic source")?;
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(
            logic["timeout_ms"]
                .as_u64()
                .unwrap_or(1000)
                .clamp(100, 5000),
        );
    runtime.set_interrupt_handler(Some(Box::new(move || {
        std::time::Instant::now() >= deadline
    })));
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    let data = json!({"kv": snapshot, "handler": handler, "input": input, "actor": actor});
    let script = format!(
        r#"
const __data = {data};
const __handlers = new Map();
const __writes = [];
const world = Object.freeze({{register: (name, fn) => __handlers.set(name, fn)}});
const __ns = (ns, options) => {{
  if (!options || options.scope !== 'session') throw Error('Host logic only exposes session KV');
  if (!Object.hasOwn(__data.kv, ns)) throw Error('Undeclared KV namespace');
  return __data.kv[ns];
}};
const __api = Object.freeze({{kv: Object.freeze({{
  get: async (ns, key, fallback = null, options) => {{const bag = __ns(ns, options); return Object.hasOwn(bag, key) ? bag[key] : fallback;}},
  set: async (ns, key, value, options) => {{__ns(ns, options)[key] = value; __writes.push({{ns, key, value}}); return {{namespace: ns, key, value}};}},
  remove: async (ns, key, options) => {{delete __ns(ns, options)[key]; __writes.push({{ns, key, remove: true}});}},
  list: async (ns, options) => Object.entries(__ns(ns, options)).map(([key,value]) => ({{namespace: ns,key,value}}))
}})}});
{source}
let __done = false, __result;
(async () => {{
  try {{
    const fn = __handlers.get(__data.handler);
    if (!fn) throw Error('Unknown world logic handler');
    const result = await fn(__data.input, __api, Object.freeze({{actor: __data.actor}}));
    __result = JSON.stringify({{ok: true, result: result ?? null, writes: __writes}});
  }} catch (error) {{__result = JSON.stringify({{ok: false, error: String(error)}});}}
  __done = true;
}})();
"#
    );
    context
        .with(|ctx| ctx.eval::<(), _>(script))
        .map_err(|e| format!("World logic: {e}"))?;
    loop {
        if context
            .with(|ctx| ctx.eval::<bool, _>("__done"))
            .unwrap_or(false)
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("World logic timed out".into());
        }
        if !runtime
            .execute_pending_job()
            .map_err(|e| format!("World logic job: {e}"))?
        {
            return Err("World logic did not complete".into());
        }
    }
    let encoded: String = context
        .with(|ctx| ctx.eval("__result"))
        .map_err(|e| e.to_string())?;
    if encoded.len() > 256 * 1024 {
        return Err("World logic result too large".into());
    }
    let output: Value = serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
    if output["ok"] != true {
        return Err(output["error"]
            .as_str()
            .unwrap_or("World logic failed")
            .into());
    }
    let writes = output["writes"]
        .as_array()
        .ok_or("World logic returned invalid writes")?;
    if writes.len() > MAX_WRITES_PER_CALL {
        return Err("World logic produced too many KV writes".into());
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let repo = crate::db::repositories::world_kv_repo::WorldKvRepository::new(&tx);
    for write in writes {
        let ns = write["ns"].as_str().ok_or("Invalid namespace")?;
        if !namespaces.iter().any(|declared| declared == ns) {
            return Err("Undeclared KV namespace".into());
        }
        let key = write["key"].as_str().ok_or("Invalid key")?;
        if write["remove"] == true {
            repo.delete("session", session_id, ns, key)?;
        } else {
            repo.set("session", session_id, ns, key, &write["value"])?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(output["result"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::world_kv_repo::WorldKvRepository;

    fn setup() -> (Connection, WorldDefinition) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, world_name TEXT);
            CREATE TABLE scoped_kv (owner_type TEXT, owner_id TEXT, namespace TEXT, key TEXT,
            value_json TEXT, updated_at TEXT, PRIMARY KEY(owner_type,owner_id,namespace,key));
            CREATE TABLE world_logic_traces (
                trace_id TEXT PRIMARY KEY, request_id TEXT NOT NULL, world_id TEXT NOT NULL,
                session_id TEXT NOT NULL, actor TEXT NOT NULL, handler TEXT NOT NULL,
                status TEXT NOT NULL, latency_ms INTEGER NOT NULL, input_json TEXT NOT NULL,
                result_json TEXT NOT NULL, error TEXT NOT NULL, created_at TEXT NOT NULL
            );
            CREATE TABLE game_turn_states (
                session_id TEXT PRIMARY KEY, world_id TEXT NOT NULL, phase TEXT NOT NULL,
                active_actor TEXT NOT NULL, turn_index INTEGER NOT NULL, sequence INTEGER NOT NULL,
                terminal INTEGER NOT NULL, last_handler TEXT NOT NULL, updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
        let mut package: Value = serde_json::from_str(include_str!(
            "../../../examples/world-packages/poison-feast/world/world.json"
        ))
        .unwrap();
        package["id"] = json!("poison-test");
        package["director_system_prompt_base"] = json!("");
        package["director_runtime_system_prompt"] = json!("");
        package["opening_character_ids"] = json!([]);
        package["player_character_id"] = Value::Null;
        package["ui_theme_config"] =
            json!({"logic":package["logic"], "storage":package["storage"]});
        package["ui_theme_config"]["logic"]["source"] = json!(include_str!(
            "../../../examples/world-packages/poison-feast/world/logic.js"
        ));
        let world: WorldDefinition = serde_json::from_value(package).unwrap();
        conn.execute("INSERT INTO sessions VALUES ('s1',?1)", [&world.name])
            .unwrap();
        (conn, world)
    }

    #[test]
    fn ui_and_agent_use_same_runtime_with_private_views() {
        let (conn, world) = setup();
        let opening = invoke(&conn, &world, "s1", "game.newGame", json!({}), "player").unwrap();
        assert_eq!(opening["turn"], "player");
        let repo = WorldKvRepository::new(&conn);
        let mut state = repo
            .get("session", "s1", "game", "state")
            .unwrap()
            .unwrap()
            .value;
        state["tubes"] = json!(["poison", "water", "poison"]);
        state["consumed"] = json!([]);
        state["turn"] = json!("devil");
        state["awaiting_devil"] = json!(true);
        state["devil_items"] = json!({"needle":1,"strong":1});
        repo.set("session", "s1", "game", "state", &state).unwrap();
        assert!(invoke(&conn, &world, "s1", "game.devilView", json!({}), "player").is_err());
        let peek = invoke(
            &conn,
            &world,
            "s1",
            "game.devilAct",
            json!({"action":"use_needle"}),
            "agent",
        )
        .unwrap();
        assert_eq!(peek["turn_ended"], false);
        assert!(peek["state"].as_str().unwrap().contains("【毒药】"));
        let view = invoke(&conn, &world, "s1", "game.getView", json!({}), "player").unwrap();
        assert_eq!(view["player_know_code"], 0);
        assert_eq!(view["tube_slots"][0]["kind"], "unknown");
        invoke(
            &conn,
            &world,
            "s1",
            "game.devilAct",
            json!({"action":"use_strong"}),
            "agent",
        )
        .unwrap();
        let drink = invoke(
            &conn,
            &world,
            "s1",
            "game.devilAct",
            json!({"action":"drink_opponent"}),
            "agent",
        )
        .unwrap();
        assert_eq!(drink["turn_ended"], true);
        let view = invoke(&conn, &world, "s1", "game.getView", json!({}), "player").unwrap();
        assert_eq!(view["player_candles"], json!([1, 0, 0]));
        assert_eq!(view["turn"], "player");
        assert_eq!(tool_definitions(&world).len(), 2);
        assert!(tool_handler(&world, "poison_feast_act", "Other NPC").is_none());
        let trace_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM world_logic_traces WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(trace_count >= 6);
    }

    #[test]
    fn agent_turn_requires_tools_until_rules_hand_back_to_player() {
        let (conn, world) = setup();
        invoke(&conn, &world, "s1", "game.newGame", json!({}), "player").unwrap();
        let repo = WorldKvRepository::new(&conn);
        let mut state = repo.get("session", "s1", "game", "state").unwrap().unwrap().value;
        state["tubes"] = json!(["water", "poison", "water"]);
        state["consumed"] = json!([]);
        state["turn"] = json!("devil");
        state["awaiting_devil"] = json!(true);
        state["devil_items"] = json!({"incense": 2});
        repo.set("session", "s1", "game", "state", &state).unwrap();
        let mut request: crate::services::llm::client::ChatRequest =
            serde_json::from_value(json!({"model":"test", "messages":[]})).unwrap();
        request.tools = Some(tool_definitions(&world));
        let speaker = "\u{5c0f}\u{6076}\u{9b54}";
        apply_agent_turn_tool_choice(&conn, &world, "s1", speaker, &mut request).unwrap();
        assert!(matches!(request.tool_choice, Some(crate::services::llm::client::ChatToolChoice::Required)));
        for action in ["use_incense", "drink_opponent"] {
            let handler = tool_handler(&world, "poison_feast_act", speaker).unwrap();
            let result = invoke(&conn, &world, "s1", &handler, json!({"action":action}), "agent").unwrap();
            assert_eq!(result["applied"], true);
            assert_eq!(result["turn_ended"], false);
            apply_agent_turn_tool_choice(&conn, &world, "s1", speaker, &mut request).unwrap();
            assert!(matches!(request.tool_choice, Some(crate::services::llm::client::ChatToolChoice::Required)));
        }
        let result = invoke(&conn, &world, "s1", "game.devilAct", json!({"action":"drink_opponent"}), "agent").unwrap();
        assert_eq!(result["turn_ended"], true);
        apply_agent_turn_tool_choice(&conn, &world, "s1", speaker, &mut request).unwrap();
        assert!(matches!(request.tool_choice, Some(crate::services::llm::client::ChatToolChoice::None)));
    }

    #[test]
    fn failed_handlers_do_not_commit_and_session_is_validated() {
        let (conn, mut world) = setup();
        world.ui_theme_config["logic"]["source"] = json!(
            r#"world.register('game.newGame',async (_,api)=>{
            await api.kv.set('game','test',1,{scope:'session'}); throw Error('abort');
        });"#
        );
        assert!(invoke(&conn, &world, "s1", "game.newGame", json!({}), "player").is_err());
        assert!(WorldKvRepository::new(&conn)
            .get("session", "s1", "game", "test")
            .unwrap()
            .is_none());
        assert!(invoke(
            &conn,
            &world,
            "other-session",
            "game.newGame",
            json!({}),
            "player"
        )
        .is_err());
        assert!(invoke(&conn, &world, "s1", "internal.not_declared", json!({}), "agent").is_err());
        world.ui_theme_config["logic"]["source"] =
            json!("world.register('game.newGame',()=>{while(true){}});");
        world.ui_theme_config["logic"]["timeout_ms"] = json!(100);
        assert!(invoke(&conn, &world, "s1", "game.newGame", json!({}), "player").is_err());
    }

    #[test]
    fn framework_command_protocol_owns_turn_handoff() {
        let (conn, world) = setup();
        let started = resolve_action(
            &conn, &world, "s1", "game.newGame", json!({}), "player", None,
        )
        .unwrap();
        assert_eq!(started.transition.next_actor.as_deref(), Some("player"));
        assert!(started.transition.known);
        let repo = WorldKvRepository::new(&conn);
        let mut state = repo
            .get("session", "s1", "game", "state")
            .unwrap()
            .unwrap()
            .value;
        state["tubes"] = json!(["water", "poison"]);
        state["consumed"] = json!([]);
        state["turn"] = json!("player");
        state["phase"] = json!("playing");
        state["player_items"] = json!({"needle":1,"spill":0,"incense":0,"strong":0,"herb":0});
        repo.set("session", "s1", "game", "state", &state).unwrap();

        let first = resolve_action(
            &conn,
            &world,
            "s1",
            "game.playerAct",
            json!({"action":"use_needle"}),
            "player",
            None,
        )
        .unwrap();
        assert!(first.transition.continue_action);
        assert_eq!(first.transition.next_actor.as_deref(), Some("player"));

        let handed_off = resolve_action(
            &conn,
            &world,
            "s1",
            "game.playerAct",
            json!({"action":"drink_opponent"}),
            "player",
            None,
        )
        .unwrap();
        assert!(handed_off.transition.turn_ended);
        assert_eq!(handed_off.transition.next_actor.as_deref(), Some("agent"));

        assert!(resolve_action(
            &conn,
            &world,
            "s1",
            "game.playerAct",
            json!({"action":"drink_self"}),
            "player",
            None,
        )
        .is_err());

        let devil = resolve_action(
            &conn,
            &world,
            "s1",
            "game.devilAct",
            json!({"action":"drink_self"}),
            "agent",
            Some("小恶魔"),
        )
        .unwrap();
        assert!(devil.transition.turn_ended);
        assert_eq!(devil.transition.next_actor.as_deref(), Some("player"));
    }
}
