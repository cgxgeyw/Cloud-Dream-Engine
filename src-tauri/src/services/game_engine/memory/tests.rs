    use super::*;
    use crate::db::repositories::memory_repo::MemoryRepository;
    use crate::db::schema;
    use crate::models::session::{AssetSelection, MessageContent, SceneRuntime, SessionState};
    use rusqlite::Connection;

    fn sample_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: "".to_string(),
            background_prompt: "".to_string(),
            opening_scene: "Harbor".to_string(),
            summary: "".to_string(),
            time_system: "".to_string(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: "".to_string(),
            director_runtime_system_prompt: "".to_string(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: Some("char-player".to_string()),
        }
    }

    fn sample_session() -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "Harbor".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "".to_string(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string(), "Bob".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            inventory_items: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "harbor-scene".to_string(),
                name: "Harbor".to_string(),
                background_hint: "".to_string(),
                temporary_tags: vec![],
                present_characters: vec![
                    "Player".to_string(),
                    "Alice".to_string(),
                    "Bob".to_string(),
                ],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    fn sample_characters() -> Vec<CharacterDefinition> {
        vec![
            CharacterDefinition {
                id: "char-a".to_string(),
                name: "Alice".to_string(),
                world_id: "world-1".to_string(),
                role: "".to_string(),
                background_prompt: "".to_string(),
                model: "".to_string(),
                memory_strategy: "".to_string(),
                recent_dialogue_rounds: 8,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: "".to_string(),
                response_contract_prompt: "".to_string(),
                narration_prompt: "".to_string(),
                runtime_system_prompt: "".to_string(),
            },
            CharacterDefinition {
                id: "char-b".to_string(),
                name: "Bob".to_string(),
                world_id: "world-1".to_string(),
                role: "".to_string(),
                background_prompt: "".to_string(),
                model: "".to_string(),
                memory_strategy: "".to_string(),
                recent_dialogue_rounds: 8,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: "".to_string(),
                response_contract_prompt: "".to_string(),
                narration_prompt: "".to_string(),
                runtime_system_prompt: "".to_string(),
            },
        ]
    }

    #[test]
    fn recall_entries_for_character_are_isolated_per_character() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        let alice_entry = MemoryEntry {
            id: "m-a".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "Alice secretly hid the silver key in locker 12.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.9,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 3,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec!["silver".to_string(), "key".to_string()],
        };
        let bob_entry = MemoryEntry {
            id: "m-b".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-b".to_string(),
            layer: "working".to_string(),
            content: "Bob believes the key was burned already.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.8,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 3,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Bob".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Bob".to_string()],
            keywords: vec!["key".to_string()],
        };
        repo.insert(&alice_entry).expect("insert alice memory");
        repo.insert(&bob_entry).expect("insert bob memory");

        let recalled_for_alice = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "Where is the silver key?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                10,
                None,
            )
            .expect("recall for alice");
        assert!(
            recalled_for_alice
                .iter()
                .any(|entry| entry.content.contains("silver key in locker 12")),
            "alice should recall her own memory"
        );
        assert!(
            recalled_for_alice
                .iter()
                .all(|entry| entry.character_id == "char-a"
                    && !entry.content.contains("burned already")),
            "alice recall must not leak bob memory"
        );
    }

    #[test]
    fn recall_entries_for_character_preserves_raw_dialogue_fields() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        let archive_entry = MemoryEntry {
            id: "m-archive-a".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "archive".to_string(),
            content: "Alice learned the vault code is 4318.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.95,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 1,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec!["vault".to_string(), "4318".to_string()],
        };
        repo.insert(&archive_entry).expect("insert archive memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "What is the vault code?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                5,
                None,
            )
            .expect("recall raw entries");

        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].layer, "archive");
        assert_eq!(recalled[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(recalled[0].role.as_deref(), Some("agent"));
        assert!(recalled[0].content.contains("4318"));
    }

    #[test]
    fn persist_turn_entries_writes_single_working_layer_without_duplicates() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let session = sample_session();
        let characters = sample_characters();
        let turn_index = 5;
        let messages = vec![
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text("Take the brass compass and hide it.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text("Alice pockets the brass compass and nods.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
        ];

        let written = service
            .persist_turn_entries(
                &conn,
                &world,
                &session,
                turn_index,
                "char-player",
                "Player",
                &characters,
                &messages,
                &["Alice".to_string(), "Bob".to_string()],
                &[],
            )
            .expect("persist turn entries");
        // 单层写入:每条消息 × 每个在场角色(Player/Alice/Bob)恰好 1 条 working。
        assert_eq!(written.len(), 2 * 3);
        assert!(
            written.iter().all(|entry| entry.layer == "working"),
            "dialogue memories should be written once with the working layer"
        );
        let mut unique_pairs = std::collections::HashSet::new();
        for entry in &written {
            assert!(
                unique_pairs.insert((entry.character_id.clone(), entry.content.clone())),
                "same content must not be stored twice for one character"
            );
        }

        let repo = MemoryRepository::new(&conn);
        let all_for_alice = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-a".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list memories");
        assert!(
            all_for_alice
                .iter()
                .any(|entry| entry.content.contains("brass compass")),
            "persisted memory should be queryable from database"
        );
    }

    #[test]
    fn persist_turn_entries_isolates_private_dialogue_by_present_participants() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let session = sample_session();
        let characters = sample_characters();
        let turn_index = 7;
        let messages = vec![
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text("Tell Alice the passphrase is moon glass.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text("Alice memorizes the passphrase: moon glass.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": turn_index })),
            },
        ];

        service
            .persist_turn_entries(
                &conn,
                &world,
                &session,
                turn_index,
                "char-player",
                "Player",
                &characters,
                &messages,
                &["Alice".to_string()],
                &[],
            )
            .expect("persist isolated turn");

        let repo = MemoryRepository::new(&conn);
        let alice_memories = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-a".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list alice memories");
        let bob_memories = repo
            .list(&MemoryQueryParams {
                world_id: Some("world-1".to_string()),
                session_id: Some("sess-1".to_string()),
                character_id: Some("char-b".to_string()),
                layer: None,
                limit: Some(200),
            })
            .expect("list bob memories");

        assert!(
            alice_memories
                .iter()
                .any(|entry| entry.content.contains("moon glass")),
            "alice should receive the private passphrase memory"
        );
        assert!(
            bob_memories
                .iter()
                .all(|entry| !entry.content.contains("moon glass")),
            "bob should not receive memories from dialogue he was not present for"
        );
    }

    #[test]
    fn recall_entries_for_character_keeps_old_memory_visible_despite_recent_noise() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();

        // 近期噪音:turn 20-25,全部 stored working。
        for index in 0..6 {
            repo.insert(&MemoryEntry {
                id: format!("m-work-{index}"),
                world_id: "world-1".to_string(),
                session_id: "sess-1".to_string(),
                character_id: "char-a".to_string(),
                layer: "working".to_string(),
                content: format!("Ambient harbor chatter {index}"),
                source: "speaker_response".to_string(),
                importance: 0.2,
                created_at: Utc::now().to_rfc3339(),
                turn_index: 20 + index,
                conversation_id: Some("sess-1".to_string()),
                event_id: None,
                item_id: None,
                scene_id: Some("harbor-scene".to_string()),
                memory_type: "dialogue".to_string(),
                speaker: Some("Alice".to_string()),
                role: Some("agent".to_string()),
                location: Some("Harbor".to_string()),
                participants: vec!["Player".to_string(), "Alice".to_string()],
                keywords: vec!["harbor".to_string()],
            })
            .expect("insert noise memory");
        }
        // 老记忆:turn 1,距最新 24 轮 > short_term 窗口(15),有效层推算为 archive,
        // 应被 archive 配额保住;存储层保持 working 不变。
        repo.insert(&MemoryEntry {
            id: "m-archive-secret".to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "Alice archived that the eclipse gate opens with moon glass.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.98,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 1,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: Some("harbor-scene".to_string()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Alice".to_string()),
            role: Some("agent".to_string()),
            location: Some("Harbor".to_string()),
            participants: vec!["Player".to_string(), "Alice".to_string()],
            keywords: vec![
                "eclipse".to_string(),
                "moon".to_string(),
                "glass".to_string(),
            ],
        })
        .expect("insert archive memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "How does the eclipse gate open?",
                "Harbor",
                Some("harbor-scene"),
                &["Player".to_string(), "Alice".to_string()],
                5,
                None,
            )
            .expect("recall with archive");

        assert!(
            recalled
                .iter()
                .any(|entry| entry.id == "m-archive-secret"
                    && entry.layer == "working"
                    && entry.content.contains("moon glass")),
            "old memory should remain recallable via the derived archive quota, stored layer unchanged"
        );
    }

    fn bare_entry(id: &str, layer: &str, source: &str, turn_index: i32) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            world_id: "world-1".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-a".to_string(),
            layer: layer.to_string(),
            content: format!("content-{id}"),
            source: source.to_string(),
            importance: 0.5,
            created_at: Utc::now().to_rfc3339(),
            turn_index,
            conversation_id: Some("sess-1".to_string()),
            event_id: None,
            item_id: None,
            scene_id: None,
            memory_type: "dialogue".to_string(),
            speaker: None,
            role: None,
            location: None,
            participants: vec![],
            keywords: vec![],
        }
    }

    #[test]
    fn derive_effective_layer_ages_dialogue_memories_by_turn_distance() {
        let cases = [
            // (turn_index, 期望有效层): newest=20, working≤3, short_term≤15
            (20, "working"),
            (17, "working"),  // age 3,边界
            (16, "short_term"), // age 4
            (5, "short_term"),  // age 15,边界
            (4, "archive"),   // age 16
            (1, "archive"),
        ];
        for (turn_index, expected) in cases {
            let entry = bare_entry("m", "working", "speaker_response", turn_index);
            assert_eq!(
                derive_effective_layer(&entry, 20, 3, 15),
                expected,
                "turn {turn_index} should derive to {expected}"
            );
        }
    }

    #[test]
    fn derive_effective_layer_passes_through_non_dialogue_and_canonical() {
        // canonical_event 直通
        let canonical = bare_entry("m", "canonical_event", "speaker_response", 1);
        assert_eq!(
            derive_effective_layer(&canonical, 20, 3, 15),
            "canonical_event"
        );
        // 非对话 source(trigger/rule/LLM 写入)保持存储层
        let rule_memory = bare_entry("m", "short_term", "rule", 1);
        assert_eq!(
            derive_effective_layer(&rule_memory, 20, 3, 15),
            "short_term"
        );
        // turn_index 缺失(≤0)无法推算年龄,回退存储层
        let no_turn = bare_entry("m", "working", "player_action", 0);
        assert_eq!(derive_effective_layer(&no_turn, 20, 3, 15), "working");
        // newest_turn 未知(空候选池)同样回退
        let entry = bare_entry("m", "working", "player_action", 3);
        assert_eq!(derive_effective_layer(&entry, 0, 3, 15), "working");
    }

    #[test]
    fn parse_memory_strategy_recognizes_keywords() {
        let default = parse_memory_strategy("");
        assert!(!default.disabled && !default.recency_bias && !default.importance_bias);
        assert!(default.retrieval_mode.is_none());

        assert!(parse_memory_strategy("off").disabled);
        assert!(parse_memory_strategy("无记忆").disabled);
        assert!(parse_memory_strategy("不用记住玩家说的话").disabled);
        // 整词匹配:"offline"/"often" 不应触发 disabled
        assert!(!parse_memory_strategy("offline archive").disabled);
        assert!(!parse_memory_strategy("often recalls").disabled);
        // disabled 优先级最高
        let disabled = parse_memory_strategy("off recent important");
        assert!(disabled.disabled && !disabled.recency_bias && !disabled.importance_bias);

        assert_eq!(
            parse_memory_strategy("semantic").retrieval_mode.as_deref(),
            Some("semantic_only")
        );
        assert_eq!(
            parse_memory_strategy("用语义检索").retrieval_mode.as_deref(),
            Some("semantic_only")
        );
        assert_eq!(
            parse_memory_strategy("lexical").retrieval_mode.as_deref(),
            Some("lexical_only")
        );

        assert!(parse_memory_strategy("recent").recency_bias);
        assert!(parse_memory_strategy("多关注最近发生的事").recency_bias);
        assert!(parse_memory_strategy("important").importance_bias);
        assert!(parse_memory_strategy("重要的事要牢记").importance_bias);

        // 种子里的自然语言描述不含任何关键词 → 全默认,行为不变
        for seed_text in [
            "记住宴会中的人际变化与诗句往来。",
            "default",
            "short memory guidance",
        ] {
            let parsed = parse_memory_strategy(seed_text);
            assert!(
                !parsed.disabled
                    && !parsed.recency_bias
                    && !parsed.importance_bias
                    && parsed.retrieval_mode.is_none(),
                "seed text {:?} should parse to defaults",
                seed_text
            );
        }
    }

    #[test]
    fn recall_with_disabled_strategy_returns_nothing() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();
        repo.insert(&bare_entry("m-1", "working", "speaker_response", 3))
            .expect("insert memory");

        let recalled = service
            .recall_entries_for_character(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("不记"),
            )
            .expect("recall with disabled strategy");
        assert!(recalled.is_empty(), "disabled strategy should skip recall");
    }

    #[test]
    fn recall_strategy_overrides_retrieval_mode() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let repo = MemoryRepository::new(&conn);
        let service = MemoryService::new();
        let world = sample_world();
        repo.insert(&bare_entry("m-1", "working", "speaker_response", 3))
            .expect("insert memory");

        let plan = service
            .prepare_character_recall(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("semantic"),
            )
            .expect("prepare recall");
        assert_eq!(plan.retrieval_mode, "semantic_only");

        let plan = service
            .prepare_character_recall(
                &conn,
                &world,
                "world-1",
                "sess-1",
                Some("char-a"),
                "content",
                "Harbor",
                None,
                &[],
                5,
                Some("important"),
            )
            .expect("prepare recall");
        assert_eq!(plan.retrieval_mode, "hybrid", "mode 未指定时保持 world 默认");
        assert_eq!(plan.archive_quota, 3, "importance_bias 应提升 archive 配额");
    }

    #[test]
    fn rank_memories_uses_effective_layers_for_quotas() {
        // B 词法分高于 C,但 B 的有效层是 archive、C 是 working:
        // working 配额先选 A、C,archive 配额再选 B。
        let memories = vec![
            bare_entry("a", "working", "speaker_response", 20),
            bare_entry("b", "working", "speaker_response", 1),
            bare_entry("c", "working", "speaker_response", 19),
        ];
        let lexical_scores: HashMap<String, f64> = [
            ("a".to_string(), 10.0),
            ("b".to_string(), 8.0),
            ("c".to_string(), 6.0),
        ]
        .into_iter()
        .collect();
        let effective_layers: HashMap<String, String> = [
            ("a".to_string(), "working".to_string()),
            ("b".to_string(), "archive".to_string()),
            ("c".to_string(), "working".to_string()),
        ]
        .into_iter()
        .collect();

        let ranked = rank_memories_by_scores(
            memories,
            &lexical_scores,
            &HashMap::new(),
            "hybrid",
            0.65,
            &effective_layers,
            2,
        );
        let order: Vec<&str> = ranked.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(order, vec!["a", "c", "b"]);
    }

    #[test]
    fn normalize_embedding_provider_accepts_chinese_builtin_local() {
        assert_eq!(normalize_embedding_provider("内置本地"), "builtin-local");
        assert_eq!(normalize_embedding_provider("builtin-local"), "builtin-local");
    }

    fn fact_update(subject: &str, predicate: &str, object: &str, action: &str) -> FactUpdate {
        FactUpdate {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            action: action.to_string(),
            subject_type: String::new(),
            object_type: String::new(),
            confidence: 0.7,
        }
    }

    #[test]
    fn persist_fact_updates_creates_entities_and_temporal_relations() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let entity_repo =
            crate::db::repositories::memory_entity_repo::MemoryEntityRepository::new(&conn);
        let relation_repo =
            crate::db::repositories::memory_relation_repo::MemoryRelationRepository::new(&conn);

        // 第 3 回合:登记"银钥匙 —藏在→ 12号柜"
        let (added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                3,
                &[fact_update("银钥匙", "藏在", "12号柜", "upsert")],
            )
            .expect("persist fact");
        assert_eq!((added, invalidated), (1, 0));
        let entities = entity_repo.list_by_session("sess-1").expect("list entities");
        assert_eq!(entities.len(), 2, "主语和宾语都应生成实体");

        // 第 4 回合:重复登记同一事实 → 不动
        let (added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                4,
                &[fact_update("银钥匙", "藏在", "12号柜", "upsert")],
            )
            .expect("persist same fact");
        assert_eq!((added, invalidated), (0, 0), "同一事实重复登记不应产生变化");
        let key_entity = entity_repo
            .find_by_normalized("sess-1", "银钥匙")
            .expect("find entity")
            .expect("entity exists");
        assert_eq!(key_entity.mention_count, 2, "实体提及次数应累计");

        // 第 5 回合:钥匙换地方了 → 旧关系失效,新关系生效
        let (added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                5,
                &[fact_update("银钥匙", "藏在", "13号柜", "upsert")],
            )
            .expect("persist contradicting fact");
        assert_eq!((added, invalidated), (1, 1));
        let active = relation_repo
            .list_active_by_session("sess-1")
            .expect("list active relations");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].object_text, "13号柜");
        assert_eq!(active[0].valid_from_turn, 5);
        let all = relation_repo
            .list_by_session("sess-1", false)
            .expect("list all relations");
        assert_eq!(all.len(), 2, "旧关系应保留为历史");
        assert!(
            all.iter()
                .any(|relation| relation.object_text == "12号柜"
                    && relation.invalid_at_turn == Some(5)),
            "旧关系应在第 5 回合被标记失效"
        );

        // 第 6 回合:作废事实
        let (added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                6,
                &[fact_update("银钥匙", "藏在", "", "invalidate")],
            )
            .expect("invalidate fact");
        assert_eq!((added, invalidated), (0, 1));
        assert!(
            relation_repo
                .list_active_by_session("sess-1")
                .expect("list active")
                .is_empty(),
            "作废后不应有活跃关系"
        );
        let key_entity = entity_repo
            .find_by_normalized("sess-1", "银钥匙")
            .expect("find entity")
            .expect("entity exists");
        assert_eq!(key_entity.mention_count, 4, "四个回合的提及都应累计");
    }

    #[test]
    fn persist_fact_updates_invalidate_only_matches_given_object() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let relation_repo =
            crate::db::repositories::memory_relation_repo::MemoryRelationRepository::new(&conn);

        service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                3,
                &[
                    fact_update("爱丽丝", "信任", "鲍勃", "upsert"),
                    fact_update("爱丽丝", "位于", "码头", "upsert"),
                ],
            )
            .expect("persist facts");

        // 宾语不匹配的作废请求:不应动任何关系
        let (_added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                4,
                &[fact_update("爱丽丝", "信任", "查理", "invalidate")],
            )
            .expect("invalidate non-matching");
        assert_eq!(invalidated, 0);
        assert_eq!(
            relation_repo
                .list_active_by_session("sess-1")
                .expect("list active")
                .len(),
            2
        );

        // 宾语匹配的作废请求:只作废那一条
        let (_added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                5,
                &[fact_update("爱丽丝", "信任", "鲍勃", "invalidate")],
            )
            .expect("invalidate one");
        assert_eq!(invalidated, 1);
        let active = relation_repo
            .list_active_by_session("sess-1")
            .expect("list active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].predicate, "位于", "只应作废宾语匹配的关系");
    }

    #[test]
    fn persist_fact_updates_skips_invalid_items() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create tables");
        let service = MemoryService::new();
        let world = sample_world();
        let (added, invalidated) = service
            .persist_fact_updates(
                &conn,
                &world,
                "sess-1",
                3,
                &[
                    fact_update("  ", "藏在", "12号柜", "upsert"),
                    fact_update("银钥匙", "  ", "12号柜", "upsert"),
                ],
            )
            .expect("persist invalid facts");
        assert_eq!((added, invalidated), (0, 0));
    }