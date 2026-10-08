import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
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

const DRAG_OPEN_THRESHOLD = 40;
const DRAG_CLOSE_THRESHOLD = 40;
const DRAG_ACTIVATE_PX = 12;
const DRAG_CANCEL_VERTICAL = 48;
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

/** 安卓 WebView 里 env(safe-area-inset-*) 恒为 0；MainActivity 把原生精确值注入主窗口，这里接成 CSS 变量。 */
function readAppSafeAreaInsets(): { top: number; bottom: number; left: number; right: number } {
  const raw = (window as { __nativeSafeAreaInsets?: Record<string, unknown> }).__nativeSafeAreaInsets ?? {};
  const read = (key: string) => {
    const value = Number(raw[key]);
    return Number.isFinite(value) && value > 0 ? Math.round(value) : 0;
  };
  return { top: read("top"), bottom: read("bottom"), left: read("left"), right: read("right") };
}

function useAppSafeAreaInsets(): { top: number; bottom: number; left: number; right: number } {
  const [insets, setInsets] = useState(() => readAppSafeAreaInsets());
  useEffect(() => {
    const update = () => setInsets(readAppSafeAreaInsets());
    update();
    window.addEventListener("native-safe-area", update);
    return () => window.removeEventListener("native-safe-area", update);
  }, []);
  return insets;
}

