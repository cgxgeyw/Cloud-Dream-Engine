//! 世界包 UI 组件目录（前后端唯一来源）。
//!
//! 数据文件在仓库根的 `shared/game-ui/catalog.json`，本模块用 `include_str!`
//! 把它编译进二进制，前端经 `frontend/src/gameUiRuntime/catalog.ts` 导入同一份
//! JSON。新增组件 / 动作 / 能力只改那一个文件；加载时的 `expect` 保证目录损坏
//! 会在启动即暴露，而不是在校验世界包时才报错。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// bundle 级特征 id：`required_when_bundle_features` 只能引用这里的值。
/// 特征本身的判定依赖世界包内容（storage/logic 配置），由 game_ui.rs 的
/// detect_bundle_features 计算；catalog 只声明"特征 → 必需能力"的映射。
pub const KNOWN_BUNDLE_FEATURES: [&str; 2] = ["storage_config", "sandbox_logic"];

/// 能力校验失败时使用的诊断（code/message 与世界包校验器的既有输出保持一致，
/// 因此随能力条目一起声明在 catalog 里，而不是硬编码在校验器中）。
#[derive(Debug, Deserialize)]
pub struct GameUiCatalogDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct GameUiCatalogCapability {
    pub id: String,
    #[allow(dead_code)]
    pub description: String,
    /// 该能力要求的最低 UI runtime 版本（如 world records 需要 v3）。
    #[serde(default)]
    pub requires_runtime_version: Option<u32>,
    /// 该能力依赖的其它能力（声明式元数据；当前校验器不据此追加诊断）。
    #[serde(default)]
    #[allow(dead_code)]
    pub requires_capabilities: Vec<String>,
    /// bundle 出现这些特征时，即使 UI 文档没用到该能力，也必须声明它。
    #[serde(default)]
    pub required_when_bundle_features: Vec<String>,
    /// 使用了该能力但 runtime 版本不足时的诊断。
    #[serde(default)]
    pub runtime_version_error: Option<GameUiCatalogDiagnostic>,
    /// 使用了该能力（或 bundle 特征要求它）但未在 bundle.capabilities 声明时的诊断。
    #[serde(default)]
    pub missing_declaration_error: Option<GameUiCatalogDiagnostic>,
}

#[derive(Debug, Deserialize)]
pub struct GameUiCatalogAction {
    pub id: String,
    #[allow(dead_code)]
    pub description: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub input: BTreeMap<String, String>,
    #[serde(default)]
    pub implies_capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GameUiCatalogComponent {
    pub id: String,
    #[allow(dead_code)]
    pub label: String,
    #[allow(dead_code)]
    pub description: String,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    #[serde(default)]
    pub implicit_actions: Vec<String>,
    #[serde(default)]
    pub implicit_capabilities: Vec<String>,
    #[serde(default)]
    pub allowed_slots: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GameUiCatalog {
    #[allow(dead_code)]
    pub schema_version: u32,
    /// 受支持的 UI 文档 schema_version 列表（旧版 catalog 文件缺省为空，
    /// validate_catalog 会拒绝空列表，因此仓库内的 catalog 必须显式给出）。
    #[serde(default)]
    pub supported_document_schema_versions: Vec<u32>,
    /// 受支持的 UI runtime 版本列表。
    #[serde(default)]
    pub supported_ui_runtime_versions: Vec<u32>,
    pub capabilities: Vec<GameUiCatalogCapability>,
    pub actions: Vec<GameUiCatalogAction>,
    pub components: Vec<GameUiCatalogComponent>,
}

static CATALOG: LazyLock<GameUiCatalog> = LazyLock::new(|| {
    let catalog: GameUiCatalog =
        serde_json::from_str(include_str!("../../../shared/game-ui/catalog.json"))
            .expect("shared game UI catalog must be valid JSON matching the catalog schema");
    validate_catalog(&catalog);
    catalog
});

pub fn game_ui_catalog() -> &'static GameUiCatalog {
    &CATALOG
}

pub fn find_component(component_id: &str) -> Option<&'static GameUiCatalogComponent> {
    game_ui_catalog()
        .components
        .iter()
        .find(|component| component.id == component_id)
}

pub fn find_action(action_id: &str) -> Option<&'static GameUiCatalogAction> {
    game_ui_catalog()
        .actions
        .iter()
        .find(|action| action.id == action_id)
}

pub fn is_supported_capability(capability_id: &str) -> bool {
    game_ui_catalog()
        .capabilities
        .iter()
        .any(|capability| capability.id == capability_id)
}

pub fn capability_ids() -> impl Iterator<Item = &'static str> {
    game_ui_catalog()
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
}

