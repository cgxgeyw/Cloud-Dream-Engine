use std::collections::BTreeSet;

use regex::Regex;
use serde_json::{Map, Value};

use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompatibilityDocumentReport, WorldUiCompatibilityReport,
    WorldUiCompatibilityTarget, WorldUiCompileRequest, WorldUiCompileResult, WorldUiDiagnostic,
    WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui_catalog::{
    capability_ids, find_action, find_component, game_ui_catalog, is_supported_capability,
};

// 组件 / 动作 / 能力的唯一来源是仓库根 `shared/game-ui/catalog.json`
// （经 `game_ui_catalog` 模块加载）。本文件不再维护任何硬编码清单。

const SUPPORTED_SCHEMA_VERSIONS: [u32; 1] = [2];
const SUPPORTED_UI_RUNTIME_VERSIONS: [u32; 2] = [2, 3];
const MAX_WORLD_STYLESHEET_BYTES: usize = 1024 * 1024;

struct CompilationState {
    diagnostics: Vec<WorldUiDiagnostic>,
    components: BTreeSet<String>,
    actions: BTreeSet<String>,
    capabilities: BTreeSet<String>,
}

impl CompilationState {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            components: BTreeSet::new(),
            actions: BTreeSet::new(),
            capabilities: BTreeSet::new(),
        }
    }

    fn error(&mut self, code: &str, message: impl Into<String>, path: impl Into<String>) {
        self.diagnostics.push(WorldUiDiagnostic {
            severity: "error".to_string(),
            code: code.to_string(),
            message: message.into(),
            path: Some(path.into()),
        });
    }

    fn warn(&mut self, code: &str, message: impl Into<String>, path: impl Into<String>) {
        self.diagnostics.push(WorldUiDiagnostic {
            severity: "warning".to_string(),
            code: code.to_string(),
            message: message.into(),
            path: Some(path.into()),
        });
    }

    fn add_action(&mut self, action_id: &str) {
        self.actions.insert(action_id.to_string());
        match find_action(action_id) {
            Some(action) => {
                for capability in &action.implies_capabilities {
                    self.capabilities.insert(capability.clone());
                }
            }
            None => {
                self.error(
                    "unknown_action",
                    format!("Unknown action id `{action_id}`."),
                    "action",
                );
            }
        }
    }
}

struct CompilationSnapshot {
    platform: Option<String>,
    schema_version: Option<u32>,
    normalized_document: Option<Value>,
    diagnostics: Vec<WorldUiDiagnostic>,
    components: Vec<String>,
    actions: Vec<String>,
    capabilities: Vec<String>,
}

pub struct GameUiService;

