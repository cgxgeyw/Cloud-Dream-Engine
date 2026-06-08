import type { CSSProperties } from "react";
import {
  assetUrl,
  type ChatMessageResponse,
  type CharacterResponse,
  type PlayerActionMode,
  type RetryFailedLlmStepRequest,
  type SaveResponse,
  type SessionSnapshotResponse,
  type SwitchCharacterProposalRequest,
  type WorldResponse,
} from "../data/apiAdapter";
import type { ContentPart } from "../data/types";
import {
  resolveSidePanelTabOrder,
  type GameUiDocument,
  type GameUiLayoutNode,
  type UiAssetConfig,
} from "../data/gameUi";

function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") {
    return content;
  }
  return content
    .filter((part) => part.type === "text")
    .map((part) => (part as { type: "text"; text: string }).text)
    .join("\n");
}

/* ============================================================
   Shared Types
   ============================================================ */

export type SideTab = string;

export type ThemeVars = CSSProperties & Record<string, string>;

export type RenderChatMessage = ChatMessageResponse & { pending?: boolean };

export type EditingTurnState = {
  turnIndex: number;
  originalContent: string;
};

export type SubmitActionOptions = {
  content?: string;
  turnIndex?: number;
  mode?: PlayerActionMode;
};

export type CharacterCreationView = {
  key: string;
  characterName: string;
  characterRole: string;
  characterBackgroundPrompt: string;
  forSwitchCharacter: boolean;
};

export type SwitchProposalView = {
  key: string;
  targetCharacterName: string;
  reason: string;
  targetCharacterId?: string;
  targetRole: string;
  targetBackgroundPrompt: string;
  targetCreatedInTurn: boolean;
  proposal: SwitchCharacterProposalRequest;
};

export type DirectorTraceView = {
  key: string;
  traceText: string;
  traceLines: string[];
  reasoning: string;
  reasoningLines: string[];
  reasoningExpanded: boolean;
};

export type AgentReasoningView = {
  reasoning: string;
  reasoningLines: string[];
};

export type DirectorRetryCardView = {
  key: string;
  retryToken: string;
  title: string;
  summary: string;
  repairSummary: string;
  provider: string;
  modelId: string;
  failureStage: string;
};

export type StructuredErrorView = {
  key: string;
  retryToken?: string;
  title: string;
  summary: string;
  repairSummary: string;
  provider: string;
  modelId: string;
  failureStage: string;
  speakerName: string;
};

export type SceneFocusMessage = {
  speaker: string;
  content: string;
};

/* ============================================================
   Shared Constants
   ============================================================ */

export const MESSAGE_KIND_RANK: Record<string, number> = {
  player_action: 0,
  director_trace: 1,
  system_action: 2,
  llm_structured_error: 2,
  agent_response: 3,
  narration: 4,
};

export const VIEW_SWITCH_PATTERN = /^(.+?)的视角已启用/;

export const CHARACTER_CREATION_STORAGE_PREFIX = "game:seen-character-creations:";

/* ============================================================
   Shared Pure Functions
   ============================================================ */

export function readSeenCharacterCreationKeys(sessionId: string): Set<string> {
  if (typeof window === "undefined") {
    return new Set();
  }

  try {
    const raw = window.localStorage.getItem(`${CHARACTER_CREATION_STORAGE_PREFIX}${sessionId}`);
    if (!raw) {
      return new Set();
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    return new Set(parsed.map((item) => String(item).trim()).filter(Boolean));
  } catch {
    return new Set();
  }
}

export function persistSeenCharacterCreationKeys(sessionId: string, keys: Set<string>): void {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.setItem(
      `${CHARACTER_CREATION_STORAGE_PREFIX}${sessionId}`,
      JSON.stringify(Array.from(keys)),
    );
  } catch {
    // 本地持久化失败时保持静默，继续使用内存中的兜底状态。
  }
}

export function buildSwitchProposalKey(message: ChatMessageResponse): string {
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  return [
    "switch",
    String(metadata.target_character_name ?? ""),
    String(metadata.location ?? ""),
    String(metadata.scene_name ?? ""),
    getMessageText(message.content),
  ].join("::");
}

