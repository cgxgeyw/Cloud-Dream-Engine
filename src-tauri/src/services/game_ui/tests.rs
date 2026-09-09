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
    fn validates_accounting_assistant_world_package_ui() {
        let service = GameUiService::new();
        let desktop = include_str!(
            "../../../../examples/world-packages/accounting-assistant/world/ui.desktop.jsonc"
        );
        let mobile = include_str!(
            "../../../../examples/world-packages/accounting-assistant/world/ui.mobile.jsonc"
        );

        let result = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: desktop.to_string(),
            mobile_file: mobile.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: include_str!(
                "../../../../examples/world-packages/accounting-assistant/world/ui.desktop.css"
            )
            .to_string(),
            mobile_stylesheet: include_str!(
                "../../../../examples/world-packages/accounting-assistant/world/ui.mobile.css"
            )
            .to_string(),
            capabilities: vec![
                "supports_world_records".to_string(),
                "supports_world_storage".to_string(),
            ],
            storage: serde_json::json!({
                "collections": { "ledger.entries": {} }
            }),
            logic: serde_json::json!({}),
        });

        assert!(
            result.ok,
            "desktop: {:?}; mobile: {:?}; bundle: {:?}",
            result.desktop.errors, result.mobile.errors, result.errors,
        );
        assert!(result
            .desktop
            .components
            .contains(&"ledger_book".to_string()));
        assert!(result
            .desktop
            .capabilities
            .contains(&"supports_world_records".to_string()));
        assert!(result.mobile.warnings.is_empty());
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