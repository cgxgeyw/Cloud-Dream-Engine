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
  const cspNonce = React.useMemo(() => getDocumentCspNonce(), []);

  const headerMount = <SceneHeaderComponent runtime={runtime} actions={actions} />;
  const sceneMount = session ? (
    <section className="game-simple-top game-ui-panel">
      <div className="game-simple-top-main">
        <div className="game-simple-world">{session.world_name || "Current World"}</div>
        <div className="game-simple-place-row">
          {session.time_label ? <span className="game-simple-top-time">{session.time_label}</span> : null}
          <strong className="game-simple-top-place">{session.location || "Current Scene"}</strong>
        </div>
      </div>
      <div className="game-simple-meta">
        <span className="game-simple-meta-item">
          <strong>Player</strong>
          <span>{session.player_character_name || "Unset"}</span>
        </span>
        <span className="game-simple-meta-item">
          <strong>Present</strong>
          <span>{session.visible_characters?.length ? session.visible_characters.join(" / ") : "None"}</span>
        </span>
      </div>
    </section>
  ) : null;

  return (
    <div className="game-root game-root--session game-root--mobile-session game-ui-root" data-game-ui-scope={gameUiScopeId} style={runtimeBackgroundStyle}>
      {themeCustomCss ? <style nonce={cspNonce}>{themeCustomCss}</style> : null}
      {loading ? <div className="game-loading">Loading session...</div> : null}
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
