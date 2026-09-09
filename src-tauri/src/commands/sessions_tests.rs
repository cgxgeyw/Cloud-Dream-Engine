use super::{merge_agent_chat_runtime_payloads, merge_speaker_fact_extractions};

    #[test]
    fn agent_memory_entries_are_aliased_into_memory_events() {
        let payloads = vec![serde_json::json!({
            "memory_entries": [
                { "content": "记住了玩家的名字", "character_names": ["林黛玉"] }
            ]
        })];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        // runtime 解析读的是 memory_events，NPC 契约用的是 memory_entries：必须被并入。
        let events = merged
            .get("memory_events")
            .and_then(|value| value.as_array())
            .expect("memory_events present");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["content"], "记住了玩家的名字");
    }

    #[test]
    fn agent_memory_events_and_entries_both_merge() {
        let payloads = vec![
            serde_json::json!({ "memory_events": [{ "content": "A", "character_names": ["甲"] }] }),
            serde_json::json!({ "memory_entries": [{ "content": "B", "character_names": ["乙"] }] }),
        ];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        let events = merged
            .get("memory_events")
            .and_then(|value| value.as_array())
            .expect("memory_events present");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn agent_fact_extractions_merge_across_speaker_payloads() {
        let payloads = vec![
            serde_json::json!({ "fact_extractions": [{ "subject": "银钥匙", "predicate": "藏在", "object": "12号柜" }] }),
            serde_json::json!({ "fact_extractions": [{ "subject": "鲍勃", "predicate": "不知道", "object": "位置" }] }),
        ];
        let merged = merge_agent_chat_runtime_payloads(&payloads);
        let facts = merged
            .get("fact_extractions")
            .and_then(|value| value.as_array())
            .expect("fact_extractions present");
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn speaker_fact_extractions_merge_into_director_payload() {
        let director_payload = serde_json::json!({
            "planned_speakers": ["林黛玉"],
            "fact_extractions": [{ "subject": "导演事实", "predicate": "发生", "object": "雷雨" }]
        });
        let speaker_payloads = vec![
            serde_json::json!({ "fact_extractions": [{ "subject": "银钥匙", "predicate": "藏在", "object": "12号柜" }] }),
            serde_json::json!({ "content": "没有事实的一轮" }),
        ];
        let merged = merge_speaker_fact_extractions(&director_payload, &speaker_payloads);
        let facts = merged
            .get("fact_extractions")
            .and_then(|value| value.as_array())
            .expect("fact_extractions present");
        assert_eq!(facts.len(), 2, "导演已有的事实在前,说话人的追加在后");
        assert_eq!(facts[0]["subject"], "导演事实");
        assert_eq!(facts[1]["subject"], "银钥匙");
        // 其它字段原样保留
        assert_eq!(
            merged.get("planned_speakers"),
            Some(&serde_json::json!(["林黛玉"]))
        );
    }

    #[test]
    fn speaker_fact_extractions_noop_when_nothing_to_merge() {
        let director_payload = serde_json::json!({ "planned_speakers": ["林黛玉"] });
        let merged = merge_speaker_fact_extractions(&director_payload, &[]);
        assert!(merged.get("fact_extractions").is_none());
        // 没有事实时不应新建字段
        assert_eq!(merged, director_payload);
    }
