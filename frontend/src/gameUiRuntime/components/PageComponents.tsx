import { lazy, memo, Suspense, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { ArrowLeft, Copy } from "lucide-react";
import type { GameUiComponentNode } from "../../data/gameUi";
import type { GameUiRuntimeActions } from "../actions";
import type { GameUiRuntimeContext } from "../runtimeContext";

const SessionMapGraph = lazy(() => import("../../components/SessionMapGraph").then((module) => ({ default: module.SessionMapGraph })));

type RuntimeComponentProps = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
  renderSlot?: (slotName: string) => ReactNode;
};

function readBooleanProp(
  node: GameUiComponentNode | undefined,
  key: string,
  fallback: boolean,
): boolean {
  const value = node?.props?.[key];
  return typeof value === "boolean" ? value : fallback;
}

function readStringProp(
  node: GameUiComponentNode | undefined,
  key: string,
  fallback = "",
): string {
  const value = node?.props?.[key];
  return typeof value === "string" && value.trim() ? value : fallback;
}

function hasSameSidePanelConfig(
  left: GameUiComponentNode | undefined,
  right: GameUiComponentNode | undefined,
): boolean {
  if (left === right) {
    return true;
  }
  if (!left || !right) {
    return false;
  }
  return left.component === right.component
    && left.class_name === right.class_name
    && left.variant === right.variant
    && JSON.stringify(left.props ?? {}) === JSON.stringify(right.props ?? {});
}

export function SceneHeaderComponent({ runtime, actions, node }: RuntimeComponentProps) {
  if (!runtime.session) {
    return null;
  }
  const sessionId = runtime.session.id;

  const showWorldName = readBooleanProp(node, "show_world_name", true);
  const showLocation = readBooleanProp(node, "show_location", true);
  const showTimeLabel = readBooleanProp(node, "show_time_label", true);
  const showPlayerIdentity = readBooleanProp(node, "show_player_identity", true);
  const playerIdentityFormat = readStringProp(node, "player_identity_format", "label");
  const showVisibleCharacters = readBooleanProp(node, "show_visible_characters", false);
  const showCopyButton = readBooleanProp(node, "show_copy_button", true);
  const showSessionId = readBooleanProp(node, "show_session_id", false);
  const showSessionIdCopyButton = readBooleanProp(node, "show_session_id_copy_button", true);
  const sessionIdLabel = readStringProp(node, "session_id_label", "会话 ID");
  const titleMode = readStringProp(node, "title_mode", runtime.capabilities.platform);
  const isMobileTitle = titleMode === "mobile";

  return (
    <div className={isMobileTitle ? "game-simple-top game-ui-panel" : "game-header"}>
      <div className={isMobileTitle ? "game-simple-top-main" : "game-header-left"}>
        <div className="game-title-group">
          {showWorldName ? <div className="game-simple-world">{runtime.session.world_name || runtime.world?.name || "当前世界"}</div> : null}
          {showLocation ? (
            <div className="game-simple-place-row">
              <strong className={isMobileTitle ? "game-simple-place" : "game-scene-name"}>
                {runtime.session.location || "当前场景"}
              </strong>
            </div>
          ) : null}
        </div>
      </div>
      <div className={isMobileTitle ? "game-simple-meta" : "game-header-meta"}>
        {showTimeLabel && runtime.session.time_label ? (
          <span className="game-simple-meta-item">
            <strong>时间</strong>
            <span>{runtime.session.time_label}</span>
          </span>
        ) : null}
        {!isMobileTitle && showPlayerIdentity && runtime.session.player_character_name ? (
          playerIdentityFormat === "action_phrase" ? (
            <span className="game-simple-meta-item game-simple-meta-item--action-phrase">
              <span className="game-action-phrase-prefix">以</span>
              <span className="game-action-phrase-name">{runtime.session.player_character_name}</span>
              <span className="game-action-phrase-suffix">之名行动</span>
            </span>
          ) : (
            <span className="game-simple-meta-item">
              <strong>玩家</strong>
              <span>{runtime.session.player_character_name}</span>
            </span>
          )
        ) : null}
        {showVisibleCharacters && runtime.visible_characters.length > 0 ? (
          <span className="game-simple-meta-item">
            <strong>在场</strong>
            <span>{runtime.visible_characters.join(" / ")}</span>
          </span>
        ) : null}
        {showCopyButton && runtime.copyable_dialogue_text && runtime.capabilities.platform !== "mobile" ? (
          <button
            type="button"
            className="game-quick-btn game-ui-button"
            data-variant="ghost"
            onClick={() => void actions.copyText(runtime.copyable_dialogue_text)}
          >
            <Copy size={14} />
          </button>
        ) : null}
        {showSessionId ? (
          <button
            type="button"
            className="game-session-diagnostic game-ui-button"
            data-variant="ghost"
            onClick={() => showSessionIdCopyButton && void actions.copyText(sessionId)}
            title={showSessionIdCopyButton ? `复制${sessionIdLabel}` : sessionIdLabel}
            aria-label={showSessionIdCopyButton ? `复制${sessionIdLabel} ${sessionId}` : sessionIdLabel}
          >
            <span className="game-session-diagnostic-label">{sessionIdLabel}</span>
            <code>{sessionId}</code>
            {showSessionIdCopyButton ? <Copy size={12} aria-hidden="true" /> : null}
          </button>
        ) : null}
      </div>
    </div>
  );
}

