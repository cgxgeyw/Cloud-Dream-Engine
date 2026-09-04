import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowDownToLine, ArrowUpToLine, ChevronDown, ChevronUp, Copy, GitBranch, Play, Square } from "lucide-react";
import type { AudioContentPart, ContentPart } from "../../data/types";
import type { GameUiComponentNode } from "../../data/gameUi";
import {
  CotBlock,
  RenderCharacterCreation,
  RenderDirectorRetryCard,
  RenderDirectorTrace,
  RenderStructuredError,
  RenderSwitchProposal,
} from "../../game/MessageCards";
import {
  parseAgentNarration,
  parseAgentReasoning,
  parseAgentToolActivity,
  parseCharacterCreationMessage,
  parseDirectorRetryCard,
  parseDirectorTrace,
  parseStructuredError,
  parseSwitchProposal,
  resolveDialogueSpeakerLabel,
  shouldHidePinnedNarrationMessage,
} from "../../game/utils";
import type { GameUiRuntimeActions } from "../actions";
import type { GameUiRuntimeContext } from "../runtimeContext";
import { MarkdownMessageContent } from "./MarkdownContent";

function MobileErrorNotice({
  speakerName,
  summary,
  retryToken,
  retrying,
  onRetry,
}: {
  speakerName: string;
  summary: string;
  retryToken?: string;
  retrying: boolean;
  onRetry?: (token: string) => void;
}) {
  return (
    <div className="game-message-row game-message-row--system game-message-row--mobile-error">
      <div className="game-mobile-error-notice">
        <div className="game-mobile-error-title">
          {speakerName ? `${speakerName}：本次发言失败` : "本次发言失败"}
        </div>
        <div className="game-mobile-error-summary">{summary || "模型返回内容无法解析，请重发。"}</div>
        {retryToken && onRetry ? (
          <button
            type="button"
            className="game-mobile-error-retry game-ui-button"
            data-variant="primary"
            disabled={retrying}
            onClick={() => onRetry(retryToken)}
          >
            {retrying ? "重发中..." : "重发"}
          </button>
        ) : null}
      </div>
    </div>
  );
}

function TypingIndicator({ speakerName }: { speakerName: string }) {
  return (
    <div className="game-message-row game-message-row--typing">
      <div className="game-message game-message--agent game-ui-message-bubble game-typing-bubble" data-variant="agent">
        <div className="game-message-speaker game-ui-message-speaker" data-variant="agent">
          {speakerName}
        </div>
        <div className="game-typing-dots">
          <span className="game-typing-dot" />
          <span className="game-typing-dot" />
          <span className="game-typing-dot" />
        </div>
      </div>
    </div>
  );
}

function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") {
    return content;
  }
  return content
    .filter((part): part is { type: "text"; text: string } => part.type === "text")
    .map((part) => part.text)
    .join("");
}

function getAudioParts(content: string | ContentPart[]): AudioContentPart[] {
  if (typeof content === "string") {
    return [];
  }
  return content.filter((part): part is AudioContentPart => part.type === "input_audio");
}

type ImageContentPart = Extract<ContentPart, { type: "image_url" }>;

function getImageParts(content: string | ContentPart[]): ImageContentPart[] {
  if (typeof content === "string") {
    return [];
  }
  return content.filter((part): part is ImageContentPart => part.type === "image_url");
}

function resolveMessageVisualRole(role: string): string {
  if (role === "assistant") return "agent";
  if (role === "narration") return "system";
  return role;
}

// 消息里的图片附件缩略图。内联样式保证在宿主与 iframe 里都可用（与语音气泡一致）。
function ImageMessageThumbnail({ part }: { part: ImageContentPart }) {
  return (
    <a
      href={part.image_url.url}
      target="_blank"
      rel="noreferrer"
      title="查看原图"
      style={{ display: "inline-block", marginTop: 6, marginRight: 6 }}
    >
      <img
        src={part.image_url.url}
        alt="图片附件"
        style={{
          maxWidth: 220,
          maxHeight: 160,
          borderRadius: 10,
          border: "1px solid rgba(127,127,127,0.35)",
          objectFit: "cover",
          display: "block",
        }}
      />
    </a>
  );
}