export function MobileNav({ children }: MobileNavProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [mode, setMode] = useState<ThemeMode>(() => resolveInitialMode());
  const [sidebarDragX, setSidebarDragX] = useState(0);
  const [sidebarDragging, setSidebarDragging] = useState(false);
  const [handleDragX, setHandleDragX] = useState(0);
  const [handleDragging, setHandleDragging] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  const mobileViewport = useMobileVisualViewport();
  const appSafeArea = useAppSafeAreaInsets();
  const viewportHeight = mobileViewport.height > 0 ? `${mobileViewport.height}px` : "100dvh";
  const mobileViewportStyle = useMemo(
    () => ({
      "--app-visual-viewport-height": viewportHeight,
      "--app-safe-area-top": `${appSafeArea.top}px`,
      "--app-safe-area-bottom": `${appSafeArea.bottom}px`,
      "--app-safe-area-left": `${appSafeArea.left}px`,
      "--app-safe-area-right": `${appSafeArea.right}px`,
    }) as CSSProperties,
    [viewportHeight, appSafeArea],
  );
  const isImmersiveRoute = location.pathname.startsWith("/game/");
  const showBackButton = !isImmersiveRoute && location.pathname !== "/";

  const sidebarRef = useRef<HTMLElement | null>(null);
  const isOpenRef = useRef(false);
  const sidebarDragXRef = useRef(0);
  /** 仅在「刚完成一次真实拖动」时为 true，用于吃掉紧随其后的 click；不使用定时器，避免误伤菜单点击 */
  const swallowClickRef = useRef(false);
  const cleanupGestureRef = useRef<null | (() => void)>(null);

  useEffect(() => {
    isOpenRef.current = isOpen;
  }, [isOpen]);

  const setSidebarDrag = useCallback((x: number) => {
    sidebarDragXRef.current = x;
    setSidebarDragX(x);
  }, []);

  const stopGesture = useCallback(() => {
    cleanupGestureRef.current?.();
    cleanupGestureRef.current = null;
  }, []);

  useEffect(() => () => stopGesture(), [stopGesture]);

  const handleNavigate = (path: string) => {
    // 拖动关闭后浏览器可能仍补发 click；只吞这一次
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
    navigate(path);
    setIsOpen(false);
    setSidebarDragX(0);
    setHandleDragX(0);
  };

  const handleBack = () => {
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
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
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
    const nextMode: ThemeMode = mode === "dark" ? "light" : "dark";
    setMode(nextMode);
    applyMode(nextMode);
    persistMode(nextMode);
  };

  const isActive = (path: string) => {
    if (path === "/") return location.pathname === "/";
    return location.pathname === path || location.pathname.startsWith(path + "/");
  };

  const attachWindowGesture = useCallback((handlers: {
    onMove: (clientX: number, clientY: number) => void;
    onEnd: () => void;
  }) => {
    stopGesture();
    const onPointerMove = (event: PointerEvent) => handlers.onMove(event.clientX, event.clientY);
    const onTouchMove = (event: TouchEvent) => {
      const touch = event.touches[0];
      if (touch) handlers.onMove(touch.clientX, touch.clientY);
    };
    const onEnd = () => handlers.onEnd();
    const cleanup = () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onEnd);
      window.removeEventListener("pointercancel", onEnd);
      window.removeEventListener("touchmove", onTouchMove);
      window.removeEventListener("touchend", onEnd);
      window.removeEventListener("touchcancel", onEnd);
    };
    cleanupGestureRef.current = cleanup;
    window.addEventListener("pointermove", onPointerMove, { passive: true });
    window.addEventListener("pointerup", onEnd, { passive: true });
    window.addEventListener("pointercancel", onEnd, { passive: true });
    window.addEventListener("touchmove", onTouchMove, { passive: true });
    window.addEventListener("touchend", onEnd, { passive: true });
    window.addEventListener("touchcancel", onEnd, { passive: true });
  }, [stopGesture]);

  /**
   * 把手：横向超过阈值才切换侧栏。
   * pointerdown 不 preventDefault、不立刻 capture，避免点按无法触发 click。
   */
  const beginHandleGesture = useCallback(
    (startX: number, startY: number, pointerId?: number, target?: Element | null) => {
      stopGesture();
      const openedAtStart = isOpenRef.current;
      let activated = false;
      let finished = false;

      const activate = () => {
        if (activated) return;
        activated = true;
        setHandleDragging(true);
        if (pointerId != null && target && "setPointerCapture" in target) {
          try {
            (target as Element).setPointerCapture(pointerId);
          } catch {
            // ignore
          }
        }
      };

      const settleOpen = (nextOpen: boolean) => {
        finished = true;
        swallowClickRef.current = true;
        setIsOpen(nextOpen);
        setHandleDragX(0);
        setHandleDragging(false);
        stopGesture();
      };

      attachWindowGesture({
        onMove: (clientX, clientY) => {
          if (finished) return;
          const dx = clientX - startX;
          const dy = Math.abs(clientY - startY);
          if (!activated && dy > DRAG_CANCEL_VERTICAL && dy > Math.abs(dx)) {
            finished = true;
            setHandleDragX(0);
            setHandleDragging(false);
            stopGesture();
            return;
          }
          if (Math.abs(dx) < DRAG_ACTIVATE_PX) {
            return;
          }
          activate();
          if (!openedAtStart) {
            setHandleDragX(Math.max(0, dx));
            if (dx >= DRAG_OPEN_THRESHOLD) settleOpen(true);
            return;
          }
          setHandleDragX(Math.min(0, dx));
          if (dx <= -DRAG_CLOSE_THRESHOLD) settleOpen(false);
        },
        onEnd: () => {
          if (finished) {
            stopGesture();
            return;
          }
          finished = true;
          // 未达阈值：视为点按，不 swallow，交给 click 开关
          setHandleDragX(0);
          setHandleDragging(false);
          stopGesture();
        },
      });
    },
    [attachWindowGesture, stopGesture],
  );

  /**
   * 侧栏：仅在横向拖动超过激活阈值后才真正拖面板。
   * 绝不能在 pointerdown 上 preventDefault / setPointerCapture，
   * 否则安卓 WebView 里菜单按钮收不到 click，路由会像「全进首页」。
   */
  const beginSidebarGesture = useCallback(
    (startX: number, startY: number, pointerId?: number) => {
      stopGesture();
      swallowClickRef.current = false;
      let activated = false;
      let finished = false;

      const activate = () => {
        if (activated) return;
        activated = true;
        setSidebarDragging(true);
        if (pointerId != null && sidebarRef.current && "setPointerCapture" in sidebarRef.current) {
          try {
            sidebarRef.current.setPointerCapture(pointerId);
          } catch {
            // ignore
          }
        }
      };

      attachWindowGesture({
        onMove: (clientX, clientY) => {
          if (finished) return;
          const dx = clientX - startX;
          const dy = Math.abs(clientY - startY);
          if (!activated) {
            // 纵向滑动交给列表滚动，不抢
            if (dy > DRAG_CANCEL_VERTICAL && dy > Math.abs(dx)) {
              finished = true;
              stopGesture();
              return;
            }
            // 未横向激活前不改位移，保证按钮 click 正常
            if (dx > -DRAG_ACTIVATE_PX) {
              return;
            }
            activate();
          }
          setSidebarDrag(Math.min(0, dx));
        },
        onEnd: () => {
          if (finished) {
            stopGesture();
            return;
          }
          finished = true;
          if (activated) {
            const width = sidebarRef.current?.offsetWidth ?? 280;
            const threshold = Math.min(SIDEBAR_CLOSE_MAX_PX, width * SIDEBAR_CLOSE_RATIO);
            const liveX = sidebarDragXRef.current;
            if (liveX <= -threshold) {
              swallowClickRef.current = true;
              setIsOpen(false);
            }
            setSidebarDrag(0);
            setSidebarDragging(false);
          }
          stopGesture();
        },
      });
    },
    [attachWindowGesture, stopGesture],
  );

  const onFabPointerDown = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    // 不 preventDefault：保留随后的 click，用于点按开关
    beginHandleGesture(event.clientX, event.clientY, event.pointerId, event.currentTarget);
  };

  const onFabClick = () => {
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
    setIsOpen(!isOpen);
    setSidebarDragX(0);
    setHandleDragX(0);
  };

  const onEdgePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    if (isOpenRef.current) return;
    beginHandleGesture(event.clientX, event.clientY, event.pointerId, event.currentTarget);
  };

  const onSidebarPointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    // 关键：这里不能 preventDefault，也不能立刻 setPointerCapture
    beginSidebarGesture(event.clientX, event.clientY, event.pointerId);
  };

  const onOverlayClick = () => {
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
    setIsOpen(false);
    setSidebarDragX(0);
    setHandleDragX(0);
  };

  return (
    <div
      className={`mobile-nav-container${isImmersiveRoute ? " mobile-nav-container--immersive" : ""}`}
      style={mobileViewportStyle}
    >
      {!isOpen ? (
        <div
          className="mobile-edge-swipe-zone"
          aria-hidden="true"
          onPointerDown={onEdgePointerDown}
        />
      ) : null}

      <button
        type="button"
        className={`mobile-fab${handleDragging ? " mobile-fab--dragging" : ""}`}
        onClick={onFabClick}
        onPointerDown={onFabPointerDown}
        aria-label={isOpen ? "收起导航菜单" : "展开导航菜单"}
        aria-expanded={isOpen}
        title={isOpen ? "收起导航菜单 / 向左拖关闭" : "展开导航菜单 / 向右拖打开"}
        style={{
          touchAction: "none",
          transform: handleDragX !== 0 ? `translateX(${handleDragX}px)` : undefined,
        }}
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
              // 仅阻止「拖动后误触发的点击」冒泡到遮罩
              if (swallowClickRef.current) {
                event.preventDefault();
                event.stopPropagation();
                return;
              }
              event.stopPropagation();
            }}
            onPointerDown={onSidebarPointerDown}
            style={{
              transform: `translateX(${sidebarDragX}px)`,
            }}
          >
            <div className="mobile-sidebar-brand">
              <button
                type="button"
                className="mobile-sidebar-brand-icon-btn"
                onClick={() => {
                  if (swallowClickRef.current) {
                    swallowClickRef.current = false;
                    return;
                  }
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