export function SceneFocusComponent({ runtime, node }: RuntimeComponentProps) {
  if (!runtime.scene_focus) {
    return null;
  }

  const showAvatar = readBooleanProp(node, "show_avatar", true);
  const showLine = readBooleanProp(node, "show_line", true);

  return (
    <div className="game-scene-center">
      {showAvatar ? (
        <div className={`game-avatar game-ui-avatar${runtime.scene_focus.portrait_path ? " game-avatar--image" : ""}`} data-variant={readStringProp(node, "avatar_variant", "focus")}>
          {runtime.scene_focus.portrait_path ? (
            <img src={runtime.scene_focus.portrait_path} alt={runtime.scene_focus.speaker} className="game-avatar-image" />
          ) : (
            runtime.scene_focus.speaker
          )}
        </div>
      ) : null}
      {showLine ? <div className="game-current-line">{runtime.scene_focus.content}</div> : null}
    </div>
  );
}

export function CharacterBarComponent({ runtime, node }: RuntimeComponentProps) {
  const showPlayer = readBooleanProp(node, "show_player", false);
  const playerName = runtime.session?.player_character_name?.trim();
  const characters = [
    ...(showPlayer && playerName ? [`\u73a9\u5bb6\uff1a${playerName}`] : []),
    ...runtime.visible_characters.map((name) => `\u5728\u573a\uff1a${name}`),
  ];

  if (characters.length === 0) {
    const emptyText = readStringProp(node, "empty_text");
    return emptyText ? <div className="game-card">{emptyText}</div> : null;
  }

  const maxItems = Number(node?.props?.max_items);
  const visibleCharacters = Number.isFinite(maxItems) && maxItems > 0
    ? characters.slice(0, maxItems)
    : characters;

  return (
    <div className="game-scene-characters">
      {visibleCharacters.map((name) => (
        <span key={name} className="game-scene-char game-ui-chip" data-variant="character">
          {name}
        </span>
      ))}
    </div>
  );
}

