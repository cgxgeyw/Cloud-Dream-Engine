import React, { useState } from "react";
import {
  type RenderChatMessage,
  type CharacterCreationView,
  type SwitchProposalView,
  type DirectorTraceView,
  type DirectorRetryCardView,
  type StructuredErrorView,
  type AgentReasoningView,
  type EditingTurnState,
  parseCharacterCreationMessage,
  parseSwitchProposal,
  parseDirectorRetryCard,
  parseStructuredError,
  parseDirectorTrace,
  parseAgentReasoning,
  shouldHidePinnedNarrationMessage,
  isMessageReasoningExpanded,
} from "../game/utils";

/* ============================================================
   CotBlock — 思维链：始终只展示前 200 字，剩余折叠
   ============================================================ */
export const COT_PREVIEW_LIMIT = 200;

export const CotBlock: React.FC<{
  text: string;
  label?: string;
  streaming?: boolean;
}> = ({ text, label = "思维链", streaming = false }) => {
  const [expanded, setExpanded] = useState(false);
  const normalized = text.trim();
  if (!normalized) {
    return null;
  }
  // 流式进行中时全量显示，让思维链逐字增长可见；流式结束后恢复 200 字折叠。
  const overflow = !streaming && normalized.length > COT_PREVIEW_LIMIT;
  const shown = streaming || expanded || !overflow ? normalized : normalized.slice(0, COT_PREVIEW_LIMIT);

  return (
    <div className="game-cot" data-streaming={streaming ? "true" : "false"}>
      <div className="game-cot-header">
        <span className="game-cot-label">{label}</span>
        {overflow ? (
          <button
            type="button"
            className="game-cot-toggle"
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? "收起" : "展开"}
          </button>
        ) : null}
      </div>
      <div className="game-cot-content">
        {shown}
        {overflow && !expanded ? <span className="game-cot-ellipsis">…</span> : null}
      </div>
    </div>
  );
};

/* ============================================================
   RenderDirectorTrace
   ============================================================ */