// 微信式语音消息气泡：显示秒数，点击播放/停止。内联样式保证在宿主与 iframe 里都可用。
function VoiceMessageBubble({ part }: { part: AudioContentPart }) {
  const [playing, setPlaying] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const rawDuration = part.input_audio.duration_secs;
  const seconds = typeof rawDuration === "number" && Number.isFinite(rawDuration)
    ? Math.max(1, Math.round(rawDuration))
    : null;

  const stop = () => {
    audioRef.current?.pause();
    audioRef.current = null;
    setPlaying(false);
  };

  const toggle = () => {
    if (playing) {
      stop();
      return;
    }
    const audio = new Audio(part.input_audio.data);
    audioRef.current = audio;
    audio.onended = stop;
    audio.onerror = stop;
    setPlaying(true);
    void audio.play().catch(stop);
  };

  useEffect(() => () => {
    audioRef.current?.pause();
    audioRef.current = null;
  }, []);

  return (
    <button
      type="button"
      onClick={toggle}
      title={"\u64ad\u653e\u8bed\u97f3"}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "6px 14px",
        borderRadius: 18,
        border: "1px solid rgba(127,127,127,0.35)",
        background: playing ? "rgba(20,184,166,0.18)" : "rgba(255,255,255,0.55)",
        color: "inherit",
        fontSize: 14,
        lineHeight: 1.4,
        cursor: "pointer",
      }}
    >
      {playing ? <Square size={13} /> : <Play size={13} />}
      <span>{seconds !== null ? `${seconds}\u2033` : "\u8bed\u97f3"}</span>
    </button>
  );
}

type MessageListComponentProps = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
};

const MESSAGE_TIMESTAMP_GAP_MS = 5 * 60 * 1000;

function parseMessageTimestamp(createdAt: string | undefined): Date | null {
  if (!createdAt) {
    return null;
  }
  const timestamp = new Date(createdAt);
  return Number.isNaN(timestamp.getTime()) ? null : timestamp;
}

function localCalendarDay(timestamp: Date): number {
  return Date.UTC(timestamp.getFullYear(), timestamp.getMonth(), timestamp.getDate());
}

export function shouldShowMessageTimestamp(
  createdAt: string | undefined,
  previousCreatedAt: string | undefined,
): boolean {
  const timestamp = parseMessageTimestamp(createdAt);
  if (!timestamp) {
    return false;
  }

  const previousTimestamp = parseMessageTimestamp(previousCreatedAt);
  if (!previousTimestamp) {
    return true;
  }

  return (
    timestamp.getTime() - previousTimestamp.getTime() >= MESSAGE_TIMESTAMP_GAP_MS
    || localCalendarDay(timestamp) !== localCalendarDay(previousTimestamp)
  );
}

export function formatMessageTimestamp(createdAt: string | undefined, now = new Date()): string | null {
  const timestamp = parseMessageTimestamp(createdAt);
  if (!timestamp) {
    return null;
  }

  const time = `${String(timestamp.getHours()).padStart(2, "0")}:${String(timestamp.getMinutes()).padStart(2, "0")}`;
  const dayDifference = (localCalendarDay(now) - localCalendarDay(timestamp)) / 86_400_000;
  if (dayDifference === 0) {
    return `今天 ${time}`;
  }
  if (dayDifference === 1) {
    return `昨天 ${time}`;
  }
  if (timestamp.getFullYear() === now.getFullYear()) {
    return `${timestamp.getMonth() + 1}月${timestamp.getDate()}日 ${time}`;
  }
  return `${timestamp.getFullYear()}年${timestamp.getMonth() + 1}月${timestamp.getDate()}日 ${time}`;
}

function readBooleanProp(
  node: GameUiComponentNode | undefined,
  key: string,
  fallback: boolean,
): boolean {
  const value = node?.props?.[key];
  return typeof value === "boolean" ? value : fallback;
}

