import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
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

const EDGE_OPEN_PX = 28;
const EDGE_OPEN_THRESHOLD = 52;
const FAB_DRAG_OPEN_THRESHOLD = 44;
const SIDEBAR_CLOSE_RATIO = 0.28;
const SIDEBAR_CLOSE_MAX_PX = 96;

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
  const [sidebarDragX, setSidebarDragX] = useState(0);
  const [sidebarDragging, setSidebarDragging] = useState(false);
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

  const sidebarRef = useRef<HTMLElement | null>(null);
  const suppressClickRef = useRef(false);
  const edgeGestureRef = useRef({ armed: false, startX: 0, startY: 0 });
  const sidebarDragRef = useRef({ active: false, pointerId: -1, startX: 0 });
  const fabDragRef = useRef({ active: false, pointerId: -1, startX: 0, startY: 0, moved: false });

  const markSuppressClick = useCallback(() => {
    suppressClickRef.current = true;
    window.setTimeout(() => {
      suppressClickRef.current = false;
    }, 80);
  }, []);

  const handleNavigate = (path: string) => {
    if (suppressClickRef.current) {
      return;
    }
    navigate(path);
    setIsOpen(false);
    setSidebarDragX(0);
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

  // 左缘右滑打开侧栏（关闭时）
  useEffect(() => {
    if (isOpen) {
      return undefined;
    }

    const onTouchStart = (event: TouchEvent) => {
      const touch = event.touches[0];
      if (!touch) return;
      if (touch.clientX <= EDGE_OPEN_PX) {
        edgeGestureRef.current = {
          armed: true,
          startX: touch.clientX,
          startY: touch.clientY,
        };
      }
    };

    const onTouchMove = (event: TouchEvent) => {
      const gesture = edgeGestureRef.current;
      if (!gesture.armed) return;
      const touch = event.touches[0];
      if (!touch) return;
      const dx = touch.clientX - gesture.startX;
      const dy = Math.abs(touch.clientY - gesture.startY);
      if (dy > 40 && dy > Math.abs(dx)) {
        gesture.armed = false;
        return;
      }
      if (dx >= EDGE_OPEN_THRESHOLD) {
        gesture.armed = false;
        setSidebarDragX(0);
        setIsOpen(true);
        markSuppressClick();
      }
    };

    const onTouchEnd = () => {
      edgeGestureRef.current.armed = false;
    };

    window.addEventListener("touchstart", onTouchStart, { passive: true });
    window.addEventListener("touchmove", onTouchMove, { passive: true });
    window.addEventListener("touchend", onTouchEnd, { passive: true });
    window.addEventListener("touchcancel", onTouchEnd, { passive: true });
    return () => {
      window.removeEventListener("touchstart", onTouchStart);
      window.removeEventListener("touchmove", onTouchMove);
      window.removeEventListener("touchend", onTouchEnd);
      window.removeEventListener("touchcancel", onTouchEnd);
    };
  }, [isOpen, markSuppressClick]);

  // 把手：按住右拖也可打开（与左缘滑动手势一致）
  const onFabPointerDown = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    fabDragRef.current = {
      active: true,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
    };
  };

  const onFabPointerMove = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = fabDragRef.current;
    if (!drag.active || drag.pointerId !== event.pointerId || isOpen) return;
    const dx = event.clientX - drag.startX;
    const dy = Math.abs(event.clientY - drag.startY);
    if (dy > 48 && dy > Math.abs(dx)) {
      drag.active = false;
      return;
    }
    if (dx >= FAB_DRAG_OPEN_THRESHOLD) {
      drag.moved = true;
      drag.active = false;
      setIsOpen(true);
      markSuppressClick();
    }
  };

  const onFabPointerUp = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = fabDragRef.current;
    if (drag.pointerId === event.pointerId) {
      drag.active = false;
    }
  };

  const onFabClick = () => {
    if (fabDragRef.current.moved) {
      fabDragRef.current.moved = false;
      return;
    }
    if (suppressClickRef.current) return;
    setIsOpen(!isOpen);
    setSidebarDragX(0);
  };

  // 侧栏：向左拖关闭
  const onSidebarPointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    sidebarDragRef.current = {
      active: true,
      pointerId: event.pointerId,
      startX: event.clientX,
    };
    setSidebarDragging(true);
    setSidebarDragX(0);
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // 某些 WebView 不支持 pointer capture 时忽略，仍可拖
    }
  };

  const onSidebarPointerMove = (event: ReactPointerEvent<HTMLElement>) => {
    const drag = sidebarDragRef.current;
    if (!drag.active || drag.pointerId !== event.pointerId) return;
    const dx = event.clientX - drag.startX;
    if (Math.abs(dx) > 6) {
      markSuppressClick();
    }
    // 只允许向左拉出关闭位移
    setSidebarDragX(Math.min(0, dx));
  };

  const onSidebarPointerEnd = (event: ReactPointerEvent<HTMLElement>) => {
    const drag = sidebarDragRef.current;
    if (!drag.active || drag.pointerId !== event.pointerId) return;
    drag.active = false;
    setSidebarDragging(false);

    const width = sidebarRef.current?.offsetWidth ?? 280;
    const threshold = Math.min(SIDEBAR_CLOSE_MAX_PX, width * SIDEBAR_CLOSE_RATIO);
    if (sidebarDragX <= -threshold) {
      markSuppressClick();
      setIsOpen(false);
    }
    setSidebarDragX(0);
  };

  const onOverlayClick = () => {
    if (suppressClickRef.current) return;
    setIsOpen(false);
    setSidebarDragX(0);
  };

  return (
    <div
      className={`mobile-nav-container${isImmersiveRoute ? " mobile-nav-container--immersive" : ""}`}
      style={mobileViewportStyle}
    >
      <button
        type="button"
        className="mobile-fab"
        onClick={onFabClick}
        onPointerDown={onFabPointerDown}
        onPointerMove={onFabPointerMove}
        onPointerUp={onFabPointerUp}
        onPointerCancel={onFabPointerUp}
        aria-label={isOpen ? "收起导航菜单" : "展开导航菜单"}
        aria-expanded={isOpen}
        title={isOpen ? "收起导航菜单" : "展开导航菜单"}
        style={{ touchAction: "none" }}
      >
        <span className="mobile-fab-icon">{isOpen ? <X size={16} /> : <Menu size={16} />}</span>
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
        <div className="mobile-overlay" onClick={onOverlayClick}>
          <nav
            ref={sidebarRef}
            className={`mobile-sidebar${sidebarDragging ? " is-dragging" : ""}`}
            onClick={(event) => {
              if (suppressClickRef.current) {
                event.preventDefault();
                event.stopPropagation();
                return;
              }
              event.stopPropagation();
            }}
            onPointerDown={onSidebarPointerDown}
            onPointerMove={onSidebarPointerMove}
            onPointerUp={onSidebarPointerEnd}
            onPointerCancel={onSidebarPointerEnd}
            style={{
              transform: `translateX(${sidebarDragX}px)`,
            }}
          >
            <div className="mobile-sidebar-brand">
              <button
                type="button"
                className="mobile-sidebar-brand-icon-btn"
                onClick={() => {
                  if (suppressClickRef.current) return;
                  setIsOpen(false);
                }}
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