impl GameUiService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_world_ui_document(
        &self,
        request: WorldUiDocumentRequest,
    ) -> WorldUiDocumentValidationResult {
        let compiled = self.compile_snapshot(&request.source, request.platform.as_deref());
        let (errors, warnings) = partition_diagnostics(&compiled.diagnostics);

        WorldUiDocumentValidationResult {
            ok: errors.is_empty(),
            platform: compiled.platform,
            schema_version: compiled.schema_version,
            components: compiled.components,
            actions: compiled.actions,
            capabilities: compiled.capabilities,
            errors,
            warnings,
            normalized_document: compiled.normalized_document,
        }
    }

    pub fn validate_world_ui_bundle(
        &self,
        request: WorldUiBundleValidationRequest,
    ) -> WorldUiBundleValidationResult {
        let runtime_version = request.runtime_version.unwrap_or(2);
        let desktop = self.validate_world_ui_document(WorldUiDocumentRequest {
            source: request.desktop_file,
            platform: Some("desktop".to_string()),
        });
        let mobile = self.validate_world_ui_document(WorldUiDocumentRequest {
            source: request.mobile_file,
            platform: Some("mobile".to_string()),
        });

        let mut diagnostics = Vec::new();
        if !SUPPORTED_UI_RUNTIME_VERSIONS.contains(&runtime_version) {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "unsupported_ui_runtime_version".to_string(),
                message: format!("Unsupported UI runtime version: {runtime_version}."),
                path: Some("bundle.runtime_version".to_string()),
            });
        }
        for (platform, stylesheet) in [
            ("desktop", &request.desktop_stylesheet),
            ("mobile", &request.mobile_stylesheet),
        ] {
            if stylesheet.len() > MAX_WORLD_STYLESHEET_BYTES {
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "stylesheet_too_large".to_string(),
                    message: format!("{platform} stylesheet exceeds the 1 MiB limit."),
                    path: Some(format!("bundle.{platform}_stylesheet")),
                });
            }
        }
        for capability in &request.capabilities {
            if !is_supported_capability(capability.as_str()) {
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_declared_capability".to_string(),
                    message: format!("Unsupported declared capability: {capability}."),
                    path: Some("bundle.capabilities".to_string()),
                });
            }
        }
        if let Err(message) =
            crate::services::world_storage::validate_storage_config(&request.storage)
        {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "invalid_world_storage_config".to_string(),
                message,
                path: Some("bundle.storage".to_string()),
            });
        }
        if let Err(message) = crate::services::world_storage::validate_logic_config(&request.logic)
        {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "invalid_world_logic_config".to_string(),
                message,
                path: Some("bundle.logic".to_string()),
            });
        }
        let uses_world_storage = desktop
            .capabilities
            .iter()
            .chain(mobile.capabilities.iter())
            .any(|capability| capability == "supports_world_storage")
            || request
                .storage
                .get("collections")
                .and_then(Value::as_object)
                .map(|collections| !collections.is_empty())
                .unwrap_or(false)
            || request
                .storage
                .get("kv_namespaces")
                .and_then(Value::as_array)
                .map(|namespaces| !namespaces.is_empty())
                .unwrap_or(false)
            || request.logic.get("runtime").and_then(Value::as_str) == Some("sandbox-js-v1");
        if uses_world_storage
            && !request
                .capabilities
                .iter()
                .any(|capability| capability == "supports_world_storage")
        {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "missing_world_storage_capability".to_string(),
                message:
                    "Worlds using storage or sandbox logic must declare supports_world_storage."
                        .to_string(),
                path: Some("bundle.capabilities".to_string()),
            });
        }
        let uses_world_records = desktop
            .capabilities
            .iter()
            .chain(mobile.capabilities.iter())
            .any(|capability| capability == "supports_world_records");
        if uses_world_records && runtime_version < 3 {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "world_records_require_runtime_v3".to_string(),
                message: "World record storage is only available in UI runtime version 3."
                    .to_string(),
                path: Some("bundle.runtime_version".to_string()),
            });
        }
        if uses_world_records
            && !request
                .capabilities
                .iter()
                .any(|capability| capability == "supports_world_records")
        {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "missing_world_records_capability".to_string(),
                message: "Worlds using ledger_book must declare supports_world_records."
                    .to_string(),
                path: Some("bundle.capabilities".to_string()),
            });
        }
        if desktop.schema_version != mobile.schema_version {
            diagnostics.push(WorldUiDiagnostic {
                severity: "warning".to_string(),
                code: "mixed_schema_versions".to_string(),
                message: "Desktop and mobile UI documents currently use different schema versions."
                    .to_string(),
                path: Some("bundle".to_string()),
            });
        }

        let (errors, warnings) = partition_diagnostics(&diagnostics);

        WorldUiBundleValidationResult {
            ok: desktop.ok && mobile.ok && errors.is_empty(),
            desktop,
            mobile,
            errors,
            warnings,
        }
    }

    pub fn compile_world_ui_document(
        &self,
        request: WorldUiCompileRequest,
    ) -> WorldUiCompileResult {
        let compiled = self.compile_snapshot(&request.source, request.platform.as_deref());
        WorldUiCompileResult {
            ok: !compiled
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
            platform: compiled.platform,
            schema_version: compiled.schema_version,
            normalized_ast: compiled.normalized_document,
            component_dependencies: compiled.components,
            action_dependencies: compiled.actions,
            capability_requirements: compiled.capabilities,
            diagnostics: compiled.diagnostics,
        }
    }

    pub fn verify_world_package_ui_compatibility(
        &self,
        request: VerifyWorldPackageUiCompatibilityRequest,
    ) -> WorldUiCompatibilityReport {
        let target = normalize_compatibility_target(request.target);
        let desktop =
            self.compatibility_report_for_document("desktop", &request.desktop_file, &target);
        let mobile =
            self.compatibility_report_for_document("mobile", &request.mobile_file, &target);

        let mut diagnostics = Vec::new();
        diagnostics.extend(
            desktop
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == "error")
                .cloned(),
        );
        diagnostics.extend(
            mobile
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == "error")
                .cloned(),
        );

        WorldUiCompatibilityReport {
            ok: desktop.ok && mobile.ok,
            target,
            documents: vec![desktop, mobile],
            diagnostics,
        }
    }

    fn compatibility_report_for_document(
        &self,
        platform: &str,
        source: &str,
        target: &WorldUiCompatibilityTarget,
    ) -> WorldUiCompatibilityDocumentReport {
        let compiled = self.compile_world_ui_document(WorldUiCompileRequest {
            source: source.to_string(),
            platform: Some(platform.to_string()),
        });

        let mut diagnostics = compiled.diagnostics.clone();
        let mut unsupported_schema_versions = Vec::new();
        let mut unsupported_components = Vec::new();
        let mut unsupported_actions = Vec::new();
        let mut unsupported_capabilities = Vec::new();

        if let Some(schema_version) = compiled.schema_version {
            if !target.supported_schema_versions.contains(&schema_version) {
                unsupported_schema_versions.push(schema_version);
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_schema_version".to_string(),
                    message: format!(
                        "Target `{}` does not support schema_version {}.",
                        target.name, schema_version
                    ),
                    path: Some(format!("{platform}.schema_version")),
                });
            }
        }

        let supported_components = target
            .supported_components
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for component in &compiled.component_dependencies {
            if !supported_components.contains(component) {
                unsupported_components.push(component.clone());
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_component".to_string(),
                    message: format!(
                        "Target `{}` does not support component `{component}`.",
                        target.name
                    ),
                    path: Some(format!("{platform}.component:{component}")),
                });
            }
        }

        let supported_actions = target
            .supported_actions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for action in &compiled.action_dependencies {
            if !supported_actions.contains(action) {
                unsupported_actions.push(action.clone());
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_action".to_string(),
                    message: format!(
                        "Target `{}` does not support action `{action}`.",
                        target.name
                    ),
                    path: Some(format!("{platform}.action:{action}")),
                });
            }
        }

        let supported_capabilities = target
            .supported_capabilities
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for capability in &compiled.capability_requirements {
            if !supported_capabilities.contains(capability) {
                unsupported_capabilities.push(capability.clone());
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_capability".to_string(),
                    message: format!(
                        "Target `{}` does not expose capability `{capability}`.",
                        target.name
                    ),
                    path: Some(format!("{platform}.capability:{capability}")),
                });
            }
        }

        WorldUiCompatibilityDocumentReport {
            platform: platform.to_string(),
            ok: !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
            schema_version: compiled.schema_version,
            component_dependencies: compiled.component_dependencies,
            action_dependencies: compiled.action_dependencies,
            capability_requirements: compiled.capability_requirements,
            unsupported_schema_versions,
            unsupported_components,
            unsupported_actions,
            unsupported_capabilities,
            diagnostics,
        }
    }

    fn compile_snapshot(
        &self,
        source: &str,
        requested_platform: Option<&str>,
    ) -> CompilationSnapshot {
        let parsed = parse_document_source(source);
        let mut state = CompilationState::new();
        let mut platform = requested_platform
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());

        let parsed_value = match parsed {
            Ok(value) => value,
            Err(error) => {
                return CompilationSnapshot {
                    platform,
                    schema_version: None,
                    normalized_document: None,
                    diagnostics: vec![WorldUiDiagnostic {
                        severity: "error".to_string(),
                        code: "parse_error".to_string(),
                        message: error,
                        path: Some("document".to_string()),
                    }],
                    components: Vec::new(),
                    actions: Vec::new(),
                    capabilities: Vec::new(),
                };
            }
        };

        let Some(object) = parsed_value.as_object() else {
            return CompilationSnapshot {
                platform,
                schema_version: None,
                normalized_document: None,
                diagnostics: vec![WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "invalid_document_root".to_string(),
                    message: "UI document root must be an object.".to_string(),
                    path: Some("document".to_string()),
                }],
                components: Vec::new(),
                actions: Vec::new(),
                capabilities: Vec::new(),
            };
        };

        let normalized = parsed_value.clone();

        let schema_version = match object.get("schema_version") {
            Some(Value::Number(number)) => number.as_u64().map(|value| value as u32),
            Some(_) => {
                state.error(
                    "invalid_schema_version",
                    "schema_version must be a number.",
                    "schema_version",
                );
                None
            }
            None => {
                state.error(
                    "missing_schema_version",
                    "schema_version is missing. UI documents must declare schema_version 2.",
                    "schema_version",
                );
                None
            }
        };

        if let Some(version) = schema_version {
            if !SUPPORTED_SCHEMA_VERSIONS.contains(&version) {
                state.error(
                    "unsupported_schema_version",
                    format!("Unsupported schema_version {version}."),
                    "schema_version",
                );
            }
        }

        if let Some(meta) = object.get("meta") {
            if !meta.is_object() {
                state.error("invalid_meta", "meta must be an object.", "meta");
            } else if let Some(meta_platform) = meta.get("platform").and_then(Value::as_str) {
                let normalized_meta_platform = meta_platform.trim().to_ascii_lowercase();
                if !normalized_meta_platform.is_empty() {
                    if let Some(requested) = requested_platform {
                        if normalized_meta_platform != requested {
                            state.warn(
                                "platform_mismatch",
                                format!(
                                    "meta.platform is `{normalized_meta_platform}` but the request platform is `{requested}`."
                                ),
                                "meta.platform",
                            );
                        }
                    }
                    platform = Some(normalized_meta_platform);
                }
            }
        }

        if let Some(ui_state) = object.get("state") {
            let Some(ui_state_object) = ui_state.as_object() else {
                state.error("invalid_state", "state must be an object.", "state");
                return build_snapshot(platform, schema_version, normalized, state);
            };
            for (key, value) in ui_state_object {
                validate_prop_value(value, &format!("state.{key}"), &mut state);
                infer_dependencies_from_value(value, &format!("state.{key}"), &mut state);
            }
        }

        let Some(layout) = object.get("layout").and_then(Value::as_object) else {
            state.error("missing_layout", "layout.root is required.", "layout");
            return build_snapshot(platform, schema_version, normalized, state);
        };
        let Some(root) = layout.get("root") else {
            state.error(
                "missing_layout_root",
                "layout.root is required.",
                "layout.root",
            );
            return build_snapshot(platform, schema_version, normalized, state);
        };

        match schema_version {
            Some(2) => self.validate_v2_node(root, "layout.root", &mut state),
            Some(_) | None => {}
        }

        if platform.as_deref() == Some("mobile") {
            validate_mobile_document_rules(root, object.get("custom_css"), &mut state);
        }

        build_snapshot(platform, schema_version, normalized, state)
    }

    fn validate_v2_node(&self, node: &Value, path: &str, state: &mut CompilationState) {
        let Some(object) = node.as_object() else {
            state.error("invalid_node", "Layout node must be an object.", path);
            return;
        };

        let node_type = read_required_string(object, "type", path, state);
        validate_common_node_fields(object, path, state);

        match node_type.as_deref() {
            Some("grid") => {
                validate_grid_fields(object, path, state);
                validate_children_v2(self, object, path, state);
            }
            Some("stack") => {
                validate_stack_fields(object, path, state);
                validate_children_v2(self, object, path, state);
            }
            Some("absolute") => {
                validate_children_v2(self, object, path, state);
            }
            Some("component") => self.validate_component_node(object, path, state),
            Some("slot") => {
                read_required_string(object, "name", path, state);
            }
            Some("when") => {
                let expr = read_required_string(object, "expr", path, state);
                if let Some(expr) = expr {
                    validate_expression(&expr, &format!("{path}.expr"), state);
                }
                match object.get("child") {
                    Some(child) => self.validate_v2_node(child, &format!("{path}.child"), state),
                    None => state.error(
                        "missing_child",
                        "`when` nodes require a child node.",
                        format!("{path}.child"),
                    ),
                }
            }
            Some("for_each") => {
                let source = read_required_string(object, "source", path, state);
                if let Some(source) = source {
                    validate_binding(&source, &format!("{path}.source"), state);
                }
                read_required_string(object, "item_as", path, state);
                if let Some(index_as) = object.get("index_as").and_then(Value::as_str) {
                    if index_as.trim().is_empty() {
                        state.error(
                            "invalid_index_as",
                            "index_as cannot be an empty string.",
                            format!("{path}.index_as"),
                        );
                    }
                }
                match object.get("child") {
                    Some(child) => self.validate_v2_node(child, &format!("{path}.child"), state),
                    None => state.error(
                        "missing_child",
                        "`for_each` nodes require a child node.",
                        format!("{path}.child"),
                    ),
                }
                if let Some(empty) = object.get("empty") {
                    self.validate_v2_node(empty, &format!("{path}.empty"), state);
                }
            }
            Some("text") | Some("badge") => {
                read_required_string(object, "text", path, state);
                if let Some(variant) = object.get("variant") {
                    if !variant.is_string() {
                        state.error(
                            "invalid_variant",
                            "variant must be a string.",
                            format!("{path}.variant"),
                        );
                    }
                }
            }
            Some("image") => {
                read_required_string(object, "src", path, state);
                if let Some(alt) = object.get("alt") {
                    if !alt.is_string() {
                        state.error(
                            "invalid_alt",
                            "alt must be a string.",
                            format!("{path}.alt"),
                        );
                    }
                }
                if let Some(fit) = object.get("fit").and_then(Value::as_str) {
                    if !matches!(fit, "cover" | "contain" | "fill" | "none" | "scale-down") {
                        state.error(
                            "invalid_fit",
                            "fit must be cover, contain, fill, none, or scale-down.",
                            format!("{path}.fit"),
                        );
                    }
                }
            }
            Some("button") => {
                read_required_string(object, "label", path, state);
                if let Some(variant) = object.get("variant") {
                    if !variant.is_string() {
                        state.error(
                            "invalid_variant",
                            "variant must be a string.",
                            format!("{path}.variant"),
                        );
                    }
                }
                if let Some(disabled_when_empty_state) = object.get("disabled_when_empty_state") {
                    if !disabled_when_empty_state.is_string() {
                        state.error(
                            "invalid_disabled_when_empty_state",
                            "disabled_when_empty_state must be a string.",
                            format!("{path}.disabled_when_empty_state"),
                        );
                    }
                }
                if let Some(action) = object.get("action") {
                    validate_action_reference(action, &format!("{path}.action"), state);
                    infer_dependencies_from_value(action, &format!("{path}.action"), state);
                }
            }
            Some("checkbox") => {
                read_required_string(object, "label", path, state);
                read_required_string(object, "value", path, state);
                read_required_string(object, "bind_checked_list", path, state);
                for key in ["checked", "disabled"] {
                    if let Some(value) = object.get(key) {
                        if !value.is_boolean() {
                            state.error(
                                "invalid_checkbox_field",
                                format!("{key} must be a boolean."),
                                format!("{path}.{key}"),
                            );
                        }
                    }
                }
                if let Some(variant) = object.get("variant") {
                    if !variant.is_string() {
                        state.error(
                            "invalid_variant",
                            "variant must be a string.",
                            format!("{path}.variant"),
                        );
                    }
                }
            }
            Some(other) => state.error(
                "unsupported_v2_node",
                format!("Node type `{other}` is not valid for schema_version 2."),
                path,
            ),
            None => {}
        }
    }

    fn validate_component_node(
        &self,
        object: &Map<String, Value>,
        path: &str,
        state: &mut CompilationState,
    ) {
        let Some(component_id) = read_required_string(object, "component", path, state) else {
            return;
        };

        state.components.insert(component_id.clone());
        if let Some(support) = find_component(&component_id) {
            for action in &support.implicit_actions {
                state.add_action(action);
            }
            for capability in &support.implicit_capabilities {
                state.capabilities.insert(capability.clone());
            }

            for prop_key in support.props.keys() {
                if object.contains_key(prop_key.as_str()) {
                    state.error(
                        "misplaced_component_prop",
                        format!(
                            "Component prop `{prop_key}` must be nested inside the `props` object."
                        ),
                        format!("{path}.{prop_key}"),
                    );
                }
            }

            if let Some(props) = object.get("props") {
                let Some(props_object) = props.as_object() else {
                    state.error(
                        "invalid_component_props",
                        "component.props must be an object.",
                        format!("{path}.props"),
                    );
                    return;
                };

                for (prop_key, prop_value) in props_object {
                    if !support.props.contains_key(prop_key.as_str()) {
                        state.error(
                            "unknown_component_prop",
                            format!(
                                "Component `{component_id}` does not support prop `{prop_key}`."
                            ),
                            format!("{path}.props.{prop_key}"),
                        );
                    }
                    validate_prop_value(prop_value, &format!("{path}.props.{prop_key}"), state);
                    infer_dependencies_from_value(
                        prop_value,
                        &format!("{path}.props.{prop_key}"),
                        state,
                    );
                }
            }

            if let Some(slots) = object.get("slots") {
                let Some(slots_object) = slots.as_object() else {
                    state.error(
                        "invalid_component_slots",
                        "component.slots must be an object.",
                        format!("{path}.slots"),
                    );
                    return;
                };

                let allowed_slots = support
                    .allowed_slots
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                for (slot_name, slot_value) in slots_object {
                    if !allowed_slots.contains(slot_name.as_str()) {
                        state.error(
                            "unknown_component_slot",
                            format!(
                                "Component `{component_id}` does not expose slot `{slot_name}`."
                            ),
                            format!("{path}.slots.{slot_name}"),
                        );
                    }
                    if let Some(items) = slot_value.as_array() {
                        for (index, item) in items.iter().enumerate() {
                            self.validate_v2_node(
                                item,
                                &format!("{path}.slots.{slot_name}[{index}]"),
                                state,
                            );
                        }
                    } else {
                        self.validate_v2_node(
                            slot_value,
                            &format!("{path}.slots.{slot_name}"),
                            state,
                        );
                    }
                }
            }
        } else {
            state.error(
                "unknown_component",
                format!("Unknown component `{component_id}`."),
                format!("{path}.component"),
            );
        }

        if let Some(anchor) = object.get("anchor") {
            validate_anchor(anchor, &format!("{path}.anchor"), state);
        }
    }
}

