import { useNavigate } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import { MenuButton, ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { useT } from "../data/i18n/context";
import { Play, Save, Globe, Settings, Wrench } from "lucide-react";
import type { ReactNode } from "react";

type MenuItem = {
  titleKey: string;
  descKey: string;
  to: string;
  icon: ReactNode;
  primary?: boolean;
};

const desktopMenuItems: MenuItem[] = [
  {
    titleKey: "home.menuStartTitle",
    descKey: "home.menuStartDesc",
    to: "/new-game",
    icon: <Play size={18} />,
    primary: true,
  },
  {
    titleKey: "home.menuContinueTitle",
    descKey: "home.menuContinueDesc",
    to: "/saves",
    icon: <Save size={18} />,
  },
  {
    titleKey: "home.menuWorldsTitle",
    descKey: "home.menuWorldsDesc",
    to: "/worlds",
    icon: <Globe size={18} />,
  },
  {
    titleKey: "home.menuSettingsTitle",
    descKey: "home.menuSettingsDesc",
    to: "/settings",
    icon: <Settings size={18} />,
  },
  {
    titleKey: "home.menuToolsTitle",
    descKey: "home.menuToolsDesc",
    to: "/mcp-tools",
    icon: <Wrench size={18} />,
  },
];

export function HomePage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const t = useT();

  // ===== Desktop Layout (菜单网格+描述) =====
  const desktopLayout = (
    <ScreenLayout
      title={t("home.title")}
      subtitle={t("home.subtitle")}
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
                key={item.titleKey}
                title={t(item.titleKey)}
                description={t(item.descKey)}
                primary={item.primary}
                icon={item.icon}
                onClick={() => navigate(item.to)}
              />
            ))}
          </div>
        </SurfacePanel>
      </div>
      <div style={{ textAlign: "center", marginTop: 32 }}>
        <p style={{ fontSize: 12, color: "var(--color-muted)", margin: 0 }}>{t("home.freeNote")}</p>
      </div>
    </ScreenLayout>
  );

  // ===== Mobile Layout (品牌着陆页+2个大按钮) =====
  const mobileLayout = (
    <div className="home-mobile-center">
      <div className="home-mobile-brand">{t("home.brand")}</div>
      <div className="home-mobile-actions">
        <button
          type="button"
          className="home-mobile-btn home-mobile-btn--primary"
          onClick={() => navigate("/new-game")}
        >
          <span className="home-mobile-btn-icon"><Play size={22} /></span>
          <span className="home-mobile-btn-text">{t("home.mobileNewGame")}</span>
        </button>
        <button
          type="button"
          className="home-mobile-btn home-mobile-btn--secondary"
          onClick={() => navigate("/saves")}
        >
          <span className="home-mobile-btn-icon"><Save size={22} /></span>
          <span className="home-mobile-btn-text">{t("home.mobileContinue")}</span>
        </button>
      </div>
      <div className="home-mobile-hint">{t("home.mobileNavHint")}</div>
      <div style={{ marginTop: 32 }}>
        <p style={{ fontSize: 12, color: "var(--color-muted)", margin: 0 }}>{t("home.freeNote")}</p>
      </div>
    </div>
  );

  return isMobile ? mobileLayout : desktopLayout;
}
