//! 世界包 UI 组件目录（前后端唯一来源）。
//!
//! 数据文件在仓库根的 `shared/game-ui/catalog.json`，本模块用 `include_str!`
//! 把它编译进二进制，前端经 `frontend/src/gameUiRuntime/catalog.ts` 导入同一份
//! JSON。新增组件 / 动作 / 能力只改那一个文件；加载时的 `expect` 保证目录损坏
//! 会在启动即暴露，而不是在校验世界包时才报错。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GameUiCatalogCapability {
    pub id: String,
    #[allow(dead_code)]
    pub description: String,
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
