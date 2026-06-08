import { lazy, Suspense, useState, type ReactNode } from "react";
import { ArrowLeft, Copy } from "lucide-react";
import type { GameUiComponentNode } from "../../data/gameUi";
import type { GameUiRuntimeActions } from "../actions";
import type { GameUiRuntimeContext } from "../runtimeContext";

const SessionMapGraph = lazy(() => import("../../components/SessionMapGraph").then((module) => ({ default: module.SessionMapGraph })));

type RuntimeComponentProps = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
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

export function SceneHeaderComponent({ runtime, actions, node }: RuntimeComponentProps) {
  if (!runtime.session) {
    return null;
  }

  const showWorldName = readBooleanProp(node, "show_world_name", true);
  const showLocation = readBooleanProp(node, "show_location", true);
  const showTimeLabel = readBooleanProp(node, "show_time_label", true);
  const showPlayerIdentity = readBooleanProp(node, "show_player_identity", true);
  const playerIdentityFormat = readStringProp(node, "player_identity_format", "label");
  const showVisibleCharacters = readBooleanProp(node, "show_visible_characters", false);
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
        {runtime.copyable_dialogue_text && runtime.capabilities.platform !== "mobile" ? (
          <button
            type="button"
            className="game-quick-btn game-ui-button"
            data-variant="ghost"
            onClick={() => void actions.copyText(runtime.copyable_dialogue_text)}
          >
            <Copy size={14} />
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

export function SidePanelTabsComponent({ runtime, actions, node }: RuntimeComponentProps) {
  const [mobileDrawerOpen, setMobileDrawerOpen] = useState(false);
  const showMapTab = readBooleanProp(node, "show_map_tab", true);
  const showAttributeTabs = readBooleanProp(node, "show_attribute_tabs", true);
  const emptyText = readStringProp(node, "empty_text", "暂无状态信息。");
  const drawerLabel = readStringProp(node, "drawer_label", "\u72b6\u6001");

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
        {visibleTabs.length === 0 ? <div className="game-card">{emptyText}</div> : null}
        {runtime.active_side_tab === "map" && (runtime.capabilities.platform !== "mobile" || mobileDrawerOpen) ? (
          <Suspense fallback={<div className="game-map-graph" />}>
            <SessionMapGraph
              key={runtime.capabilities.platform === "mobile" ? `mobile-map-${mobileDrawerOpen ? "open" : "closed"}` : "desktop-map"}
              nodes={runtime.map_graph.nodes}
              edges={runtime.map_graph.edges}
              compact={runtime.capabilities.platform === "mobile"}
            />
          </Suspense>
        ) : null}
        {runtime.active_side_tab.startsWith("attribute:") && runtime.active_attribute_content ? (
          <div className="game-card game-attribute-tab-content">{runtime.active_attribute_content}</div>
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
      <aside className={mobileDrawerClassName}>
        <button
          type="button"
          className="game-status-handle game-ui-button"
          data-variant="ghost"
          aria-label={mobileDrawerOpen ? "\u5173\u95ed\u72b6\u6001\u62bd\u5c49" : "\u6253\u5f00\u72b6\u6001\u62bd\u5c49"}
          aria-expanded={mobileDrawerOpen}
          onClick={() => setMobileDrawerOpen((isOpen) => !isOpen)}
        >
          {drawerLabel}
        </button>
        <div className="game-status-drawer game-ui-panel" data-variant="sidebar">
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
