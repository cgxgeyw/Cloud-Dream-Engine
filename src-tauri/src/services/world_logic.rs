//! Host-side world logic. UI and character tools share the package's JavaScript
//! rules and commit session KV writes atomically under the database lock.
use crate::models::world::WorldDefinition;
use crate::services::llm::client::ChatToolDefinition;
use rquickjs::{Context, Runtime};
use rusqlite::Connection;
use serde_json::{json, Value};

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
    let (Some(handler), Some(field)) = (trigger["view_handler"].as_str(), trigger["when_field"].as_str()) else {
        return Ok(());
    };
    let view = invoke(conn, world, session_id, handler, json!({}), "player")?;
    request.tool_choice = Some(if view[field] == true {
        crate::services::llm::client::ChatToolChoice::Required
    } else {
        crate::services::llm::client::ChatToolChoice::None
    });
    Ok(())
}

pub fn invoke(
    conn: &Connection,
    world: &WorldDefinition,
    session_id: &str,
    handler: &str,
    input: Value,
    actor: &str,
) -> Result<Value, String> {
    let logic = &world.ui_theme_config["logic"];
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
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let repo = crate::db::repositories::world_kv_repo::WorldKvRepository::new(&tx);
    for write in output["writes"].as_array().into_iter().flatten() {
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
            value_json TEXT, updated_at TEXT, PRIMARY KEY(owner_type,owner_id,namespace,key));",
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
        world.ui_theme_config["logic"]["source"] =
            json!("world.register('game.newGame',()=>{while(true){}});");
        world.ui_theme_config["logic"]["timeout_ms"] = json!(100);
        assert!(invoke(&conn, &world, "s1", "game.newGame", json!({}), "player").is_err());
    }
}
