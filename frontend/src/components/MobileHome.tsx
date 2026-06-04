import { useNavigate } from "react-router-dom";
import { MobileButton } from "./MobileLayout";
import { Bookmark, Layers, Globe, Server, Settings, ChevronRight } from "lucide-react";

/**
 * 移动端首页内容 - iOS风格
 */
export function MobileHome() {
  const navigate = useNavigate();

  return (
    <div style={{ padding: "20px 0" }}>
      {/* 欢迎区域 */}
      <div style={{ padding: "0 16px 24px", textAlign: "center" }}>
        <div style={{ fontSize: "32px", fontWeight: "700", color: "var(--ios-text)", letterSpacing: "-1px" }}>
          梦境叙事
        </div>
        <div style={{ fontSize: "15px", color: "var(--ios-text-secondary)", marginTop: "8px" }}>
          沉浸式叙事游戏引擎
        </div>
      </div>

      {/* 快捷操作 */}
      <div className="ios-list ios-list--inset">
        <button
          type="button"
          className="ios-list-item ios-list-item--has-icon"
          onClick={() => navigate("/saves")}
        >
          <div className="ios-list-item-icon ios-list-item-icon--orange">
            <Bookmark size={17} />
          </div>
          <div className="ios-list-item-content">
            <div className="ios-list-item-title">继续故事</div>
            <div className="ios-list-item-subtitle">从存档继续你的冒险</div>
          </div>
          <span className="ios-list-item-arrow">
            <ChevronRight size={14} />
          </span>
        </button>
        <button
          type="button"
          className="ios-list-item ios-list-item--has-icon"
          onClick={() => navigate("/new-game")}
        >
          <div className="ios-list-item-icon ios-list-item-icon--blue">
            <Layers size={17} />
          </div>
          <div className="ios-list-item-content">
            <div className="ios-list-item-title">新的游戏</div>
            <div className="ios-list-item-subtitle">开始全新的冒险旅程</div>
          </div>
          <span className="ios-list-item-arrow">
            <ChevronRight size={14} />
          </span>
        </button>
      </div>

      {/* 功能入口 */}
      <div className="ios-list-header">功能</div>
      <div className="ios-list ios-list--inset">
        <button
          type="button"
          className="ios-list-item ios-list-item--has-icon"
          onClick={() => navigate("/worlds")}
        >
          <div className="ios-list-item-icon ios-list-item-icon--green">
            <Globe size={17} />
          </div>
          <div className="ios-list-item-content">
            <div className="ios-list-item-title">世界设定</div>
          </div>
          <span className="ios-list-item-arrow">
            <ChevronRight size={14} />
          </span>
        </button>
        <button
          type="button"
          className="ios-list-item ios-list-item--has-icon"
          onClick={() => navigate("/mcp-tools")}
        >
          <div className="ios-list-item-icon ios-list-item-icon--purple">
            <Server size={17} />
          </div>
          <div className="ios-list-item-content">
            <div className="ios-list-item-title">MCP 工具</div>
          </div>
          <span className="ios-list-item-arrow">
            <ChevronRight size={14} />
          </span>
        </button>
        <button
          type="button"
          className="ios-list-item ios-list-item--has-icon"
          onClick={() => navigate("/settings")}
        >
          <div className="ios-list-item-icon ios-list-item-icon--gray">
            <Settings size={17} />
          </div>
          <div className="ios-list-item-content">
            <div className="ios-list-item-title">设置</div>
          </div>
          <span className="ios-list-item-arrow">
            <ChevronRight size={14} />
          </span>
        </button>
      </div>

      {/* 提示 */}
      <div style={{ padding: "24px 16px", textAlign: "center" }}>
        <p style={{ fontSize: "13px", color: "var(--ios-text-tertiary)", margin: 0, lineHeight: 1.5 }}>
          点击左上角菜单按钮可随时切换页面
        </p>
      </div>
    </div>
  );
}
