import { useNavigate } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import { MenuButton, ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { Play, Save, Globe, Settings, Wrench } from "lucide-react";
import type { ReactNode } from "react";

type MenuItem = {
  title: string;
  description: string;
  to: string;
  icon: ReactNode;
  primary?: boolean;
};

const desktopMenuItems: MenuItem[] = [
  {
    title: "开始冒险",
    description: "选择一个世界，设定你的角色，开启全新的故事旅程。",
    to: "/new-game",
    icon: <Play size={18} />,
    primary: true,
  },
  {
    title: "继续旅途",
    description: "从存档中恢复进度，延续未完的故事或创建新的分支。",
    to: "/saves",
    icon: <Save size={18} />,
  },
  {
    title: "世界工坊",
    description: "创建和编辑世界配置，管理每个世界的角色与设定。",
    to: "/worlds",
    icon: <Globe size={18} />,
  },
  {
    title: "偏好设置",
    description: "自定义模型、背景外观、数据导入导出等选项。",
    to: "/settings",
    icon: <Settings size={18} />,
  },
  {
    title: "工具箱",
    description: "管理和配置 MCP 工具，按需扩展世界主控的能力。",
    to: "/mcp-tools",
    icon: <Wrench size={18} />,
  },
];

export function HomePage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();

  // ===== Desktop Layout (菜单网格+描述) =====
  const desktopLayout = (
    <ScreenLayout
      title="首页"
      subtitle="选择你的下一步行动，一切故事从这里开始。"
      maxWidth={860}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr",
          gap: 8,
          maxWidth: 480,
          margin: "0 auto",
        }}
      >
        <SurfacePanel className="surface-panel--pad-md">
          <div className="grid grid--gap-sm">
            {desktopMenuItems.map((item) => (
              <MenuButton
                key={item.title}
                title={item.title}
                description={item.description}
                primary={item.primary}
                icon={item.icon}
                onClick={() => navigate(item.to)}
              />
            ))}
          </div>
        </SurfacePanel>
      </div>
      <div style={{ textAlign: "center", marginTop: 32 }}>
        <p style={{ fontSize: 12, color: "var(--color-muted)", margin: 0 }}>本软件下载、安装及使用过程完全免费</p>
      </div>
    </ScreenLayout>
  );

  // ===== Mobile Layout (品牌着陆页+2个大按钮) =====
  const mobileLayout = (
    <div className="home-mobile-center">
      <div className="home-mobile-brand">云朵梦境引擎</div>
      <div className="home-mobile-actions">
        <button
          type="button"
          className="home-mobile-btn home-mobile-btn--primary"
          onClick={() => navigate("/new-game")}
        >
          <span className="home-mobile-btn-icon"><Play size={22} /></span>
          <span className="home-mobile-btn-text">新的游戏</span>
        </button>
        <button
          type="button"
          className="home-mobile-btn home-mobile-btn--secondary"
          onClick={() => navigate("/saves")}
        >
          <span className="home-mobile-btn-icon"><Save size={22} /></span>
          <span className="home-mobile-btn-text">继续游戏</span>
        </button>
      </div>
      <div className="home-mobile-hint">点击左上角菜单展开导航</div>
      <div style={{ marginTop: 32 }}>
        <p style={{ fontSize: 12, color: "var(--color-muted)", margin: 0 }}>本软件下载、安装及使用过程完全免费</p>
      </div>
    </div>
  );

  return isMobile ? mobileLayout : desktopLayout;
}