export const RenderDirectorTrace: React.FC<{
  trace: DirectorTraceView;
}> = ({ trace }) => {
  const visibleTraceLines = trace.traceLines.length
    ? trace.traceLines
    : trace.traceText.split("\n").map((line) => line.trim()).filter(Boolean);

  return (
    <div className="game-director-trace" data-streaming={trace.reasoningExpanded ? "true" : "false"}>
      <div className="game-director-trace-title">
        {trace.reasoningExpanded ? "世界主控正在思考…" : "世界主控思维链"}
      </div>
      {trace.reasoning ? <CotBlock text={trace.reasoning} streaming={trace.reasoningExpanded} /> : null}
      <div className="game-director-trace-block">
        <div className="game-director-trace-label">正文</div>
        <div className="game-director-trace-lines">
          {visibleTraceLines.length ? visibleTraceLines.map((line, i) => (
            <div key={`${trace.key}-${i}`} className="game-director-trace-line">{line}</div>
          )) : (
            <div className="game-director-trace-line game-director-trace-line--pending">
              {trace.reasoningExpanded ? "世界主控正在输出正文..." : "当前还没有正文输出"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

/* ============================================================
   RenderCharacterCreation
   ============================================================ */
export const RenderCharacterCreation: React.FC<{
  creation: CharacterCreationView;
}> = ({ creation }) => (
  <div className="game-system-action game-system-action--creation game-ui-panel" data-variant="system">
    <div className="game-system-action-header">
      <div>
        <div className="game-system-action-kicker">新角色</div>
        <div className="game-system-action-title">{creation.characterName}</div>
      </div>
    </div>
  </div>
);

/* ============================================================
   Props for RenderSwitchProposal
   ============================================================ */
export interface RenderSwitchProposalProps {
  proposal: SwitchProposalView;
  switching: boolean;
  onAccept: (proposal: SwitchProposalView) => void;
  onDismiss: (key: string) => void;
}

/* ============================================================
   RenderSwitchProposal
   ============================================================ */
export const RenderSwitchProposal: React.FC<RenderSwitchProposalProps> = ({
  proposal,
  switching,
  onAccept,
  onDismiss,
}) => (
  <div className="game-system-action game-system-action--switch game-ui-panel" data-variant="system">
    <div className="game-system-action-header">
      <div>
        <div className="game-system-action-kicker">世界主控建议</div>
        <div className="game-system-action-title">切换至：{proposal.targetCharacterName}</div>
      </div>
      <div className="game-system-action-badges">
        {proposal.targetCreatedInTurn ? (
          <span className="game-system-action-badge game-system-action-badge--strong game-ui-badge" data-variant="strong">
            本回合新建
          </span>
        ) : null}
        <span className="game-system-action-badge game-ui-badge" data-variant="neutral">等待确认</span>
      </div>
    </div>
    <div className="game-system-action-text">{proposal.reason}</div>
    <div className="game-system-action-grid">
      {proposal.targetRole ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">身份</span>
          <span className="game-system-action-item-value">{proposal.targetRole}</span>
        </div>
      ) : null}
      {proposal.proposal.scene_name ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">目标场景</span>
          <span className="game-system-action-item-value">{proposal.proposal.scene_name}</span>
        </div>
      ) : null}
      {proposal.proposal.location ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">目标地点</span>
          <span className="game-system-action-item-value">{proposal.proposal.location}</span>
        </div>
      ) : null}
      {proposal.proposal.visible_characters?.length ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">目标场景人物</span>
          <span className="game-system-action-item-value">
            {proposal.proposal.visible_characters.join(" / ")}
          </span>
        </div>
      ) : null}
    </div>
    {proposal.targetBackgroundPrompt ? (
      <div className="game-system-action-text">
        <div className="game-system-action-item-label">角色提示词</div>
        <div>{proposal.targetBackgroundPrompt}</div>
      </div>
    ) : null}
    <div className="game-system-action-actions">
      <button
        type="button"
        className="game-switch-proposal-accept game-ui-button"
        data-variant="primary"
        disabled={switching}
        onClick={() => onAccept(proposal)}
      >
        {switching ? "切换中..." : "确认切换"}
      </button>
      <button
        type="button"
        className="game-switch-proposal-reject game-ui-button"
        data-variant="danger"
        disabled={switching}
        onClick={() => onDismiss(proposal.key)}
      >
        拒绝
      </button>
    </div>
  </div>
);

/* ============================================================
   Props for RenderDirectorRetryCard
   ============================================================ */
export interface RenderDirectorRetryCardProps {
  card: DirectorRetryCardView;
  retryingToken: string | null;
  branching: boolean;
  loading: boolean;
  hasSession: boolean;
  onRetry: (retryToken: string) => void;
  onBranch: () => void;
  onDismiss: (key: string) => void;
}

/* ============================================================
   RenderDirectorRetryCard
   ============================================================ */
export const RenderDirectorRetryCard: React.FC<RenderDirectorRetryCardProps> = ({
  card,
  retryingToken,
  branching,
  loading,
  hasSession,
  onRetry,
  onBranch,
  onDismiss,
}) => (
  <div className="game-system-action game-system-action--switch game-ui-panel" data-variant="system">
    <div className="game-system-action-header">
      <div>
        <div className="game-system-action-kicker">世界主控异常</div>
        <div className="game-system-action-title">{card.title}</div>
      </div>
      <div className="game-system-action-badges">
        <span className="game-system-action-badge game-system-action-badge--strong game-ui-badge" data-variant="strong">
          等待处理
        </span>
      </div>
    </div>
    <div className="game-system-action-text">{card.summary}</div>
    <div className="game-system-action-grid">
      {card.failureStage ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">失败阶段</span>
          <span className="game-system-action-item-value">{card.failureStage}</span>
        </div>
      ) : null}
      {card.provider ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">模型提供商</span>
          <span className="game-system-action-item-value">{card.provider}</span>
        </div>
      ) : null}
      {card.modelId ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">模型</span>
          <span className="game-system-action-item-value">{card.modelId}</span>
        </div>
      ) : null}
    </div>
    {card.repairSummary ? (
      <div className="game-system-action-text">
        <div className="game-system-action-item-label">修复摘要</div>
        <div>{card.repairSummary}</div>
      </div>
    ) : null}
    <div className="game-system-action-actions">
      <button
        type="button"
        className="game-switch-proposal-accept game-ui-button"
        data-variant="primary"
        disabled={retryingToken === card.retryToken}
        onClick={() => onRetry(card.retryToken)}
      >
        {retryingToken === card.retryToken ? "重发中..." : "重发"}
      </button>
      <button
        type="button"
        className="game-message-action-btn game-ui-button"
        data-variant="ghost"
        disabled={branching || loading || !hasSession}
        onClick={onBranch}
      >
        分支
      </button>
      <button
        type="button"
        className="game-switch-proposal-reject game-ui-button"
        data-variant="danger"
        disabled={retryingToken === card.retryToken}
        onClick={() => onDismiss(card.key)}
      >
        关闭
      </button>
    </div>
  </div>
);