fn build_snapshot(
    platform: Option<String>,
    schema_version: Option<u32>,
    normalized_document: Value,
    state: CompilationState,
) -> CompilationSnapshot {
    CompilationSnapshot {
        platform,
        schema_version,
        normalized_document: Some(normalized_document),
        components: state.components.into_iter().collect(),
        actions: state.actions.into_iter().collect(),
        capabilities: state.capabilities.into_iter().collect(),
        diagnostics: state.diagnostics,
    }
}

fn parse_document_source(source: &str) -> Result<Value, String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err("UI document source is empty.".to_string());
    }
    json5::from_str::<Value>(trimmed).map_err(|error| error.to_string())
}

fn partition_diagnostics(
    diagnostics: &[WorldUiDiagnostic],
) -> (Vec<WorldUiDiagnostic>, Vec<WorldUiDiagnostic>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for diagnostic in diagnostics {
        if diagnostic.severity == "error" {
            errors.push(diagnostic.clone());
        } else {
            warnings.push(diagnostic.clone());
        }
    }
    (errors, warnings)
}

fn read_required_string(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    state: &mut CompilationState,
) -> Option<String> {
    match object.get(key) {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Some(Value::String(_)) => {
            state.error(
                "empty_string",
                format!("{key} cannot be empty."),
                format!("{path}.{key}"),
            );
            None
        }
        Some(_) => {
            state.error(
                "invalid_string",
                format!("{key} must be a string."),
                format!("{path}.{key}"),
            );
            None
        }
        None => {
            state.error(
                "missing_string",
                format!("{key} is required."),
                format!("{path}.{key}"),
            );
            None
        }
    }
}

