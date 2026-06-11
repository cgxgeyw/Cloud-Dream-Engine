import { useEffect } from "react";
import { Copy, GitBranch } from "lucide-react";
import type { ContentPart } from "../../data/types";
import type { GameUiComponentNode } from "../../data/gameUi";
import {
  RenderCharacterCreation,
  RenderDirectorRetryCard,
  RenderDirectorTrace,
  RenderStructuredError,
  RenderSwitchProposal,
} from "../../game/MessageCards";
import {
  isMessageReasoningExpanded,
  parseAgentReasoning,
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

type MessageListComponentProps = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
};

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
  const mobileSimple = readBooleanProp(node, "mobile_simple", false) && runtime.capabilities.platform === "mobile";

  useEffect(() => {
    runtime.message_preferences.set_auto_scroll_enabled(autoScroll);
  }, [autoScroll, runtime.message_preferences.set_auto_scroll_enabled]);

  return (
    <div className="game-chat-messages" ref={runtime.chat_messages_ref}>
      {runtime.messages.map((message, index) => {
        if (!showPendingState && message.pending) {
          return null;
        }

        if (shouldHidePinnedNarrationMessage(message, runtime.latest_narration)) {
          return null;
        }

        if (mobileSimple && !["system", "agent", "player"].includes(message.role)) {
          return null;
        }

        const directorTrace = parseDirectorTrace(message);
        if (directorTrace) {
          if (mobileSimple) {
            return null;
          }
          return (
            <RenderDirectorTrace
              key={directorTrace.key}
              trace={directorTrace}
              expandedKeys={runtime.message_state.expanded_director_trace_keys}
              setExpandedKeys={runtime.message_state.set_expanded_director_trace_keys}
            />
          );
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
            return null;
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
            return null;
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

        const agentReasoning = showAgentReasoning && !mobileSimple ? parseAgentReasoning(message) : null;
        const messageTurnIndex = Number(message.metadata?.turn_index ?? 0);
        const canResendPlayerTurn = message.role === "player" && Number.isInteger(messageTurnIndex) && messageTurnIndex > 0;
        const isEditingThisTurn = runtime.editing?.turnIndex === messageTurnIndex;
        const turnActionsLocked =
          runtime.ui_state.submitting
          || !canResendPlayerTurn
          || (!!runtime.editing && !isEditingThisTurn);
        const speakerLabel = resolveDialogueSpeakerLabel(message, runtime.world_characters);
        const isMobile = runtime.capabilities.platform === "mobile";

        return (
          <div
            key={`${message.role}-${index}-${message.speaker ?? "none"}-${message.pending ? "pending" : "committed"}`}
            className={`game-message-row game-message-row--${message.role}${message.pending ? " game-message-row--pending" : ""}`}
          >
            <div className={`game-message game-message--${message.role}${message.pending ? " game-message--pending" : ""} game-ui-message-bubble`} data-variant={message.role}>
              {message.role !== "system" ? (
                <div className={`game-message-speaker${message.role === "player" ? " game-message-speaker--player" : ""} game-ui-message-speaker`} data-variant={message.role}>
                  {message.pending ? `${speakerLabel} / 发送中` : speakerLabel}
                </div>
              ) : null}
              {message.role === "agent" && agentReasoning ? (
                <div className="game-agent-blocks">
                  <details className="game-agent-reasoning" open={isMessageReasoningExpanded(message)}>
                    <summary className="game-agent-reasoning-summary">
                      <span>推理</span>
                      <span className="game-agent-reasoning-summary-meta">
                        {isMessageReasoningExpanded(message) ? "生成中" : "展开"}
                      </span>
                    </summary>
                    <div className="game-agent-reasoning-content">
                      {agentReasoning.reasoningLines.map((line, reasoningIndex) => (
                        <div key={`${message.speaker ?? "agent"}-reasoning-${reasoningIndex}`} className="game-agent-reasoning-line">
                          {line}
                        </div>
                      ))}
                    </div>
                  </details>
                  <div className="game-agent-answer">
                    <div className="game-agent-answer-label">回复</div>
                    <div className="game-message-content game-message-content--default">{getMessageText(message.content)}</div>
                  </div>
                </div>
              ) : (
                <div className={`game-message-content ${message.role === "system" ? "game-message-content--system" : "game-message-content--default"}`}>
                  {getMessageText(message.content)}
                </div>
              )}
            </div>

            {!message.pending && isMobile && (message.role === "agent" || message.role === "player") ? (
              <div className={`game-message-inline-actions${message.role === "player" ? " game-message-inline-actions--player" : ""}${message.role === "agent" ? " game-message-inline-actions--agent" : ""}`}>
                {message.role === "player" ? (
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
                {message.role === "agent" ? (
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

            {!message.pending && !isMobile && message.role !== "agent" ? (
              <div className="game-message-actions">
                <button type="button" className="game-message-action-btn game-message-action-btn--copy game-ui-button" data-variant="ghost" onClick={() => void actions.copyText(getMessageText(message.content))} aria-label="复制消息" title="复制消息">
                  <Copy size={12} />
                </button>
                {message.role === "player" ? (
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

            {!message.pending && !isMobile && message.role === "agent" ? (
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
        );
      })}
      {runtime.ui_state.submitting && (() => {
        const lastAgent = [...runtime.messages].reverse().find((m) => m.role === "agent");
        const speakerName = lastAgent?.speaker || runtime.session?.player_character_name || "";
        return <TypingIndicator speakerName={speakerName} />;
      })()}
    </div>
  );
}