export function buildCharacterCreationKey(message: ChatMessageResponse): string {
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  const stableId =
    String(metadata.character_id ?? "").trim()
    || [
      String(metadata.character_name ?? "").trim(),
      String(metadata.turn_index ?? "").trim(),
    ].filter(Boolean).join("::")
    || getMessageText(message.content).trim();
  return [
    "character-created",
    stableId,
  ].join("::");
}

export function buildDirectorTraceKey(message: ChatMessageResponse): string {
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  return [
    "director-trace",
    String(metadata.trace_source ?? ""),
    String(metadata.turn_index ?? "streaming"),
    String(metadata.world_phase ?? ""),
    String(metadata.next_scene_name ?? ""),
    String(metadata.next_location ?? ""),
    String(metadata.next_time_label ?? ""),
  ].join("::");
}

export function buildDirectorRetryCardKey(message: ChatMessageResponse): string {
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  return [
    "director-retry",
    String(metadata.retry_token ?? ""),
    String(metadata.failure_stage ?? ""),
    String(metadata.turn_index ?? ""),
  ].join("::");
}

export function buildStructuredErrorKey(message: ChatMessageResponse): string {
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  return [
    "structured-error",
    String(metadata.retry_token ?? ""),
    String(metadata.failure_stage ?? ""),
    String(metadata.speaker_name ?? ""),
    String(metadata.turn_index ?? ""),
  ].join("::");
}

export function parseCharacterCreationMessage(message: ChatMessageResponse): CharacterCreationView | null {
  if (message.role !== "system") {
    return null;
  }

  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  if (metadata.action_type !== "character_created") {
    return null;
  }

  const characterName = String(metadata.character_name ?? "").trim();
  if (!characterName) {
    return null;
  }

  return {
    key: buildCharacterCreationKey(message),
    characterName,
    characterRole: String(metadata.character_role ?? "").trim(),
    characterBackgroundPrompt: String(metadata.character_background_prompt ?? "").trim(),
    forSwitchCharacter: Boolean(metadata.for_switch_character),
  };
}

export function parseSwitchProposal(message: ChatMessageResponse): SwitchProposalView | null {
  if (message.role !== "system") {
    return null;
  }

  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  if (metadata.action_type !== "switch_character") {
    return null;
  }

  const targetCharacterName = String(metadata.target_character_name ?? "").trim();
  if (!targetCharacterName) {
    return null;
  }

  const sceneTags = Array.isArray(metadata.scene_tags)
    ? metadata.scene_tags.map((item) => String(item).trim()).filter(Boolean)
    : [];
  const visibleCharacters = Array.isArray(metadata.visible_characters)
    ? metadata.visible_characters.map((item) => String(item).trim()).filter(Boolean)
    : [];

  return {
    key: buildSwitchProposalKey(message),
    targetCharacterName,
    targetCharacterId: String(metadata.target_character_id ?? "").trim() || undefined,
    targetRole: String(metadata.target_role ?? "").trim(),
    targetBackgroundPrompt: String(metadata.target_background_prompt ?? "").trim(),
    targetCreatedInTurn: Boolean(metadata.target_created_in_turn),
    reason: getMessageText(message.content).trim() || `建议切换至：${targetCharacterName}`,
    proposal: {
      target_character_name: targetCharacterName,
      reason: getMessageText(message.content).trim() || undefined,
      location: String(metadata.location ?? "").trim() || undefined,
      scene_name: String(metadata.scene_name ?? "").trim() || undefined,
      scene_background_hint: String(metadata.scene_background_hint ?? "").trim() || undefined,
      scene_tags: sceneTags,
      visible_characters: visibleCharacters,
    },
  };
}

export function parseDirectorRetryCard(message: ChatMessageResponse): DirectorRetryCardView | null {
  if (message.role !== "system") {
    return null;
  }
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  if (metadata.action_type !== "director_retry_required") {
    return null;
  }
  const retryToken = String(metadata.retry_token ?? "").trim();
  if (!retryToken) {
    return null;
  }
  return {
    key: buildDirectorRetryCardKey(message),
    retryToken,
    title: String(metadata.title ?? "世界主控回复异常").trim() || "世界主控回复异常",
    summary: String(metadata.summary ?? "").trim() || getMessageText(message.content).trim(),
    repairSummary: String(metadata.repair_summary ?? "").trim(),
    provider: String(metadata.provider ?? "").trim(),
    modelId: String(metadata.model_id ?? "").trim(),
    failureStage: String(metadata.failure_stage ?? "").trim(),
  };
}

