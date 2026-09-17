import type { GameUiActionReference, WorldLogicConfig } from "../../data/gameUi";
import { invokeWorldLogic } from "../../worldFrame/WorldLogicRuntime";

function readString(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return "";
}

/**
 * Game shells (desktop/mobile) are not the world iframe and have no IPC bridge.
 * Pure compute handlers still run; storage-capable handlers need the frame path.
 */
export async function runShellLogicAction(
  action: GameUiActionReference,
  logic: WorldLogicConfig | undefined,
): Promise<unknown> {
  const args = (action.args ?? {}) as Record<string, unknown>;
  const handler = readString(args.handler);
  const input = (args.input ?? args) as unknown;

  if (handler === "ui.setTab") {
    const raw = input && typeof input === "object" && "tab" in input
      ? readString((input as { tab?: unknown }).tab)
      : "chat";
    const allowed = ["chat", "growth", "profile"];
    return allowed.includes(raw) ? raw : "chat";
  }

  if (!handler) {
    return undefined;
  }
  if (!logic || logic.runtime !== "sandbox-js-v1" || !logic.source.trim()) {
    return undefined;
  }
  return invokeWorldLogic(logic, handler, input, async () => undefined);
}