/* ============================================================
   Props for RenderStructuredError
   ============================================================ */
export interface RenderStructuredErrorProps {
  error: StructuredErrorView;
  retryingToken: string | null;
  branching: boolean;
  loading: boolean;
  hasSession: boolean;
  onRetry: (retryToken: string) => void;
  onBranch: () => void;
  onCopy: (text: string) => void;
}

/* ============================================================
   RenderStructuredError
   ============================================================ */
export const RenderStructuredError: React.FC<RenderStructuredErrorProps> = ({
  error: err,
  retryingToken,
  branching,
  loading,
  hasSession,
  onRetry,
  onBranch,
  onCopy,
}) => (
  <div className="game-system-action game-system-action--creation game-ui-panel" data-variant="system">
    <div className="game-system-action-header">
      <div>
        <div className="game-system-action-kicker">结构化输出异常</div>
        <div className="game-system-action-title">{err.title}</div>
      </div>
      <div className="game-system-action-badges">
        <span className="game-system-action-badge game-ui-badge" data-variant="neutral">回合暂停</span>
      </div>
    </div>
    <div className="game-system-action-text">{err.summary}</div>
    <div className="game-system-action-grid">
      {err.speakerName ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">角色</span>
          <span className="game-system-action-item-value">{err.speakerName}</span>
        </div>
      ) : null}
      {err.failureStage ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">失败阶段</span>
          <span className="game-system-action-item-value">{err.failureStage}</span>
        </div>
      ) : null}
      {err.modelId ? (
        <div className="game-system-action-item">
          <span className="game-system-action-item-label">模型</span>
          <span className="game-system-action-item-value">{err.modelId}</span>
        </div>
      ) : null}
    </div>
    {err.repairSummary ? (
      <div className="game-system-action-text">
        <div className="game-system-action-item-label">修复摘要</div>
        <div>{err.repairSummary}</div>
      </div>
    ) : null}
    <div className="game-message-actions game-message-actions--agent">
      <button type="button" className="game-message-action-btn game-message-action-btn--copy game-ui-button" data-variant="ghost" onClick={() => onCopy(`${err.title}\n${err.summary}`)} aria-label="复制这条错误信息" title="复制">
        {/* Inline SVG: Copy */}
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
      </button>
      <button type="button" className="game-message-action-btn game-message-action-btn--branch game-ui-button" data-variant="ghost" disabled={branching || loading || !hasSession} onClick={onBranch} aria-label="从当前失败状态创建分支" title="分支">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>
      </button>
      {err.retryToken ? (
        <button type="button" className="game-message-action-btn game-message-action-btn--resend game-ui-button" data-variant="ghost" disabled={retryingToken === err.retryToken} onClick={() => onRetry(err.retryToken!)}>
          {retryingToken === err.retryToken ? "重发中..." : "重发"}
        </button>
      ) : null}
    </div>
  </div>
);
