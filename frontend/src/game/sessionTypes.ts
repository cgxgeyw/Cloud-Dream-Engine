// Extracted from useGameSession.ts (bag / option types)
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

