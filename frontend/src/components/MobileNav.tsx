import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { ArrowLeft, Globe, Menu, Moon, Play, Save, Settings, Sun, Wrench, X, BookOpen } from "lucide-react";
import appIconUrl from "../assets/app-icon.svg";
import { isAndroidRuntime } from "../data/apiAdapter";
import { resolveParentPath } from "../lib/routeBack";
import { readNativeSafeAreaInsets } from "../lib/nativeInsets";
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
  { path: '/knowledge', label: '知识库', Icon: BookOpen },
];

const DRAG_CLOSE_THRESHOLD = 40;
const DRAG_ACTIVATE_PX = 4;
const DRAG_CANCEL_VERTICAL = 20;
const SIDEBAR_CLOSE_RATIO = 0.28;
const SIDEBAR_CLOSE_MAX_PX = 96;
const HANDLE_REVEAL_RATIO = 0.26;
const HANDLE_WIDTH = (viewportWidth: number) => Math.max(240, Math.min(viewportWidth - 24, 320));

const FAB_SIZE = 52;
const FAB_CONDENSED_SIZE = 44;
const FAB_EDGE_MARGIN = 16;
const FAB_VERTICAL_MARGIN = 12;
/** 超过该位移才算拖动，否则按点按处理（避免手抖导致菜单点不开） */
const FAB_DRAG_THRESHOLD = 6;
const FAB_STORAGE_KEY = "od-mobile-fab-position";

type FabSide = "left" | "right";
type StoredFabPosition = { side: FabSide; ratio: number };

function readStoredFabPosition(): StoredFabPosition | null {
  try {
    const raw = window.localStorage.getItem(FAB_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredFabPosition>;
    if (parsed.side !== "left" && parsed.side !== "right") return null;
    if (typeof parsed.ratio !== "number" || !Number.isFinite(parsed.ratio)) return null;
    return { side: parsed.side, ratio: Math.min(1, Math.max(0, parsed.ratio)) };
  } catch {
    return null;
  }
}

function fabMaxTop(bottomInset: number) {
  return Math.max(
    FAB_VERTICAL_MARGIN,
    window.innerHeight - FAB_SIZE - Math.max(bottomInset, 0) - FAB_VERTICAL_MARGIN,
  );
}

/** 编辑器页面自带返回按钮，不再叠加全局悬浮返回，避免同一高度出现两个返回。 */
const SELF_MANAGED_BACK_PATTERN = /^\/(characters\/(new|[^/]+\/edit)|worlds\/(new|[^/]+\/edit)|worlds\/[^/]+\/characters|mcp-tools|new-game|new-game\/setup\/[^/]+|knowledge)\/?$/;



function useMobileVisualViewport(): MobileViewportState {
  const [viewport, setViewport] = useState<MobileViewportState>(() => ({
    height: typeof window === "undefined" ? 0 : Math.round(window.visualViewport?.height ?? window.innerHeight),
  }));

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    let frameId = 0;
    // 安卓 edge-to-edge + adjustResize 时键盘常常不缩 WebView，visualViewport 高度不变；
    // 这里用原生 IME inset 兜底收缩，保证世界界面 iframe 与输入区不被键盘盖住。
    let maxLayoutHeight = 0;
    const updateViewport = () => {
      window.cancelAnimationFrame(frameId);
      frameId = window.requestAnimationFrame(() => {
        const layout = Math.round(window.innerHeight);
        if (layout > maxLayoutHeight) maxLayoutHeight = layout;
        const visualHeight = Math.round(window.visualViewport?.height ?? layout);
        const nativeKeyboard = Math.max(0, Math.round(Number((window as { __nativeKeyboardHeight?: unknown }).__nativeKeyboardHeight) || 0));
        const alreadyShrunk = layout < maxLayoutHeight - 8 || visualHeight < maxLayoutHeight - 8;
        const nextHeight = !alreadyShrunk && nativeKeyboard > 0
          ? Math.max(1, maxLayoutHeight - nativeKeyboard)
          : visualHeight;
        setViewport((current) => (current.height === nextHeight ? current : { height: nextHeight }));
      });
    };

    updateViewport();
    window.visualViewport?.addEventListener("resize", updateViewport);
    window.visualViewport?.addEventListener("scroll", updateViewport);
    window.addEventListener("resize", updateViewport);
    window.addEventListener("orientationchange", updateViewport);
    window.addEventListener("native-keyboard", updateViewport);

    return () => {
      window.cancelAnimationFrame(frameId);
      window.visualViewport?.removeEventListener("resize", updateViewport);
      window.visualViewport?.removeEventListener("scroll", updateViewport);
      window.removeEventListener("resize", updateViewport);
      window.removeEventListener("orientationchange", updateViewport);
      window.removeEventListener("native-keyboard", updateViewport);
    };
  }, []);

  return viewport;
}

