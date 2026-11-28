use rusqlite::Connection;

use crate::models::oag::{OagActor, OagQueryContext, OagQueryRequest, OagQueryResponse};

use super::canon;
use super::query;

pub const OAG_QUERY_TOOL_NAME: &str = "oag_query";

pub fn capability() -> serde_json::Value {
    serde_json::json!({
        "tool_name": OAG_QUERY_TOOL_NAME,
        "description": "查询当前世界的结构化实体、属性、关系、事件和知识，重点用于理解关系、势力与全局格局。只在已有 OAG 上下文不足时调用，不要重复查询其中已经提供的信息。本工具只读，不修改世界。op 可选 resolve（解析实体）、entity（实体详情）、find（条件查找）、traverse（沿关系多跳追踪）、aggregate（统计）、absence（当前可见信息中不存在匹配）、knows（查询知情记录）、timeline（时间线）。关系类型必须使用当前 OAG 上下文中出现过的类型键，不要猜测。世界、会话、视角和当前回合由系统固定。",
        "arguments_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["op"],
            "properties": {
                "op": {
                    "type": "string",
                    "enum": ["resolve", "entity", "find", "traverse", "aggregate", "absence", "knows", "timeline"]
                },
                "entity": { "$ref": "#/$defs/entity" },
                "object_type": { "type": "string", "description": "世界本体声明的对象类型键。" },
                "attribute": { "$ref": "#/$defs/attribute" },
                "link": { "$ref": "#/$defs/link" },
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": { "$ref": "#/$defs/traverse_step" }
                },
                "metric": {
                    "type": "string",
                    "enum": ["count", "count_distinct", "sum", "avg", "min", "max"]
                },
                "metric_attribute": { "type": "string" },
                "group_by": {
                    "type": "string",
                    "enum": ["object_type", "attribute", "link_target"]
                },
                "group_attribute": { "type": "string" },
                "target": { "$ref": "#/$defs/knowledge_target" },
                "mentioned": { "type": "string" },
                "from_turn": { "type": "integer", "minimum": 0 },
                "to_turn": { "type": "integer", "minimum": 0 },
                "event_type": { "type": "string" },
                "participant": { "$ref": "#/$defs/entity" },
                "location": { "$ref": "#/$defs/entity" },
                "include_history": { "type": "boolean", "default": false },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20, "default": 20 }
            },
            "$defs": {
                "entity": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "object_type": { "type": "string" }
                    }
                },
                "attribute": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["key"],
                    "properties": {
                        "key": { "type": "string" },
                        "op": {
                            "type": "string",
                            "enum": ["eq", "ne", "gt", "gte", "lt", "lte", "in", "contains"],
                            "default": "eq"
                        },
                        "value": {}
                    }
                },
                "link": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["link_type"],
                    "properties": {
                        "link_type": { "type": "string" },
                        "direction": { "type": "string", "enum": ["out", "in", "both"], "default": "out" },
                        "to": { "$ref": "#/$defs/entity" }
                    }
                },
                "traverse_step": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["link_type"],
                    "properties": {
                        "link_type": { "type": "string" },
                        "direction": { "type": "string", "enum": ["out", "in", "both"], "default": "out" }
                    }
                },
                "knowledge_target": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {
                        "kind": { "type": "string", "enum": ["object", "attribute", "link", "event", "memory"] },
                        "id": { "type": "string" }
                    }
                }
            }
        }
    })
}

