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

export const DesktopGameShell: React.FC<{
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

  const runtime = createGameUiRuntimeContext(bag, "desktop");
  const actions = createGameUiRuntimeActions(bag, runtime, navigate);
  const componentRenderers = createGameUiComponentRenderers(runtime, actions);
  const cspNonce = React.useMemo(() => getDocumentCspNonce(), []);
  const runtimeData = React.useMemo(
    () => ({
      session: runtime.session,
      world: runtime.world,
      player: runtime.player,
      attributes: runtime.attributes,
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
      await actions.submitMessage({ mode: action.mode as never, content });
    },
    [actions],
  );

  const headerMount = <SceneHeaderComponent runtime={runtime} actions={actions} />;
  const sceneMount = session ? (
    <section className="game-scene game-ui-panel">
      <div className="game-scene-header">
        <div className="game-scene-header-main">
          {session.time_label ? <span className="game-scene-time">{session.time_label}</span> : null}
        </div>
        <div className="game-scene-header-side">
          {session.player_character_name ? (
            <span className="game-scene-badge game-ui-badge" data-variant="info">
              {`当前玩家：${session.player_character_name}`}
            </span>
          ) : null}
          {runtime.copyable_dialogue_text ? (
            <button
              type="button"
              className="game-quick-btn game-scene-copy-btn game-ui-button"
              data-variant="ghost"
              onClick={() => void actions.copyText(runtime.copyable_dialogue_text)}
            >
              复制对话
            </button>
          ) : null}
        </div>
      </div>
    </section>
  ) : null;

  return (
    <div className="game-root game-root--session game-root--desktop-session game-ui-root" data-game-ui-scope={gameUiScopeId} style={runtimeBackgroundStyle}>
      {themeCustomCss ? <style nonce={cspNonce}>{themeCustomCss}</style> : null}
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
