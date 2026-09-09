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

function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") return content;
  return content
    .filter((p): p is { type: "text"; text: string } => p.type === "text")
    .map((p) => p.text)
    .join("");
}

function worldAllowsScheduleNotification(world: WorldResponse | null): boolean {
  const toolIds = world?.director_config?.allowed_mcp_tool_ids;
  return Array.isArray(toolIds) && toolIds.some((id) => id === SCHEDULE_NOTIFICATION_TOOL_ID);
}

function mayCreateNotificationFromInput(text: string, hasAudio: boolean): boolean {
  if (hasAudio) {
    return true;
  }
  return /提醒|通知|叫我|闹钟|定时|日程|安排|待办|稍后|明天|后天|今天|今晚|早上|中午|下午|晚上|分钟|小时|点|:[0-9]{2}|remind|notify|alarm|timer|schedule/i.test(text);
}

function stringifyRuntimeAttributeValue(value: unknown): string {
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

function buildAttributeSideTabsFromRuntimeAttributes(
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

function filterRuntimeAttributesForWorld(
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

function normalizeSessionMapGraph(
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

function findAttributeItemsForTab(
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

export interface GameSessionStateBag {
  session: SessionSnapshotResponse | null;
  sessionId: string;
  loading: boolean;
  error: string;

  themeWorld: WorldResponse | null;
  playerCharacter: CharacterResponse | null;
  worldCharacters: CharacterResponse[];
  currentSave: SaveResponse | null;

  messages: RenderChatMessage[];

  inputValue: string;
  setInputValue: React.Dispatch<React.SetStateAction<string>>;
  inputImages: File[];
  setInputImages: React.Dispatch<React.SetStateAction<File[]>>;
  inputAudios: File[];
  setInputAudios: React.Dispatch<React.SetStateAction<File[]>>;
  chatAutoScrollEnabled: boolean;
  setChatAutoScrollEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  clearActionError: () => void;
  submitting: boolean;
  streamingResponseActive: boolean;
  branching: boolean;
  actionError: string | null;

  editingTurn: EditingTurnState | null;
  startEditingTurn: (content: string, turnIndex: number) => void;
  cancelEditingTurn: () => void;

  sideTab: SideTab;
  setSideTab: React.Dispatch<React.SetStateAction<SideTab>>;

  switching: boolean;
  dismissedProposalKeys: Set<string>;
  dismissSwitchProposal: (key: string) => void;
  handleAcceptSwitchProposal: (proposal: SwitchProposalView) => Promise<void>;

  expandedDirectorTraceKeys: Set<string>;
  setExpandedDirectorTraceKeys: React.Dispatch<React.SetStateAction<Set<string>>>;

  activeCharacterCreationKeys: string[];

  retryingToken: string | null;
  dismissedRetryCardKeys: Set<string>;
  dismissDirectorRetryCard: (key: string) => void;
  handleRetryFailedStep: (request: RetryFailedLlmStepRequest) => Promise<void>;

  handleBranch: () => Promise<void>;
  handleSubmitAction: (options: SubmitActionOptions) => Promise<void>;
  /** 回答消息交互；newlyAnswered=false 表示重复提交（幂等返回首次结果） */
  handleAnswerInteraction: (
    messageId: string,
    interactionId: string,
    answer: unknown,
  ) => Promise<{ answer: unknown; newlyAnswered: boolean; interaction: MessageInteraction | null } | null>;
  /** 最近一次完成回合的信号（世界包事件 turn_completed 的触发源） */
  lastCompletedTurn: { sessionId: string; turnIndex: number; seq: number } | null;

  optimisticPlayerMessage: RenderChatMessage | null;

  worldUiEnvelope: WorldUiEnvelope;
  themeStyle: Record<string, string>;
  gameUiScopeId: string;
  parsedGameUi: ReturnType<typeof parseGameUiDocument>;
  runtimeBackgroundAsset: string;
  runtimeBackgroundStyle: React.CSSProperties & Record<string, string>;
  themeCustomCss: string;
  worldUiRuntimeVersion: 2 | 3;
  mapGraphNodes: SessionMapNode[];
  mapGraphEdges: SessionMapEdge[];
  runtimeAttributes: SessionRuntimeAttributesResponse;
  attributeSideTabs: Array<[string, string]>;
  worldCharacterNameSet: Set<string>;
  sideTabs: Array<{ key: string; label: string }>;
  activeAttributeTab: string;
  activeAttributeContent: string;
  activeAttributeItems: RuntimeAttributeItem[];
  latestNarration: string;
  dialogueMessages: ChatMessageResponse[];
  renderedDialogueMessages: RenderChatMessage[];
  copyableDialogueText: string;

  latestSceneFocus: SceneFocusMessage | null;
  sceneFocusSpeaker: string;
  sceneFocusContent: string;
  activePortraitPath: string;
  showSceneFocus: boolean;
  showSceneCharacters: boolean;

  chatMessagesRef: React.RefObject<HTMLDivElement | null>;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;

  handleCopyDialogue: () => Promise<void>;
  handleCopyMessage: (text: string) => Promise<void>;
}

export interface UseGameSessionOptions {
  isMobile?: boolean;
}

export function useGameSession(
  options: UseGameSessionOptions = {},
): GameSessionStateBag {
  const { isMobile = false } = options;
  const navigate = useNavigate();
  const { sessionId: sessionIdParam } = useParams<{ sessionId: string }>();

  const [session, setSession] = useState<SessionSnapshotResponse | null>(null);
  // 地图只跟随已提交的会话快照更新。流式帧只增长聊天消息，避免地图在
  // AI 回复期间被空快照或中间快照触发重排/卸载。
  const [committedMapGraph, setCommittedMapGraph] = useState<{
    nodes: SessionMapNode[];
    edges: SessionMapEdge[];
  }>({ nodes: [], edges: [] });
  // 流式帧只携带正在增长的消息。把它与完整会话快照拆开，避免每个 token 都
  // 刷新地图、属性、场景等本应在回合完成后才更新的区域。
  const [streamingMessages, setStreamingMessages] = useState<ChatMessageResponse[] | null>(null);
  const [themeWorld, setThemeWorld] = useState<WorldResponse | null>(null);
  const [playerCharacter, setPlayerCharacter] = useState<CharacterResponse | null>(null);
  const [worldCharacters, setWorldCharacters] = useState<CharacterResponse[]>([]);
  const [runtimeAttributes, setRuntimeAttributes] = useState<SessionRuntimeAttributesResponse>({
    session_attributes: [],
    character_attributes: [],
  });
  const [runtimeAttributesRevision, setRuntimeAttributesRevision] = useState(0);
  const [currentSave, setCurrentSave] = useState<SaveResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [inputImages, setInputImages] = useState<File[]>([]);
  const [inputAudios, setInputAudios] = useState<File[]>([]);
  const [chatAutoScrollEnabled, setChatAutoScrollEnabled] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [streamingResponseActive, setStreamingResponseActive] = useState(false);
  const [branching, setBranching] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [editingTurn, setEditingTurn] = useState<EditingTurnState | null>(null);
  // 回合完成信号：每次成功提交（含重发/编辑/重新生成）seq 自增，
  // 供世界包事件（turn_completed）在沙箱层消费。
  const [lastCompletedTurn, setLastCompletedTurn] = useState<{
    sessionId: string;
    turnIndex: number;
    seq: number;
  } | null>(null);
  const [sideTab, setSideTab] = useState<SideTab>("map");
  const [switching, setSwitching] = useState(false);
  const [retryingToken, setRetryingToken] = useState<string | null>(null);
  const [optimisticPlayerMessage, setOptimisticPlayerMessage] =
    useState<RenderChatMessage | null>(null);
  const [dismissedProposalKeys, setDismissedProposalKeys] = useState<Set<string>>(
    new Set(),
  );
  const [dismissedRetryCardKeys, setDismissedRetryCardKeys] = useState<Set<string>>(
    new Set(),
  );
  const [expandedDirectorTraceKeys, setExpandedDirectorTraceKeys] = useState<
    Set<string>
  >(new Set());

  const applySessionSnapshot = useCallback((snapshot: SessionSnapshotResponse, options?: {
    streaming?: boolean;
  }) => {
    if (options?.streaming) {
      setStreamingMessages(snapshot.messages ?? []);
      return;
    }

    setStreamingMessages(null);
    setSession(snapshot);
    setCommittedMapGraph((previous) => {
      const normalized = normalizeSessionMapGraph(snapshot.map_graph_nodes, snapshot.map_graph_edges);
      // Some completion/error responses are session overlays and omit the
      // topology. Never let such a response erase a map that was already
      // committed for this session; a real topology update always carries
      // the node list (including an intentionally empty initial map).
      if (normalized.nodes.length === 0 && previous.nodes.length > 0) {
        return previous;
      }
      return normalized;
    });
    setRuntimeAttributesRevision((revision) => revision + 1);
  }, []);
  const [activeCharacterCreationKeys, setActiveCharacterCreationKeys] =
    useState<string[]>([]);

  const chatMessagesRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const previousMapCountRef = useRef(0);
  const submitInFlightRef = useRef(false);
  const shouldAutoScrollRef = useRef(true);
  const seenCharacterCreationsRef = useRef<Set<string>>(new Set());
  // 记录已为“某组缺失角色名”触发过的刷新签名，避免角色名始终匹配不上时
  // 反复 fetchWorldCharacters 形成请求风暴。
  const attemptedMissingCharacterFetchRef = useRef<string>("");
  const runtimeAttributeRefreshTimersRef = useRef<number[]>([]);
  const runtimeAttributeSessionIdRef = useRef("");

  const sessionId = sessionIdParam ?? "";
  const worldId = themeWorld?.id ?? "";
  const playerCharacterId = session?.player_character_id ?? "";
  const gameUiScopeId = normalizeGameUiScopeId(useId());
  const gameUiScopeSelector = useMemo(
    () => createGameUiScopeSelector(gameUiScopeId),
    [gameUiScopeId],
  );

  useEffect(() => {
    runtimeAttributeSessionIdRef.current = sessionId;
  }, [sessionId]);

  const clearRuntimeAttributeRefreshTimers = useCallback(() => {
    runtimeAttributeRefreshTimersRef.current.forEach((timer) => {
      window.clearTimeout(timer);
    });
    runtimeAttributeRefreshTimersRef.current = [];
  }, []);

  const refreshRuntimeAttributes = useCallback(async () => {
    if (!sessionId) {
      setRuntimeAttributes({ session_attributes: [], character_attributes: [] });
      return;
    }

    try {
      const data = await fetchSessionRuntimeAttributes(sessionId);
      if (runtimeAttributeSessionIdRef.current !== sessionId) {
        return;
      }
      setRuntimeAttributes(data);
    } catch {
      if (runtimeAttributeSessionIdRef.current !== sessionId) {
        return;
      }
      setRuntimeAttributes({ session_attributes: [], character_attributes: [] });
    }
  }, [sessionId]);

  const scheduleRuntimeAttributeRefresh = useCallback(() => {
    clearRuntimeAttributeRefreshTimers();
    void refreshRuntimeAttributes();
    runtimeAttributeRefreshTimersRef.current = [250, 1000, 2000].map((delay) =>
      window.setTimeout(() => void refreshRuntimeAttributes(), delay),
    );
  }, [clearRuntimeAttributeRefreshTimers, refreshRuntimeAttributes]);

  useEffect(
    () => () => {
      clearRuntimeAttributeRefreshTimers();
    },
    [clearRuntimeAttributeRefreshTimers],
  );

  useEffect(() => {
    clearRuntimeAttributeRefreshTimers();
    setSession(null);
    setStreamingMessages(null);
    setCommittedMapGraph({ nodes: [], edges: [] });
    setThemeWorld(null);
    setPlayerCharacter(null);
    setWorldCharacters([]);
    setRuntimeAttributes({ session_attributes: [], character_attributes: [] });
    setCurrentSave(null);
    setLoading(true);
    setError(null);
    setInputValue("");
    setSubmitting(false);
    setBranching(false);
    setActionError(null);
    setEditingTurn(null);
    setSideTab("map");
    previousMapCountRef.current = 0;
    setSwitching(false);
    setRetryingToken(null);
    setOptimisticPlayerMessage(null);
    setDismissedProposalKeys(new Set());
    setDismissedRetryCardKeys(new Set());
    setExpandedDirectorTraceKeys(new Set());
    setActiveCharacterCreationKeys([]);
    shouldAutoScrollRef.current = true;
    seenCharacterCreationsRef.current = readSeenCharacterCreationKeys(sessionId);
  }, [applySessionSnapshot, clearRuntimeAttributeRefreshTimers, sessionId]);

  useEffect(() => {
    if (!editingTurn || !inputRef.current) {
      return;
    }

    inputRef.current.focus();
    const length = inputRef.current.value.length;
    inputRef.current.setSelectionRange(length, length);
    inputRef.current.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [editingTurn]);

  useEffect(() => {
    if (!sessionId) {
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);

    let cancelled = false;
    let websocket: WebSocket | null = null;
    let hasLoadedSession = false;

    async function loadSession() {
      try {
        const data = await fetchSession(sessionId);
        if (cancelled) {
          return;
        }

        hasLoadedSession = true;
        applySessionSnapshot(data);
        setLoading(false);

        if (isTauriEnvironment()) {
          return;
        }

        websocket = new WebSocket(toSessionWebSocketUrl(sessionId));
        websocket.onmessage = (event) => {
          if (cancelled) {
            return;
          }

          let payload: {
            type: string;
            payload?: SessionSnapshotResponse;
            detail?: string;
          };
          try {
            payload = JSON.parse(event.data);
          } catch {
            // 服务端推送非 JSON 或半截/畸形帧时跳过该帧，避免未捕获异常导致
            // onmessage 中断、后续帧无法处理。
            console.warn("[session] 忽略无法解析的会话流帧");
            return;
          }

          if (
            payload.type === "session.snapshot"
            && payload.payload
            && !submitInFlightRef.current
          ) {
            hasLoadedSession = true;
            applySessionSnapshot(payload.payload);
          }

          if (payload.type === "error") {
            const detail = payload.detail ?? "会话流发生错误";
            if (!hasLoadedSession) {
              setError(detail);
            } else {
              setActionError(formatActionErrorMessage(detail));
            }
          }
        };
      } catch (loadError) {
        if (cancelled) {
          return;
        }
        const msg = loadError instanceof Error ? loadError.message : String(loadError ?? "加载会话失败");
        console.error("[loadSession] 失败:", loadError);
        setError(msg);
        setLoading(false);
      }
    }

    void loadSession();

    return () => {
      cancelled = true;
      websocket?.close();
    };
  }, [applySessionSnapshot, sessionId]);

  useEffect(() => {
    if (!sessionId || !isTauriEnvironment()) {
      return;
    }

    let cancelled = false;
    let unsubscribe: (() => void) | null = null;

    void onSessionSnapshot(sessionId, (snapshot) => {
      if (!cancelled && !submitInFlightRef.current) {
        applySessionSnapshot(snapshot);
      }
    })
      .then((nextUnsubscribe) => {
        if (cancelled) {
          nextUnsubscribe();
        } else {
          unsubscribe = nextUnsubscribe;
        }
      })
      .catch((listenError) => {
        console.warn("[useGameSession] session snapshot listener failed:", listenError);
      });

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [applySessionSnapshot, sessionId]);

  useEffect(() => {
    if (!session?.world_name) {
      setThemeWorld(null);
      return;
    }

    const worldName = session.world_name;
    const playerCharacterId = session.player_character_id;
    let cancelled = false;

    async function loadWorld() {
      try {
        const sessionPlayer = playerCharacterId
          ? await fetchCharacter(playerCharacterId).catch(() => null)
          : null;
        if (cancelled) {
          return;
        }
        // 常规路径：从玩家角色拿 world_id，只拉单个世界，避免全量列表。
        if (sessionPlayer?.world_id) {
          const world = await fetchWorld(sessionPlayer.world_id).catch(() => null);
          if (!cancelled) {
            setThemeWorld(world);
          }
          return;
        }
        // 兜底：没有玩家角色时按世界名在列表中匹配。
        const worlds = await fetchWorlds();
        if (!cancelled) {
          setThemeWorld(worlds.find((world) => world.name === worldName) ?? null);
        }
      } catch {
        if (!cancelled) {
          setThemeWorld(null);
        }
      }
    }

    void loadWorld();

    return () => {
      cancelled = true;
    };
  }, [session?.player_character_id, session?.world_name]);

  useEffect(() => {
    if (!playerCharacterId) {
      setPlayerCharacter(null);
      return;
    }

    let cancelled = false;

    async function loadPlayerCharacter() {
      try {
        const data = await fetchCharacter(playerCharacterId);
        if (!cancelled) {
          setPlayerCharacter(data);
        }
      } catch {
        if (!cancelled) {
          setPlayerCharacter(null);
        }
      }
    }

    void loadPlayerCharacter();

    return () => {
      cancelled = true;
    };
  }, [playerCharacterId]);

  useEffect(() => {
    if (!worldId) {
      setWorldCharacters([]);
      return;
    }

    let cancelled = false;

    async function loadWorldCharacters() {
      try {
        const data = await fetchWorldCharacters(worldId);
        if (!cancelled) {
          setWorldCharacters(data);
        }
      } catch {
        if (!cancelled) {
          setWorldCharacters([]);
        }
      }
    }

    void loadWorldCharacters();

    return () => {
      cancelled = true;
    };
  }, [worldId]);

  useEffect(() => {
    if (!sessionId) {
      setRuntimeAttributes({ session_attributes: [], character_attributes: [] });
      return;
    }

    scheduleRuntimeAttributeRefresh();
  }, [runtimeAttributesRevision, scheduleRuntimeAttributeRefresh, sessionId]);

  useEffect(() => {
    if (!themeWorld?.id || !session?.messages?.length) {
      return;
    }

    const stableWorldId = themeWorld.id;
    const knownCharacterNames = new Set(
      worldCharacters.map((character) => character.name.trim()).filter(Boolean),
    );
    const missingNames = session.messages
      .map((message) => parseCharacterCreationMessage(message))
      .filter((creation): creation is NonNullable<typeof creation> => !!creation)
      .map((creation) => creation.characterName.trim())
      .filter((name) => name && !knownCharacterNames.has(name));

    if (missingNames.length === 0) {
      return;
    }

    // 同一组缺失角色名只刷新一次：若后端始终返回不含这些名字的列表，
    // 不再因 setWorldCharacters 产生新引用而反复触发本 effect 形成请求风暴。
    const signature = Array.from(new Set(missingNames)).sort().join("|");
    if (attemptedMissingCharacterFetchRef.current === signature) {
      return;
    }
    attemptedMissingCharacterFetchRef.current = signature;

    let cancelled = false;

    async function reloadWorldCharacters() {
      try {
        const data = await fetchWorldCharacters(stableWorldId);
        if (!cancelled) {
          setWorldCharacters(data);
        }
      } catch {
        // Ignore refresh failures and keep the existing in-memory character list.
      }
    }

    void reloadWorldCharacters();

    return () => {
      cancelled = true;
    };
  }, [session?.messages, themeWorld?.id, worldCharacters]);

  useEffect(() => {
    if (!session?.id) {
      setCurrentSave(null);
      return;
    }

    const currentSessionId = session.id;
    let cancelled = false;

    async function loadCurrentSave() {
      try {
        const saves = await fetchSaves();
        if (!cancelled) {
          setCurrentSave(
            saves.find((item) => item.session_id === currentSessionId) ?? null,
          );
        }
      } catch {
        if (!cancelled) {
          setCurrentSave(null);
        }
      }
    }

    void loadCurrentSave();

    return () => {
      cancelled = true;
    };
  }, [session?.id]);

  const activeSessionMessages = streamingMessages ?? session?.messages ?? [];
  const messages = useMemo<RenderChatMessage[]>(
    () =>
      activeSessionMessages.map((message) => ({
        ...message,
        pending: false,
      })),
    [activeSessionMessages],
  );

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    let needsUpdate = false;
    const nextKeys = new Set(activeCharacterCreationKeys);
    const seenKeys = seenCharacterCreationsRef.current;

    for (const message of session?.messages ?? []) {
      const creation = parseCharacterCreationMessage(message);
      if (!creation || seenKeys.has(creation.key)) {
        continue;
      }

      seenKeys.add(creation.key);
      nextKeys.add(creation.key);
      needsUpdate = true;
    }

    if (needsUpdate) {
      setActiveCharacterCreationKeys(Array.from(nextKeys));
      persistSeenCharacterCreationKeys(sessionId, seenKeys);
    }
  }, [activeCharacterCreationKeys, session?.messages, sessionId]);

  const worldUiEnvelope = useMemo(
    () => normalizeWorldUiEnvelope(themeWorld?.ui_theme_config),
    [themeWorld?.ui_theme_config],
  );

  const themeStyle = useMemo<Record<string, string>>(
    () => ({ ...buildWorldThemeStyle() }),
    [],
  );
  const parsedGameUi = useMemo(
    () =>
      parseGameUiDocument(
        resolveUiFile(worldUiEnvelope, isMobile ? "mobile" : "desktop"),
        isMobile ? "mobile" : "desktop",
      ),
    [isMobile, worldUiEnvelope],
  );

  const runtimeBackgroundAsset = useMemo(
    () => resolveRuntimeBackgroundAsset(worldUiEnvelope.assets, themeWorld, session),
    [session, themeWorld, worldUiEnvelope],
  );

  const runtimeBackgroundStyle = useMemo<
    React.CSSProperties & Record<string, string>
  >(() => {
    if (!runtimeBackgroundAsset) {
      return themeStyle as React.CSSProperties & Record<string, string>;
    }

    return {
      ...themeStyle,
      "--game-runtime-bg-image": `url("${assetUrl(runtimeBackgroundAsset)}")`,
    } as React.CSSProperties & Record<string, string>;
  }, [runtimeBackgroundAsset, themeStyle]);

  const themeCustomCss = useMemo(
    () => {
      const generatedStylesheet = buildGameUiStylesheet(
        parsedGameUi.document,
        runtimeBackgroundAsset ? assetUrl(runtimeBackgroundAsset) : undefined,
        gameUiScopeSelector,
      );
      const worldStylesheet = resolveUiStylesheet(
        worldUiEnvelope,
        isMobile ? "mobile" : "desktop",
      ).trim();
      return worldStylesheet
        ? `${generatedStylesheet}\n/* World UI v3 stylesheet */\n${worldStylesheet}`
        : generatedStylesheet;
    },
    [gameUiScopeSelector, isMobile, parsedGameUi.document, runtimeBackgroundAsset, worldUiEnvelope],
  );

  const mapGraphNodes = committedMapGraph.nodes;
  const mapGraphEdges = committedMapGraph.edges;
  const visibleRuntimeAttributes = useMemo(
    () => filterRuntimeAttributesForWorld(runtimeAttributes, themeWorld),
    [runtimeAttributes, themeWorld],
  );
  const attributeSideTabs = useMemo<Array<[string, string]>>(
    () => buildAttributeSideTabsFromRuntimeAttributes(
      visibleRuntimeAttributes,
      session?.inventory_items ?? [],
      session?.id ?? sessionId,
      session?.player_character_id ?? "",
    ),
    [session?.id, session?.inventory_items, session?.player_character_id, sessionId, visibleRuntimeAttributes],
  );
  const worldCharacterNameSet = useMemo(
    () =>
      new Set(
        worldCharacters
          .map((character) => character.name.trim())
          .filter(Boolean),
      ),
    [worldCharacters],
  );
  const sideTabs = useMemo(
    () =>
      resolveStatusTabs(parsedGameUi.document, mapGraphNodes.length, attributeSideTabs),
    [attributeSideTabs, mapGraphNodes.length, parsedGameUi.document],
  );

  useEffect(() => {
    const mapBecameAvailable = mapGraphNodes.length > 0 && previousMapCountRef.current === 0;
    previousMapCountRef.current = mapGraphNodes.length;
    if (mapBecameAvailable && sideTabs.some((tab) => tab.key === "map")) {
      setSideTab("map");
      return;
    }

    if (!sideTabs.length) {
      if (sideTab) {
        setSideTab("");
      }
      return;
    }

    if (!sideTabs.some((tab) => tab.key === sideTab)) {
      setSideTab(sideTabs[0].key);
    }
  }, [mapGraphNodes.length, sideTab, sideTabs]);

  const activeAttributeTab = sideTab.startsWith("attribute:")
    ? sideTab.slice("attribute:".length)
    : "";
  const activeAttributeContent = activeAttributeTab
    ? attributeSideTabs.find(([label]) => label === activeAttributeTab)?.[1] ?? ""
    : "";
  const activeAttributeItems = useMemo(
    () => activeAttributeTab
      ? findAttributeItemsForTab(
          visibleRuntimeAttributes,
          activeAttributeTab,
          session?.id ?? sessionId,
          session?.player_character_id ?? "",
        )
      : [],
    [activeAttributeTab, session?.id, session?.player_character_id, sessionId, visibleRuntimeAttributes],
  );

  const latestNarration = useMemo(() => {
    const currentLine = session?.current_line?.trim();
    if (currentLine) {
      return currentLine;
    }

    const latestSystemMessage = [...(session?.messages ?? [])]
      .reverse()
      .find(
        (message) =>
          message.role === "system" &&
          message.metadata?.action_type == null &&
          getMessageText(message.content).trim(),
      );

    return latestSystemMessage ? getMessageText(latestSystemMessage.content).trim() : "";
  }, [session?.current_line, session?.messages]);

  const dialogueMessages = useMemo<ChatMessageResponse[]>(() => {
    return messages.filter((message) => {
      if (message.pending) {
        return true;
      }
      if (message.role === "system" && !message.metadata?.action_type) {
        return false;
      }
      return true;
    });
  }, [messages]);

  const renderedDialogueMessages = useMemo<RenderChatMessage[]>(() => {
    const sorted = [...dialogueMessages].sort((left, right) => {
      const leftIndex = Number(
        left.metadata?.system_index ?? left.metadata?.turn_index ?? 0,
      );
      const rightIndex = Number(
        right.metadata?.system_index ?? right.metadata?.turn_index ?? 0,
      );
      if (leftIndex !== rightIndex) {
        return leftIndex - rightIndex;
      }
      const leftRank = MESSAGE_KIND_RANK[left.metadata?.message_kind as string] ?? 99;
      const rightRank = MESSAGE_KIND_RANK[right.metadata?.message_kind as string] ?? 99;
      return leftRank - rightRank;
    });

    const result: RenderChatMessage[] = [];
    for (let index = 0; index < sorted.length; index += 1) {
      const message = sorted[index];
      if (message.role === "player") {
        result.push({
          ...message,
          speaker: resolvePlayerMessageSpeaker(
            sorted,
            index,
            session?.player_character_name,
          ),
        });
      } else {
        result.push(message);
      }
    }

    if (optimisticPlayerMessage) {
      result.push(optimisticPlayerMessage);
    }

    return result;
  }, [dialogueMessages, optimisticPlayerMessage, session?.player_character_name]);

  const copyableDialogueText = useMemo(() => {
    const lines: string[] = [];
    for (const message of renderedDialogueMessages) {
      if (message.pending) {
        continue;
      }

      const content = getMessageText(message.content).trim();
      if (!content) {
        continue;
      }

      if (message.role === "system" && message.metadata?.action_type === "director_trace") {
        continue;
      }
      if (
        message.role === "system" &&
        message.metadata?.action_type === "director_retry_required"
      ) {
        continue;
      }
      if (
        message.role === "system" &&
        message.metadata?.action_type === "structured_output_error"
      ) {
        continue;
      }

      const speaker =
        message.role === "agent"
          ? message.speaker?.trim() || "角色"
          : message.role === "player"
            ? message.speaker?.trim() || "玩家"
            : "系统";
      lines.push(`${speaker}: ${content}`);
    }
    return lines.join("\n\n");
  }, [renderedDialogueMessages]);

  const latestSceneFocus = useMemo<SceneFocusMessage | null>(() => {
    const sessionMessages = session?.messages ?? [];
    for (let index = sessionMessages.length - 1; index >= 0; index -= 1) {
      const message = sessionMessages[index];
      if (message.role !== "agent") {
        continue;
      }
      const content = getMessageText(message.content).trim();
      const speaker = message.speaker?.trim() ?? "";
      if (!content || !speaker) {
        continue;
      }
      return { speaker, content };
    }
    return null;
  }, [session?.messages]);

  const sceneFocusSpeaker =
    latestSceneFocus?.speaker ||
    session?.current_speaker?.trim() ||
    session?.player_character_name?.trim() ||
    "当前角色";
  const sceneFocusContent =
    latestSceneFocus?.content ||
    session?.current_line?.trim() ||
    "等待角色发言。";
  const activePortraitPath = useMemo(() => {
    if (!session || !sceneFocusSpeaker) {
      return "";
    }

    if (
      session.current_speaker?.trim() === sceneFocusSpeaker.trim() &&
      session.assets.active_speaker_portrait_path
    ) {
      return assetUrl(session.assets.active_speaker_portrait_path);
    }

    const visiblePortrait = session.assets.visible_character_portraits.find(
      (item) =>
        item.character_name.trim() === sceneFocusSpeaker.trim() &&
        item.portrait_asset_path,
    );
    return visiblePortrait?.portrait_asset_path
      ? assetUrl(visiblePortrait.portrait_asset_path)
      : "";
  }, [sceneFocusSpeaker, session]);
  const showSceneFocus = Boolean(session);
  const showSceneCharacters = Boolean(session?.visible_characters?.length);

  useEffect(() => {
    const container = chatMessagesRef.current;
    if (!container) {
      return;
    }

    const updateAutoScrollState = () => {
      const distanceFromBottom =
        container.scrollHeight - container.scrollTop - container.clientHeight;
      shouldAutoScrollRef.current = distanceFromBottom <= 24;
    };

    updateAutoScrollState();
    container.addEventListener("scroll", updateAutoScrollState, { passive: true });
    return () => {
      container.removeEventListener("scroll", updateAutoScrollState);
    };
  }, [session?.id]);

  useLayoutEffect(() => {
    const container = chatMessagesRef.current;
    if (!container || !chatAutoScrollEnabled || !shouldAutoScrollRef.current) {
      return;
    }

    const scrollToBottom = () => {
      container.scrollTop = container.scrollHeight;
    };
    let trailingFrame = 0;
    const frame = window.requestAnimationFrame(() => {
      scrollToBottom();
      trailingFrame = window.requestAnimationFrame(() => {
        if (shouldAutoScrollRef.current) {
          scrollToBottom();
        }
      });
    });
    // 流式文本换行与延后加载的内容会在本次 React 提交之后改变 scrollHeight。
    // 再等一帧可保持跟随，而不会覆盖用户已主动向上滚动的阅读位置。
    return () => {
      window.cancelAnimationFrame(frame);
      window.cancelAnimationFrame(trailingFrame);
    };
  }, [chatAutoScrollEnabled, renderedDialogueMessages]);

  const clearActionError = useCallback(() => {
    setActionError(null);
  }, []);

  const ensureNotificationPermissionForSubmit = useCallback(
    async (text: string, hasAudio: boolean): Promise<boolean> => {
      if (
        !isTauriEnvironment()
        || !isAndroidRuntime()
        || !worldAllowsScheduleNotification(themeWorld)
        || !mayCreateNotificationFromInput(text, hasAudio)
      ) {
        return true;
      }

      // \u884c\u7a0b\u63d0\u9192\u6539\u4e3a\u5199\u5165\u7cfb\u7edf\u65e5\u5386,\u53d1\u9001\u524d\u8bf7\u6c42\u65e5\u5386\u6743\u9650\u3002\u5b89\u5353\u6743\u9650\u5f39\u7a97\u662f\u5f02\u6b65\u7684,
      // requestWorldPermissions \u89e6\u53d1\u5f39\u7a97\u540e\u5373\u8fd4\u56de(granted \u53ef\u80fd\u4e3a null),\u56e0\u6b64\u8fd9\u91cc
      // \u4e0d\u963b\u585e\u53d1\u9001:\u82e5\u7528\u6237\u5c1a\u672a\u6388\u6743,\u65e5\u5386\u5199\u5165\u4f1a\u5931\u8d25\u5e76\u7531 Rust \u5de5\u5177\u7ed3\u679c\u56de\u6d41\u9519\u8bef\u63d0\u793a\u3002
      try {
        await requestWorldPermissions(["calendar"]);
      } catch (permissionError) {
        console.warn("[notification] failed to request Android calendar permission:", permissionError);
      }
      return true;
    },
    [themeWorld],
  );

  // 回答消息交互（第 5 项）：命令幂等，返回最新快照直接应用；
  // 返回值供沙箱层决定是否触发 interaction_answered 世界事件。
  const handleAnswerInteraction = useCallback(
    async (messageId: string, interactionId: string, answer: unknown) => {
      if (!sessionId) return null;
      try {
        const response = await answerInteraction(sessionId, messageId, interactionId, answer);
        applySessionSnapshot(response.session);
        const answeredMessage = response.session.messages.find((message) => message.message_id === messageId);
        const rawInteraction = answeredMessage?.metadata?.interaction;
        const interaction = rawInteraction && typeof rawInteraction === "object" && !Array.isArray(rawInteraction)
          ? rawInteraction as MessageInteraction
          : null;
        return { answer: response.answer, newlyAnswered: response.newly_answered, interaction };
      } catch (error) {
        setActionError(error instanceof Error ? error.message : String(error));
        return null;
      }
    },
    [sessionId, applySessionSnapshot],
  );

  const handleSubmitAction = useCallback(
    async (options: SubmitActionOptions = {}) => {
      const mode: PlayerActionMode = options.mode ?? (editingTurn ? "edit" : "submit");
      const textContent = (options.content ?? inputValue).trim();
      const images = inputImages;
      const audios = inputAudios;

      if (!textContent && images.length === 0 && audios.length === 0) {
        return;
      }

      const resolvedTurnIndex =
        mode === "submit"
          ? undefined
          : Number.isInteger(options.turnIndex) && (options.turnIndex ?? 0) > 0
            ? options.turnIndex
            : editingTurn?.turnIndex;
      const isReplay = mode !== "submit";

      if (isReplay && resolvedTurnIndex === undefined) {
        setActionError(
          mode === "edit"
            ? "未找到要编辑的回合，无法重新生成。"
            : "未找到要重发的回合，无法重新生成。",
        );
        return;
      }

      if (submitInFlightRef.current) {
        return;
      }
      submitInFlightRef.current = true;
      // 自己刚发送的回合必须接管到底部；之后用户手动上滚仍会解除跟随。
      shouldAutoScrollRef.current = true;
      setSubmitting(true);
      setActionError(null);

      try {
        // 构建 content: string | ContentPart[]
        let content: string | ContentPart[] = textContent;
        if (images.length > 0 || audios.length > 0) {
          const parts: ContentPart[] = [];
          // 添加图片部分
          for (const file of images) {
            const base64 = await new Promise<string>((resolve, reject) => {
              const reader = new FileReader();
              reader.onload = () => resolve(reader.result as string);
              reader.onerror = reject;
              reader.readAsDataURL(file);
            });
            parts.push({
              type: "image_url",
              image_url: { url: base64 }
            });
          }
          // 添加音频部分
          for (const file of audios) {
            const base64 = await new Promise<string>((resolve, reject) => {
              const reader = new FileReader();
              reader.onload = () => resolve(reader.result as string);
              reader.onerror = reject;
              reader.readAsDataURL(file);
            });
            // 从文件名或MIME类型推断格式
            const format = file.name.split('.').pop()?.toLowerCase() || "wav";
            parts.push({
              type: "input_audio",
              input_audio: {
                data: base64,
                format,
                duration_secs: (file as File & { durationSecs?: number }).durationSecs,
              }
            });
          }
          // 添加文本部分（如果有文本）
          if (textContent) {
            parts.push({
              type: "text",
              text: textContent
            });
          }
          content = parts;
        }

        const notificationPermissionReady = await ensureNotificationPermissionForSubmit(
          textContent,
          audios.length > 0,
        );
        if (!notificationPermissionReady) {
          return;
        }

        setStreamingResponseActive(true);
        // 提交前已提交的玩家消息条数：附件消息 content 是 multipart，无法用正文字符串匹配。
        const playerMessageCountBefore = (session?.messages ?? []).filter(
          (message) => message.role === "player",
        ).length;
        if (isReplay) {
          setOptimisticPlayerMessage(null);
          setInputImages([]);
          setInputAudios([]);
        } else {
          setOptimisticPlayerMessage({
            role: "player",
            content,
            speaker: session?.player_character_name ?? "玩家",
            created_at: new Date().toISOString(),
            pending: true,
          });
          setInputValue("");
          setInputImages([]);
          setInputAudios([]);
        }

        const playerContent = textContent;
        const snapshot = await streamPlayerAction(
          sessionId,
          {
            content,
            action_mode: mode,
            resend_from_turn_index: resolvedTurnIndex,
          },
          {
            onSnapshot: (nextSnapshot) => {
              // 服务端快照一旦包含新的已提交玩家发言（文本或附件），立即清除乐观消息。
              if (!isReplay) {
                const committedPlayerCount = (nextSnapshot.messages ?? []).filter(
                  (m) => m.role === "player",
                ).length;
                const alreadyInSession = committedPlayerCount > playerMessageCountBefore
                  || nextSnapshot.messages?.some(
                    (m) => m.role === "player"
                      && typeof m.content === "string"
                      && m.content.trim() === playerContent.trim(),
                  );
                if (alreadyInSession) {
                  setOptimisticPlayerMessage(null);
                }
              }
              applySessionSnapshot(nextSnapshot, { streaming: true });
            },
            onError: (detail) => {
              setActionError(formatActionErrorMessage(detail));
            },
          },
        );

        if (snapshot) {
          applySessionSnapshot(snapshot);
          const completedTurnIndex = (snapshot.messages ?? []).reduce((max, message) => {
            const raw = message.metadata && (message.metadata as Record<string, unknown>).turn_index;
            const value = typeof raw === "number" ? raw : Number(raw);
            return Number.isFinite(value) && value > max ? value : max;
          }, 0);
          if (completedTurnIndex > 0) {
            setLastCompletedTurn((prev) => ({
              sessionId,
              turnIndex: completedTurnIndex,
              seq: (prev?.seq ?? 0) + 1,
            }));
          }
        }

        setOptimisticPlayerMessage(null);
        if (isReplay) {
          setEditingTurn(null);
          setInputValue("");
        }
      } catch (submitError) {
        setActionError(
          formatActionErrorMessage(
            submitError instanceof Error
              ? submitError.message
              : String(submitError ?? "发言失败"),
          ),
        );
        setOptimisticPlayerMessage(null);
        setInputImages(images);
        setInputAudios(audios);

        if (mode === "edit" && resolvedTurnIndex !== undefined) {
          setEditingTurn({ turnIndex: resolvedTurnIndex, originalContent: textContent });
          setInputValue(textContent);
        } else if (mode === "submit") {
          setInputValue(textContent);
        } else {
          setEditingTurn(null);
        }
      } finally {
        setStreamingResponseActive(false);
        setSubmitting(false);
        scheduleRuntimeAttributeRefresh();
        submitInFlightRef.current = false;
      }
    },
    [
      applySessionSnapshot,
      editingTurn,
      ensureNotificationPermissionForSubmit,
      inputAudios,
      inputImages,
      inputValue,
      scheduleRuntimeAttributeRefresh,
      session,
      sessionId,
    ],
  );

  const startEditingTurn = useCallback((content: string, turnIndex: number) => {
    setEditingTurn({ turnIndex, originalContent: content });
    setInputValue(content);
    setActionError(null);
  }, []);

  const cancelEditingTurn = useCallback(() => {
    setEditingTurn(null);
    setInputValue("");
    setActionError(null);
  }, []);

  const handleBranch = useCallback(async () => {
    if (!session?.id) {
      return;
    }

    try {
      setBranching(true);
      setError(null);
      const saves = await fetchSaves();
      const matched = saves.find((item) => item.session_id === session.id);
      if (!matched) {
        throw new Error("当前会话没有对应的存档快照，无法创建分支");
      }
      const branched = await branchSave(matched.id);
      navigate(`/game/${branched.session_id}`);
    } catch (branchError) {
      setError(
        branchError instanceof Error ? branchError.message : "创建分支失败",
      );
    } finally {
      setBranching(false);
    }
  }, [navigate, session]);

  const dismissSwitchProposal = useCallback((key: string) => {
    setDismissedProposalKeys((current) => new Set(current).add(key));
  }, []);

  const handleAcceptSwitchProposal = useCallback(
    async (proposal: SwitchProposalView) => {
      if (!session?.id || !themeWorld) {
        return;
      }

      try {
        setSwitching(true);
        setError(null);

        let targetCharacterId = proposal.targetCharacterId ?? "";
        if (!targetCharacterId) {
          let characters = worldCharacters;
          let target = characters.find(
            (character) => character.name === proposal.targetCharacterName,
          );
          if (!target) {
            characters = await fetchWorldCharacters(themeWorld.id);
            target = characters.find(
              (character) => character.name === proposal.targetCharacterName,
            );
          }
          targetCharacterId = target?.id ?? "";
        }

        if (!targetCharacterId) {
          throw new Error(`未找到角色：${proposal.targetCharacterName}`);
        }

        const payload: SwitchPlayerCharacterRequest = {
          player_character_id: targetCharacterId,
          proposal: {
            target_character_name: proposal.targetCharacterName,
            reason: proposal.reason || undefined,
            location: proposal.proposal.location || undefined,
            scene_name: proposal.proposal.scene_name || undefined,
            scene_background_hint:
              proposal.proposal.scene_background_hint || undefined,
            scene_tags: proposal.proposal.scene_tags,
            visible_characters: proposal.proposal.visible_characters,
          },
        };

        await switchPlayerCharacter(session.id, payload);
        dismissSwitchProposal(proposal.key);
      } catch (switchError) {
        setError(
          formatActionErrorMessage(
            switchError instanceof Error
              ? switchError.message
              : String(switchError ?? "切换角色失败"),
          ),
        );
      } finally {
        setSwitching(false);
      }
    },
    [dismissSwitchProposal, session, themeWorld, worldCharacters],
  );

  const dismissDirectorRetryCard = useCallback((key: string) => {
    setDismissedRetryCardKeys((current) => new Set(current).add(key));
  }, []);

  const handleRetryFailedStep = useCallback(
    async (request: RetryFailedLlmStepRequest) => {
      if (!session?.id) {
        return;
      }

      try {
        setRetryingToken(request.retry_token);
        setActionError(null);
        const snapshot = await retryFailedLlmStep(session.id, request);
        applySessionSnapshot(snapshot);
      } catch (retryError) {
        setActionError(
          formatActionErrorMessage(
            retryError instanceof Error
              ? retryError.message
              : String(retryError ?? "重发失败"),
          ),
        );
      } finally {
        setRetryingToken(null);
        scheduleRuntimeAttributeRefresh();
      }
    },
    [applySessionSnapshot, scheduleRuntimeAttributeRefresh, session],
  );

  const handleCopyDialogue = useCallback(async () => {
    const text = copyableDialogueText.trim();
    if (!text) {
      return;
    }
    try {
      await copyTextToClipboard(text);
      showToast("已复制");
    } catch {
      // Ignore clipboard failures and keep the session usable.
    }
  }, [copyableDialogueText]);

  const handleCopyMessage = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) {
      return;
    }
    try {
      await copyTextToClipboard(trimmed);
      showToast("已复制");
    } catch {
      // Ignore clipboard failures and keep the session usable.
    }
  }, []);

  // bag 作为稳定引用返回：下游 shell 的 runtime/actions memo 才能生效。
  // 任一真实状态变化会更新对应依赖并生成新 bag 对象。
  return useMemo(
    () => ({
      session,
      sessionId,
      loading,
      error: error ?? "",
      themeWorld,
      playerCharacter,
      worldCharacters,
      currentSave,
      messages,
      inputValue,
      setInputValue,
      inputImages,
      setInputImages,
      inputAudios,
      setInputAudios,
      chatAutoScrollEnabled,
      setChatAutoScrollEnabled,
      clearActionError,
      submitting,
      streamingResponseActive,
      branching,
      actionError,
      editingTurn,
      startEditingTurn,
      cancelEditingTurn,
      sideTab,
      setSideTab,
      switching,
      dismissedProposalKeys,
      dismissSwitchProposal,
      handleAcceptSwitchProposal,
      expandedDirectorTraceKeys,
      setExpandedDirectorTraceKeys,
      activeCharacterCreationKeys,
      retryingToken,
      dismissedRetryCardKeys,
      dismissDirectorRetryCard,
      handleRetryFailedStep,
      handleBranch,
      handleSubmitAction,
      handleAnswerInteraction,
      lastCompletedTurn,
      optimisticPlayerMessage,
      worldUiEnvelope,
      themeStyle,
      gameUiScopeId,
      parsedGameUi,
      runtimeBackgroundAsset,
      runtimeBackgroundStyle,
      themeCustomCss,
      worldUiRuntimeVersion: worldUiEnvelope.runtime_version,
      mapGraphNodes,
      mapGraphEdges,
      runtimeAttributes,
      attributeSideTabs,
      worldCharacterNameSet,
      sideTabs,
      activeAttributeTab,
      activeAttributeContent,
      activeAttributeItems,
      latestNarration,
      dialogueMessages,
      renderedDialogueMessages,
      copyableDialogueText,
      latestSceneFocus,
      sceneFocusSpeaker,
      sceneFocusContent,
      activePortraitPath,
      showSceneFocus,
      showSceneCharacters,
      chatMessagesRef,
      inputRef,
      handleCopyDialogue,
      handleCopyMessage,
    }),
    [
      session,
      sessionId,
      loading,
      error,
      themeWorld,
      playerCharacter,
      worldCharacters,
      currentSave,
      messages,
      inputValue,
      inputImages,
      inputAudios,
      chatAutoScrollEnabled,
      clearActionError,
      submitting,
      streamingResponseActive,
      branching,
      actionError,
      editingTurn,
      startEditingTurn,
      cancelEditingTurn,
      sideTab,
      switching,
      dismissedProposalKeys,
      dismissSwitchProposal,
      handleAcceptSwitchProposal,
      expandedDirectorTraceKeys,
      activeCharacterCreationKeys,
      retryingToken,
      dismissedRetryCardKeys,
      dismissDirectorRetryCard,
      handleRetryFailedStep,
      handleBranch,
      handleSubmitAction,
      handleAnswerInteraction,
      lastCompletedTurn,
      optimisticPlayerMessage,
      worldUiEnvelope,
      themeStyle,
      gameUiScopeId,
      parsedGameUi,
      runtimeBackgroundAsset,
      runtimeBackgroundStyle,
      themeCustomCss,
      mapGraphNodes,
      mapGraphEdges,
      runtimeAttributes,
      attributeSideTabs,
      worldCharacterNameSet,
      sideTabs,
      activeAttributeTab,
      activeAttributeContent,
      activeAttributeItems,
      latestNarration,
      dialogueMessages,
      renderedDialogueMessages,
      copyableDialogueText,
      latestSceneFocus,
      sceneFocusSpeaker,
      sceneFocusContent,
      activePortraitPath,
      showSceneFocus,
      showSceneCharacters,
      handleCopyDialogue,
      handleCopyMessage,
    ],
  );
}