pub fn execute(
    conn: &Connection,
    world_id: &str,
    session_id: &str,
    actor: OagActor,
    as_of_turn: i32,
    arguments: &serde_json::Value,
) -> Result<OagQueryResponse, String> {
    let mut request: OagQueryRequest = serde_json::from_value(arguments.clone())
        .map_err(|error| format!("OAG 查询参数无效：{error}"))?;
    request.as_of_turn = None;
    request.include_retired = false;
    let ontology = canon::load_active_ontology(conn, world_id)?
        .ok_or_else(|| "当前世界尚未导入 OAG 本体".to_string())?;
    query::query(
        conn,
        OagQueryContext {
            world_id: world_id.to_string(),
            session_id: session_id.to_string(),
            actor,
            as_of_turn,
            max_nodes: 200,
        },
        ontology,
        request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{oag_schema, schema};
    use crate::models::oag::{OagObject, OagObjectTypeDefinition, OagOntology};

    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        oag_schema::create(&conn).unwrap();
        conn.execute(
            "INSERT INTO worlds (id, name) VALUES ('world', 'World')",
            [],
        )
        .unwrap();
        let ontology = OagOntology {
            object_types: vec![OagObjectTypeDefinition {
                key: "person".to_string(),
                label: "人物".to_string(),
                parent: None,
                actor: true,
                platform_role: String::new(),
                attributes: Vec::new(),
            }],
            link_types: Vec::new(),
            character_object_type: Some("person".to_string()),
            location_object_type: None,
        };
        canon::import(
            &conn,
            "world",
            Some(&ontology),
            &Default::default(),
            &[],
            &std::collections::HashMap::new(),
            &serde_json::json!({}),
        )
        .unwrap();
        conn
    }

    #[test]
    fn capability_is_one_read_only_query_tool_without_runtime_context_fields() {
        let tool = capability();
        assert_eq!(tool["tool_name"], OAG_QUERY_TOOL_NAME);
        assert_eq!(
            tool["arguments_schema"]["required"],
            serde_json::json!(["op"])
        );
        let properties = tool["arguments_schema"]["properties"].as_object().unwrap();
        for key in [
            "actor",
            "world_id",
            "session_id",
            "as_of_turn",
            "max_nodes",
            "include_retired",
        ] {
            assert!(!properties.contains_key(key));
        }
    }

    #[test]
    fn execution_ignores_model_turn_and_uses_fixed_actor_scope() {
        let conn = database();
        let repo = crate::db::repositories::oag_repo::OagRepository::new(&conn);
        let object = OagObject {
            id: "person-a".to_string(),
            world_id: "world".to_string(),
            session_id: "session".to_string(),
            object_type: "person".to_string(),
            canonical_key: "runtime-character:a".to_string(),
            name: "甲".to_string(),
            name_normalized: "甲".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            source: "runtime".to_string(),
            visibility: "private".to_string(),
            asserted_by_id: Some("person-a".to_string()),
            source_event_id: None,
            truth_status: "confirmed".to_string(),
            created_turn: 5,
            updated_turn: 5,
            retired_at: String::new(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        repo.upsert_object(&object).unwrap();
        let mut observer = object;
        observer.id = "person-b".to_string();
        observer.canonical_key = "runtime-character:b".to_string();
        observer.name = "乙".to_string();
        observer.name_normalized = "乙".to_string();
        observer.asserted_by_id = Some("person-b".to_string());
        observer.created_turn = 0;
        observer.updated_turn = 0;
        repo.upsert_object(&observer).unwrap();

        let response = execute(
            &conn,
            "world",
            "session",
            OagActor::Director,
            4,
            &serde_json::json!({
                "op": "find",
                "object_type": "person",
                "as_of_turn": 10,
                "actor": "author",
                "world_id": "other-world"
            }),
        )
        .unwrap();
        assert_eq!(response.count, 1);
        assert_eq!(response.results[0]["id"], "person-b");

        let response = execute(
            &conn,
            "world",
            "session",
            OagActor::Object("person-b".to_string()),
            5,
            &serde_json::json!({ "op": "find", "object_type": "person" }),
        )
        .unwrap();
        assert_eq!(response.count, 1);
        assert_eq!(response.results[0]["id"], "person-b");

        let response = execute(
            &conn,
            "world",
            "session",
            OagActor::Object("person-a".to_string()),
            5,
            &serde_json::json!({ "op": "find", "object_type": "person" }),
        )
        .unwrap();
        assert_eq!(response.count, 1);
    }
}
