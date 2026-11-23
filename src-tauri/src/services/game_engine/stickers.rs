use crate::models::world::WorldDefinition;

pub(crate) const SEARCH_STICKER_TOOL: &str = "search_sticker";
pub(crate) const SEND_STICKER_TOOL: &str = "send_sticker";

pub(crate) fn is_tool(name: &str) -> bool {
    name.trim() == SEARCH_STICKER_TOOL || name.trim() == SEND_STICKER_TOOL
}

#[derive(Clone, Debug)]
pub(crate) struct WorldSticker {
    pub key: String,
    pub label: String,
    pub asset_path: String,
    pub description: String,
}

/// Sticker pack declared by the world package.
///
/// The engine stores it in `ui_theme_config.stickers` so the same list feeds
/// both the character tool and the world UI snapshot.
pub(crate) fn world_stickers(world: &WorldDefinition) -> Vec<WorldSticker> {
    let mut seen = std::collections::HashSet::new();
    world
        .ui_theme_config
        .get("stickers")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let key = item.get("key")?.as_str()?.trim();
                    let asset_path = item.get("asset_path")?.as_str()?.trim();
                    if key.is_empty() || asset_path.is_empty() || !seen.insert(key.to_string()) {
                        return None;
                    }
                    let label = item
                        .get("label")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or(key)
                        .to_string();
                    let description = item
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    Some(WorldSticker {
                        key: key.to_string(),
                        label,
                        asset_path: asset_path.to_string(),
                        description,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn tool_capabilities(world: &WorldDefinition) -> Vec<serde_json::Value> {
    if world_stickers(world).is_empty() {
        return Vec::new();
    }
    vec![search_capability(), send_capability()]
}

fn search_capability() -> serde_json::Value {
    serde_json::json!({
        "tool_name": SEARCH_STICKER_TOOL,
        "description": "按情绪、画面或用途在图库中查找图片。传入一句自然语言描述（如「开心庆祝」「被夸后得意」「无语翻白眼」），返回最相关的图片 key、名称和描述。想发图但不确定用哪张时先调用，再用返回的 key 调用 send_sticker。",
        "arguments_schema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "想找的画面、情绪或用途，一句短描述",
                },
                "limit": {
                    "type": "integer",
                    "description": "最多返回几张，默认 5，范围 1-8",
                    "minimum": 1,
                    "maximum": 8,
                }
            },
            "required": ["query"]
        }
    })
}

fn send_capability() -> serde_json::Value {
    serde_json::json!({
        "tool_name": SEND_STICKER_TOOL,
        "description": "发送图库里的一张图片，图片会显示在你这条回复旁边。key 必须来自 search_sticker 的返回结果；没查过就先调用 search_sticker。只在情绪明显时使用，一次回复最多调用一次，并且必须同时正常说话，不要只发图片。",
        "arguments_schema": {
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "search_sticker 返回的图片 key",
                },
                "caption": {
                    "type": "string",
                    "description": "可选的一句短语，不超过 12 字；正常台词仍然写在 content 里",
                }
            },
            "required": ["key"]
        }
    })
}

pub(crate) fn search(world: &WorldDefinition, arguments: &serde_json::Value) -> serde_json::Value {
    let stickers = world_stickers(world);
    if stickers.is_empty() {
        return serde_json::json!({
            "ok": false,
            "error": "当前世界没有可用图片",
        });
    }
    let total = stickers.len();
    let query = arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let limit = arguments
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 8) as usize;
    let normalized_query = crate::services::game_engine::memory::normalize_memory_text(query);
    let terms = crate::services::game_engine::memory::build_memory_search_terms(query);
    let mut scored = stickers
        .into_iter()
        .enumerate()
        .map(|(order, sticker)| {
            let key_text =
                crate::services::game_engine::memory::normalize_memory_text(&sticker.key);
            let label_text =
                crate::services::game_engine::memory::normalize_memory_text(&sticker.label);
            let description_text =
                crate::services::game_engine::memory::normalize_memory_text(&sticker.description);
            let searchable = format!("{key_text} {label_text} {description_text}");
            let mut score = if normalized_query.is_empty() { 1 } else { 0 };
            if !normalized_query.is_empty() {
                if label_text == normalized_query {
                    score += 30;
                }
                if label_text.contains(&normalized_query) {
                    score += 12;
                }
                if key_text.contains(&normalized_query) {
                    score += 8;
                }
                if description_text.contains(&normalized_query) {
                    score += 6;
                }
                let hits = terms
                    .iter()
                    .filter(|term| !term.is_empty() && searchable.contains(term.as_str()))
                    .count();
                if hits > 0 {
                    score += 2 + (hits.min(8) as i32) * 2;
                }
            }
            (score, order, sticker)
        })
        .collect::<Vec<_>>();
    let matched = scored.iter().any(|(score, _, _)| *score > 0);
    if !matched {
        scored.iter_mut().for_each(|item| item.0 = 0);
    } else {
        scored.retain(|(score, _, _)| *score > 0);
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    let results = scored
        .iter()
        .map(|(_, _, sticker)| {
            serde_json::json!({
                "key": sticker.key,
                "label": sticker.label,
                "description": sticker.description,
            })
        })
        .collect::<Vec<_>>();
    let hint = if normalized_query.is_empty() {
        "未提供查询词，按图库顺序列出可用图片。"
    } else if !matched {
        "没有明显匹配，以下是图库中的部分图片；只有确实贴合当前情绪时才用。"
    } else {
        "选好后调用 send_sticker 发送，一次回复最多一张。"
    };
    serde_json::json!({
        "ok": true,
        "query": query,
        "matched": matched,
        "total": total,
        "results": results,
        "hint": hint,
    })
}

