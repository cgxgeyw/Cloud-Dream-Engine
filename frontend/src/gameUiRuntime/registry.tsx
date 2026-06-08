import type { ReactNode } from "react";
import type { GameUiComponentRenderer } from "../components/GameUiRenderer";
import type { GameUiMountId } from "../data/gameUi";
import type { GameUiRuntimeActions } from "./actions";
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

export function createGameUiComponentRegistry(
  runtime: GameUiRuntimeContext,
  actions: GameUiRuntimeActions,
): Record<string, GameUiRegisteredComponentDefinition> {
  return {
    scene_header: {
      id: "scene_header",
      version: 1,
      propsSchema: {
        show_world_name: "boolean",
        show_location: "boolean",
        show_time_label: "boolean",
        show_player_identity: "boolean",
        player_identity_format: "label|action_phrase",
        show_visible_characters: "boolean",
        title_mode: "desktop|mobile",
      },
      render: ({ node }) => <SceneHeaderComponent runtime={runtime} actions={actions} node={node} />,
    },
    scene_focus: {
      id: "scene_focus",
      version: 1,
      propsSchema: {
        show_avatar: "boolean",
        show_line: "boolean",
        avatar_variant: "string",
      },
      render: ({ node }) => <SceneFocusComponent runtime={runtime} actions={actions} node={node} />,
    },
    character_bar: {
      id: "character_bar",
      version: 1,
      propsSchema: {
        empty_text: "string",
        max_items: "number",
      },
      render: ({ node }) => <CharacterBarComponent runtime={runtime} actions={actions} node={node} />,
    },
    narration_card: {
      id: "narration_card",
      version: 1,
      propsSchema: {
        title: "string",
        show_copy_button: "boolean",
        empty_text: "string",
      },
      render: ({ node }) => <NarrationCardComponent runtime={runtime} actions={actions} node={node} />,
    },
    message_list: {
      id: "message_list",
      version: 1,
      propsSchema: {
        auto_scroll: "boolean",
        show_pending_state: "boolean",
        show_agent_reasoning: "boolean",
      },
      render: ({ node }) => <MessageListComponent runtime={runtime} actions={actions} node={node} />,
    },
    input_composer: {
      id: "input_composer",
      version: 1,
      propsSchema: {
        placeholder: "string",
        submit_label: "string",
        editing_submit_label: "string",
        show_image_button: "boolean",
        show_audio_button: "boolean",
        show_session_meta: "boolean",
        enter_to_submit: "boolean",
      },
      render: ({ node }) => <InputComposerComponent runtime={runtime} actions={actions} node={node} />,
    },
    side_panel_tabs: {
      id: "side_panel_tabs",
      version: 1,
      propsSchema: {
        show_map_tab: "boolean",
        show_attribute_tabs: "boolean",
        empty_text: "string",
      },
      render: ({ node }) => <SidePanelTabsComponent runtime={runtime} actions={actions} node={node} />,
    },
    floating_actions: {
      id: "floating_actions",
      version: 1,
      propsSchema: {
        show_back: "boolean",
        show_debug: "boolean",
        show_settings: "boolean",
        layout: "row|column|wrap",
      },
      render: ({ node }) => <FloatingActionsComponent runtime={runtime} actions={actions} node={node} />,
    },
  };
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
