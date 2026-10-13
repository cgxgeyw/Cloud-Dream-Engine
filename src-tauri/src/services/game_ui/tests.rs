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
    #[test]
    fn accepts_svg_and_canvas_nodes_with_known_shapes() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "stack",
                    "children": [
                        {
                            "type": "svg",
                            "view_box": "0 0 100 100",
                            "children": [
                                { "type": "svg_circle", "cx": "50", "cy": "50", "r": "40" },
                                { "type": "svg_path", "d": "M0 0 L10 10", "stroke_width": "2" },
                                { "type": "svg_text", "x": "50", "y": "10", "text": "spin" }
                            ]
                        },
                        {
                            "type": "canvas",
                            "width": "200",
                            "height": "120",
                            "ops": [
                                { "type": "clear", "color": "#ffffff" },
                                { "type": "arc", "x": "100", "y": "60", "radius": "40",
                                  "start_angle": "0", "end_angle": "270", "fill": "#ff0000" },
                                { "type": "text", "x": "10", "y": "20", "text": "hi" }
                            ]
                        }
                    ]
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(result.ok, "errors: {:?}", result.errors);
    }

    #[test]
    fn rejects_unknown_svg_primitive_and_canvas_op() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "svg",
                    "children": [
                        { "type": "svg_script", "cx": "1" },
                        { "type": "svg_circle", "cx": "1" }
                    ]
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_svg_primitive"));

        let canvas_source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "canvas",
                    "ops": [ { "type": "eval", "code": "alert(1)" } ]
                }
            }
        }"##;
        let canvas_result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: canvas_source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(!canvas_result.ok);
        assert!(canvas_result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_canvas_op"));
    }

    #[test]
    fn svg_primitives_require_their_geometry_fields() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "svg",
                    "children": [ { "type": "svg_circle", "cx": "10" } ]
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_svg_primitive"));
    }

    #[test]
    fn accepts_whitelisted_expression_functions() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "when",
                    "expr": "math_random_int(1, 6) >= 4 && math_clamp(150, 0, 100) == 100",
                    "child": { "type": "text", "text": "high roll" }
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(result.ok, "errors: {:?}", result.errors);
    }

    #[test]
    fn rejects_expression_functions_outside_the_whitelist() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "when",
                    "expr": "fetch('https://example.com') == 1",
                    "child": { "type": "text", "text": "nope" }
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "unknown_expression_function"));
    }

    #[test]
    fn still_rejects_backtick_and_bracket_syntax_in_expressions() {
        let service = GameUiService::new();
        let source = r##"{
            "schema_version": 2,
            "layout": {
                "root": {
                    "type": "when",
                    "expr": "state.hp[0] > 0",
                    "child": { "type": "text", "text": "nope" }
                }
            }
        }"##;

        let result = service.validate_world_ui_document(WorldUiDocumentRequest {
            source: source.to_string(),
            platform: Some("desktop".to_string()),
        });
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|diagnostic| diagnostic.code == "unsafe_expression"));
    }

    /// 小恶魔的毒酒宴世界包必须通过宿主真实校验器。
    ///
    /// 上一轮这个包线上整块牌桌不渲染（绑定写错 + for_each/when 布局属性被丢弃 +
    /// 缺根高度链），而当时没有任何一层测试覆盖世界包文档。这里把它钉进后端测试，
    /// 以后改坏会直接在 cargo test 阶段失败，而不是等用户截图。
    #[test]
    fn validates_poison_feast_world_package_ui() {
        let service = GameUiService::new();
        let desktop = include_str!("../../../../examples/world-packages/poison-feast/world/ui.desktop.jsonc");
        let mobile = include_str!("../../../../examples/world-packages/poison-feast/world/ui.mobile.jsonc");
        let desktop_css = include_str!("../../../../examples/world-packages/poison-feast/world/ui.desktop.css");
        let mobile_css = include_str!("../../../../examples/world-packages/poison-feast/world/ui.mobile.css");
        let logic_source = include_str!("../../../../examples/world-packages/poison-feast/world/logic.js");

        for (platform, source) in [("desktop", desktop), ("mobile", mobile)] {
            let result = service.validate_world_ui_document(WorldUiDocumentRequest {
                source: source.to_string(),
                platform: Some(platform.to_string()),
            });
            assert!(
                result.ok,
                "[{platform}] errors: {:?}",
                result.errors
            );
        }

        let bundle = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: desktop.to_string(),
            mobile_file: mobile.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: desktop_css.to_string(),
            mobile_stylesheet: mobile_css.to_string(),
            capabilities: vec!["supports_world_storage".to_string()],
            storage: serde_json::json!({
                "kv_namespaces": ["game", "variables"],
                "collections": {}
            }),
            logic: serde_json::json!({
                "runtime": "sandbox-js-v1",
                "source": logic_source,
            }),
        });
        assert!(
            bundle.ok,
            "bundle errors: {:?}; desktop: {:?}; mobile: {:?}",
            bundle.errors, bundle.desktop.errors, bundle.mobile.errors
        );
    }

    #[test]
    fn imports_poison_feast_world_package_archive() {
        use std::io::{Cursor, Write};
        let package_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/world-packages/poison-feast");
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in [
            "manifest.json", "world/world.json", "world/logic.js",
            "world/ui.desktop.jsonc", "world/ui.mobile.jsonc",
            "world/ui.desktop.css", "world/ui.mobile.css",
            "characters/little-devil/character.json", "assets/bg_tavern.jpg",
        ] {
            archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            archive.write_all(&std::fs::read(package_dir.join(name)).unwrap()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let data_dir = std::env::temp_dir().join(format!("poison-feast-import-{}", uuid::Uuid::new_v4()));
        let result = crate::services::world_package::WorldPackageService::import_package_archive(&data_dir, bytes);
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).unwrap();
        }
        let imported = result.unwrap_or_else(|error| panic!("package import failed: {error}"));
        assert_eq!(imported.characters.len(), 1);
        assert!(imported.desktop_ui_stylesheet.contains(".game-message-row--player"));
        assert!(imported.mobile_ui_stylesheet.contains(".game-message-row--player"));
    }
