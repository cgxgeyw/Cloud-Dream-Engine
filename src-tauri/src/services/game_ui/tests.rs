    use super::GameUiService;
    use crate::models::world::{
        VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
        WorldUiCompatibilityTarget, WorldUiCompileRequest, WorldUiDocumentRequest,
    };

    #[test]
    fn validates_and_compiles_v2_seed_documents() {
        let service = GameUiService::new();
        let desktop = include_str!("../../db/seeds/assets/gwtw-desktop-ui.jsonc");
        let mobile = include_str!("../../db/seeds/assets/gwtw-mobile-ui.jsonc");

        let desktop_result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: desktop.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(desktop_result.ok);
        assert_eq!(desktop_result.schema_version, Some(2));

        let bundle = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: desktop.to_string(),
            mobile_file: mobile.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: String::new(),
            mobile_stylesheet: String::new(),
            capabilities: Vec::new(),
            storage: serde_json::json!({}),
            logic: serde_json::json!({}),
        });
        assert!(
            bundle.ok,
            "desktop errors: {:?}; mobile errors: {:?}; bundle errors: {:?}",
            bundle.desktop.errors, bundle.mobile.errors, bundle.errors
        );

        let compiled = service.compile_world_ui_document(WorldUiCompileRequest {
            source: desktop.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(compiled.ok);
        assert!(compiled
            .component_dependencies
            .contains(&"input_composer".to_string()));
    }

    #[test]
    fn warns_on_unknown_game_class_in_stylesheet() {
        let service = GameUiService::new();
        let desktop = include_str!("../../db/seeds/assets/gwtw-desktop-ui.jsonc");
        let mobile = include_str!("../../db/seeds/assets/gwtw-mobile-ui.jsonc");
        let bundle = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: desktop.to_string(),
            mobile_file: mobile.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: ".game-textarea { width: 100%; }\n.game-not-a-real-thing { color: red; }\n"
                .to_string(),
            mobile_stylesheet: String::new(),
            capabilities: Vec::new(),
            storage: serde_json::json!({}),
            logic: serde_json::json!({}),
        });
        assert!(bundle.ok);
        assert!(
            bundle
                .warnings
                .iter()
                .any(|w| w.code == "unknown_game_class" && w.message.contains("game-not-a-real-thing")),
            "warnings: {:?}",
            bundle.warnings
        );
        assert!(
            !bundle
                .warnings
                .iter()
                .any(|w| w.code == "unknown_game_class" && w.message.contains("game-textarea")),
            "game-textarea should be known; warnings: {:?}",
            bundle.warnings
        );
    }

    #[test]
    fn validates_additional_mobile_seed_documents() {
        let service = GameUiService::new();
        for mobile in [
            include_str!("../../db/seeds/assets/default-mobile-ui.jsonc"),
            include_str!("../../db/seeds/assets/poetry-mobile-ui.jsonc"),
        ] {
            let result = service.validate_world_ui_document(WorldUiDocumentRequest {
                source: mobile.to_string(),
                platform: Some("mobile".to_string()),
            });

            assert!(
                result.ok,
                "mobile seed errors: {:?}; warnings: {:?}",
                result.errors, result.warnings
            );
            assert!(result.components.contains(&"input_composer".to_string()));
            assert!(result.components.contains(&"side_panel_tabs".to_string()));
        }
    }

    #[test]
    fn validates_v3_migration_bundles_for_preserved_example_worlds() {
        let service = GameUiService::new();
        for (desktop, mobile) in [
            (
                include_str!("../../db/seeds/assets/poetry-desktop-ui.jsonc"),
                include_str!("../../db/seeds/assets/poetry-mobile-ui.jsonc"),
            ),
            (
                include_str!("../../db/seeds/assets/schedule-assistant-desktop-ui.jsonc"),
                include_str!("../../db/seeds/assets/schedule-assistant-mobile-ui.jsonc"),
            ),
        ] {
            let result = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
                desktop_file: desktop.to_string(),
                mobile_file: mobile.to_string(),
                runtime_version: Some(3),
                desktop_stylesheet: ".desktop-entry { min-width: 0; }".to_string(),
                mobile_stylesheet: ".mobile-entry { min-width: 0; }".to_string(),
                capabilities: vec![
                    "supports_file_picker".to_string(),
                    "supports_mic".to_string(),
                ],
                storage: serde_json::json!({}),
                logic: serde_json::json!({}),
            });
            assert!(
                result.ok,
                "desktop: {:?}; mobile: {:?}; bundle: {:?}",
                result.desktop.errors, result.mobile.errors, result.errors,
            );
        }
    }

    #[test]
    fn ledger_book_requires_runtime_v3_and_explicit_capability() {
        let service = GameUiService::new();
        let document = r#"{
          schema_version: 2,
          layout: {
            root: {
              type: "component",
              component: "ledger_book",
              props: { collection: "ledger.entries" }
            }
          }
        }"#;

        let result = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: document.to_string(),
            mobile_file: document.to_string(),
            runtime_version: Some(2),
            desktop_stylesheet: String::new(),
            mobile_stylesheet: String::new(),
            capabilities: Vec::new(),
            storage: serde_json::json!({}),
            logic: serde_json::json!({}),
        });

        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "world_records_require_runtime_v3"));
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_world_records_capability"));
    }

    #[test]
    fn storage_features_require_declared_world_storage_capability() {
        let service = GameUiService::new();
        let document = r#"{
          schema_version: 2,
          layout: {
            root: {
              type: "component",
              component: "scene_header"
            }
          }
        }"#;
        let validate = |storage: serde_json::Value, logic: serde_json::Value| {
            service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
                desktop_file: document.to_string(),
                mobile_file: document.to_string(),
                runtime_version: Some(2),
                desktop_stylesheet: String::new(),
                mobile_stylesheet: String::new(),
                capabilities: Vec::new(),
                storage,
                logic,
            })
        };

        // storage 配置非空 ⇒ 命中 catalog 声明的 storage_config 特征。
        let result = validate(serde_json::json!({ "collections": { "ledger.entries": {} } }), serde_json::json!({}));
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_world_storage_capability"));

        // 沙箱 logic runtime ⇒ 命中 sandbox_logic 特征。
        let result = validate(
            serde_json::json!({}),
            serde_json::json!({ "runtime": "sandbox-js-v1", "entry": "logic.js" }),
        );
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_world_storage_capability"));

        // 无特征、文档未使用 ⇒ 不要求声明。
        let result = validate(serde_json::json!({}), serde_json::json!({}));
        assert!(!result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_world_storage_capability"));
    }

    #[test]
    fn button_nodes_may_carry_children_without_a_label() {
        let service = GameUiService::new();
        let validate = |source: &str| {
            service.validate_world_ui_document(WorldUiDocumentRequest {
                source: source.to_string(),
                platform: Some("mobile".to_string()),
            })
        };

        // 有子内容的按钮（例如整根图表柱子做成可点区域）可以不写 label，
        // 可读名称由子内容本身承担。
        let result = validate(
            r#"{
          schema_version: 2,
          layout: {
            root: {
              type: "button",
              label: "",
              children: [
                { type: "when", expr: "bar.selected == true", child: { type: "text", text: "选中" } },
                { type: "when", expr: "bar.selected != true", child: { type: "text", text: "未选中" } }
              ]
            }
          }
        }"#,
        );
        assert!(result.ok, "errors: {:?}", result.errors);
        assert!(!result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.path.as_deref() == Some("layout.root.label")));

        // 既没 label 又没 children 的按钮既看不见也没有可读名称，仍然要求 label。
        let result = validate(
            r#"{
          schema_version: 2,
          layout: { root: { type: "button" } }
        }"#,
        );
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_string"
                && diagnostic.path.as_deref() == Some("layout.root.label")));

        // children 不是数组、或子节点本身有问题时照常报错。
        let result = validate(
            r#"{
          schema_version: 2,
          layout: { root: { type: "button", label: "继续", children: { type: "text" } } }
        }"#,
        );
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_children"));

        let result = validate(
            r#"{
          schema_version: 2,
          layout: { root: { type: "button", label: "继续", children: [{ type: "text" }] } }
        }"#,
        );
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.path.as_deref() == Some("layout.root.children[0].text")));
    }

    #[test]
    fn compatibility_reports_unsupported_components() {
        let service = GameUiService::new();
        let report = service.verify_world_package_ui_compatibility(
            VerifyWorldPackageUiCompatibilityRequest {
                desktop_file: r#"{
                  schema_version: 2,
                  layout: {
                    root: {
                      type: "component",
                      component: "unknown_widget"
                    }
                  }
                }"#
                .to_string(),
                mobile_file: r#"{
                  schema_version: 2,
                  layout: {
                    root: {
                      type: "component",
                      component: "scene_header"
                    }
                  }
                }"#
                .to_string(),
                target: Some(WorldUiCompatibilityTarget {
                    name: "limited".to_string(),
                    supported_schema_versions: vec![2],
                    supported_components: vec!["scene_header".to_string()],
                    supported_actions: vec![],
                    supported_capabilities: vec![],
                }),
            },
        );

        assert!(!report.ok);
        assert!(report.documents[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unknown_component"));
    }