pub(crate) fn execute(
    world: &WorldDefinition,
    call: &crate::services::llm::client::ChatToolCall,
) -> (serde_json::Value, Option<WorldSticker>) {
    let stickers = world_stickers(world);
    let key = call
        .arguments
        .get("key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if key.is_empty() {
        return (
            serde_json::json!({
                "ok": false,
                "error": "缺少 key 参数；请先调用 search_sticker 查找图片，再用返回的 key 发送。",
            }),
            None,
        );
    }
    let Some(sticker) = stickers.iter().find(|sticker| sticker.key == key) else {
        let allowed = stickers
            .iter()
            .take(20)
            .map(|sticker| sticker.key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if stickers.len() > 20 { " 等" } else { "" };
        return (
            serde_json::json!({
                "ok": false,
                "error": format!("未知的图片 key：{key}；请先调用 search_sticker 重新查找。可用 key 示例：{allowed}{suffix}"),
            }),
            None,
        );
    };
    (
        serde_json::json!({
            "ok": true,
            "sticker": { "key": sticker.key, "label": sticker.label },
            "message": format!("已发送图片：{}", sticker.label),
        }),
        Some(sticker.clone()),
    )
}

pub(crate) fn message_metadata(sticker: &WorldSticker) -> serde_json::Value {
    serde_json::json!({
        "key": sticker.key,
        "label": sticker.label,
        "asset_path": sticker.asset_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_world(stickers: serde_json::Value) -> WorldDefinition {
        let mut world: WorldDefinition = serde_json::from_value(serde_json::json!({
            "id": "world-sticker",
            "name": "贴纸世界",
            "genre": "",
            "background_prompt": "",
            "opening_scene": "",
            "summary": "",
            "time_system": "",
            "map_nodes": {},
            "time_config": {},
            "director_config": {},
            "ui_theme_config": {},
            "director_system_prompt_base": "",
            "director_runtime_system_prompt": "",
            "opening_messages": [],
            "opening_character_ids": [],
            "player_character_id": null
        }))
        .unwrap();
        world.ui_theme_config["stickers"] = stickers;
        world
    }

    fn call(key: &str) -> crate::services::llm::client::ChatToolCall {
        crate::services::llm::client::ChatToolCall {
            id: "call-1".to_string(),
            tool_name: SEND_STICKER_TOOL.to_string(),
            arguments: serde_json::json!({ "key": key }),
        }
    }

    fn search_call(query: &str) -> serde_json::Value {
        serde_json::json!({ "query": query })
    }

    #[test]
    fn capabilities_include_search_and_send() {
        let world = test_world(serde_json::json!([
            { "key": "happy", "label": "开心", "asset_path": "assets/stickers/happy.webp" },
            { "key": "sad", "label": "难过", "asset_path": "assets/stickers/sad.webp" }
        ]));
        let capabilities = tool_capabilities(&world);
        assert_eq!(capabilities.len(), 2);
        assert_eq!(capabilities[0]["tool_name"], SEARCH_STICKER_TOOL);
        assert_eq!(
            capabilities[0]["arguments_schema"]["required"],
            serde_json::json!(["query"])
        );
        assert_eq!(capabilities[1]["tool_name"], SEND_STICKER_TOOL);
        assert!(capabilities[1]["arguments_schema"]["properties"]["key"]
            .get("enum")
            .is_none());
    }

    #[test]
    fn world_without_stickers_hides_the_tools() {
        assert!(tool_capabilities(&test_world(serde_json::json!([]))).is_empty());
    }

    #[test]
    fn search_matches_label_and_description() {
        let world = test_world(serde_json::json!([
            { "key": "happy", "label": "开心", "description": "被夸奖或收到礼物", "asset_path": "assets/stickers/happy.webp" },
            { "key": "sad", "label": "难过", "description": "被批评或受委屈", "asset_path": "assets/stickers/sad.webp" }
        ]));
        let by_label = search(&world, &search_call("开心"));
        assert_eq!(by_label["matched"], true);
        assert_eq!(by_label["results"][0]["key"], "happy");
        let by_description = search(&world, &search_call("受委屈"));
        assert_eq!(by_description["results"][0]["key"], "sad");
        let limited = search(
            &world,
            &serde_json::json!({ "query": "开心 难过", "limit": 1 }),
        );
        assert_eq!(limited["results"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn search_falls_back_to_catalog_when_nothing_matches() {
        let world = test_world(serde_json::json!([
            { "key": "happy", "label": "开心", "asset_path": "assets/stickers/happy.webp" },
            { "key": "sad", "label": "难过", "asset_path": "assets/stickers/sad.webp" }
        ]));
        let result = search(&world, &search_call("飞船引擎"));
        assert_eq!(result["ok"], true);
        assert_eq!(result["matched"], false);
        assert_eq!(result["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn execute_rejects_unknown_key() {
        let world = test_world(serde_json::json!([
            { "key": "happy", "label": "开心", "asset_path": "assets/stickers/happy.webp" }
        ]));
        let (result, sticker) = execute(&world, &call("unknown"));
        assert_eq!(result["ok"], false);
        assert!(sticker.is_none());
    }

    #[test]
    fn execute_returns_metadata_for_valid_key() {
        let world = test_world(serde_json::json!([
            { "key": "happy", "label": "开心", "asset_path": "assets/stickers/happy.webp" }
        ]));
        let (result, sticker) = execute(&world, &call("happy"));
        assert_eq!(result["ok"], true);
        let sticker = sticker.expect("valid key returns the sticker");
        assert_eq!(
            message_metadata(&sticker),
            serde_json::json!({
                "key": "happy",
                "label": "开心",
                "asset_path": "assets/stickers/happy.webp"
            })
        );
    }
}