/** 安卓 WebView 里 env(safe-area-inset-*) 恒为 0；MainActivity 把原生精确值注入主窗口，这里接成 CSS 变量。 */
function readAppSafeAreaInsets(): { top: number; bottom: number; left: number; right: number } {
  const native = readNativeSafeAreaInsets();
  // 原生值尚未注入（且无缓存）时先留出状态栏高度，避免首帧内容顶到状态栏上。
  const fallbackTop = isAndroidRuntime() ? 28 : 0;
  return {
    top: Math.max(native?.top ?? 0, native ? 0 : fallbackTop),
    bottom: native?.bottom ?? 0,
    left: native?.left ?? 0,
    right: native?.right ?? 0,
  };
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
  const [fabCondensed, setFabCondensed] = useState(false);
  const [fabSide, setFabSide] = useState<FabSide>(() => readStoredFabPosition()?.side ?? "right");
  const [fabY, setFabY] = useState<number>(() => {
    const stored = readStoredFabPosition();
    const maxTop = fabMaxTop(readAppSafeAreaInsets().bottom);
    const ratio = stored?.ratio ?? 0.99;
    return Math.min(maxTop, FAB_VERTICAL_MARGIN + ratio * Math.max(0, maxTop - FAB_VERTICAL_MARGIN));
  });
  const [fabDragPos, setFabDragPos] = useState<{ x: number; y: number } | null>(null);
  const [fabDragging, setFabDragging] = useState(false);
  const [handleReveal, setHandleReveal] = useState(0);
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
  /** 编辑器页内禁用边缘/把手拖动，避免与编辑内容的横向手势冲突；仍可用顶部菜单按钮打开 */
  const isEditorRoute = /^\/(characters\/(new|[^/]+\/edit)|worlds\/(new|[^/]+\/edit)|worlds\/[^/]+\/characters)\/?$/.test(location.pathname);
  const showBackButton = !isImmersiveRoute
    && location.pathname !== "/"
    && !SELF_MANAGED_BACK_PATTERN.test(location.pathname);

  const sidebarRef = useRef<HTMLElement | null>(null);
  const isOpenRef = useRef(false);
  const sidebarDragXRef = useRef(0);
  const handleRevealRef = useRef(0);
  /** 仅在「刚完成一次真实拖动」时为 true，用于吃掉紧随其后的 click；不使用定时器，避免误伤菜单点击 */
  const swallowClickRef = useRef(false);
  const cleanupGestureRef = useRef<null | (() => void)>(null);
  const fabDragRef = useRef<null | { dx: number; dy: number; startX: number; startY: number; active: boolean }>(null);
  const fabDragEndedAtRef = useRef(0);
  /** 位置按可用高度的比例保存，旋转/键盘/安全区变化后仍落在相同相对高度 */
  const fabRatioRef = useRef<number>(readStoredFabPosition()?.ratio ?? 0.99);

  useEffect(() => {
    isOpenRef.current = isOpen;
  }, [isOpen]);

  useEffect(() => {
    setFabCondensed(false);
  }, [location.pathname]);

  useEffect(() => {
    let lastY = 0;
    let settleTimer = 0;
    const handleScroll = (event: Event) => {
      const target = event.target;
      const scrollEl = target instanceof HTMLElement ? target : document.scrollingElement;
      const y = scrollEl instanceof HTMLElement ? scrollEl.scrollTop : window.scrollY;
      const delta = y - lastY;
      if (Math.abs(delta) < 6) return;
      lastY = y;
      if (delta > 0 && y > 24) setFabCondensed(true);
      else if (delta < 0) setFabCondensed(false);
      window.clearTimeout(settleTimer);
      settleTimer = window.setTimeout(() => setFabCondensed(false), 1400);
    };
    document.addEventListener("scroll", handleScroll, true);
    return () => {
      document.removeEventListener("scroll", handleScroll, true);
      window.clearTimeout(settleTimer);
    };
  }, []);

  const clampFabPosition = useCallback(
    (x: number, y: number) => {
      const minX = 8;
      const maxX = Math.max(minX, window.innerWidth - FAB_SIZE - 8);
      const minY = Math.max(8, appSafeArea.top + 8);
      const maxY = Math.max(minY, window.innerHeight - FAB_SIZE - Math.max(appSafeArea.bottom, 0) - 8);
      return {
        x: Math.min(Math.max(x, minX), maxX),
        y: Math.min(Math.max(y, minY), maxY),
      };
    },
    [appSafeArea.top, appSafeArea.bottom],
  );

  const persistFabPosition = useCallback(
    (side: FabSide, y: number) => {
      const maxTop = fabMaxTop(appSafeArea.bottom);
      const range = maxTop - FAB_VERTICAL_MARGIN;
      const ratio = range > 0 ? (y - FAB_VERTICAL_MARGIN) / range : 1;
      const safeRatio = Math.min(1, Math.max(0, ratio));
      fabRatioRef.current = safeRatio;
      try {
        window.localStorage.setItem(FAB_STORAGE_KEY, JSON.stringify({ side, ratio: safeRatio }));
      } catch {
        // localStorage 不可用时保持当前会话位置
      }
    },
    [appSafeArea.bottom],
  );

  const layoutFabY = useCallback(() => {
    const maxTop = fabMaxTop(appSafeArea.bottom);
    return FAB_VERTICAL_MARGIN + fabRatioRef.current * Math.max(0, maxTop - FAB_VERTICAL_MARGIN);
  }, [appSafeArea.bottom]);

  useEffect(() => {
    setFabY(layoutFabY());
  }, [layoutFabY]);

  useEffect(() => {
    const handleResize = () => setFabY(layoutFabY());
    window.addEventListener("resize", handleResize);
    window.visualViewport?.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      window.visualViewport?.removeEventListener("resize", handleResize);
    };
  }, [layoutFabY]);

  const setReveal = useCallback((value: number) => {
    handleRevealRef.current = value;
    setHandleReveal(value);
  }, []);

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
    // 返回固定遵循路由层级（上一级），不使用浏览器历史，避免从外部入口进入时退错页面。
    navigate(resolveParentPath(location.pathname, location.search));
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
        setReveal(0);
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
            setReveal(0);
            stopGesture();
            return;
          }
          if (Math.abs(dx) < DRAG_ACTIVATE_PX) {
            return;
          }
          activate();
          if (!openedAtStart) {
            // 跟手拉开抽屉：把手位移 + 面板随进度滑出，松手后按进度决定开合。
            const width = HANDLE_WIDTH(window.innerWidth);
            setReveal(Math.min(1, Math.max(0, dx / width)));
            setHandleDragX(Math.min(18, Math.max(0, dx) * 0.22));
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
          if (!openedAtStart && handleRevealRef.current >= HANDLE_REVEAL_RATIO) {
            swallowClickRef.current = true;
            setIsOpen(true);
          }
          setHandleDragX(0);
          setHandleDragging(false);
          setReveal(0);
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

  /**
   * 悬浮球拖动：Pointer Events + setPointerCapture 一套代码覆盖鼠标/触摸/触控笔。
   * 位移不足阈值时保留 click（点按开合菜单）；拖动松手后抑制 click，并按最近的一侧贴边。
   */
  const onFabPointerDown = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    if (isEditorRoute) return;
    // 不 preventDefault：保留随后的 click，用于点按开关
    const rect = event.currentTarget.getBoundingClientRect();
    fabDragRef.current = {
      dx: event.clientX - rect.left,
      dy: event.clientY - rect.top,
      startX: event.clientX,
      startY: event.clientY,
      active: false,
    };
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // 某些 WebView 下 capture 失败也能继续拖动
    }
  };

  const onFabPointerMove = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = fabDragRef.current;
    if (!drag) return;
    if (!drag.active) {
      const distance = Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY);
      if (distance < FAB_DRAG_THRESHOLD) return;
      drag.active = true;
      setFabDragging(true);
    }
    setFabDragPos(clampFabPosition(event.clientX - drag.dx, event.clientY - drag.dy));
  };

  const onFabPointerUp = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = fabDragRef.current;
    fabDragRef.current = null;
    if (!drag) return;
    try {
      event.currentTarget.releasePointerCapture(event.pointerId);
    } catch {
      // ignore
    }
    setFabDragging(false);
    if (!drag.active) {
      setFabDragPos(null);
      return;
    }
    const finalPos = clampFabPosition(event.clientX - drag.dx, event.clientY - drag.dy);
    const side: FabSide = finalPos.x + FAB_SIZE / 2 < window.innerWidth / 2 ? "left" : "right";
    setFabSide(side);
    setFabY(finalPos.y);
    setFabDragPos(null);
    persistFabPosition(side, finalPos.y);
    // 触摸拖动后浏览器可能不再补发 click，用时间窗兜底，避免吞掉下一次真实点按
    fabDragEndedAtRef.current = performance.now();
  };

  const onFabPointerCancel = () => {
    fabDragRef.current = null;
    setFabDragging(false);
    setFabDragPos(null);
  };

  const onFabKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    event.preventDefault();
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      const side: FabSide = event.key === "ArrowLeft" ? "left" : "right";
      setFabSide(side);
      persistFabPosition(side, fabY);
      return;
    }
    const nextY = clampFabPosition(0, fabY + (event.key === "ArrowUp" ? -24 : 24)).y;
    setFabY(nextY);
    persistFabPosition(fabSide, nextY);
  };

  const onFabClick = () => {
    if (swallowClickRef.current) {
      swallowClickRef.current = false;
      return;
    }
    if (performance.now() - fabDragEndedAtRef.current < 400) {
      return;
    }
    setIsOpen(!isOpen);
    setSidebarDragX(0);
    setHandleDragX(0);
    setReveal(0);
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
    setReveal(0);
  };

  useEffect(() => {
    const openSidebar = () => {
      setSidebarDragX(0);
      setHandleDragX(0);
      setReveal(0);
      setIsOpen(true);
    };
    window.addEventListener("od-mobile-nav-open", openSidebar);
    return () => window.removeEventListener("od-mobile-nav-open", openSidebar);
  }, []);

  const fabCondensedNow = fabCondensed && !isOpen && !fabDragging && !fabDragPos;
  const fabLayoutSize = fabCondensedNow ? FAB_CONDENSED_SIZE : FAB_SIZE;
  const fabLeft = fabDragPos
    ? fabDragPos.x
    : fabSide === "left"
      ? FAB_EDGE_MARGIN + appSafeArea.left
      : window.innerWidth - fabLayoutSize - FAB_EDGE_MARGIN - appSafeArea.right;
  const fabStyle: CSSProperties = {
    left: `${Math.round(fabLeft)}px`,
    top: `${Math.round(fabDragPos ? fabDragPos.y : fabY)}px`,
  };

  return (
    <div
      className={`mobile-nav-container${isImmersiveRoute ? " mobile-nav-container--immersive" : ""}`}
      style={mobileViewportStyle}
    >
      {isEditorRoute || isImmersiveRoute ? null : (
      <button
        type="button"
        className={`mobile-fab mobile-fab--${fabSide}${isOpen ? " mobile-fab--open" : ""}${fabCondensedNow ? " mobile-fab--condensed" : ""}${fabDragging ? " is-dragging" : ""}`}
        style={fabStyle}
        onPointerDown={onFabPointerDown}
        onPointerMove={onFabPointerMove}
        onPointerUp={onFabPointerUp}
        onPointerCancel={onFabPointerCancel}
        onKeyDown={onFabKeyDown}
        onClick={onFabClick}
        aria-label={isOpen ? "关闭主菜单" : "打开主菜单（可拖动贴边）"}
        aria-expanded={isOpen}
      >
        <span className="mobile-fab-icon" aria-hidden="true">
          {isOpen ? <X size={20} /> : <Menu size={22} />}
        </span>
      </button>
      )}
      {showBackButton ? (
        <button
          type="button"
          className="mobile-back-btn"
          onClick={handleBack}
          aria-label="返回"
        >
          <ArrowLeft size={18} />
        </button>
      ) : null}

      {isOpen || handleReveal > 0 ? (
        <div
          className={`mobile-overlay${handleReveal > 0 ? " mobile-overlay--revealing" : ""}`}
          onClick={onOverlayClick}
          style={handleReveal > 0 ? { opacity: handleReveal } : undefined}
        >
          <nav
            ref={sidebarRef}
            className={`mobile-sidebar${sidebarDragging ? " is-dragging" : ""}${handleReveal > 0 ? " mobile-sidebar--revealing" : ""}`}
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
              transform: handleReveal > 0
                ? `translateX(${(handleReveal - 1) * 100}%)`
                : `translateX(${sidebarDragX}px)`,
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
