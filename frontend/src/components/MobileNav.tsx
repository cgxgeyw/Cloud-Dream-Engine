import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from "react";
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

type MobileViewportState = {
  height: number;
};

const navItems = [
  { path: "/new-game", label: "新的游戏", Icon: Play },
  { path: "/saves", label: "读取存档", Icon: Save },
  { path: "/worlds", label: "世界设定", Icon: Globe },
  { path: "/settings", label: "设置", Icon: Settings },
  { path: "/mcp-tools", label: "MCP 工具", Icon: Wrench },
];

function resolveParentPath(pathname: string, search: string) {
  const params = new URLSearchParams(search);
  const worldId = params.get("worldId");

  if (pathname === "/" || pathname.startsWith("/game/") || pathname.startsWith("/debug/")) {
    return "/";
  }

  if (pathname === "/new-game" || pathname === "/saves" || pathname === "/worlds" || pathname === "/settings" || pathname === "/mcp-tools") {
    return "/";
  }

  if (pathname.startsWith("/new-game/setup/")) {
    return "/new-game";
  }

  if (pathname === "/worlds/new" || /^\/worlds\/[^/]+\/edit$/.test(pathname)) {
    return "/worlds";
  }

  if (/^\/worlds\/[^/]+\/characters$/.test(pathname)) {
    return "/worlds";
  }

  if (pathname === "/characters/new" || /^\/characters\/[^/]+\/edit$/.test(pathname)) {
    return worldId ? `/worlds/${encodeURIComponent(worldId)}/characters` : "/worlds";
  }

  const segments = pathname.split("/").filter(Boolean);
  if (segments.length <= 1) {
    return "/";
  }
  return `/${segments.slice(0, -1).join("/")}`;
}

function useMobileVisualViewport(): MobileViewportState {
  const [viewport, setViewport] = useState<MobileViewportState>(() => ({
    height: typeof window === "undefined" ? 0 : Math.round(window.visualViewport?.height ?? window.innerHeight),
  }));

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    let frameId = 0;
    const updateViewport = () => {
      window.cancelAnimationFrame(frameId);
      frameId = window.requestAnimationFrame(() => {
        const nextHeight = Math.round(window.visualViewport?.height ?? window.innerHeight);
        setViewport((current) => (current.height === nextHeight ? current : { height: nextHeight }));
      });
    };

    updateViewport();
    window.visualViewport?.addEventListener("resize", updateViewport);
    window.visualViewport?.addEventListener("scroll", updateViewport);
    window.addEventListener("resize", updateViewport);
    window.addEventListener("orientationchange", updateViewport);

    return () => {
      window.cancelAnimationFrame(frameId);
      window.visualViewport?.removeEventListener("resize", updateViewport);
      window.visualViewport?.removeEventListener("scroll", updateViewport);
      window.removeEventListener("resize", updateViewport);
      window.removeEventListener("orientationchange", updateViewport);
    };
  }, []);

  return viewport;
}

export function MobileNav({ children }: MobileNavProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [mode, setMode] = useState<ThemeMode>(() => resolveInitialMode());
  const navigate = useNavigate();
  const location = useLocation();
  const mobileViewport = useMobileVisualViewport();
  const viewportHeight = mobileViewport.height > 0 ? `${mobileViewport.height}px` : "100dvh";
  const mobileViewportStyle = useMemo(
    () => ({
      "--app-visual-viewport-height": viewportHeight,
    }) as CSSProperties,
    [viewportHeight],
  );
  // 调试页是内容页，需要正常的返回按钮与滚动能力，不能按沉浸式处理。
  const isImmersiveRoute = location.pathname.startsWith("/game/");
  const showBackButton = !isImmersiveRoute && location.pathname !== "/";

  const handleNavigate = (path: string) => {
    navigate(path);
    setIsOpen(false);
  };

  const handleBack = () => {
    // 优先与侧滑/系统返回保持一致：有应用内历史时走历史回退（navigate(-1)）。
    // 仅当处于首屏/深链（history idx 为 0 或缺失，没有可回退的上一页）时，
    // 才回退到逻辑父级，避免「返回」无效或离开应用。
    const historyIdx =
      typeof window !== "undefined" && window.history.state && typeof window.history.state.idx === "number"
        ? (window.history.state.idx as number)
        : 0;
    if (historyIdx > 0) {
      navigate(-1);
    } else {
      navigate(resolveParentPath(location.pathname, location.search));
    }
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
    <div
      className={`mobile-nav-container${isImmersiveRoute ? " mobile-nav-container--immersive" : ""}`}
      style={mobileViewportStyle}
    >
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
          onClick={handleBack}
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
                  aria-label="关闭菜单"
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
