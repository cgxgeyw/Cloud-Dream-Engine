
use super::GameUiService;
use crate::models::world::{WorldUiBundleValidationRequest, WorldUiDocumentRequest};

#[test]
fn storage_features_require_declared_world_storage_capability() {
    let service = GameUiService::new();
    let document = "<!doctype html><html><body><main></main></body></html>";
    let validate = |storage: serde_json::Value, logic: serde_json::Value| {
        service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: document.to_string(),
            mobile_file: document.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: String::new(),
            mobile_stylesheet: String::new(),
            capabilities: Vec::new(),
            storage,
            logic,
        })
    };

    // storage 配置非空 ⇒ 命中 catalog 声明的 storage_config 特征。
    let result = validate(
        serde_json::json!({ "collections": { "ledger.entries": {} } }),
        serde_json::json!({}),
    );
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
fn validates_html_world_ui_document() {
    let result = GameUiService::new().validate_world_ui_document(WorldUiDocumentRequest {
            source: "<!doctype html><html><body><main id='game'></main><script>document.body.dataset.ready='yes'</script></body></html>".into(),
            platform: Some("desktop".into()),
        });
    assert!(result.ok, "HTML UI errors: {:?}", result.errors);
    assert_eq!(result.schema_version, None);
    assert!(result.normalized_document.unwrap()["html"]
        .as_str()
        .unwrap()
        .contains("<main"));
}

#[test]
fn rejects_jsonc_world_ui_document() {
    let result = GameUiService::new().validate_world_ui_document(WorldUiDocumentRequest {
        source: r#"{"schema_version":2,"layout":{"root":{"type":"stack","children":[]}}}"#.into(),
        platform: Some("mobile".into()),
    });
    assert!(!result.ok);
    assert_eq!(result.errors[0].code, "invalid_html_entry");
}

#[test]
fn reports_html_security_and_source_locations() {
    let result = GameUiService::new().validate_world_ui_document(WorldUiDocumentRequest {
        source: "<html>\n<body><iframe src=\"https://example.test\"></iframe>\n</body>".into(),
        platform: Some("desktop".into()),
    });
    assert!(!result.ok);
    assert!(result
        .errors
        .iter()
        .any(|diagnostic| diagnostic.code == "forbidden_html_element"));
    assert!(result.errors.iter().all(|diagnostic| diagnostic
        .path
        .as_deref()
        .unwrap_or("")
        .contains(':')));
}

#[test]
fn validates_logic_source_without_network_or_host_access() {
    let diagnostics = crate::services::game_ui::validate_world_logic_source(
        "export function run() { fetch('https://example.test'); }",
    );
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "forbidden_logic_api"));
}
