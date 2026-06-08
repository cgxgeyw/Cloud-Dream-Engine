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
import {
  assetUrl,
  branchSave,
  fetchCharacter,
  fetchSessionRuntimeAttributes,
  fetchSaves,
  fetchSession,
  fetchWorlds,
  fetchWorldCharacters,
  isTauriEnvironment,
  retryFailedLlmStep,
  streamPlayerAction,
  switchPlayerCharacter,
  toSessionWebSocketUrl,
  type CharacterResponse,
  type ChatMessageResponse,
  type PlayerActionMode,
  type RetryFailedLlmStepRequest,
  type SaveResponse,
  type SessionRuntimeAttributesResponse,
  type SessionMapEdge,
  type SessionMapNode,
  type SessionSnapshotResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import type { ContentPart } from "../data/types";
import {
  buildGameUiStylesheet,
  createGameUiScopeSelector,
  normalizeGameUiScopeId,
  normalizeWorldUiEnvelope,
  parseGameUiDocument,
  resolveUiFile,
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

function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") return content;
  return content
    .filter((p): p is { type: "text"; text: string } => p.type === "text")
    .map((p) => p.text)
    .join("");
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

function shouldShowRuntimeAttributeInSidePanel(item: {
  display_policy?: Record<string, unknown> | null;
  influence_policy?: Record<string, unknown> | null;
}): boolean {
  const displayVisible = item.display_policy?.game_visible;
  if (typeof displayVisible === "boolean") {
    return displayVisible;
  }
  const panelConfig = item.influence_policy?.["ui.status_panel"];
  if (panelConfig && typeof panelConfig === "object" && "enabled" in panelConfig) {
    return Boolean((panelConfig as { enabled?: unknown }).enabled);
  }
  return false;
}

function buildAttributeSideTabsFromRuntimeAttributes(
  runtimeAttributes: SessionRuntimeAttributesResponse,
): Array<[string, string]> {
  return [...runtimeAttributes.session_attributes, ...runtimeAttributes.character_attributes]
    .map((group) => {
      const lines = group.items
        .filter((item) => shouldShowRuntimeAttributeInSidePanel(item))
        .map((item) => {
          const value = stringifyRuntimeAttributeValue(item.value);
          if (!value) {
            return "";
          }
          return `${item.label || item.key}: ${value}`;
        })
        .filter(Boolean);
      return [group.owner_label.trim(), lines.join("\n")] as [string, string];
    })
    .filter(([label, content]) => label && content);
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

  optimisticPlayerMessage: RenderChatMessage | null;

  worldUiEnvelope: WorldUiEnvelope;
  themeStyle: Record<string, string>;
  gameUiScopeId: string;
  parsedGameUi: ReturnType<typeof parseGameUiDocument>;
  runtimeBackgroundAsset: string;
  runtimeBackgroundStyle: React.CSSProperties & Record<string, string>;
  themeCustomCss: string;
  mapGraphNodes: SessionMapNode[];
  mapGraphEdges: SessionMapEdge[];
  attributeSideTabs: Array<[string, string]>;
  worldCharacterNameSet: Set<string>;
  sideTabs: Array<{ key: string; label: string }>;
  activeAttributeTab: string;
  activeAttributeContent: string;
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
  const [themeWorld, setThemeWorld] = useState<WorldResponse | null>(null);
  const [playerCharacter, setPlayerCharacter] = useState<CharacterResponse | null>(null);
  const [worldCharacters, setWorldCharacters] = useState<CharacterResponse[]>([]);
  const [runtimeAttributes, setRuntimeAttributes] = useState<SessionRuntimeAttributesResponse>({
    session_attributes: [],
    character_attributes: [],
  });
  const [currentSave, setCurrentSave] = useState<SaveResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [inputImages, setInputImages] = useState<File[]>([]);
  const [inputAudios, setInputAudios] = useState<File[]>([]);
  const [chatAutoScrollEnabled, setChatAutoScrollEnabled] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [branching, setBranching] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [editingTurn, setEditingTurn] = useState<EditingTurnState | null>(null);
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
  const [activeCharacterCreationKeys, setActiveCharacterCreationKeys] =
    useState<string[]>([]);

  const chatMessagesRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const shouldAutoScrollRef = useRef(true);
  const seenCharacterCreationsRef = useRef<Set<string>>(new Set());

  const sessionId = sessionIdParam ?? "";
  const worldId = themeWorld?.id ?? "";
  const playerCharacterId = session?.player_character_id ?? "";
  const gameUiScopeId = normalizeGameUiScopeId(useId());
  const gameUiScopeSelector = useMemo(
    () => createGameUiScopeSelector(gameUiScopeId),
    [gameUiScopeId],
  );

  useEffect(() => {
    setSession(null);
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
    setSwitching(false);
    setRetryingToken(null);
    setOptimisticPlayerMessage(null);
    setDismissedProposalKeys(new Set());
    setDismissedRetryCardKeys(new Set());
    setExpandedDirectorTraceKeys(new Set());
    setActiveCharacterCreationKeys([]);
    shouldAutoScrollRef.current = true;
    seenCharacterCreationsRef.current = readSeenCharacterCreationKeys(sessionId);
  }, [sessionId]);

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
        setSession(data);
        setLoading(false);

        if (isTauriEnvironment()) {
          return;
        }

        websocket = new WebSocket(toSessionWebSocketUrl(sessionId));
        websocket.onmessage = (event) => {
          if (cancelled) {
            return;
          }

          const payload = JSON.parse(event.data) as {
            type: string;
            payload?: SessionSnapshotResponse;
            detail?: string;
          };

          if (payload.type === "session.snapshot" && payload.payload) {
            hasLoadedSession = true;
            setSession(payload.payload);
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
  }, [sessionId]);

  useEffect(() => {
    if (!session?.world_name) {
      setThemeWorld(null);
      return;
    }

    const worldName = session.world_name;
    let cancelled = false;

    async function loadWorld() {
      try {
        const worlds = await fetchWorlds();
        if (!cancelled) {
          setThemeWorld(
            worlds.find((world) => world.name === worldName) ?? null,
          );
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
  }, [session?.world_name]);

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

    let cancelled = false;

    async function loadRuntimeAttributes() {
      try {
        const data = await fetchSessionRuntimeAttributes(sessionId);
        if (!cancelled) {
          setRuntimeAttributes(data);
        }
      } catch {
        if (!cancelled) {
          setRuntimeAttributes({ session_attributes: [], character_attributes: [] });
        }
      }
    }

    void loadRuntimeAttributes();

    return () => {
      cancelled = true;
    };
  }, [sessionId, session?.messages?.length, session?.current_line]);

  useEffect(() => {
    if (!themeWorld?.id || !session?.messages?.length) {
      return;
    }

    const stableWorldId = themeWorld.id;
    const knownCharacterNames = new Set(
      worldCharacters.map((character) => character.name.trim()).filter(Boolean),
    );
    const missingCreatedCharacter = session.messages.some((message) => {
      const creation = parseCharacterCreationMessage(message);
      if (!creation) {
        return false;
      }
      return !knownCharacterNames.has(creation.characterName.trim());
    });

    if (!missingCreatedCharacter) {
      return;
    }

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

  const messages = useMemo<RenderChatMessage[]>(
    () =>
      (session?.messages ?? []).map((message) => ({
        ...message,
        pending: false,
      })),
    [session?.messages],
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
    () =>
      buildGameUiStylesheet(
        parsedGameUi.document,
        runtimeBackgroundAsset ? assetUrl(runtimeBackgroundAsset) : undefined,
        gameUiScopeSelector,
      ),
    [gameUiScopeSelector, parsedGameUi.document, runtimeBackgroundAsset],
  );

  const mapGraphNodes = useMemo(
    () => session?.map_graph_nodes ?? [],
    [session?.map_graph_nodes],
  );
  const mapGraphEdges = useMemo(
    () => session?.map_graph_edges ?? [],
    [session?.map_graph_edges],
  );
  const attributeSideTabs = useMemo<Array<[string, string]>>(
    () => buildAttributeSideTabsFromRuntimeAttributes(runtimeAttributes),
    [runtimeAttributes],
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
    if (!sideTabs.length) {
      if (sideTab) {
        setSideTab("");
      }
      return;
    }

    if (!sideTabs.some((tab) => tab.key === sideTab)) {
      setSideTab(sideTabs[0].key);
    }
  }, [sideTab, sideTabs]);

  const activeAttributeTab = sideTab.startsWith("attribute:")
    ? sideTab.slice("attribute:".length)
    : "";
  const activeAttributeContent = activeAttributeTab
    ? attributeSideTabs.find(([label]) => label === activeAttributeTab)?.[1] ?? ""
    : "";

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

    const frame = window.requestAnimationFrame(() => {
      container.scrollTop = container.scrollHeight;
    });
    return () => {
      window.cancelAnimationFrame(frame);
    };
  }, [chatAutoScrollEnabled, renderedDialogueMessages]);

  const clearActionError = useCallback(() => {
    setActionError(null);
  }, []);

  const handleSubmitAction = useCallback(
    async (options: SubmitActionOptions = {}) => {
      const mode: PlayerActionMode = options.mode ?? (editingTurn ? "edit" : "submit");
      const textContent = (options.content ?? inputValue).trim();
      const images = inputImages;
      const audios = inputAudios;

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
            input_audio: { data: base64, format }
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

      try {
        setSubmitting(true);
        setActionError(null);

        if (isReplay) {
          setOptimisticPlayerMessage(null);
          setInputImages([]);
          setInputAudios([]);
        } else {
          setOptimisticPlayerMessage({
            role: "player",
            content: textContent,
            speaker: session?.player_character_name ?? "玩家",
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
              // 一旦服务端快照中已包含本次玩家发言，立即清除乐观消息，避免底部重复显示
              if (!isReplay) {
                const alreadyInSession = nextSnapshot.messages?.some(
                  (m) => m.role === "player" && typeof m.content === "string" && m.content.trim() === playerContent.trim(),
                );
                if (alreadyInSession) {
                  setOptimisticPlayerMessage(null);
                }
              }
              setSession(nextSnapshot);
            },
            onError: (detail) => {
              setActionError(formatActionErrorMessage(detail));
            },
          },
        );

        if (snapshot) {
          setSession(snapshot);
        }

        setOptimisticPlayerMessage(null);
        if (isReplay) {
          setEditingTurn(null);
          setInputValue("");
        }
      } catch (submitError) {
        setActionError(
          formatActionErrorMessage(
            submitError instanceof Error ? submitError.message : "发言失败",
          ),
        );
        setOptimisticPlayerMessage(null);

        if (mode === "edit" && resolvedTurnIndex !== undefined) {
          setEditingTurn({ turnIndex: resolvedTurnIndex, originalContent: textContent });
          setInputValue(textContent);
        } else if (mode === "submit") {
          setInputValue(textContent);
        } else {
          setEditingTurn(null);
        }
      } finally {
        setSubmitting(false);
      }
    },
    [editingTurn, inputValue, session, sessionId],
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

        const payload: {
          target_character_name: string;
          reason?: string;
          location?: string;
          scene_name?: string;
          scene_background_hint?: string;
          scene_tags?: string[];
          visible_characters?: string[];
          target_character_id?: string;
        } = {
          target_character_name: proposal.targetCharacterName,
          reason: proposal.reason || undefined,
          location: proposal.proposal.location || undefined,
          scene_name: proposal.proposal.scene_name || undefined,
          scene_background_hint:
            proposal.proposal.scene_background_hint || undefined,
          scene_tags: proposal.proposal.scene_tags,
          visible_characters: proposal.proposal.visible_characters,
        };

        if (targetCharacterId) {
          payload.target_character_id = targetCharacterId;
        }

        await switchPlayerCharacter(session.id, payload as never);
        dismissSwitchProposal(proposal.key);
      } catch (switchError) {
        setError(
          switchError instanceof Error ? switchError.message : "切换角色失败",
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
        setSession(snapshot);
      } catch (retryError) {
        setActionError(
          retryError instanceof Error ? retryError.message : "重发失败",
        );
      } finally {
        setRetryingToken(null);
      }
    },
    [session],
  );

  const handleCopyDialogue = useCallback(async () => {
    const text = copyableDialogueText.trim();
    if (!text) {
      return;
    }
    try {
      await copyTextToClipboard(text);
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
    } catch {
      // Ignore clipboard failures and keep the session usable.
    }
  }, []);

  return {
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
    attributeSideTabs,
    worldCharacterNameSet,
    sideTabs,
    activeAttributeTab,
    activeAttributeContent,
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
  };
}