export function parseStructuredError(message: ChatMessageResponse): StructuredErrorView | null {
  if (message.role !== "system") {
    return null;
  }
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  if (metadata.action_type !== "structured_output_error") {
    return null;
  }
  return {
    key: buildStructuredErrorKey(message),
    retryToken: String(metadata.retry_token ?? "").trim() || undefined,
    title: String(metadata.title ?? "结构化输出异常").trim() || "结构化输出异常",
    summary: String(metadata.summary ?? "").trim() || getMessageText(message.content).trim(),
    repairSummary: String(metadata.repair_summary ?? "").trim(),
    provider: String(metadata.provider ?? "").trim(),
    modelId: String(metadata.model_id ?? "").trim(),
    failureStage: String(metadata.failure_stage ?? "").trim(),
    speakerName: String(metadata.speaker_name ?? "").trim(),
  };
}

export function resolvePlayerMessageSpeaker(
  messages: ChatMessageResponse[],
  index: number,
  currentPlayerName?: string | null,
): string {
  const explicitSpeaker = messages[index]?.speaker?.trim();
  if (explicitSpeaker) {
    return explicitSpeaker;
  }

  const hasSwitchMarker = messages.some(
    (message) => message.role === "system" && VIEW_SWITCH_PATTERN.test(getMessageText(message.content).trim()),
  );
  let resolvedSpeaker = hasSwitchMarker ? "玩家" : (currentPlayerName?.trim() || "玩家");

  for (let cursor = 0; cursor <= index; cursor += 1) {
    const message = messages[cursor];
    if (message.role !== "system") {
      continue;
    }
    const match = getMessageText(message.content).trim().match(VIEW_SWITCH_PATTERN);
    if (match?.[1]?.trim()) {
      resolvedSpeaker = match[1].trim();
    }
  }

  return resolvedSpeaker;
}

export function buildWorldThemeStyle(): ThemeVars {
  return {
    "--game-local-bg-image": "none",
  };
}

export function resolveRuntimeBackgroundAsset(
  assetsConfig: UiAssetConfig,
  world: WorldResponse | null,
  session: SessionSnapshotResponse | null,
): string {
  const sessionBackgroundAsset = session?.assets?.background_asset_path?.trim();
  if (sessionBackgroundAsset) {
    return sessionBackgroundAsset;
  }

  const sceneNames = [
    session?.scene?.name,
    session?.location,
    world?.opening_scene,
  ]
    .map((item) => String(item ?? "").trim())
    .filter(Boolean);

  for (const sceneName of sceneNames) {
    const sceneAssets = assetsConfig.local_scene_backgrounds[sceneName] ?? [];
    if (sceneAssets.length > 0) {
      return sceneAssets[0];
    }
  }

  return assetsConfig.local_background_assets[0] ?? "";
}

export function formatActionErrorMessage(error: string): string {
  if (error.includes("未配置文本模型")) {
    return "未配置 LLM。请先到「设置」里添加文本模型，然后再回来发言。";
  }
  if (error.includes("缺少调用地址")) {
    return "当前文本模型缺少调用地址，请先到「设置」里补全文本模型配置。";
  }
  if (/^Request failed:\s*400\b/i.test(error)) {
    return "请求被后端拒绝，可以修改内容后重发。";
  }
  if (/^Request failed:/i.test(error)) {
    return "请求失败，请稍后重试。";
  }
  return error || "发言失败";
}

export function parseDirectorTrace(message: ChatMessageResponse): DirectorTraceView | null {
  if (message.role !== "system") {
    return null;
  }
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  if (metadata.action_type !== "director_trace") {
    return null;
  }
  const traceLines = Array.isArray(metadata.trace_lines)
    ? metadata.trace_lines.map((item) => String(item).trim()).filter(Boolean)
    : [];
  const traceText = String(metadata.trace_text ?? getMessageText(message.content) ?? "").trim();
  const reasoning = String(metadata.reasoning ?? "").trim();
  if (!traceText && traceLines.length === 0) {
    return null;
  }
  return {
    key: buildDirectorTraceKey(message),
    traceText,
    traceLines,
    reasoning,
    reasoningLines: reasoning.split("\n").map((line) => line.trimEnd()).filter((line) => line.trim().length > 0),
    reasoningExpanded: Boolean(metadata.reasoning_expanded),
  };
}

