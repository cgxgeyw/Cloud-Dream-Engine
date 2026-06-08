import React from "react";
import { useNavigate } from "react-router-dom";
import { GameUiRenderer } from "../../components/GameUiRenderer";
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
        />
      ) : null}
    </div>
  );
};
