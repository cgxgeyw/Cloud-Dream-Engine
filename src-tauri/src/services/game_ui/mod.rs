use std::collections::BTreeSet;

use regex::Regex;
use serde_json::Value;

use crate::models::world::{
    VerifyWorldPackageUiCompatibilityRequest, WorldUiBundleValidationRequest,
    WorldUiBundleValidationResult, WorldUiCompatibilityDocumentReport, WorldUiCompatibilityReport,
    WorldUiCompatibilityTarget, WorldUiCompileRequest, WorldUiCompileResult, WorldUiDiagnostic,
    WorldUiDocumentRequest, WorldUiDocumentValidationResult,
};
use crate::services::game_ui_catalog::{game_ui_catalog, is_supported_capability};

// 组件 / 动作 / 能力 / 受支持版本列表的唯一来源是仓库根 `shared/game-ui/catalog.json`
// （经 `game_ui_catalog` 模块加载）。本文件不再维护任何硬编码清单。

const MAX_WORLD_STYLESHEET_BYTES: usize = 1024 * 1024;

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

pub const MAX_WORLD_LOGIC_BYTES: usize = 256 * 1024;

pub fn validate_world_logic_source(source: &str) -> Vec<WorldUiDiagnostic> {
    let mut diagnostics = Vec::new();
    if source.len() > MAX_WORLD_LOGIC_BYTES {
        push_source_diagnostic(
            &mut diagnostics,
            "error",
            "logic_too_large",
            "世界逻辑脚本超过 256 KiB 上限。",
            source,
            0,
            "logic.js",
        );
    }
    for forbidden in [
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "import ",
        "export ",
        "eval(",
        "Function(",
        "document.",
        "window.",
    ] {
        if let Some(offset) = source.find(forbidden) {
            push_source_diagnostic(
                &mut diagnostics,
                "error",
                "forbidden_logic_api",
                format!("logic.js 不能直接使用 `{forbidden}`；请通过 world SDK 或框架事件访问系统能力。"),
                source,
                offset,
                "logic.js",
            );
        }
    }
    let mut braces = 0i32;
    let mut parentheses = 0i32;
    let mut brackets = 0i32;
    let mut quote = None;
    let bytes = source.as_bytes();
    for (index, byte) in bytes.iter().copied().enumerate() {
        match quote {
            Some(current) if byte == current && (index == 0 || bytes[index - 1] != b'\\') => {
                quote = None
            }
            Some(_) => {}
            None => match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'{' => braces += 1,
                b'}' => braces -= 1,
                b'(' => parentheses += 1,
                b')' => parentheses -= 1,
                b'[' => brackets += 1,
                b']' => brackets -= 1,
                _ => {}
            },
        }
        if braces < 0 || parentheses < 0 || brackets < 0 {
            push_source_diagnostic(
                &mut diagnostics,
                "error",
                "invalid_logic_syntax",
                "logic.js 包含不匹配的括号。",
                source,
                index,
                "logic.js",
            );
            break;
        }
    }
    if quote.is_some() || braces != 0 || parentheses != 0 || brackets != 0 {
        push_source_diagnostic(
            &mut diagnostics,
            "error",
            "invalid_logic_syntax",
            "logic.js 包含未闭合的字符串或括号。",
            source,
            source.len().saturating_sub(1),
            "logic.js",
        );
    }
    diagnostics
}

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
        let runtime_version = request.runtime_version.unwrap_or(3);
        let desktop = self.validate_world_ui_document(WorldUiDocumentRequest {
            source: request.desktop_file,
            platform: Some("desktop".to_string()),
        });
        let mobile = self.validate_world_ui_document(WorldUiDocumentRequest {
            source: request.mobile_file,
            platform: Some("mobile".to_string()),
        });

        let mut diagnostics = Vec::new();
        if !game_ui_catalog()
            .supported_ui_runtime_versions
            .contains(&runtime_version)
        {
            diagnostics.push(WorldUiDiagnostic {
                severity: "error".to_string(),
                code: "unsupported_ui_runtime_version".to_string(),
                message: format!("不支持的 UI 运行时版本：{runtime_version}。"),
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
            validate_stylesheet(stylesheet, platform, &mut diagnostics);
            for unknown in find_unknown_game_classes(stylesheet) {
                diagnostics.push(WorldUiDiagnostic {
                    severity: "warning".to_string(),
                    code: "unknown_game_class".to_string(),
                    message: format!(
                        "{platform} stylesheet references `.{unknown}`, which is not a known game-ui class. Check docs/game-ui-input-alignment-rfc.md anchors."
                    ),
                    path: Some(format!("bundle.{platform}_stylesheet")),
                });
            }
        }
        for capability in &request.capabilities {
            if !is_supported_capability(capability.as_str()) {
                diagnostics.push(WorldUiDiagnostic {
                    severity: "error".to_string(),
                    code: "unsupported_declared_capability".to_string(),
                    message: format!("不支持的能力声明：{capability}。"),
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
        // 能力要求全部声明在 catalog 的能力条目上：
        // - UI 文档用到了某能力，或 bundle 特征命中其 required_when_bundle_features，
        //   就必须在 bundle.capabilities 里声明它（missing_declaration_error）；
        // - 声明了 requires_runtime_version 的能力在 runtime 版本不足时报
        //   runtime_version_error。诊断 code/message 随条目声明，校验器不再硬编码
        //   具体 capability id 与版本/文案的绑定。
        let bundle_features = detect_bundle_features(&request.storage, &request.logic);
        for capability in &game_ui_catalog().capabilities {
            let used_by_document = desktop
                .capabilities
                .iter()
                .chain(mobile.capabilities.iter())
                .any(|used| used == &capability.id);
            let required_by_features = capability
                .required_when_bundle_features
                .iter()
                .any(|feature| bundle_features.contains(feature.as_str()));
            if !used_by_document && !required_by_features {
                continue;
            }
            if let (Some(required_version), Some(error)) = (
                capability.requires_runtime_version,
                &capability.runtime_version_error,
            ) {
                if runtime_version < required_version {
                    diagnostics.push(WorldUiDiagnostic {
                        severity: "error".to_string(),
                        code: error.code.clone(),
                        message: error.message.clone(),
                        path: Some("bundle.runtime_version".to_string()),
                    });
                }
            }
            if !request
                .capabilities
                .iter()
                .any(|declared| declared == &capability.id)
            {
                if let Some(error) = &capability.missing_declaration_error {
                    diagnostics.push(WorldUiDiagnostic {
                        severity: "error".to_string(),
                        code: error.code.clone(),
                        message: error.message.clone(),
                        path: Some("bundle.capabilities".to_string()),
                    });
                }
            }
        }
        if desktop.schema_version != mobile.schema_version {
            diagnostics.push(WorldUiDiagnostic {
                severity: "warning".to_string(),
                code: "mixed_schema_versions".to_string(),
                message: "桌面端与移动端 UI 文档目前使用了不同的 schema 版本。".to_string(),
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
                        "目标 `{}` 不支持 schema 版本 {}。",
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
                    message: format!("目标 `{}` 不支持组件 `{component}`。", target.name),
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
                    message: format!("目标 `{}` 不支持动作 `{action}`。", target.name),
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
                    message: format!("目标 `{}` 未声明能力 `{capability}`。", target.name),
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
        let platform = requested_platform
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let mut diagnostics = Vec::new();
        validate_html_document(source, &mut diagnostics);

        CompilationSnapshot {
            platform,
            schema_version: None,
            normalized_document: (!source.trim().is_empty())
                .then(|| serde_json::json!({"html": source})),
            diagnostics,
            components: Vec::new(),
            actions: Vec::new(),
            capabilities: Vec::new(),
        }
    }
}

const MAX_WORLD_HTML_BYTES: usize = 2 * 1024 * 1024;

fn validate_html_document(source: &str, diagnostics: &mut Vec<WorldUiDiagnostic>) {
    if source.trim().is_empty() {
        push_html_diagnostic(
            diagnostics,
            "error",
            "empty_html_document",
            "世界 UI HTML 不能为空。",
            source,
            0,
        );
        return;
    }
    if source.len() > MAX_WORLD_HTML_BYTES {
        push_html_diagnostic(
            diagnostics,
            "error",
            "html_too_large",
            format!(
                "世界 UI HTML 不能超过 {} MiB。",
                MAX_WORLD_HTML_BYTES / 1024 / 1024
            ),
            source,
            0,
        );
    }
    if !source.trim_start().starts_with('<') {
        push_html_diagnostic(
            diagnostics,
            "error",
            "invalid_html_entry",
            "世界 UI 必须是 HTML 文档；旧 JSONC UI DSL 已移除。",
            source,
            0,
        );
        return;
    }

    let lower = source.to_ascii_lowercase();
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    let mut stack: Vec<(String, usize)> = Vec::new();
    let void_tags = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    let forbidden_tags = ["iframe", "object", "embed", "base"];
    let mut has_html = false;
    let mut has_body = false;
    let mut has_viewport = false;

    while let Some(relative) = source[cursor..].find('<') {
        let start = cursor + relative;
        if lower[start..].starts_with("<!--") {
            let Some(comment_end) = lower[start + 4..].find("-->") else {
                push_html_diagnostic(
                    diagnostics,
                    "error",
                    "unterminated_html_comment",
                    "HTML 注释没有闭合。",
                    source,
                    start,
                );
                break;
            };
            cursor = start + 4 + comment_end + 3;
            continue;
        }
        let Some(end) = find_html_tag_end(bytes, start) else {
            push_html_diagnostic(
                diagnostics,
                "error",
                "unterminated_html_tag",
                "HTML 标签缺少 `>`。",
                source,
                start,
            );
            break;
        };
        let raw = source[start..=end].trim();
        if raw.starts_with("<!") || raw.starts_with("<?") {
            cursor = end + 1;
            continue;
        }

        let inner = raw.trim_start_matches('<').trim_end_matches('>').trim();
        let closing = inner.starts_with('/');
        let inner = inner.trim_start_matches('/').trim_start();
        let name_end = inner
            .find(|character: char| character.is_ascii_whitespace() || character == '/')
            .unwrap_or(inner.len());
        let name = inner[..name_end].to_ascii_lowercase();
        if name.is_empty() {
            push_html_diagnostic(
                diagnostics,
                "error",
                "invalid_html_tag",
                "HTML 标签缺少有效的标签名。",
                source,
                start,
            );
            cursor = end + 1;
            continue;
        }

        if closing {
            match stack.pop() {
                Some((open, _)) if open == name => {}
                Some((open, open_offset)) => {
                    push_html_diagnostic(
                        diagnostics,
                        "error",
                        "mismatched_html_tag",
                        format!("HTML 标签 `</{name}>` 与 `<{open}>` 不匹配。"),
                        source,
                        start,
                    );
                    stack.push((open, open_offset));
                }
                None => push_html_diagnostic(
                    diagnostics,
                    "error",
                    "unexpected_html_closing_tag",
                    format!("HTML 关闭标签 `</{name}>` 没有对应的开始标签。"),
                    source,
                    start,
                ),
            }
            cursor = end + 1;
            continue;
        }

        has_html |= name == "html";
        has_body |= name == "body";
        if name == "meta" && lower[start..=end].contains("name=\"viewport\"") {
            has_viewport = true;
        }
        if forbidden_tags.contains(&name.as_str()) {
            push_html_diagnostic(
                diagnostics,
                "error",
                "forbidden_html_element",
                format!("HTML 元素 `<{name}>` 由世界 UI 运行时禁止使用。"),
                source,
                start,
            );
        }
        let attributes = &inner[name_end..];
        let external_url = Regex::new(r#"(?i)(?:src|href|action)\s*=\s*['\"]\s*(?:https?:|//)"#)
            .expect("external URL regex");
        if external_url.is_match(attributes) {
            push_html_diagnostic(
                diagnostics,
                "error",
                "external_html_resource",
                "世界 UI 只能引用包内资源或由框架提供的 asset URL。",
                source,
                start,
            );
        }
        if name == "script" && attributes.to_ascii_lowercase().contains("src=") {
            push_html_diagnostic(
                diagnostics,
                "error",
                "external_script_not_supported",
                "世界 UI 不支持外链脚本；请把脚本写在 HTML 内，或通过 world SDK 调用框架能力。",
                source,
                start,
            );
        }
        if name == "link" && attributes.to_ascii_lowercase().contains("stylesheet") {
            push_html_diagnostic(
                diagnostics,
                "error",
                "external_stylesheet_not_supported",
                "世界 UI 不支持外链样式表；请使用包内 stylesheet 文件。",
                source,
                start,
            );
        }

        let self_closing = raw.ends_with("/>") || void_tags.contains(&name.as_str());
        if !self_closing {
            stack.push((name.clone(), start));
        }
        cursor = end + 1;
        if name == "script" || name == "style" {
            let close_marker = format!("</{name}");
            if let Some(close_offset) = lower[cursor..].find(&close_marker) {
                cursor += close_offset;
            } else {
                push_html_diagnostic(
                    diagnostics,
                    "error",
                    "unterminated_html_element",
                    format!("`<{name}>` 没有对应的关闭标签。"),
                    source,
                    start,
                );
                break;
            }
        }
    }

    for (name, offset) in stack.into_iter().rev() {
        push_html_diagnostic(
            diagnostics,
            "error",
            "unclosed_html_tag",
            format!("HTML 标签 `<{name}>` 没有关闭。"),
            source,
            offset,
        );
    }
    if !has_html {
        push_html_diagnostic(
            diagnostics,
            "warning",
            "html_root_missing",
            "建议提供完整的 `<html>` 根元素；浏览器会自动补齐文档片段。",
            source,
            0,
        );
    }
    if !has_body {
        push_html_diagnostic(
            diagnostics,
            "warning",
            "html_body_missing",
            "建议提供 `<body>` 元素，以便桌面和移动端保持一致的布局基线。",
            source,
            0,
        );
    }
    if !has_viewport {
        push_html_diagnostic(
            diagnostics,
            "warning",
            "viewport_meta_missing",
            "移动端 UI 建议声明 `<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">`。",
            source,
            0,
        );
    }
}

fn find_html_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in bytes.iter().enumerate().skip(start) {
        match (quote, *byte) {
            (Some(current), value) if value == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(*byte),
            (None, b'>') => return Some(offset),
            _ => {}
        }
    }
    None
}

fn push_html_diagnostic(
    diagnostics: &mut Vec<WorldUiDiagnostic>,
    severity: &str,
    code: &str,
    message: impl Into<String>,
    source: &str,
    offset: usize,
) {
    push_source_diagnostic(
        diagnostics,
        severity,
        code,
        message,
        source,
        offset,
        "document",
    );
}

fn push_source_diagnostic(
    diagnostics: &mut Vec<WorldUiDiagnostic>,
    severity: &str,
    code: &str,
    message: impl Into<String>,
    source: &str,
    offset: usize,
    path_prefix: &str,
) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map(|(_, tail)| tail.chars().count() + 1)
        .unwrap_or_else(|| prefix.chars().count() + 1);
    diagnostics.push(WorldUiDiagnostic {
        severity: severity.to_string(),
        code: code.to_string(),
        message: message.into(),
        path: Some(format!("{path_prefix}:{line}:{column}")),
    });
}

fn validate_stylesheet(source: &str, platform: &str, diagnostics: &mut Vec<WorldUiDiagnostic>) {
    let mut braces = 0i32;
    let mut quote = None;
    let mut index = 0usize;
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            let Some(end) = source[index + 2..].find("*/") else {
                push_source_diagnostic(
                    diagnostics,
                    "error",
                    "unterminated_css_comment",
                    format!("{platform} stylesheet 的注释没有闭合。"),
                    source,
                    index,
                    &format!("bundle.{platform}_stylesheet"),
                );
                break;
            };
            index += end + 4;
            continue;
        }
        let byte = bytes[index];
        match quote {
            Some(current) if byte == current => quote = None,
            Some(_) => {}
            None => match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'{' => braces += 1,
                b'}' => {
                    braces -= 1;
                    if braces < 0 {
                        push_source_diagnostic(
                            diagnostics,
                            "error",
                            "unexpected_css_closing_brace",
                            format!("{platform} stylesheet 出现多余的 `}}`。"),
                            source,
                            index,
                            &format!("bundle.{platform}_stylesheet"),
                        );
                        braces = 0;
                    }
                }
                _ => {}
            },
        }
        index += 1;
    }
    if quote.is_some() {
        push_source_diagnostic(
            diagnostics,
            "error",
            "unterminated_css_string",
            format!("{platform} stylesheet 的字符串没有闭合。"),
            source,
            source.len().saturating_sub(1),
            &format!("bundle.{platform}_stylesheet"),
        );
    }
    if braces > 0 {
        push_source_diagnostic(
            diagnostics,
            "error",
            "unclosed_css_block",
            format!("{platform} stylesheet 的 CSS 块没有闭合。"),
            source,
            source.len().saturating_sub(1),
            &format!("bundle.{platform}_stylesheet"),
        );
    }
    let external_url =
        Regex::new(r#"(?i)url\(\s*['\"]?\s*(?:https?:|//)"#).expect("external CSS URL regex");
    if let Some(matched) = external_url.find(source) {
        push_source_diagnostic(
            diagnostics,
            "error",
            "external_css_resource",
            "世界 UI stylesheet 只能引用包内资源或 asset URL。",
            source,
            matched.start(),
            &format!("bundle.{platform}_stylesheet"),
        );
    }
}

/// 计算 bundle 级特征标志。特征 id 与 catalog 能力条目的
/// `required_when_bundle_features` 对应（game_ui_catalog::KNOWN_BUNDLE_FEATURES）：
/// - `storage_config`：storage.collections / kv_namespaces 非空；
/// - `sandbox_logic`：logic.runtime 为 sandbox-js-v1。
/// 特征判定依赖世界包内容，无法声明在静态 catalog 里，因此留在这个独立函数中。
fn detect_bundle_features(storage: &Value, logic: &Value) -> BTreeSet<&'static str> {
    let mut features = BTreeSet::new();
    let has_collections = storage
        .get("collections")
        .and_then(Value::as_object)
        .map(|collections| !collections.is_empty())
        .unwrap_or(false);
    let has_kv_namespaces = storage
        .get("kv_namespaces")
        .and_then(Value::as_array)
        .map(|namespaces| !namespaces.is_empty())
        .unwrap_or(false);
    if has_collections || has_kv_namespaces {
        features.insert("storage_config");
    }
    if logic.get("runtime").and_then(Value::as_str) == Some("sandbox-js-v1") {
        features.insert("sandbox_logic");
    }
    features
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

fn current_compatibility_target() -> WorldUiCompatibilityTarget {
    let catalog = game_ui_catalog();
    WorldUiCompatibilityTarget {
        name: "current-client".to_string(),
        supported_schema_versions: catalog.supported_document_schema_versions.clone(),
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

/// 世界包 stylesheet 里引用的、看起来像宿主游戏 UI 的 class。
/// 已知集合来自 theme.css / 生成基线 / 注册组件内部 DOM；未知仅 warning，
/// 避免误伤作者自定义 class（作者 class 不会以 game- 开头时不会命中）。
fn find_unknown_game_classes(stylesheet: &str) -> Vec<String> {
    // 粗提 `.game-foo` / `.game-foo-bar`（含 BEM `--`）
    let class_re = match Regex::new(r"\.((?:game-)[a-zA-Z0-9_-]+)") {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };
    let mut unknown = BTreeSet::new();
    for cap in class_re.captures_iter(stylesheet) {
        let Some(name) = cap.get(1) else {
            continue;
        };
        let name = name.as_str();
        if !is_known_game_class(name) {
            unknown.insert(name.to_string());
        }
    }
    unknown.into_iter().collect()
}

fn is_known_game_class(name: &str) -> bool {
    // 稳定族前缀：theme.css / 生成基线 / 注册组件内部 DOM。
    // 未知 game-* 仅 warning，不阻断导入。
    const FAMILIES: &[&str] = &[
        "game-root",
        "game-ui-",
        "game-message",
        "game-chat",
        "game-input-",
        "game-submit",
        "game-send",
        "game-textarea",
        "game-status",
        "game-side-",
        "game-tab",
        "game-attribute",
        "game-map",
        "game-avatar",
        "game-typing",
        "game-cot",
        "game-agent",
        "game-director",
        "game-tool-",
        "game-narration",
        "game-markdown",
        "game-md-",
        "game-simple-",
        "game-scene",
        "game-header",
        "game-back",
        "game-quick-",
        "game-mobile-",
        "game-loading",
        "game-error",
        "game-panel",
        "game-card",
        "game-badge",
        "game-session-diagnostic",
        "game-voice",
        "game-composer",
        "game-speaker",
        "game-content",
        "game-time",
        "game-place",
        "game-world",
        "game-meta",
        "game-title",
        "game-location",
        "game-player",
        "game-empty",
        "game-item",
        "game-list",
        "game-grid",
        "game-row",
        "game-stack",
        "game-shell",
        "game-main",
        "game-dock",
        "game-drawer",
        "game-handle",
        "game-close",
        "game-edit",
        "game-resend",
        "game-branch",
        "game-copy",
        "game-retry",
        "game-confirm",
        "game-ghost",
        "game-primary",
        "game-danger",
        "game-success",
        "game-warning",
        "game-info",
        "game-muted",
        "game-dim",
        "game-center",
        "game-left",
        "game-right",
        "game-top",
        "game-bottom",
        "game-full",
        "game-auto",
        "game-none",
        "game-hidden",
        "game-visible",
        "game-flex",
        "game-block",
        "game-inline",
        "game-absolute",
        "game-relative",
        "game-fixed",
        "game-sticky",
        "game-z-",
        "game-p-",
        "game-m-",
        "game-w-",
        "game-h-",
        "game-min-",
        "game-max-",
        "game-gap-",
        "game-rounded",
        "game-border",
        "game-bg-",
        "game-text-",
        "game-font-",
        "game-shadow",
        "game-opacity",
        "game-transition",
        "game-cursor",
        "game-select",
        "game-pointer",
        "game-events",
        "game-overflow",
        "game-truncate",
        "game-whitespace",
        "game-break",
        "game-underline",
        "game-uppercase",
        "game-lowercase",
        "game-capitalize",
        "game-italic",
        "game-antialiased",
        "game-sr-only",
        "game-focus",
        "game-hover",
        "game-active",
        "game-disabled",
        "game-checked",
        "game-selected",
        "game-expanded",
        "game-collapsed",
        "game-open",
        "game-closed",
    ];
    FAMILIES.iter().any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests;
