// 世界包 UI 组件目录（前后端唯一来源）。
// 数据文件在仓库根 shared/game-ui/catalog.json：前端从这里导入，
// 后端 src-tauri/src/services/game_ui_catalog.rs 用 include_str! 编译进二进制。
// 新增组件 / 动作 / 能力只改那一个 JSON 文件。
import catalogJson from "../../../shared/game-ui/catalog.json";

export type GameUiCatalogCapability = {
  id: string;
  description: string;
};

export type GameUiCatalogAction = {
  id: string;
  description: string;
  input: Record<string, string>;
  implies_capabilities?: string[];
};

export type GameUiCatalogComponent = {
  id: string;
  label: string;
  description: string;
  props: Record<string, string>;
  implicit_actions: string[];
  implicit_capabilities: string[];
  allowed_slots: string[];
};

export type GameUiCatalog = {
  schema_version: number;
  capabilities: GameUiCatalogCapability[];
  actions: GameUiCatalogAction[];
  components: GameUiCatalogComponent[];
};

// JSON 模块推导类型会把不同条目推成联合类型，无法直接赋给接口，这里显式断言；
// 结构正确性由 catalog.test.ts 与后端加载时的 validate_catalog 双重保证。
export const GAME_UI_CATALOG = catalogJson as unknown as GameUiCatalog;

export function getCatalogComponent(componentId: string): GameUiCatalogComponent | undefined {
  return GAME_UI_CATALOG.components.find((component) => component.id === componentId);
}

export function getCatalogAction(actionId: string): GameUiCatalogAction | undefined {
  return GAME_UI_CATALOG.actions.find((action) => action.id === actionId);
}