export function NarrationCardComponent({ runtime, actions, node }: RuntimeComponentProps) {
  const title = readStringProp(node, "title", "旁白");
  const showCopyButton = readBooleanProp(node, "show_copy_button", runtime.capabilities.platform === "mobile");
  const emptyText = readStringProp(node, "empty_text", "暂无旁白。");
  const content = runtime.latest_narration || emptyText;

  return (
    <div className="game-narration-panel game-ui-panel">
      <div className="game-narration-label">
        <span>{title}</span>
        {showCopyButton && runtime.latest_narration ? (
          <button
            type="button"
            className="game-message-action-btn game-message-action-btn--copy game-ui-button"
            data-variant="ghost"
            onClick={() => void actions.copyText(runtime.latest_narration)}
            aria-label="复制旁白"
            title="复制旁白"
          >
            <Copy size={12} />
          </button>
        ) : null}
      </div>
      <div className="game-narration-content">{content}</div>
    </div>
  );
}

function SidePanelTabs({ runtime, actions, node, renderSlot }: RuntimeComponentProps) {
  const [mobileDrawerOpen, setMobileDrawerOpen] = useState(false);
  const [handleTop, setHandleTop] = useState<number | null>(null);
  const [handleDragging, setHandleDragging] = useState(false);
  const drawerSurfaceRef = useRef<HTMLElement | null>(null);
  const handleRef = useRef<HTMLButtonElement | null>(null);
  const handleDragRef = useRef<{ pointerId: number; startY: number; startTop: number; moved: boolean } | null>(null);
  const suppressHandleClickRef = useRef(false);
  const showMapTab = readBooleanProp(node, "show_map_tab", true);
  const showAttributeTabs = readBooleanProp(node, "show_attribute_tabs", true);
  const emptyText = readStringProp(node, "empty_text", "暂无状态信息。");
  const drawerLabel = readStringProp(node, "drawer_label", "\u72b6\u6001");
  const customContent = renderSlot?.("content");

  const clampHandleTop = (nextTop: number): number => {
    const surface = drawerSurfaceRef.current;
    const handle = handleRef.current;
    if (!surface || !handle) {
      return nextTop;
    }
    const surfaceHeight = surface.getBoundingClientRect().height;
    const handleHeight = handle.getBoundingClientRect().height;
    if (surfaceHeight <= 0 || handleHeight <= 0) {
      return nextTop;
    }
    const maxTop = Math.max(8, surfaceHeight - handleHeight - 8);
    return Math.min(Math.max(nextTop, 8), maxTop);
  };

  const beginHandleDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) {
      return;
    }
    const surface = drawerSurfaceRef.current;
    if (!surface) {
      return;
    }
    const surfaceRect = surface.getBoundingClientRect();
    const handleRect = event.currentTarget.getBoundingClientRect();
    handleDragRef.current = {
      pointerId: event.pointerId,
      startY: event.clientY,
      startTop: handleRect.top - surfaceRect.top,
      moved: false,
    };
    setHandleDragging(true);
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // 指针可能已经释放，忽略即可。
    }
  };

  const moveHandleDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = handleDragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    if (!drag.moved && Math.abs(event.clientY - drag.startY) < 6) {
      return;
    }
    drag.moved = true;
    setHandleTop(clampHandleTop(drag.startTop + event.clientY - drag.startY));
  };

  const endHandleDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const drag = handleDragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    handleDragRef.current = null;
    setHandleDragging(false);
    suppressHandleClickRef.current = drag.moved;
    try {
      event.currentTarget.releasePointerCapture(drag.pointerId);
    } catch {
      // capture 可能已经释放，忽略即可。
    }
  };

  // 旋转屏幕或调整窗口后，把贴边按钮拉回可视范围。
  useEffect(() => {
    if (handleTop === null) {
      return;
    }
    const reclamp = () => {
      setHandleTop((current) => (current === null ? current : clampHandleTop(current)));
    };
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handleTop]);

  const visibleTabs = runtime.side_tabs.filter((tab) => {
    if (tab.key === "map") {
      return showMapTab;
    }
    if (tab.key.startsWith("attribute:")) {
      return showAttributeTabs;
    }
    return true;
  });

  const tabsContent = (
    <>
      {visibleTabs.length > 0 ? (
        <div className="game-tabs">
          {visibleTabs.map((tab) => (
            <button
              key={tab.key}
              type="button"
              onClick={() => actions.switchSideTab(tab.key)}
              className={`game-tab game-ui-button${runtime.active_side_tab === tab.key ? " game-tab--active" : ""}`}
              data-variant={runtime.active_side_tab === tab.key ? "primary" : "ghost"}
            >
              {tab.label}
            </button>
          ))}
        </div>
      ) : null}
      <div className="game-panel game-side-content game-ui-panel" data-variant="sidebar">
        {customContent ? customContent : null}
        {!customContent && visibleTabs.length === 0 ? <div className="game-card">{emptyText}</div> : null}
        {!customContent ? (
          <div
            className="game-side-map-slot"
            hidden={runtime.active_side_tab !== "map"
              || (runtime.capabilities.platform === "mobile" && !mobileDrawerOpen)}
          >
            <Suspense fallback={<div className="game-map-graph" />}>
              <SessionMapGraph
                nodes={runtime.map_graph.nodes}
                edges={runtime.map_graph.edges}
                compact={runtime.capabilities.platform === "mobile"}
                visible={runtime.active_side_tab === "map"
                  && (runtime.capabilities.platform !== "mobile" || mobileDrawerOpen)}
              />
            </Suspense>
          </div>
        ) : null}
        {!customContent && runtime.active_side_tab.startsWith("attribute:") && runtime.active_attribute_content ? (
          runtime.active_attribute_items.length > 0 ? (
            <div className="game-attribute-items">
              {runtime.active_attribute_items.map((item) => {
                const presentation = typeof item.display_policy.presentation === "string"
                  ? item.display_policy.presentation
                  : "value";
                const maximum = typeof item.display_policy.max === "number" && item.display_policy.max > 0
                  ? item.display_policy.max
                  : 100;
                const numericValue = typeof item.value === "number" ? item.value : null;
                const meterPercent = numericValue === null
                  ? 0
                  : Math.max(0, Math.min(100, (numericValue / maximum) * 100));
                const values = Array.isArray(item.value) ? item.value : null;
                return (
                  <section className="game-attribute-item" key={item.schema_id} data-presentation={presentation}>
                    <div className="game-attribute-item-heading">
                      <span>{item.label || item.key}</span>
                      {presentation === "meter" && numericValue !== null ? <strong>{numericValue}</strong> : null}
                    </div>
                    {presentation === "meter" && numericValue !== null ? (
                      <div className="game-attribute-meter" aria-label={`${item.label || item.key} ${numericValue}`}>
                        <span style={{ width: `${meterPercent}%` }} />
                      </div>
                    ) : values ? (
                      <div className="game-attribute-list">
                        {values.map((value, index) => <span key={`${item.schema_id}-${index}`}>{String(value)}</span>)}
                      </div>
                    ) : (
                      <div className="game-attribute-value">{String(item.value ?? "")}</div>
                    )}
                  </section>
                );
              })}
            </div>
          ) : <div className="game-card game-attribute-tab-content">{runtime.active_attribute_content}</div>
        ) : null}
      </div>
    </>
  );

  if (runtime.capabilities.platform === "mobile") {
    const mobileDrawerClassName = [
      "game-status",
      "game-status--mobile-drawer",
      mobileDrawerOpen ? "game-status--mobile-drawer-open" : "",
      runtime.active_side_tab === "map" ? "game-status--map-active" : "",
      runtime.active_side_tab.startsWith("attribute:") ? "game-status--attribute-active" : "",
    ].filter(Boolean).join(" ");

    return (
      <aside className={mobileDrawerClassName} ref={drawerSurfaceRef}>
        <button
          ref={handleRef}
          type="button"
          className={`game-status-handle game-ui-button${handleDragging ? " game-status-handle--dragging" : ""}`}
          data-variant="ghost"
          aria-label={mobileDrawerOpen ? "\u5173\u95ed\u72b6\u6001\u62bd\u5c49" : "\u6253\u5f00\u72b6\u6001\u62bd\u5c49"}
          aria-expanded={mobileDrawerOpen}
          style={{ touchAction: "none", ...(handleTop !== null ? { top: handleTop } : null) }}
          onPointerDown={beginHandleDrag}
          onPointerMove={moveHandleDrag}
          onPointerUp={endHandleDrag}
          onPointerCancel={endHandleDrag}
          onClick={() => {
            if (suppressHandleClickRef.current) {
              suppressHandleClickRef.current = false;
              return;
            }
            setMobileDrawerOpen((isOpen) => !isOpen);
          }}
        >
          {drawerLabel}
        </button>
        <div className="game-status-drawer game-ui-panel" data-variant="sidebar">
          <button
            type="button"
            className="game-status-drawer-close game-ui-button"
            data-variant="ghost"
            aria-label="关闭状态抽屉"
            onClick={() => setMobileDrawerOpen(false)}
          >
            {"收起"}
          </button>
          {tabsContent}
        </div>
      </aside>
    );
  }

  return (
    <aside className="game-status">
      {tabsContent}
    </aside>
  );
}

