import React from "react";
import { useNavigate } from "react-router-dom";
import { GameUiRenderer, type GameUiRenderContext } from "../../components/GameUiRenderer";
import type { GameUiActionReference } from "../../data/gameUi";
import { createGameUiRuntimeActions } from "../../gameUiRuntime/actions";
import { InputComposerComponent } from "../../gameUiRuntime/components/InputComposer";
import { MessageListComponent } from "../../gameUiRuntime/components/MessageList";
import {
  CharacterBarComponent,
  FloatingActionsComponent,
  NarrationCardComponent,
  SceneFocusComponent,
  SceneHeaderComponent,
  SidePanelTabsComponent,
} from "../../gameUiRuntime/components/PageComponents";
import { createGameUiComponentRenderers } from "../../gameUiRuntime/registry";
import { createGameUiRuntimeContext } from "../../gameUiRuntime/runtimeContext";
import { getDocumentCspNonce } from "../cspNonce";
import type { GameSessionStateBag } from "../useGameSession";
import { resolvePlayerActionMode } from "../utils";

type MobileViewportState = {
  height: number;
};

function useMobileVisualViewport(): MobileViewportState {
  const [viewport, setViewport] = React.useState<MobileViewportState>(() => ({
    height: typeof window === "undefined" ? 0 : Math.round(window.visualViewport?.height ?? window.innerHeight),
  }));

  React.useEffect(() => {
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

export const MobileGameShell: React.FC<{
  bag: GameSessionStateBag;
}> = ({ bag }) => {
  const navigate = useNavigate();

  const {
    session,
    loading,
    error,
    gameUiScopeId,
    runtimeBackgroundStyle,
    themeCustomCss,
    parsedGameUi,
  } = bag;

  const runtime = createGameUiRuntimeContext(bag, "mobile");
  const actions = createGameUiRuntimeActions(bag, runtime, navigate);
  const componentRenderers = createGameUiComponentRenderers(runtime, actions);
  const mobileViewport = useMobileVisualViewport();
  const cspNonce = React.useMemo(() => getDocumentCspNonce(), []);
  const runtimeData = React.useMemo(
    () => ({
      session: runtime.session,
      world: runtime.world,
      player: runtime.player,
      attributes: runtime.attributes,
      attributes_by_owner: runtime.attributes_by_owner,
      attribute_items: runtime.attribute_items,
      messages: runtime.messages,
      visible_characters: runtime.visible_characters,
    }),
    [runtime],
  );
  const handleDslAction = React.useCallback(
    async (action: GameUiActionReference, context: GameUiRenderContext) => {
      if (action.id !== "@submit_message" && action.id !== "submit_message") {
        return;
      }
      const content = renderActionText(action.content_template ?? action.content ?? "", context).trim();
      if (!content) {
        return;
      }
      await actions.submitMessage({ mode: resolvePlayerActionMode(action.mode), content });
    },
    [actions],
  );
  const viewportHeight = mobileViewport.height > 0 ? `${mobileViewport.height}px` : "100dvh";
  const mobileViewportStyle = React.useMemo(
    () => ({
      ...runtimeBackgroundStyle,
      "--game-visual-viewport-height": viewportHeight,
      height: viewportHeight,
      minHeight: viewportHeight,
      maxHeight: viewportHeight,
      overflow: "hidden",
    }) as React.CSSProperties,
    [runtimeBackgroundStyle, viewportHeight],
  );
  const mobileKeyboardCss = React.useMemo(
    () => `
[data-game-ui-scope="${gameUiScopeId}"].game-root--mobile-session {
  height: var(--game-visual-viewport-height, 100dvh) !important;
  min-height: var(--game-visual-viewport-height, 100dvh) !important;
  max-height: var(--game-visual-viewport-height, 100dvh) !important;
  overflow: hidden !important;
}
[data-game-ui-scope="${gameUiScopeId}"].game-root--mobile-session .game-ui-layout,
[data-game-ui-scope="${gameUiScopeId}"].game-root--mobile-session .game-ui-layout > .game-ui-node {
  height: 100% !important;
  min-height: 0 !important;
  max-height: 100% !important;
}
[data-game-ui-scope="${gameUiScopeId}"].game-root--mobile-session .game-input-area {
  scroll-margin-bottom: 18px;
}
`,
    [gameUiScopeId],
  );

  const headerMount = <SceneHeaderComponent runtime={runtime} actions={actions} />;
  const sceneMount = session ? (
    <section className="game-simple-top game-ui-panel">
      <div className="game-simple-top-main">
        <div className="game-simple-world">{session.world_name || "当前世界"}</div>
        <div className="game-simple-place-row">
          {session.time_label ? <span className="game-simple-top-time">{session.time_label}</span> : null}
          <strong className="game-simple-top-place">{session.location || "当前场景"}</strong>
        </div>
      </div>
      <div className="game-simple-meta">
        <span className="game-simple-meta-item">
          <strong>玩家</strong>
          <span>{session.player_character_name || "未设定"}</span>
        </span>
        <span className="game-simple-meta-item">
          <strong>在场</strong>
          <span>{session.visible_characters?.length ? session.visible_characters.join(" / ") : "无人"}</span>
        </span>
      </div>
    </section>
  ) : null;

  return (
    <div className="game-root game-root--session game-root--mobile-session game-ui-root" data-game-ui-scope={gameUiScopeId} style={mobileViewportStyle}>
      {themeCustomCss ? <style nonce={cspNonce}>{themeCustomCss}</style> : null}
      <style nonce={cspNonce}>{mobileKeyboardCss}</style>
      {loading ? <div className="game-loading">正在加载会话...</div> : null}
      {error ? <div className="game-loading game-error-text">{error}</div> : null}
      {!loading && !error && session ? (
        <GameUiRenderer
          document={parsedGameUi.document}
          mounts={{
            header: headerMount,
            scene: sceneMount,
            scene_focus: <SceneFocusComponent runtime={runtime} actions={actions} />,
            character_bar: <CharacterBarComponent runtime={runtime} actions={actions} />,
            narration: <NarrationCardComponent runtime={runtime} actions={actions} />,
            message_list: <MessageListComponent runtime={runtime} actions={actions} />,
            input_area: <InputComposerComponent runtime={runtime} actions={actions} />,
            side_panel: <SidePanelTabsComponent runtime={runtime} actions={actions} />,
            floating_actions: <FloatingActionsComponent runtime={runtime} actions={actions} />,
          }}
          componentRenderers={componentRenderers}
          runtimeData={runtimeData}
          onAction={handleDslAction}
        />
      ) : null}
    </div>
  );
};

function renderActionText(template: string, context: GameUiRenderContext): string {
  return template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, expression: string) => {
    const value = resolveTemplatePath(context, expression.trim());
    if (Array.isArray(value)) {
      return value.map((item) => String(item)).filter(Boolean).join("\u3001");
    }
    return value == null ? "" : String(value);
  });
}

function resolveTemplatePath(context: GameUiRenderContext, expression: string): unknown {
  const normalized = expression.startsWith("$") ? expression.slice(1) : expression;
  const [root, ...parts] = normalized.split(".").filter(Boolean);
  let current: unknown = root === "state"
    ? context.state
    : root === "data"
      ? context.data
      : root in context.locals
        ? context.locals[root]
        : context.data[root];
  for (const part of parts) {
    if (current == null || typeof current !== "object") {
      return "";
    }
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}
