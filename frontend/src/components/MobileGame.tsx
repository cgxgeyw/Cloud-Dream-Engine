import { useState, useRef, useEffect } from "react";
import type { ChatMessageResponse, SessionSnapshotResponse, CharacterResponse } from "../data/apiAdapter";
import type { ContentPart } from "../data/types";
import { Send, ChevronLeft, MoreHorizontal, GitBranch } from "lucide-react";

function getMessageText(content: string | ContentPart[]): string {
  if (typeof content === "string") {
    return content;
  }
  return content
    .filter((part) => part.type === "text")
    .map((part) => (part as { type: "text"; text: string }).text)
    .join("\n");
}

// 游戏页面属性
interface MobileGameProps {
  session: SessionSnapshotResponse | null;
  messages: ChatMessageResponse[];
  playerCharacter: CharacterResponse | null;
  inputValue: string;
  onInputChange: (value: string) => void;
  onSubmit: () => void;
  onBranch: () => void;
  loading: boolean;
  submitting: boolean;
  branching: boolean;
  error: string | null;
  actionError: string | null;
}

/**
 * 移动端游戏页面 - iOS风格
 */
export function MobileGame({
  session,
  messages,
  playerCharacter,
  inputValue,
  onInputChange,
  onSubmit,
  onBranch,
  loading,
  submitting,
  branching,
  error,
  actionError,
}: MobileGameProps) {
  const chatContainerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [showActions, setShowActions] = useState(false);

  // 自动滚动到底部
  useEffect(() => {
    if (chatContainerRef.current) {
      chatContainerRef.current.scrollTop = chatContainerRef.current.scrollHeight;
    }
  }, [messages]);

  // 自动调整输入框高度
  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.style.height = "auto";
      inputRef.current.style.height = `${Math.min(inputRef.current.scrollHeight, 100)}px`;
    }
  }, [inputValue]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      onSubmit();
    }
  };

  // 过滤和排序消息
  const visibleMessages = messages.filter((msg) => {
    if (msg.role === "system") {
      const actionType = msg.metadata?.action_type as string;
      return actionType === "switch_character" || actionType === "character_created";
    }
    return true;
  }) as Array<Omit<ChatMessageResponse, "metadata"> & {
    metadata?: Record<string, string | number | boolean | null | undefined> | null;
  }>;

  if (loading) {
    return (
      <div className="ios-game">
        <div className="ios-loading" style={{ minHeight: "100vh" }}>
          <div className="ios-spinner" />
          <p style={{ marginTop: "16px", color: "rgba(255,255,255,0.6)" }}>正在加载会话...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="ios-game">
        <div className="ios-empty" style={{ minHeight: "100vh" }}>
          <p style={{ color: "#ff3b30" }}>加载失败：{error}</p>
        </div>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="ios-game">
        <div className="ios-empty" style={{ minHeight: "100vh" }}>
          <p style={{ color: "rgba(255,255,255,0.6)" }}>会话不存在</p>
        </div>
      </div>
    );
  }

  return (
    <div className="ios-game">
      {/* 游戏头部信息 */}
      <header className="ios-game-header">
        <button
          type="button"
          className="ios-nav-btn"
          onClick={() => window.history.back()}
          style={{ color: "white" }}
        >
          <ChevronLeft size={22} />
        </button>
        <div className="ios-game-info">
          <div className="ios-game-world">{session.world_name}</div>
          <div className="ios-game-location">{session.location || "当前场景"}</div>
        </div>
        <button
          type="button"
          className="ios-nav-btn"
          onClick={() => setShowActions(!showActions)}
          style={{ color: "white" }}
        >
          <MoreHorizontal size={22} />
        </button>
      </header>

      {/* 操作菜单 */}
      {showActions && (
        <div style={{
          position: "fixed",
          top: "calc(56px + var(--ios-safe-top))",
          right: "16px",
          background: "rgba(30, 30, 30, 0.95)",
          borderRadius: "12px",
          padding: "8px",
          zIndex: 101,
          backdropFilter: "blur(20px)",
        }}>
          <button
            type="button"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              padding: "12px 16px",
              background: "transparent",
              border: "none",
              color: "white",
              fontSize: "15px",
              cursor: "pointer",
              borderRadius: "8px",
            }}
            onClick={() => {
              onBranch();
              setShowActions(false);
            }}
            disabled={branching}
          >
            <GitBranch size={18} />
            <span>{branching ? "创建中..." : "创建分支"}</span>
          </button>
        </div>
      )}

      {/* 聊天消息区域 */}
      <div className="ios-game-chat" ref={chatContainerRef}>
        {visibleMessages.length === 0 ? (
          <div className="ios-message ios-message--system">
            会话刚刚开始...
          </div>
        ) : (
          visibleMessages.map((message, index) => {
            const isPlayer = message.role === "player";
            const isSystem = message.role === "system";
            const speaker = message.speaker || (isPlayer ? "玩家" : "角色");

            if (isSystem) {
              const metadata = message.metadata as Record<string, string | number | boolean | null | undefined> | undefined;
              const actionType = metadata?.action_type as string;
              if (actionType === "switch_character") {
                return (
                  <div key={index} className="ios-message ios-message--system">
                    建议切换至：{message.metadata?.target_character_name}
                  </div>
                );
              }
              if (actionType === "character_created") {
                return (
                  <div key={index} className="ios-message ios-message--system">
                    新角色加入：{message.metadata?.character_name}
                  </div>
                );
              }
              return null;
            }

            return (
              <div
                key={index}
                className={`ios-message ${isPlayer ? "ios-message--player" : "ios-message--agent"}`}
              >
                <div className="ios-message-speaker">{speaker}</div>
                <div>{getMessageText(message.content)}</div>
              </div>
            );
          })
        )}
      </div>

      {/* 输入区域 */}
      <div className="ios-game-input">
        {actionError && (
          <div style={{
            background: "rgba(255, 59, 48, 0.9)",
            color: "white",
            padding: "10px 16px",
            borderRadius: "10px",
            marginBottom: "10px",
            fontSize: "14px",
          }}>
            {actionError}
          </div>
        )}
        <div className="ios-game-input-box">
          <textarea
            ref={inputRef}
            className="ios-game-input-field"
            value={inputValue}
            onChange={(e) => onInputChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入发言或行动..."
            rows={1}
          />
          <button
            type="button"
            className="ios-game-send"
            onClick={onSubmit}
            disabled={submitting || !inputValue.trim()}
          >
            <Send size={18} />
          </button>
        </div>
      </div>
    </div>
  );
}