export const SidePanelTabsComponent = memo(
  SidePanelTabs,
  (previous, next) => {
    // 自定义插槽可能依赖完整 runtime，交由默认渲染路径处理。内置状态栏只关心
    // 以下稳定数据；流式消息变化时跳过它，避免地图/属性面板参与每个 token 的刷新。
    if (previous.node?.slots?.content || next.node?.slots?.content) {
      return false;
    }
    return hasSameSidePanelConfig(previous.node, next.node)
      && previous.runtime.capabilities.platform === next.runtime.capabilities.platform
      && previous.runtime.side_tabs === next.runtime.side_tabs
      && previous.runtime.active_side_tab === next.runtime.active_side_tab
      && previous.runtime.active_attribute_content === next.runtime.active_attribute_content
      && previous.runtime.active_attribute_items === next.runtime.active_attribute_items
      && previous.runtime.map_graph.nodes === next.runtime.map_graph.nodes
      && previous.runtime.map_graph.edges === next.runtime.map_graph.edges;
  },
);

export function FloatingActionsComponent({ runtime, actions, node }: RuntimeComponentProps) {
  const showBack = readBooleanProp(node, "show_back", true);
  const showDebug = readBooleanProp(node, "show_debug", true);
  const showSettings = readBooleanProp(node, "show_settings", true);
  const backLabel = readStringProp(node, "back_label");
  const debugLabel = readStringProp(node, "debug_label", "\u8c03\u8bd5");
  const settingsLabel = readStringProp(node, "settings_label", "\u8bbe\u7f6e");
  const layout = readStringProp(node, "layout", runtime.capabilities.platform === "mobile" ? "row" : "row");
  const className = layout === "column"
    ? "game-header-actions game-header-actions--column"
    : runtime.capabilities.platform === "mobile"
      ? "game-mobile-shell-actions"
      : "game-header-actions";

  return (
    <div className={className}>
      {showBack ? (
        <button type="button" onClick={actions.navigateBack} className="game-back-btn game-ui-button" data-variant="ghost">
          <ArrowLeft size={18} />
          {backLabel ? <span>{backLabel}</span> : null}
        </button>
      ) : null}
      {showDebug && runtime.session ? (
        <button type="button" onClick={actions.navigateDebug} className="game-quick-btn game-ui-button" data-variant="ghost">
          {debugLabel}
        </button>
      ) : null}
      {showSettings ? (
        <button type="button" onClick={actions.navigateSettings} className="game-quick-btn game-ui-button" data-variant="ghost">
          {settingsLabel}
        </button>
      ) : null}
    </div>
  );
}

export function renderPageSlotContent(content: ReactNode) {
  return content;
}