fn validate_common_node_fields(
    object: &Map<String, Value>,
    path: &str,
    state: &mut CompilationState,
) {
    for key in [
        "id",
        "class_name",
        "area",
        "width",
        "height",
        "min_width",
        "min_height",
        "max_width",
        "max_height",
        "padding",
        "margin",
        "align",
        "justify",
    ] {
        if let Some(value) = object.get(key) {
            if !value.is_string() {
                state.error(
                    "invalid_node_field",
                    format!("{key} must be a string."),
                    format!("{path}.{key}"),
                );
            }
        }
    }

    if let Some(visible) = object.get("visible") {
        if !visible.is_boolean() {
            state.error(
                "invalid_visible",
                "visible must be a boolean.",
                format!("{path}.visible"),
            );
        }
    }

    if let Some(style) = object.get("style") {
        validate_style_object(style, &format!("{path}.style"), state);
    }
}

fn validate_grid_fields(object: &Map<String, Value>, path: &str, state: &mut CompilationState) {
    validate_string_array(object.get("columns"), &format!("{path}.columns"), state);
    validate_string_array(object.get("rows"), &format!("{path}.rows"), state);

    if let Some(areas) = object.get("areas") {
        let Some(rows) = areas.as_array() else {
            state.error(
                "invalid_areas",
                "areas must be a 2D string array.",
                format!("{path}.areas"),
            );
            return;
        };

        let mut width = None;
        for (row_index, row) in rows.iter().enumerate() {
            let Some(columns) = row.as_array() else {
                state.error(
                    "invalid_areas_row",
                    "Each areas row must be an array of strings.",
                    format!("{path}.areas[{row_index}]"),
                );
                continue;
            };

            if let Some(expected) = width {
                if expected != columns.len() {
                    state.error(
                        "inconsistent_grid_areas",
                        "Every areas row must have the same number of columns.",
                        format!("{path}.areas[{row_index}]"),
                    );
                }
            } else {
                width = Some(columns.len());
            }

            for (column_index, area_name) in columns.iter().enumerate() {
                if area_name
                    .as_str()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    state.error(
                        "invalid_grid_area_name",
                        "Grid area names must be non-empty strings.",
                        format!("{path}.areas[{row_index}][{column_index}]"),
                    );
                }
            }
        }
    }
}

