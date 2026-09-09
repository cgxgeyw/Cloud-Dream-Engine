// Extracted from useGameSession.ts (pure helpers)
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useNavigate, useParams } from "react-router-dom";
import { showToast } from "../components/Toast";
import {
  assetUrl,
  branchSave,
  fetchCharacter,
  fetchSessionRuntimeAttributes,
  fetchSaves,
  fetchSession,
  fetchWorld,
  fetchWorlds,
  fetchWorldCharacters,
  isAndroidRuntime,
  isTauriEnvironment,
  onSessionSnapshot,
  requestWorldPermissions,
  retryFailedLlmStep,
  streamPlayerAction,
  switchPlayerCharacter,
  toSessionWebSocketUrl,
  type CharacterResponse,
  type ChatMessageResponse,
  type PlayerActionMode,
  type RetryFailedLlmStepRequest,
  type RuntimeAttributeGroup,
  type RuntimeAttributeItem,
  type SaveResponse,
  type SessionRuntimeAttributesResponse,
  type SessionMapEdge,
  type SessionMapNode,
  type SessionSnapshotResponse,
  type SwitchPlayerCharacterRequest,
  type WorldResponse,
  answerInteraction,
} from "../data/apiAdapter";
import type { ContentPart, MessageInteraction } from "../data/types";
import {
  buildGameUiStylesheet,
  createGameUiScopeSelector,
  normalizeGameUiScopeId,
  normalizeWorldUiEnvelope,
  parseGameUiDocument,
  resolveUiFile,
  resolveUiStylesheet,
  type WorldUiEnvelope,
} from "../data/gameUi";
import {
  type EditingTurnState,
  type RenderChatMessage,
  type SceneFocusMessage,
  type SideTab,
  type SubmitActionOptions,
  type SwitchProposalView,
  MESSAGE_KIND_RANK,
  buildWorldThemeStyle,
  copyTextToClipboard,
  formatActionErrorMessage,
  parseCharacterCreationMessage,
  persistSeenCharacterCreationKeys,
  readSeenCharacterCreationKeys,
  resolvePlayerMessageSpeaker,
  resolveRuntimeBackgroundAsset,
  resolveStatusTabs,
} from "./utils";

const SCHEDULE_NOTIFICATION_TOOL_ID = "mcp-tool-schedule-notification";


export function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") return content;
  return content
    .filter((p): p is { type: "text"; text: string } => p.type === "text")
    .map((p) => p.text)
    .join("");
}

export function worldAllowsScheduleNotification(world: WorldResponse | null): boolean {
  const toolIds = world?.director_config?.allowed_mcp_tool_ids;
  return Array.isArray(toolIds) && toolIds.some((id) => id === SCHEDULE_NOTIFICATION_TOOL_ID);
}

export function mayCreateNotificationFromInput(text: string, hasAudio: boolean): boolean {
  if (hasAudio) {
    return true;
  }
  return /提醒|通知|叫我|闹钟|定时|日程|安排|待办|稍后|明天|后天|今天|今晚|早上|中午|下午|晚上|分钟|小时|点|:[0-9]{2}|remind|notify|alarm|timer|schedule/i.test(text);
}

export function stringifyRuntimeAttributeValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value.trim();
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value
      .map((item) => stringifyRuntimeAttributeValue(item))
      .filter(Boolean)
      .join("\n");
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return "";
  }
}

export function buildAttributeSideTabsFromRuntimeAttributes(
  runtimeAttributes: SessionRuntimeAttributesResponse,
  inventoryItems: SessionSnapshotResponse["inventory_items"],
  sessionId: string,
  playerCharacterId: string,
): Array<[string, string]> {
  const grouped = new Map<string, string[]>();
  const playerGroups = runtimeAttributes.character_attributes.filter((group) =>
    group.owner_id === playerCharacterId
    || group.owner_id.endsWith(`:${playerCharacterId}`)
    || group.owner_id === `${sessionId}:${playerCharacterId}`
  );
  for (const owner of [...runtimeAttributes.session_attributes, ...playerGroups]) {
    const orderedItems = [...owner.items].sort((left, right) => {
      const leftOrder = typeof left.display_policy.order === "number" ? left.display_policy.order : 0;
      const rightOrder = typeof right.display_policy.order === "number" ? right.display_policy.order : 0;
      return leftOrder - rightOrder;
    });
    for (const item of orderedItems) {
      if (item.display_policy.hidden === true) continue;
      const value = stringifyRuntimeAttributeValue(item.value);
      if (!value) continue;
      const configuredGroup = typeof item.display_policy.group === "string"
        ? item.display_policy.group.trim()
        : "";
      const label = configuredGroup || owner.owner_label.trim();
      if (!label) continue;
      const lines = grouped.get(label) ?? [];
      lines.push(`${item.label || item.key}: ${value}`);
      grouped.set(label, lines);
    }
  }
  const inventoryLines = inventoryItems.map((item) => {
    const quantity = item.quantity > 1 ? ` x${item.quantity}` : "";
    const detail = item.description.trim() ? `\n  ${item.description.trim()}` : "";
    return `${item.name}${quantity}${detail}`;
  });
  grouped.set("背包", inventoryLines.length > 0 ? inventoryLines : ["暂无物品"]);
  return Array.from(grouped, ([label, lines]) => [label, lines.join("\n")]);
}

