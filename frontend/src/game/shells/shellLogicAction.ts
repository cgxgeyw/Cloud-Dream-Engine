import type { GameUiActionReference, WorldLogicConfig } from "../../data/gameUi";
import { invokeWorldLogic } from "../../worldFrame/WorldLogicRuntime";
import {
  createWorldRecord,
  listWorldRecords,
  updateWorldRecord,
} from "../../data/tauriApi";

function readString(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return "";
}

type ShellStorageBridge = {
  worldId: string;
};

function createShellStorageSend(worldId: string) {
  return async (action: {
    type: string;
    collection?: string;
    data?: Record<string, unknown>;
    recordId?: string;
    [key: string]: unknown;
  }): Promise<unknown> => {
    if (!worldId) {
      throw new Error("当前会话缺少 world_id，无法读写世界存储。");
    }
    const collection = String(action.collection ?? "");
    if (action.type === "world-record-list") {
      return listWorldRecords(worldId, collection);
    }
    if (action.type === "world-record-create") {
      return createWorldRecord(worldId, {
        collection,
        data: action.data ?? {},
      });
    }
    if (action.type === "world-record-update" && action.recordId) {
      return updateWorldRecord(worldId, action.recordId, {
        collection,
        data: action.data ?? {},
      });
    }
    throw new Error(`Shell 存储桥不支持：${action.type}`);
  };
}

/**
 * Game shells (desktop/mobile) 的 DSL 动作。
 * 业务 handler 一律走世界包 logic.js；仅 ui.setTab 作为通用页签切换留在宿主。
 */
export async function runShellLogicAction(
  action: GameUiActionReference,
  logic: WorldLogicConfig | undefined,
  storage?: ShellStorageBridge,
): Promise<unknown> {
  const args = (action.args ?? {}) as Record<string, unknown>;
  const handler = readString(args.handler);
  const input = (args.input ?? args) as unknown;

  if (handler === "ui.setTab") {
    const raw = input && typeof input === "object" && "tab" in input
      ? readString((input as { tab?: unknown }).tab)
      : "chat";
    const allowed = ["chat", "records", "plan", "profile", "growth", "mine"];
    return allowed.includes(raw) ? raw : "chat";
  }

  const sendAction = storage?.worldId
    ? createShellStorageSend(storage.worldId)
    : async () => {
        throw new Error("世界存储不可用：未提供 world_id。");
      };

  if (!handler) {
    return undefined;
  }
  if (!logic || logic.runtime !== "sandbox-js-v1" || !logic.source.trim()) {
    return undefined;
  }
  return invokeWorldLogic(logic, handler, input, sendAction);
}