fn validate_stack_fields(object: &Map<String, Value>, path: &str, state: &mut CompilationState) {
    if let Some(direction) = object.get("direction").and_then(Value::as_str) {
        if direction != "horizontal" && direction != "vertical" {
            state.error(
                "invalid_direction",
                "direction must be `horizontal` or `vertical`.",
                format!("{path}.direction"),
            );
        }
    }

    if let Some(wrap) = object.get("wrap") {
        if !wrap.is_boolean() {
            state.error(
                "invalid_wrap",
                "wrap must be a boolean.",
                format!("{path}.wrap"),
            );
        }
    }
}

fn validate_children_v2(
    service: &GameUiService,
    object: &Map<String, Value>,
    path: &str,
    state: &mut CompilationState,
) {
    if let Some(children) = object.get("children") {
        let Some(children) = children.as_array() else {
            state.error(
                "invalid_children",
                "children must be an array.",
                format!("{path}.children"),
            );
            return;
        };
        for (index, child) in children.iter().enumerate() {
            service.validate_v2_node(child, &format!("{path}.children[{index}]"), state);
        }
    }
}

fn validate_string_array(value: Option<&Value>, path: &str, state: &mut CompilationState) {
    let Some(value) = value else {
        return;
    };
    let Some(items) = value.as_array() else {
        state.error(
            "invalid_string_array",
            "Value must be an array of strings.",
            path,
        );
        return;
    };
    for (index, item) in items.iter().enumerate() {
        if item
            .as_str()
            .map(|text| text.trim().is_empty())
            .unwrap_or(true)
        {
            state.error(
                "invalid_string_array_item",
                "Array items must be non-empty strings.",
                format!("{path}[{index}]"),
            );
        }
    }
}