fn validate_catalog(catalog: &GameUiCatalog) {
    assert_unique("capability", catalog.capabilities.iter().map(|entry| entry.id.as_str()));
    assert_unique("action", catalog.actions.iter().map(|entry| entry.id.as_str()));
    assert_unique(
        "component",
        catalog.components.iter().map(|entry| entry.id.as_str()),
    );

    assert!(
        !catalog.supported_document_schema_versions.is_empty(),
        "catalog supported_document_schema_versions cannot be empty"
    );
    assert!(
        !catalog.supported_ui_runtime_versions.is_empty(),
        "catalog supported_ui_runtime_versions cannot be empty"
    );

    for capability in &catalog.capabilities {
        if let Some(required) = capability.requires_runtime_version {
            assert!(
                catalog.supported_ui_runtime_versions.contains(&required),
                "catalog capability `{}` requires unsupported UI runtime version {required}",
                capability.id
            );
            let error = capability.runtime_version_error.as_ref().unwrap_or_else(|| {
                panic!(
                    "catalog capability `{}` declares requires_runtime_version without runtime_version_error",
                    capability.id
                )
            });
            assert!(
                !error.code.trim().is_empty() && !error.message.trim().is_empty(),
                "catalog capability `{}` has an empty runtime_version_error",
                capability.id
            );
        }
        for dependency in &capability.requires_capabilities {
            assert!(
                catalog.capabilities.iter().any(|entry| &entry.id == dependency),
                "catalog capability `{}` requires unknown capability `{dependency}`",
                capability.id
            );
        }
        for feature in &capability.required_when_bundle_features {
            assert!(
                KNOWN_BUNDLE_FEATURES.contains(&feature.as_str()),
                "catalog capability `{}` references unknown bundle feature `{feature}`",
                capability.id
            );
        }
        if let Some(error) = &capability.missing_declaration_error {
            assert!(
                !error.code.trim().is_empty() && !error.message.trim().is_empty(),
                "catalog capability `{}` has an empty missing_declaration_error",
                capability.id
            );
        }
    }

    for action in &catalog.actions {
        for capability in &action.implies_capabilities {
            assert!(
                catalog
                    .capabilities
                    .iter()
                    .any(|entry| &entry.id == capability),
                "catalog action `{}` implies unknown capability `{capability}`",
                action.id
            );
        }
    }
    for component in &catalog.components {
        assert!(
            !component.id.trim().is_empty(),
            "catalog component id cannot be empty"
        );
        for action in &component.implicit_actions {
            assert!(
                catalog.actions.iter().any(|entry| &entry.id == action),
                "catalog component `{}` references unknown action `{action}`",
                component.id
            );
        }
        for capability in &component.implicit_capabilities {
            assert!(
                catalog
                    .capabilities
                    .iter()
                    .any(|entry| &entry.id == capability),
                "catalog component `{}` references unknown capability `{capability}`",
                component.id
            );
        }
    }
}

fn assert_unique<'a>(kind: &str, ids: impl Iterator<Item = &'a str>) {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        assert!(
            seen.insert(id),
            "duplicate {kind} id `{id}` in shared game UI catalog"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_and_is_internally_consistent() {
        let catalog = game_ui_catalog();
        assert_eq!(catalog.schema_version, 1);
        assert!(!catalog.capabilities.is_empty());
        assert!(!catalog.actions.is_empty());
        assert!(!catalog.components.is_empty());
        // validate_catalog 已在加载时跑过；这里显式再跑一遍作为回归锚点。
        validate_catalog(catalog);
    }

    #[test]
    fn capability_requirements_are_declared_in_catalog() {
        let catalog = game_ui_catalog();
        assert_eq!(catalog.supported_document_schema_versions, vec![2]);
        assert_eq!(catalog.supported_ui_runtime_versions, vec![2, 3]);

        let records = catalog
            .capabilities
            .iter()
            .find(|capability| capability.id == "supports_world_records")
            .expect("supports_world_records must exist");
        assert_eq!(records.requires_runtime_version, Some(3));
        assert!(records.runtime_version_error.is_some());
        assert!(records.missing_declaration_error.is_some());

        let storage = catalog
            .capabilities
            .iter()
            .find(|capability| capability.id == "supports_world_storage")
            .expect("supports_world_storage must exist");
        assert_eq!(
            storage.required_when_bundle_features,
            vec!["storage_config".to_string(), "sandbox_logic".to_string()]
        );
        assert!(storage.missing_declaration_error.is_some());
    }

    #[test]
    fn every_component_prop_has_a_type_hint() {
        for component in &game_ui_catalog().components {
            for (prop, type_hint) in &component.props {
                assert!(
                    !type_hint.trim().is_empty(),
                    "component `{}` prop `{prop}` has an empty type hint",
                    component.id
                );
            }
        }
    }
}
