// 动作清单的唯一来源是 shared/game-ui/catalog.json（见 ./catalog）。
// 这里保留原有导出形态，供运行时与文档引用。
import { GAME_UI_CATALOG } from "./catalog";

export const GAME_UI_ACTION_IDS = GAME_UI_CATALOG.actions.map((action) => action.id);

export type GameUiActionId = string;

export type GameUiActionSchema = {
  id: GameUiActionId;
  description: string;
  input: Record<string, string>;
};

export const GAME_UI_ACTION_SCHEMAS: readonly GameUiActionSchema[] = GAME_UI_CATALOG.actions;
