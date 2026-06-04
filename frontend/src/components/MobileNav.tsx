import { useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { ArrowLeft, Globe, Menu, Moon, Play, Save, Settings, Sun, Wrench, X } from "lucide-react";
import appIconUrl from "../assets/app-icon.svg";
import {
  applyMode,
  persistMode,
  resolveInitialMode,
  type ThemeMode,
} from "../data/theme";

function CloudIcon({ size = 40 }: { size?: number }) {
  return <img src={appIconUrl} alt="" width={size} height={size} style={{ borderRadius: "20%" }} />;
}

type MobileNavProps = {
  children: ReactNode;
};

const navItems = [
  { path: "/new-game", label: "新的游戏", Icon: Play },
  { path: "/saves", label: "读取存档", Icon: Save },
  { path: "/worlds", label: "世界设定", Icon: Globe },
  { path: "/settings", label: "设置", Icon: Settings },
  { path: "/mcp-tools", label: "MCP 工具", Icon: Wrench },
];

export function MobileNav({ children }: MobileNavProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [mode, setMode] = useState<ThemeMode>(() => resolveInitialMode());
  const navigate = useNavigate();
  const location = useLocation();
  const isImmersiveRoute =
    location.pathname.startsWith("/game/") || location.pathname.startsWith("/debug/");
  const showBackButton = !isImmersiveRoute && location.pathname !== "/";

  const handleNavigate = (path: string) => {
    navigate(path);
    setIsOpen(false);
  };

  const handleToggleTheme = () => {
    const nextMode: ThemeMode = mode === "dark" ? "light" : "dark";
    setMode(nextMode);
    applyMode(nextMode);
    persistMode(nextMode);
  };

  const isActive = (path: string) => {
    if (path === "/") return location.pathname === "/";
    return location.pathname === path || location.pathname.startsWith(path + "/");
  };

  return (
    <div className={`mobile-nav-container${isImmersiveRoute ? " mobile-nav-container--immersive" : ""}`}>
      <button
        type="button"
        className="mobile-fab"
        onClick={() => setIsOpen(!isOpen)}
        aria-label="Toggle menu"
      >
        <span className="mobile-fab-icon">{isOpen ? <X size={20} /> : <Menu size={20} />}</span>
      </button>
      {showBackButton ? (
        <button
          type="button"
          className="mobile-back-btn"
          onClick={() => navigate(-1)}
          aria-label="返回"
        >
          <ArrowLeft size={17} />
          <span>返回</span>
        </button>
      ) : null}

      {isOpen ? (
        <div className="mobile-overlay" onClick={() => setIsOpen(false)}>
          <nav className="mobile-sidebar" onClick={(event) => event.stopPropagation()}>
            <div className="mobile-sidebar-brand">
              <button
                type="button"
                className="mobile-sidebar-brand-icon-btn"
                onClick={() => setIsOpen(false)}
                aria-label="Close menu"
              >
                <CloudIcon size={36} />
              </button>
              <div className="mobile-sidebar-brand-text">
                <span className="mobile-sidebar-brand-title">云朵梦境</span>
                <span className="mobile-sidebar-brand-subtitle">CLOUD DREAM ENGINE</span>
              </div>
            </div>

            <ul className="mobile-nav-list">
              {navItems.map((item) => (
                <li key={item.path}>
                  <button
                    type="button"
                    className={`mobile-nav-item ${isActive(item.path) ? " mobile-nav-item--active" : ""}`}
                    onClick={() => handleNavigate(item.path)}
                  >
                    <span className="mobile-nav-icon">
                      <item.Icon size={18} />
                    </span>
                    <span className="mobile-nav-label">{item.label}</span>
                  </button>
                </li>
              ))}
            </ul>

            <div className="mobile-sidebar-footer">
              <button
                type="button"
                className="mobile-theme-btn"
                onClick={handleToggleTheme}
                aria-label={mode === "dark" ? "Switch to light mode" : "Switch to dark mode"}
              >
                <span className="mobile-theme-btn-icon">
                  {mode === "dark" ? <Sun size={18} /> : <Moon size={18} />}
                </span>
                <span className="mobile-theme-btn-label">{mode === "dark" ? "切换到浅色" : "切换到深色"}</span>
              </button>
            </div>
          </nav>
        </div>
      ) : null}

      <main className={`mobile-content${isImmersiveRoute ? " mobile-content--immersive" : ""}`}>{children}</main>
    </div>
  );
}