fn validate_style_object(value: &Value, path: &str, state: &mut CompilationState) {
    let Some(object) = value.as_object() else {
        state.error("invalid_style", "style must be an object.", path);
        return;
    };

    for (key, entry) in object {
        if !matches!(
            entry,
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
        ) {
            state.error(
                "invalid_style_value",
                format!("Style value `{key}` must be a primitive or null."),
                format!("{path}.{key}"),
            );
        }
    }
}

fn validate_anchor(value: &Value, path: &str, state: &mut CompilationState) {
    let Some(object) = value.as_object() else {
        state.error("invalid_anchor", "anchor must be an object.", path);
        return;
    };
    for key in ["top", "right", "bottom", "left"] {
        if let Some(entry) = object.get(key) {
            if !entry.is_string() {
                state.error(
                    "invalid_anchor_value",
                    format!("anchor.{key} must be a string."),
                    format!("{path}.{key}"),
                );
            }
        }
    }
}

fn validate_mobile_document_rules(
    root: &Value,
    custom_css: Option<&Value>,
    state: &mut CompilationState,
) {
    let is_standalone_ledger = state.components.contains("ledger_book");
    if !is_standalone_ledger && !state.components.contains("input_composer") {
        state.warn(
            "mobile_missing_input_composer",
            "Mobile UI documents should include input_composer so the runtime can keep the composer visible when the keyboard opens.",
            "layout.root",
        );
    }

    if !is_standalone_ledger && !state.components.contains("side_panel_tabs") {
        state.warn(
            "mobile_missing_side_panel_tabs",
            "Mobile UI documents should put status, map, and custom tabs in side_panel_tabs instead of inline chat content.",
            "layout.root",
        );
    }

    inspect_mobile_node(root, "layout.root", state);

    if let Some(Value::String(css)) = custom_css {
        let normalized = css.to_ascii_lowercase();
        if normalized.contains("game-root--mobile-session")
            && (normalized.contains("height: 100dvh")
                || normalized.contains("height:100dvh")
                || normalized.contains("min-height: 100dvh")
                || normalized.contains("min-height:100dvh")
                || normalized.contains("height: 100vh")
                || normalized.contains("height:100vh")
                || normalized.contains("min-height: 100vh")
                || normalized.contains("min-height:100vh"))
        {
            state.warn(
                "mobile_css_fixed_viewport_height",
                "Mobile custom_css should avoid fixed 100vh/100dvh heights on the session root; use the runtime visible-height variable so the keyboard can resize content.",
                "custom_css",
            );
        }
    }
}

