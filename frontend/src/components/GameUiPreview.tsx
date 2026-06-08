import type { CSSProperties, ReactNode } from "react";
import {
  GameUiRenderer,
  type GameUiComponentRenderer,
} from "./GameUiRenderer";
import {
  resolveSidePanelTabOrder,
  type GameUiDocument,
  type GameUiMountId,
  type GameUiPlatform,
} from "../data/gameUi";

type PreviewMessage = {
  role: string;
  content: string;
  speaker?: string | null;
};

type PreviewStatusTab = {
  key: string;
  label: string;
  content: string;
};

type GameUiPreviewProps = {
  platform: GameUiPlatform;
  document: GameUiDocument;
  stylesheet?: string;
  scopeId?: string;
  rootStyle?: CSSProperties;
  worldName: string;
  location: string;
  timeLabel: string;
  playerName: string;
  visibleCharacters: string[];
  focusSpeaker?: string;
  focusContent?: string;
  portraitSrc?: string;
  narration?: string;
  messages: PreviewMessage[];
  statusTabs?: PreviewStatusTab[];
  parseError?: string | null;
  usedFallback?: boolean;
};

export function GameUiPreview({
  platform,
  document,
  stylesheet,
  scopeId,
  rootStyle,
  worldName,
  location,
  timeLabel,
  playerName,
  visibleCharacters,
  focusSpeaker,
  focusContent,
  portraitSrc,
  narration,
  messages,
  statusTabs = [],
  parseError,
  usedFallback = false,
}: GameUiPreviewProps) {
  const orderedStatusTabs = resolveSidePanelTabOrder(document, statusTabs);

  const mounts: Partial<Record<GameUiMountId, ReactNode>> = {
    header: (
      <div className="game-simple-top game-ui-panel">
        <div className="game-simple-top-main">
          <div className="game-simple-world">{worldName}</div>
          <div className="game-simple-place-row">
            <span className="game-simple-place">{location}</span>
          </div>
        </div>
        <div className="game-simple-meta game-simple-meta-row">
          <div className="game-simple-meta-item">
            <strong>时间</strong>
            <span>{timeLabel}</span>
          </div>
          <div className="game-simple-meta-item">
            <strong>玩家</strong>
            <span>{playerName}</span>
          </div>
          {usedFallback || parseError ? (
            <div className="game-simple-meta-item">
              <strong>界面</strong>
              <span>{usedFallback ? "使用回退配置" : "已加载"}</span>
            </div>
          ) : null}
        </div>
      </div>
    ),
    scene: (
      <div className="game-ui-panel">
        <div className="game-scene-header">
          <div className="game-scene-header-main">
            <h2 className="game-scene-name">{location}</h2>
            <span className="game-scene-time">{timeLabel}</span>
          </div>
          <div className="game-scene-header-side">
            <span className="game-scene-badge game-ui-badge" data-variant="info">
              {`当前玩家：${playerName}`}
            </span>
          </div>
        </div>
      </div>
    ),
    scene_focus: focusSpeaker && focusContent ? (
      <div className="game-scene-center">
        <div className={`game-avatar game-ui-avatar${portraitSrc ? " game-avatar--image" : ""}`} data-variant="focus">
          {portraitSrc ? <img src={portraitSrc} alt={focusSpeaker} className="game-avatar-image" /> : focusSpeaker}
        </div>
        <div className="game-current-line">{focusContent}</div>
      </div>
    ) : null,
    character_bar: visibleCharacters.length > 0 ? (
      <div className="game-scene-characters">
        {visibleCharacters.map((name) => (
          <span key={name} className="game-scene-char game-ui-chip" data-variant="character">
            {name}
          </span>
        ))}
      </div>
    ) : null,
    narration: (
      <div className="game-narration-panel game-ui-panel">
        <div className="game-narration-label">旁白</div>
        <div className="game-narration-content">{narration || "暂无旁白。"}</div>
      </div>
    ),
    message_list: (
      <div className="game-chat-messages game-chat-messages--simple">
        {messages.map((message, index) => (
          <div key={`${message.role}-${index}-${message.speaker ?? "none"}`} className={`game-message-row game-message-row--${message.role}`}>
            <div className={`game-message game-message--${message.role} game-ui-message-bubble`} data-variant={message.role}>
              {message.role !== "system" ? (
                <div className={`game-message-speaker${message.role === "player" ? " game-message-speaker--player" : ""} game-ui-message-speaker`} data-variant={message.role}>
                  {message.speaker?.trim() || (message.role === "player" ? "玩家" : "角色")}
                </div>
              ) : null}
              <div className={`game-message-content ${message.role === "system" ? "game-message-content--system" : "game-message-content--default"}`}>
                {message.content}
              </div>
            </div>
          </div>
        ))}
      </div>
    ),
    side_panel: (
      <div className="game-status">
        {orderedStatusTabs.length > 0 ? (
          <>
            <div className="game-tabs">
              {orderedStatusTabs.map((tab, index) => (
                <button
                  key={tab.key}
                  type="button"
                  className={`game-tab game-ui-button${index === 0 ? " game-tab--active" : ""}`}
                  data-variant={index === 0 ? "primary" : "ghost"}
                  disabled
                >
                  {tab.label}
                </button>
              ))}
            </div>
            <div className="game-panel game-side-content game-ui-panel" data-variant="sidebar">
              <div className="game-card game-attribute-tab-content">
                {orderedStatusTabs[0]?.content || "暂无内容。"}
              </div>
            </div>
          </>
        ) : (
          <div className="game-panel game-side-content game-ui-panel" data-variant="sidebar">
            <div className="game-card">未配置侧边栏标签。</div>
          </div>
        )}
      </div>
    ),
    input_area: (
      <div className="game-input-area game-ui-panel">
        <div className="game-input-compose">
          <textarea
            className="game-textarea game-ui-textarea"
            readOnly
            value={"在这里输入消息或行动。\n按 Shift + Enter 换行。"}
          />
          <button type="button" className="game-submit-btn game-submit-btn--inline game-ui-button" data-variant="primary" disabled>
            发送
          </button>
        </div>
      </div>
    ),
    floating_actions: (
      <div className="game-simple-actions">
        <span className="game-scene-badge game-ui-badge" data-variant="info">
          {platform === "desktop" ? "桌面界面" : "移动界面"}
        </span>
      </div>
    ),
  };

  const componentRenderers: Partial<Record<string, GameUiComponentRenderer>> = {
    scene_header: () => mounts.header ?? null,
    scene_focus: () => mounts.scene_focus ?? null,
    character_bar: () => mounts.character_bar ?? null,
    narration_card: () => mounts.narration ?? null,
    message_list: () => mounts.message_list ?? null,
    input_composer: () => mounts.input_area ?? null,
    side_panel_tabs: () => mounts.side_panel ?? null,
    floating_actions: () => mounts.floating_actions ?? null,
  };

  return (
    <div className="game-root game-root--preview world-style-preview-root game-ui-root" data-game-ui-scope={scopeId} style={rootStyle}>
      {stylesheet ? <style>{stylesheet}</style> : null}
      {parseError ? <div className="game-input-bubble">{parseError}</div> : null}
      <GameUiRenderer
        document={document}
        mounts={mounts}
        componentRenderers={componentRenderers}
      />
    </div>
  );
}