export function MessageListComponent({ runtime, actions, node }: MessageListComponentProps) {
  const autoScroll = readBooleanProp(node, "auto_scroll", true);
  const showPendingState = readBooleanProp(node, "show_pending_state", true);
  const showAgentReasoning = readBooleanProp(node, "show_agent_reasoning", true);
  const showTypingIndicator = readBooleanProp(node, "show_typing_indicator", true);
  const mobileSimple = readBooleanProp(node, "mobile_simple", false) && runtime.capabilities.platform === "mobile";
  const showScrollControls = readBooleanProp(node, "show_scroll_controls", true);
  const showScrollTopButton = readBooleanProp(node, "show_scroll_top_button", true);
  const showScrollUpButton = readBooleanProp(node, "show_scroll_up_button", true);
  const showScrollDownButton = readBooleanProp(node, "show_scroll_down_button", true);
  const showScrollBottomButton = readBooleanProp(node, "show_scroll_bottom_button", true);
  const [scrollState, setScrollState] = useState({ atTop: true, atBottom: true });
  const initializedScrollSessionRef = useRef<string | null>(null);

  useEffect(() => {
    actions.setAutoScrollEnabled(autoScroll);
  }, [actions, autoScroll]);

  const updateScrollState = useCallback(() => {
    const container = runtime.chat_messages_ref.current;
    if (!container) {
      return;
    }
    const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
    setScrollState({
      atTop: container.scrollTop <= 12,
      atBottom: distanceFromBottom <= 24,
    });
  }, [runtime.chat_messages_ref]);

  useEffect(() => {
    const container = runtime.chat_messages_ref.current;
    if (!container) {
      return undefined;
    }
    const handleScroll = () => updateScrollState();
    container.addEventListener("scroll", handleScroll, { passive: true });
    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(() => updateScrollState());
    resizeObserver?.observe(container);
    updateScrollState();
    return () => {
      container.removeEventListener("scroll", handleScroll);
      resizeObserver?.disconnect();
    };
  }, [runtime.chat_messages_ref, updateScrollState]);

  useLayoutEffect(() => {
    const container = runtime.chat_messages_ref.current;
    const sessionId = runtime.session?.id ?? null;
    if (!container || !sessionId || runtime.messages.length === 0 || initializedScrollSessionRef.current === sessionId) {
      return;
    }
    let trailingFrame = 0;
    const frame = requestAnimationFrame(() => {
      container.scrollTop = container.scrollHeight;
      trailingFrame = requestAnimationFrame(() => {
        container.scrollTop = container.scrollHeight;
        initializedScrollSessionRef.current = sessionId;
        updateScrollState();
      });
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(trailingFrame);
    };
  }, [runtime.chat_messages_ref, runtime.messages.length, runtime.session?.id, updateScrollState]);

  const scrollToPosition = useCallback((top: number) => {
    const container = runtime.chat_messages_ref.current;
    if (!container) {
      return;
    }
    container.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
  }, [runtime.chat_messages_ref]);

  const scrollOneMessage = useCallback((direction: "up" | "down") => {
    const container = runtime.chat_messages_ref.current;
    if (!container) {
      return;
    }
    const containerTop = container.getBoundingClientRect().top;
    const currentTop = container.scrollTop;
    const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-game-message-row]"));
    const positions = rows.map((row) => row.getBoundingClientRect().top - containerTop + currentTop);
    const target = direction === "up"
      ? positions.filter((position) => position < currentTop - 8).pop() ?? 0
      : positions.find((position) => position > currentTop + 8) ?? Number.MAX_SAFE_INTEGER;
    scrollToPosition(target);
  }, [runtime.chat_messages_ref, scrollToPosition]);

  const hasActiveAgentStream = runtime.messages.some((message) => {
    if (message.role !== "agent") {
      return false;
    }
    const metadata = (message.metadata ?? {}) as Record<string, unknown>;
    if (metadata.streaming === true) {
      return true;
    }
    if (!runtime.ui_state.submitting) {
      return false;
    }
    return (
      metadata.message_kind === "agent_response"
      && (
        String(metadata.reasoning ?? "").trim().length > 0
        || getMessageText(message.content).trim().length > 0
      )
    );
  });

  let previousVisibleMessageCreatedAt: string | undefined;

  return (
    <div className="game-chat-messages-shell">
      <div className="game-chat-messages" ref={runtime.chat_messages_ref}>
      {runtime.messages.map((message, index) => {
        if (!showPendingState && message.pending) {
          return null;
        }

        if (shouldHidePinnedNarrationMessage(message, runtime.latest_narration)) {
          return null;
        }

        const visualRole = resolveMessageVisualRole(message.role);

        if (mobileSimple && !["system", "agent", "player"].includes(visualRole)) {
          return null;
        }

        const directorTrace = parseDirectorTrace(message);
        if (directorTrace) {
          return <RenderDirectorTrace key={directorTrace.key} trace={directorTrace} />;
        }

        const characterCreation = parseCharacterCreationMessage(message);
        if (characterCreation) {
          if (mobileSimple) {
            return null;
          }
          if (!runtime.message_state.active_character_creation_keys.includes(characterCreation.key)) {
            return null;
          }
          return <RenderCharacterCreation key={characterCreation.key} creation={characterCreation} />;
        }

        const switchProposal = parseSwitchProposal(message);
        if (switchProposal) {
          if (mobileSimple) {
            return null;
          }
          if (runtime.message_state.dismissed_proposal_keys.has(switchProposal.key)) {
            return null;
          }
          return (
            <RenderSwitchProposal
              key={switchProposal.key}
              proposal={switchProposal}
              switching={runtime.ui_state.switching}
              onAccept={(proposal) => void actions.acceptSwitchProposal(proposal)}
              onDismiss={actions.dismissSwitchProposal}
            />
          );
        }

        const retryCard = parseDirectorRetryCard(message);
        if (retryCard) {
          if (mobileSimple) {
            if (runtime.message_state.dismissed_retry_card_keys.has(retryCard.key)) {
              return null;
            }
            return (
              <MobileErrorNotice
                key={retryCard.key}
                speakerName={runtime.session?.player_character_name ?? ""}
                summary={retryCard.summary || retryCard.title}
                retryToken={retryCard.retryToken}
                retrying={runtime.ui_state.retrying_token === retryCard.retryToken}
                onRetry={(token) => void actions.retryTurn(token)}
              />
            );
          }
          if (runtime.message_state.dismissed_retry_card_keys.has(retryCard.key)) {
            return null;
          }
          return (
            <RenderDirectorRetryCard
              key={retryCard.key}
              card={retryCard}
              retryingToken={runtime.ui_state.retrying_token}
              branching={runtime.ui_state.branching}
              loading={runtime.ui_state.loading}
              hasSession={!!runtime.session}
              onRetry={(token) => void actions.retryTurn(token)}
              onBranch={() => void actions.branchFromCurrent()}
              onDismiss={actions.dismissRetryCard}
            />
          );
        }

        const structuredError = parseStructuredError(message);
        if (structuredError) {
          if (mobileSimple) {
            return (
              <MobileErrorNotice
                key={structuredError.key}
                speakerName={structuredError.speakerName}
                summary={structuredError.summary || structuredError.title}
                retryToken={structuredError.retryToken}
                retrying={runtime.ui_state.retrying_token === structuredError.retryToken}
                onRetry={(token) => void actions.retryTurn(token)}
              />
            );
          }
          return (
            <RenderStructuredError
              key={structuredError.key}
              error={structuredError}
              retryingToken={runtime.ui_state.retrying_token}
              branching={runtime.ui_state.branching}
              loading={runtime.ui_state.loading}
              hasSession={!!runtime.session}
              onRetry={(token) => void actions.retryTurn(token)}
              onBranch={() => void actions.branchFromCurrent()}
              onCopy={(text) => void actions.copyText(text)}
            />
          );
        }

        const agentReasoning = showAgentReasoning ? parseAgentReasoning(message) : null;
        const agentNarration = message.role === "agent" ? parseAgentNarration(message) : null;
        const agentToolActivity = message.role === "agent" ? parseAgentToolActivity(message) : null;
        const messageMetadata = (message.metadata ?? {}) as Record<string, unknown>;
        const isAgentStreaming =
          message.role === "agent"
          && (messageMetadata.streaming === true
            || (runtime.ui_state.submitting
              && messageMetadata.message_kind === "agent_response"
              && !message.metadata?.recovered
              && index === runtime.messages.length - 1));
        const messageTurnIndex = Number(message.metadata?.turn_index ?? 0);
        const canResendPlayerTurn = visualRole === "player" && Number.isInteger(messageTurnIndex) && messageTurnIndex > 0;
        const isEditingThisTurn = runtime.editing?.turnIndex === messageTurnIndex;
        const turnActionsLocked =
          runtime.ui_state.submitting
          || !canResendPlayerTurn
          || (!!runtime.editing && !isEditingThisTurn);
        const speakerLabel = resolveDialogueSpeakerLabel(message, runtime.world_characters);
        const isMobile = runtime.capabilities.platform === "mobile";
        const isWorldInteraction = messageMetadata.message_kind === "world_interaction";

        if (isWorldInteraction) {
          return null;
        }

        const showTimestamp = shouldShowMessageTimestamp(message.created_at, previousVisibleMessageCreatedAt);
        const timestampLabel = showTimestamp ? formatMessageTimestamp(message.created_at) : null;
        previousVisibleMessageCreatedAt = message.created_at;

        return (
          <React.Fragment
            key={`${message.role}-${index}-${message.speaker ?? "none"}-${message.pending ? "pending" : "committed"}`}
          >
          {timestampLabel ? (
            <time className="game-message-timestamp" dateTime={message.created_at}>
              {timestampLabel}
            </time>
          ) : null}
          <div
            data-game-message-row
            className={`game-message-row game-message-row--${visualRole}${message.pending ? " game-message-row--pending" : ""}`}
          >
            <div className={`game-message game-message--${visualRole}${message.pending ? " game-message--pending" : ""} game-ui-message-bubble`} data-variant={visualRole}>
              {visualRole !== "system" ? (
                <div className={`game-message-speaker${visualRole === "player" ? " game-message-speaker--player" : ""} game-ui-message-speaker`} data-variant={visualRole}>
                  {message.pending ? `${speakerLabel} / 发送中` : speakerLabel}
                </div>
              ) : null}
              {agentToolActivity?.status === "calling" ? (
                <div className="game-tool-activity">
                  {agentToolActivity.tools.map((tool, toolIndex) => (
                    <div className="game-tool-activity-item" key={`${tool.id || tool.name}-${toolIndex}`}>
                      <span className="game-tool-activity-dot" aria-hidden="true" />
                      <span>{`正在调用 ${tool.name}${tool.argsPreview ? `（${tool.argsPreview}）` : ""}…`}</span>
                    </div>
                  ))}
                </div>
              ) : null}
              {message.role === "agent" && agentReasoning ? (
                <div className="game-agent-blocks">
                  <CotBlock text={agentReasoning.reasoning} streaming={isAgentStreaming} />
                  <div className="game-agent-answer">
                    <div className="game-agent-answer-label">回复</div>
                    <div className="game-message-content game-message-content--default">
                      <MarkdownMessageContent text={getMessageText(message.content)} streaming={isAgentStreaming} />
                    </div>
                  </div>
                </div>
              ) : (
                <div className={`game-message-content ${visualRole === "system" ? "game-message-content--system" : "game-message-content--default"}`}>
                  {visualRole === "player" ? (
                    getMessageText(message.content)
                  ) : (
                    <MarkdownMessageContent text={getMessageText(message.content)} streaming={isAgentStreaming} />
                  )}
                  {getImageParts(message.content).map((part, partIndex) => (
                    <ImageMessageThumbnail key={`image-${partIndex}`} part={part} />
                  ))}
                  {getAudioParts(message.content).map((part, partIndex) => (
                    <VoiceMessageBubble key={`audio-${partIndex}`} part={part} />
                  ))}
                </div>
              )}
              {agentToolActivity?.status === "done" ? (
                <div className="game-tool-activity game-tool-activity--done">
                  {`已调用：${agentToolActivity.tools.map((tool) => tool.name).join("、")}`}
                </div>
              ) : null}
            </div>

            {!message.pending && isMobile && (visualRole === "agent" || visualRole === "player") ? (
              <div className={`game-message-inline-actions${visualRole === "player" ? " game-message-inline-actions--player" : ""}${visualRole === "agent" ? " game-message-inline-actions--agent" : ""}`}>
                {visualRole === "player" ? (
                  <>
                    <button
                      type="button"
                      className={`game-message-action-btn game-ui-button${isEditingThisTurn ? " game-message-action-btn--active" : ""}`}
                      data-variant="ghost"
                      disabled={turnActionsLocked}
                      onClick={() => {
                        if (!turnActionsLocked) {
                          actions.startEditingTurn(getMessageText(message.content), messageTurnIndex);
                        }
                      }}
                    >
                      {isEditingThisTurn ? "\u7f16\u8f91\u4e2d" : "\u7f16\u8f91"}
                    </button>
                    <button
                      type="button"
                      className="game-message-action-btn game-message-action-btn--resend game-ui-button"
                      data-variant="ghost"
                      disabled={runtime.ui_state.submitting || !canResendPlayerTurn || !!runtime.editing}
                      onClick={() => {
                        if (!(runtime.ui_state.submitting || !canResendPlayerTurn || runtime.editing)) {
                          void actions.submitMessage({
                            content: getMessageText(message.content),
                            turnIndex: messageTurnIndex,
                            mode: "resend",
                          });
                        }
                      }}
                    >
                      {"\u91cd\u53d1"}
                    </button>
                  </>
                ) : null}
                {visualRole === "agent" ? (
                  <>
                    <button type="button" className="game-message-action-btn game-message-action-btn--copy game-ui-button" data-variant="ghost" onClick={() => void actions.copyText(getMessageText(message.content))} aria-label="\u590d\u5236" title="\u590d\u5236">
                      <Copy size={12} />
                    </button>
                    <button
                      type="button"
                      className="game-message-action-btn game-message-action-btn--branch game-ui-button"
                      data-variant="ghost"
                      disabled={runtime.ui_state.branching || runtime.ui_state.loading || !runtime.session}
                      onClick={() => void actions.branchFromCurrent()}
                      aria-label="\u521b\u5efa\u5206\u652f"
                      title="\u521b\u5efa\u5206\u652f"
                    >
                      <GitBranch size={12} />
                    </button>
                  </>
                ) : null}
              </div>
            ) : null}

            {!message.pending && !isMobile && visualRole !== "agent" ? (
              <div className="game-message-actions">
                <button type="button" className="game-message-action-btn game-message-action-btn--copy game-ui-button" data-variant="ghost" onClick={() => void actions.copyText(getMessageText(message.content))} aria-label="复制消息" title="复制消息">
                  <Copy size={12} />
                </button>
                {visualRole === "player" ? (
                  <>
                    <button
                      type="button"
                      className={`game-message-action-btn game-ui-button${isEditingThisTurn ? " game-message-action-btn--active" : ""}`}
                      data-variant="ghost"
                      disabled={turnActionsLocked}
                      onClick={() => {
                        if (!turnActionsLocked) {
                          actions.startEditingTurn(getMessageText(message.content), messageTurnIndex);
                        }
                      }}
                    >
                      {isEditingThisTurn ? "编辑中" : "编辑"}
                    </button>
                    <button
                      type="button"
                      className="game-message-action-btn game-message-action-btn--resend game-ui-button"
                      data-variant="ghost"
                      disabled={runtime.ui_state.submitting || !canResendPlayerTurn || !!runtime.editing}
                      onClick={() => {
                        if (!(runtime.ui_state.submitting || !canResendPlayerTurn || runtime.editing)) {
                          void actions.submitMessage({
                            content: getMessageText(message.content),
                            turnIndex: messageTurnIndex,
                            mode: "resend",
                          });
                        }
                      }}
                    >
                      重发
                    </button>
                  </>
                ) : null}
              </div>
            ) : null}

            {!message.pending && !isMobile && visualRole === "agent" ? (
              <div className="game-message-actions game-message-actions--agent">
                <button type="button" className="game-message-action-btn game-message-action-btn--copy game-ui-button" data-variant="ghost" onClick={() => void actions.copyText(getMessageText(message.content))} aria-label="复制回复" title="复制回复">
                  <Copy size={12} />
                </button>
                <button
                  type="button"
                  className="game-message-action-btn game-message-action-btn--branch game-ui-button"
                  data-variant="ghost"
                  disabled={runtime.ui_state.branching || runtime.ui_state.loading || !runtime.session}
                  onClick={() => void actions.branchFromCurrent()}
                  aria-label="创建分支"
                  title="创建分支"
                >
                  <GitBranch size={12} />
                </button>
              </div>
            ) : null}
          </div>
          {agentNarration ? (
            <div className="game-message-row game-message-row--narration">
              <div className="game-narration">{agentNarration}</div>
            </div>
          ) : null}
          </React.Fragment>
        );
      })}
      {showTypingIndicator && runtime.ui_state.submitting && !runtime.ui_state.streaming_response_active && !hasActiveAgentStream && (() => {
        const lastAgent = [...runtime.messages].reverse().find((m) => m.role === "agent");
        const speakerName = lastAgent?.speaker || runtime.session?.player_character_name || "";
        return <TypingIndicator speakerName={speakerName} />;
      })()}
      </div>
      {showScrollControls ? (
        <div className="game-chat-scroll-controls" aria-label="聊天记录滚动控制">
          {showScrollTopButton ? (
            <button
              type="button"
              className="game-chat-scroll-btn game-chat-scroll-btn--top game-ui-button"
              data-scroll-action="top"
              data-variant="ghost"
              disabled={scrollState.atTop}
              onClick={() => scrollToPosition(0)}
              aria-label="滚动到顶部"
              title="滚动到顶部"
            >
              <ArrowUpToLine size={16} aria-hidden="true" />
            </button>
          ) : null}
          {showScrollUpButton ? (
            <button
              type="button"
              className="game-chat-scroll-btn game-chat-scroll-btn--up game-ui-button"
              data-scroll-action="up"
              data-variant="ghost"
              disabled={scrollState.atTop}
              onClick={() => scrollOneMessage("up")}
              aria-label="向上滚动一条消息"
              title="向上滚动一条消息"
            >
              <ChevronUp size={16} aria-hidden="true" />
            </button>
          ) : null}
          {showScrollDownButton ? (
            <button
              type="button"
              className="game-chat-scroll-btn game-chat-scroll-btn--down game-ui-button"
              data-scroll-action="down"
              data-variant="ghost"
              disabled={scrollState.atBottom}
              onClick={() => scrollOneMessage("down")}
              aria-label="向下滚动一条消息"
              title="向下滚动一条消息"
            >
              <ChevronDown size={16} aria-hidden="true" />
            </button>
          ) : null}
          {showScrollBottomButton ? (
            <button
              type="button"
              className="game-chat-scroll-btn game-chat-scroll-btn--bottom game-ui-button"
              data-scroll-action="bottom"
              data-variant="ghost"
              disabled={scrollState.atBottom}
              onClick={() => scrollToPosition(Number.MAX_SAFE_INTEGER)}
              aria-label="滚动到底部"
              title="滚动到底部"
            >
              <ArrowDownToLine size={16} aria-hidden="true" />
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