fn inspect_mobile_node(node: &Value, path: &str, state: &mut CompilationState) {
    let Some(object) = node.as_object() else {
        return;
    };

    for key in ["height", "min_height"] {
        if object
            .get(key)
            .and_then(Value::as_str)
            .map(is_fixed_mobile_viewport_height)
            .unwrap_or(false)
        {
            state.warn(
                "mobile_fixed_viewport_height",
                "Mobile layout nodes should not use fixed 100vh/100dvh heights; keyboard resizing works best with the runtime visible-height variable.",
                format!("{path}.{key}"),
            );
        }
    }

    if let Some(style) = object.get("style").and_then(Value::as_object) {
        for key in ["height", "min_height"] {
            if style
                .get(key)
                .and_then(Value::as_str)
                .map(is_fixed_mobile_viewport_height)
                .unwrap_or(false)
            {
                state.warn(
                    "mobile_fixed_viewport_height",
                    "Mobile layout style should not use fixed 100vh/100dvh heights; keyboard resizing works best with the runtime visible-height variable.",
                    format!("{path}.style.{key}"),
                );
            }
        }

        let has_fixed_viewport_height = ["height", "min_height"].iter().any(|key| {
            style
                .get(*key)
                .and_then(Value::as_str)
                .map(is_fixed_mobile_viewport_height)
                .unwrap_or(false)
        });

        if path == "layout.root"
            && has_fixed_viewport_height
            && style
                .get("overflow")
                .and_then(Value::as_str)
                .map(|value| value.trim().eq_ignore_ascii_case("hidden"))
                .unwrap_or(false)
        {
            state.warn(
                "mobile_root_overflow_hidden",
                "Mobile root overflow:hidden can trap the focused composer under the keyboard. Prefer constraining the scroll area inside the layout.",
                format!("{path}.style.overflow"),
            );
        }
    }

    for key in ["children"] {
        if let Some(children) = object.get(key).and_then(Value::as_array) {
            for (index, child) in children.iter().enumerate() {
                inspect_mobile_node(child, &format!("{path}.{key}[{index}]"), state);
            }
        }
    }

    for key in ["child", "empty"] {
        if let Some(child) = object.get(key) {
            inspect_mobile_node(child, &format!("{path}.{key}"), state);
        }
    }

    if let Some(slots) = object.get("slots").and_then(Value::as_object) {
        for (slot_name, slot_value) in slots {
            if let Some(items) = slot_value.as_array() {
                for (index, item) in items.iter().enumerate() {
                    inspect_mobile_node(item, &format!("{path}.slots.{slot_name}[{index}]"), state);
                }
            } else {
                inspect_mobile_node(slot_value, &format!("{path}.slots.{slot_name}"), state);
            }
        }
    }
}

fn is_fixed_mobile_viewport_height(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized == "100vh" || normalized == "100dvh"
}

fn validate_prop_value(value: &Value, path: &str, state: &mut CompilationState) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_prop_value(item, &format!("{path}[{index}]"), state);
            }
        }
        Value::Object(object) => {
            for (key, item) in object {
                validate_prop_value(item, &format!("{path}.{key}"), state);
            }
        }
    }
}

fn validate_action_reference(value: &Value, path: &str, state: &mut CompilationState) {
    let Some(object) = value.as_object() else {
        state.error("invalid_action", "action must be an object.", path);
        return;
    };

    let Some(action_id) = read_required_string(object, "id", path, state) else {
        return;
    };
    let normalized_action_id = action_id.trim().trim_start_matches('@');
    state.add_action(normalized_action_id);

    if let Some(args) = object.get("args") {
        let Some(args_object) = args.as_object() else {
            state.error(
                "invalid_action_args",
                "action.args must be an object.",
                format!("{path}.args"),
            );
            return;
        };
        for (key, item) in args_object {
            validate_prop_value(item, &format!("{path}.args.{key}"), state);
            infer_dependencies_from_value(item, &format!("{path}.args.{key}"), state);
        }
    }

    for key in [
        "content",
        "content_template",
        "mode",
        "result_state",
        "error_state",
        "pending_state",
    ] {
        if let Some(item) = object.get(key) {
            if !item.is_string() {
                state.error(
                    "invalid_action_field",
                    format!("action.{key} must be a string."),
                    format!("{path}.{key}"),
                );
            }
        }
    }
}

