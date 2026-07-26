import type { ReactNode } from "react";
import type { GameUiComponentRenderer } from "../components/GameUiRenderer";
import type { GameUiMountId } from "../data/gameUi";
import type { GameUiRuntimeActions } from "./actions";
import { GAME_UI_CATALOG } from "./catalog";
import { InputComposerComponent } from "./components/InputComposer";
import { MessageListComponent } from "./components/MessageList";
import {
  CharacterBarComponent,
  FloatingActionsComponent,
  NarrationCardComponent,
  SceneFocusComponent,
  SceneHeaderComponent,
  SidePanelTabsComponent,
} from "./components/PageComponents";
import type { GameUiRuntimeContext } from "./runtimeContext";

export type GameUiRegisteredComponentDefinition = {
  id: string;
  version: 1;
  propsSchema: Record<string, string>;
  render: GameUiComponentRenderer;
};

// 组件清单（id、props、动作、能力）的唯一来源是 shared/game-ui/catalog.json；
// 本文件只负责把目录中的组件接到各自的 React 渲染器上。
// ledger_book 只在世界框架内渲染（WorldFrameRuntimeView / GameUiPreview），
// 因此目录里有它但这里没有渲染器，注册表会自动跳过。
function createComponentRenderers(
  runtime: GameUiRuntimeContext,
  actions: GameUiRuntimeActions,
): Record<string, GameUiComponentRenderer> {
  return {
    scene_header: ({ node }) => <SceneHeaderComponent runtime={runtime} actions={actions} node={node} />,
    scene_focus: ({ node }) => <SceneFocusComponent runtime={runtime} actions={actions} node={node} />,
    character_bar: ({ node }) => <CharacterBarComponent runtime={runtime} actions={actions} node={node} />,
    narration_card: ({ node }) => <NarrationCardComponent runtime={runtime} actions={actions} node={node} />,
    message_list: ({ node }) => <MessageListComponent runtime={runtime} actions={actions} node={node} />,
    input_composer: ({ node }) => <InputComposerComponent runtime={runtime} actions={actions} node={node} />,
    side_panel_tabs: ({ node, renderSlot }) => <SidePanelTabsComponent runtime={runtime} actions={actions} node={node} renderSlot={renderSlot} />,
    floating_actions: ({ node }) => <FloatingActionsComponent runtime={runtime} actions={actions} node={node} />,
  };
}

export function createGameUiComponentRegistry(
  runtime: GameUiRuntimeContext,
  actions: GameUiRuntimeActions,
): Record<string, GameUiRegisteredComponentDefinition> {
  const renderers = createComponentRenderers(runtime, actions);
  return Object.fromEntries(
    GAME_UI_CATALOG.components
      .filter((component) => renderers[component.id])
      .map((component) => [
        component.id,
        {
          id: component.id,
          version: 1 as const,
          propsSchema: component.props,
          render: renderers[component.id],
        },
      ]),
  );
}

export function createGameUiComponentRenderers(
  runtime: GameUiRuntimeContext,
  actions: GameUiRuntimeActions,
): Partial<Record<string, GameUiComponentRenderer>> {
  const registry = createGameUiComponentRegistry(runtime, actions);
  return Object.fromEntries(
    Object.entries(registry).map(([key, definition]) => [key, definition.render]),
  );
}

export function createLegacyGameUiRuntimeMounts(
  runtime: GameUiRuntimeContext,
  actions: GameUiRuntimeActions,
): Partial<Record<GameUiMountId, ReactNode>> {
  return {
    scene_focus: <SceneFocusComponent runtime={runtime} actions={actions} />,
    character_bar: <CharacterBarComponent runtime={runtime} actions={actions} />,
    narration: <NarrationCardComponent runtime={runtime} actions={actions} />,
    message_list: <MessageListComponent runtime={runtime} actions={actions} />,
    input_area: <InputComposerComponent runtime={runtime} actions={actions} />,
    side_panel: <SidePanelTabsComponent runtime={runtime} actions={actions} />,
    floating_actions: <FloatingActionsComponent runtime={runtime} actions={actions} />,
  };
}
