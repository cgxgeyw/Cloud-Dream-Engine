import type { GameUiPreviewProps } from "../components/GameUiPreview";
import type { SubmitActionOptions, SwitchProposalView } from "../game/utils";
import type { WorldFrameRuntimePayload } from "./runtimeSnapshot";

export const WORLD_FRAME_PROTOCOL_VERSION = 3 as const;

export type WorldFramePreviewPayload = Omit<GameUiPreviewProps, "rootStyle"> & {
  rootStyle?: Record<string, string | number>;
};

export type WorldFrameAction =
  | { type: "clear-action-error" }
  | { type: "set-draft-value"; value: string }
  | { type: "set-auto-scroll"; enabled: boolean }
  | { type: "submit-message"; options: SubmitActionOptions }
  | { type: "start-editing-turn"; content: string; turnIndex: number }
  | { type: "cancel-editing-turn" }
  | { type: "branch-from-current" }
  | { type: "retry-turn"; retryToken: string }
  | { type: "accept-switch-proposal"; proposal: SwitchProposalView }
  | { type: "dismiss-switch-proposal"; proposalKey: string }
  | { type: "dismiss-retry-card"; cardKey: string }
  | { type: "copy-text"; text: string }
  | { type: "switch-side-tab"; tabKey: string }
  | { type: "navigate"; target: "back" | "home" | "settings" | "debug" }
  | { type: "pick-image" }
  | { type: "remove-image"; index: number }
  | { type: "start-recording" }
  | { type: "stop-recording"; send?: boolean }
  | { type: "voice-mode"; enabled: boolean }
  | { type: "remove-audio"; index: number }
  | { type: "world-record-list"; collection: string }
  | { type: "world-record-create"; collection: string; data: Record<string, unknown> }
  | { type: "world-record-update"; collection: string; recordId: string; data: Record<string, unknown> }
  | { type: "world-record-delete"; collection: string; recordId: string }
  | { type: "world-kv-list"; namespace: string }
  | { type: "world-kv-get"; namespace: string; key: string }
  | { type: "world-kv-set"; namespace: string; key: string; value: unknown }
  | { type: "world-kv-delete"; namespace: string; key: string };

export type WorldFrameConnectMessage = {
  type: "world-frame/connect";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
};

export type WorldFrameBootMessage = {
  type: "world-frame/boot";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
};

export type WorldFrameRenderPreviewMessage = {
  type: "world-frame/render-preview";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
  revision: number;
  payload: WorldFramePreviewPayload;
};

export type WorldFrameRenderRuntimeMessage = {
  type: "world-frame/render-runtime";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
  revision: number;
  payload: WorldFrameRuntimePayload;
};

export type WorldFrameActionResultMessage = {
  type: "world-frame/action-result";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
  requestId: string;
  ok: boolean;
  error?: string;
  result?: unknown;
};

export type WorldFrameHostMessage =
  | WorldFrameRenderPreviewMessage
  | WorldFrameRenderRuntimeMessage
  | WorldFrameActionResultMessage;

export type WorldFrameReadyMessage = {
  type: "world-frame/ready";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
};

export type WorldFrameErrorMessage = {
  type: "world-frame/error";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
  message: string;
};

export type WorldFrameActionMessage = {
  type: "world-frame/action";
  protocolVersion: typeof WORLD_FRAME_PROTOCOL_VERSION;
  channelId: string;
  requestId: string;
  action: WorldFrameAction;
};

export type WorldFrameClientMessage =
  | WorldFrameReadyMessage
  | WorldFrameErrorMessage
  | WorldFrameActionMessage;

export function isWorldFrameConnectMessage(value: unknown): value is WorldFrameConnectMessage {
  return isProtocolRecord(value)
    && value.type === "world-frame/connect"
    && hasChannelId(value);
}

export function isWorldFrameBootMessage(value: unknown): value is WorldFrameBootMessage {
  return isProtocolRecord(value) && value.type === "world-frame/boot";
}

export function isWorldFrameHostMessage(value: unknown): value is WorldFrameHostMessage {
  if (!isProtocolRecord(value) || !hasChannelId(value)) {
    return false;
  }
  if (value.type === "world-frame/action-result") {
    return typeof value.requestId === "string"
      && typeof value.ok === "boolean"
      && (value.error === undefined || typeof value.error === "string");
  }
  return (value.type === "world-frame/render-preview" || value.type === "world-frame/render-runtime")
    && typeof value.revision === "number"
    && Number.isInteger(value.revision)
    && value.revision >= 0
    && isRecord(value.payload);
}

export function isWorldFrameClientMessage(value: unknown): value is WorldFrameClientMessage {
  if (!isProtocolRecord(value) || !hasChannelId(value)) {
    return false;
  }
  if (value.type === "world-frame/ready") {
    return true;
  }
  if (value.type === "world-frame/error") {
    return typeof value.message === "string";
  }
  return value.type === "world-frame/action"
    && typeof value.requestId === "string"
    && isWorldFrameAction(value.action);
}

function isWorldFrameAction(value: unknown): value is WorldFrameAction {
  if (!isRecord(value) || typeof value.type !== "string") {
    return false;
  }
  const known = [
    "clear-action-error",
    "set-draft-value",
    "set-auto-scroll",
    "submit-message",
    "start-editing-turn",
    "cancel-editing-turn",
    "branch-from-current",
    "retry-turn",
    "accept-switch-proposal",
    "dismiss-switch-proposal",
    "dismiss-retry-card",
    "copy-text",
    "switch-side-tab",
    "navigate",
    "pick-image",
    "remove-image",
    "start-recording",
    "stop-recording",
    "voice-mode",
    "remove-audio",
  ].includes(value.type);
  if (known) {
    return true;
  }
  if (value.type === "world-kv-list") {
    return isWorldStorageName(value.namespace);
  }
  if (value.type === "world-kv-get" || value.type === "world-kv-delete") {
    return isWorldStorageName(value.namespace) && isWorldStorageKey(value.key);
  }
  if (value.type === "world-kv-set") {
    return isWorldStorageName(value.namespace)
      && isWorldStorageKey(value.key)
      && Object.prototype.hasOwnProperty.call(value, "value");
  }
  if (!isWorldRecordCollection(value.collection)) {
    return false;
  }
  if (value.type === "world-record-list") {
    return true;
  }
  if (value.type === "world-record-create") {
    return isRecord(value.data);
  }
  if (value.type === "world-record-update") {
    return typeof value.recordId === "string" && value.recordId.length > 0 && isRecord(value.data);
  }
  return value.type === "world-record-delete"
    && typeof value.recordId === "string"
    && value.recordId.length > 0;
}

function isWorldRecordCollection(value: unknown): value is string {
  return isWorldStorageName(value);
}

function isWorldStorageName(value: unknown): value is string {
  return typeof value === "string"
    && /^[a-z0-9._-]{1,64}$/i.test(value.trim());
}

function isWorldStorageKey(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && value.trim().length <= 128
    && !/[\u0000-\u001f\u007f]/.test(value);
}

function isProtocolRecord(value: unknown): value is Record<string, unknown> {
  return isRecord(value) && value.protocolVersion === WORLD_FRAME_PROTOCOL_VERSION;
}

function hasChannelId(value: Record<string, unknown>): boolean {
  return typeof value.channelId === "string" && value.channelId.length > 0;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