fn infer_dependencies_from_value(value: &Value, path: &str, state: &mut CompilationState) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if let Some(action_id) = trimmed.strip_prefix('@') {
                state.add_action(action_id);
            } else if trimmed.starts_with('$') {
                validate_binding(trimmed, path, state);
            }

            for capability in capability_ids() {
                let binding_token = format!("$capabilities.{capability}");
                let expr_token = format!("capabilities.{capability}");
                if trimmed == binding_token || trimmed.contains(&expr_token) {
                    state.capabilities.insert(capability.to_string());
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                infer_dependencies_from_value(item, &format!("{path}[{index}]"), state);
            }
        }
        Value::Object(object) => {
            if let Some(action_id) = object.get("id").and_then(Value::as_str) {
                if let Some(action_id) = action_id.trim().strip_prefix('@') {
                    state.add_action(action_id);
                }
            }

            for (key, item) in object {
                infer_dependencies_from_value(item, &format!("{path}.{key}"), state);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn validate_expression(expr: &str, path: &str, state: &mut CompilationState) {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        state.error("empty_expression", "expr cannot be empty.", path);
        return;
    }

    let function_like = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*\s*\(").expect("function regex");
    let illegal = Regex::new(r"[`\[\]{};?]|=>").expect("illegal expression regex");

    if illegal.is_match(trimmed) || function_like.is_match(trimmed) {
        state.error(
            "unsafe_expression",
            "expr contains syntax that is outside the supported safe expression subset.",
            path,
        );
    }

    for capability in capability_ids() {
        if trimmed.contains(&format!("capabilities.{capability}")) {
            state.capabilities.insert(capability.to_string());
        }
    }
}

fn validate_binding(binding: &str, path: &str, state: &mut CompilationState) {
    let binding_pattern = Regex::new(r"^\$[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*$")
        .expect("binding regex");
    if !binding_pattern.is_match(binding.trim()) {
        state.error(
            "invalid_binding",
            "Binding values must use a simple dotted path such as `$session.location`.",
            path,
        );
    }
}

fn current_compatibility_target() -> WorldUiCompatibilityTarget {
    let catalog = game_ui_catalog();
    WorldUiCompatibilityTarget {
        name: "current-client".to_string(),
        supported_schema_versions: SUPPORTED_SCHEMA_VERSIONS.to_vec(),
        supported_components: catalog
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect(),
        supported_actions: catalog
            .actions
            .iter()
            .map(|action| action.id.clone())
            .collect(),
        supported_capabilities: catalog
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect(),
    }
}

fn normalize_compatibility_target(
    target: Option<WorldUiCompatibilityTarget>,
) -> WorldUiCompatibilityTarget {
    let default_target = current_compatibility_target();
    let Some(target) = target else {
        return default_target;
    };

    WorldUiCompatibilityTarget {
        name: if target.name.trim().is_empty() {
            default_target.name
        } else {
            target.name.trim().to_string()
        },
        supported_schema_versions: dedupe_u32_list(target.supported_schema_versions),
        supported_components: dedupe_string_list(target.supported_components),
        supported_actions: dedupe_string_list(target.supported_actions),
        supported_capabilities: dedupe_string_list(target.supported_capabilities),
    }
}

fn dedupe_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn dedupe_u32_list(values: Vec<u32>) -> Vec<u32> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::GameUiService;
    use crate::models::world::{
        VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
        WorldUiCompatibilityTarget, WorldUiCompileRequest, WorldUiDocumentRequest,
    };

    #[test]
    fn validates_and_compiles_v2_seed_documents() {
        let service = GameUiService::new();
        let desktop = include_str!("../db/seeds/assets/gwtw-desktop-ui.jsonc");
        let mobile = include_str!("../db/seeds/assets/gwtw-mobile-ui.jsonc");

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
            include_str!("../db/seeds/assets/default-mobile-ui.jsonc"),
            include_str!("../db/seeds/assets/poetry-mobile-ui.jsonc"),
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
                include_str!("../db/seeds/assets/poetry-desktop-ui.jsonc"),
                include_str!("../db/seeds/assets/poetry-mobile-ui.jsonc"),
            ),
            (
                include_str!("../db/seeds/assets/schedule-assistant-desktop-ui.jsonc"),
                include_str!("../db/seeds/assets/schedule-assistant-mobile-ui.jsonc"),
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
            "../../../examples/world-packages/accounting-assistant/world/ui.desktop.jsonc"
        );
        let mobile = include_str!(
            "../../../examples/world-packages/accounting-assistant/world/ui.mobile.jsonc"
        );

        let result = service.validate_world_ui_bundle(WorldUiBundleValidationRequest {
            desktop_file: desktop.to_string(),
            mobile_file: mobile.to_string(),
            runtime_version: Some(3),
            desktop_stylesheet: include_str!(
                "../../../examples/world-packages/accounting-assistant/world/ui.desktop.css"
            )
            .to_string(),
            mobile_stylesheet: include_str!(
                "../../../examples/world-packages/accounting-assistant/world/ui.mobile.css"
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
}