export function parseAgentReasoning(message: ChatMessageResponse): AgentReasoningView | null {
  if (message.role !== "agent") {
    return null;
  }
  const metadata = (message.metadata ?? {}) as Record<string, unknown>;
  const reasoning = String(metadata.reasoning ?? "").trim();
  if (!reasoning) {
    return null;
  }
  return {
    reasoning,
    reasoningLines: reasoning.split("\n").map((line) => line.trimEnd()).filter((line) => line.trim().length > 0),
  };
}

export function shouldHidePinnedNarrationMessage(
  message: Pick<RenderChatMessage, "role" | "content" | "metadata">,
  latestNarration: string,
): boolean {
  return Boolean(
    latestNarration &&
    message.role === "system" &&
    message.metadata?.action_type == null &&
    getMessageText(message.content).trim(),
  );
}

export function isMessageReasoningExpanded(message: ChatMessageResponse): boolean {
  return Boolean((message.metadata ?? {}).reasoning_expanded);
}

export async function copyTextToClipboard(text: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  if (typeof document === "undefined") {
    throw new Error("clipboard unavailable");
  }

  const textArea = document.createElement("textarea");
  textArea.value = text;
  textArea.style.position = "fixed";
  textArea.style.opacity = "0";
  textArea.style.left = "-9999px";
  document.body.appendChild(textArea);
  textArea.focus();
  textArea.select();
  const copied = document.execCommand("copy");
  document.body.removeChild(textArea);
  if (!copied) {
    throw new Error("copy failed");
  }
}

export function resolveDialogueSpeakerLabel(
  message: Pick<RenderChatMessage, "role" | "speaker">,
  _worldCharacters?: CharacterResponse[],
): string {
  if (message.role === "player") {
    return message.speaker?.trim() || "玩家";
  }
  if (message.role === "agent") {
    return message.speaker?.trim() || "角色";
  }
  return message.speaker?.trim() || "系统";
}

function nodeDisablesMapSideTab(node: GameUiLayoutNode | undefined): boolean {
  if (!node || typeof node !== "object") {
    return false;
  }

  if (
    node.type === "component" &&
    node.component === "side_panel_tabs" &&
    node.props?.show_map_tab === false
  ) {
    return true;
  }

  if ("children" in node && Array.isArray(node.children)) {
    if (node.children.some((child) => nodeDisablesMapSideTab(child))) {
      return true;
    }
  }

  if (node.type === "component" && node.slots) {
    for (const slot of Object.values(node.slots)) {
      const slotNodes = Array.isArray(slot) ? slot : [slot];
      if (slotNodes.some((child) => nodeDisablesMapSideTab(child))) {
        return true;
      }
    }
  }

  if ("child" in node && nodeDisablesMapSideTab(node.child)) {
    return true;
  }

  if ("empty" in node && nodeDisablesMapSideTab(node.empty)) {
    return true;
  }

  return false;
}

function shouldOfferMapSideTab(document: GameUiDocument, mapCount: number): boolean {
  if (mapCount <= 0) {
    return false;
  }
  return !nodeDisablesMapSideTab(document.layout.root);
}

export function resolveStatusTabs(
  document: GameUiDocument,
  mapCount: number,
  attributeTabs: Array<[string, string]>,
): Array<{ key: string; label: string }> {
  const available = new Map<string, { key: string; label: string }>();
  if (shouldOfferMapSideTab(document, mapCount)) {
    available.set("map", { key: "map", label: "地图" });
  }
  for (const [key] of attributeTabs) {
    available.set(`attribute:${key}`, { key: `attribute:${key}`, label: key });
  }
  const availableTabs = Array.from(available.values());
  const orderedTabs = resolveSidePanelTabOrder(document, availableTabs);
  const orderedKeys = new Set(orderedTabs.map((tab) => tab.key));
  return [
    ...orderedTabs,
    ...availableTabs.filter((tab) => !orderedKeys.has(tab.key)),
  ];
}