export function filterRuntimeAttributesForWorld(
  runtimeAttributes: SessionRuntimeAttributesResponse,
  world: WorldResponse | null,
): SessionRuntimeAttributesResponse {
  const rawSchemas = world?.ui_theme_config?.attribute_schemas;
  if (!Array.isArray(rawSchemas) || rawSchemas.length === 0) {
    return runtimeAttributes;
  }

  const declaredKeys = new Set(
    rawSchemas
      .filter((schema): schema is Record<string, unknown> => Boolean(schema) && typeof schema === "object")
      .map((schema) => (typeof schema.key === "string" ? schema.key.trim() : ""))
      .filter(Boolean),
  );
  if (declaredKeys.size === 0) {
    return runtimeAttributes;
  }

  const filterGroup = (group: RuntimeAttributeGroup): RuntimeAttributeGroup => ({
    ...group,
    items: group.items.filter((item) => {
      if (declaredKeys.has(item.key)) {
        return true;
      }
      const applicableWorldIds = item.display_policy.applicable_world_ids;
      return Array.isArray(applicableWorldIds)
        && applicableWorldIds.some((id) => id === world?.id);
    }),
  });

  return {
    session_attributes: runtimeAttributes.session_attributes.map(filterGroup).filter((group) => group.items.length > 0),
    character_attributes: runtimeAttributes.character_attributes.map(filterGroup).filter((group) => group.items.length > 0),
  };
}

export function normalizeSessionMapGraph(
  rawNodes: unknown,
  rawEdges: unknown,
): { nodes: SessionMapNode[]; edges: SessionMapEdge[] } {
  const nodes: SessionMapNode[] = [];
  const nodeIds = new Set<string>();
  if (Array.isArray(rawNodes)) {
    for (const rawNode of rawNodes) {
      if (!rawNode || typeof rawNode !== "object") {
        continue;
      }
      const value = rawNode as Record<string, unknown>;
      const nodeId = typeof value.node_id === "string" ? value.node_id.trim() : "";
      const label = typeof value.label === "string" ? value.label.trim() : "";
      if (!nodeId || !label || nodeIds.has(nodeId)) {
        continue;
      }
      nodeIds.add(nodeId);
      nodes.push({
        node_id: nodeId,
        label,
        discovered: value.discovered === true,
        current: value.current === true,
      });
    }
  }

  const edges: SessionMapEdge[] = [];
  const edgeIds = new Set<string>();
  if (Array.isArray(rawEdges)) {
    for (const rawEdge of rawEdges) {
      if (!rawEdge || typeof rawEdge !== "object") {
        continue;
      }
      const value = rawEdge as Record<string, unknown>;
      const edgeId = typeof value.edge_id === "string" ? value.edge_id.trim() : "";
      const sourceNodeId = typeof value.source_node_id === "string" ? value.source_node_id.trim() : "";
      const targetNodeId = typeof value.target_node_id === "string" ? value.target_node_id.trim() : "";
      if (
        !edgeId
        || !sourceNodeId
        || !targetNodeId
        || edgeIds.has(edgeId)
        || !nodeIds.has(sourceNodeId)
        || !nodeIds.has(targetNodeId)
      ) {
        continue;
      }
      edgeIds.add(edgeId);
      edges.push({ edge_id: edgeId, source_node_id: sourceNodeId, target_node_id: targetNodeId });
    }
  }

  return { nodes, edges };
}

export function findAttributeItemsForTab(
  runtimeAttributes: SessionRuntimeAttributesResponse,
  tabLabel: string,
  sessionId: string,
  playerCharacterId: string,
) {
  const playerGroups = runtimeAttributes.character_attributes.filter((group) =>
    group.owner_id === playerCharacterId
    || group.owner_id.endsWith(`:${playerCharacterId}`)
    || group.owner_id === `${sessionId}:${playerCharacterId}`
  );
  return [...runtimeAttributes.session_attributes, ...playerGroups]
    .flatMap((owner) => owner.items.filter((item) => {
      const configuredGroup = typeof item.display_policy.group === "string"
        ? item.display_policy.group.trim()
        : "";
      return (configuredGroup || owner.owner_label.trim()) === tabLabel && item.display_policy.hidden !== true;
    }))
    .sort((left, right) => {
      const leftOrder = typeof left.display_policy.order === "number" ? left.display_policy.order : 0;
      const rightOrder = typeof right.display_policy.order === "number" ? right.display_policy.order : 0;
      return leftOrder - rightOrder;
    });
}
