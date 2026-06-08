import type { Dispatch, RefObject, SetStateAction } from "react";
import type {
  CharacterResponse,
  SaveResponse,
  SessionMapEdge,
  SessionMapNode,
} from "../data/apiAdapter";
import type { GameSessionStateBag } from "../game/useGameSession";
import type {
  EditingTurnState,
  RenderChatMessage,
  SideTab,
} from "../game/utils";
import {
  createGameUiPlatformCapabilities,
  type GameUiPlatform,
  type GameUiPlatformCapabilities,
} from "./capabilities";

export type GameUiRuntimeContext = {
  capabilities: GameUiPlatformCapabilities;
  session: {
    id: string;
    world_name: string;
    location: string;
    time_label: string | null;
    player_character_name: string | null;
    visible_characters: string[];
  } | null;
  world: {
    id: string;
    name: string;
  } | null;
  player: {
    id: string;
    name: string;
  } | null;
  world_characters: CharacterResponse[];
  messages: RenderChatMessage[];
  latest_narration: string;
  copyable_dialogue_text: string;
  scene_focus: {
    speaker: string;
    content: string;
    portrait_path?: string;
  } | null;
  visible_characters: string[];
  side_tabs: Array<{ key: string; label: string }>;
  active_side_tab: SideTab;
  active_attribute_content: string;
  draft_input: {
    value: string;
    set_value: Dispatch<SetStateAction<string>>;
    images: File[];
    set_images: Dispatch<SetStateAction<File[]>>;
    audios: File[];
    set_audios: Dispatch<SetStateAction<File[]>>;
    input_ref: RefObject<HTMLTextAreaElement | null>;
  };
  message_preferences: {
    auto_scroll_enabled: boolean;
    set_auto_scroll_enabled: Dispatch<SetStateAction<boolean>>;
  };
  editing: EditingTurnState | null;
  ui_state: {
    loading: boolean;
    page_error: string | null;
    submitting: boolean;
    branching: boolean;
    switching: boolean;
    retrying_token: string | null;
  };
  errors: {
    action_error: string | null;
  };
  current_save: SaveResponse | null;
  map_graph: {
    nodes: SessionMapNode[];
    edges: SessionMapEdge[];
  };
  message_state: {
    active_character_creation_keys: string[];
    expanded_director_trace_keys: Set<string>;
    set_expanded_director_trace_keys: Dispatch<SetStateAction<Set<string>>>;
    dismissed_proposal_keys: Set<string>;
    dismissed_retry_card_keys: Set<string>;
  };
  chat_messages_ref: RefObject<HTMLDivElement | null>;
};

export function createGameUiRuntimeContext(
  bag: GameSessionStateBag,
  platform: GameUiPlatform,
): GameUiRuntimeContext {
  return {
    capabilities: createGameUiPlatformCapabilities(platform),
    session: bag.session
      ? {
          id: bag.session.id,
          world_name: bag.session.world_name ?? "",
          location: bag.session.location ?? "",
          time_label: bag.session.time_label ?? null,
          player_character_name: bag.session.player_character_name ?? null,
          visible_characters: bag.session.visible_characters ?? [],
        }
      : null,
    world: bag.themeWorld
      ? {
          id: bag.themeWorld.id,
          name: bag.themeWorld.name,
        }
      : null,
    player: bag.playerCharacter
      ? {
          id: bag.playerCharacter.id,
          name: bag.playerCharacter.name,
        }
      : null,
    world_characters: bag.worldCharacters,
    messages: bag.renderedDialogueMessages,
    latest_narration: bag.latestNarration,
    copyable_dialogue_text: bag.copyableDialogueText,
    scene_focus: bag.showSceneFocus
      ? {
          speaker: bag.sceneFocusSpeaker,
          content: bag.sceneFocusContent,
          portrait_path: bag.activePortraitPath || undefined,
        }
      : null,
    visible_characters: bag.session?.visible_characters ?? [],
    side_tabs: bag.sideTabs,
    active_side_tab: bag.sideTab,
    active_attribute_content: bag.activeAttributeContent,
    draft_input: {
      value: bag.inputValue,
      set_value: bag.setInputValue,
      images: bag.inputImages,
      set_images: bag.setInputImages,
      audios: bag.inputAudios,
      set_audios: bag.setInputAudios,
      input_ref: bag.inputRef,
    },
    message_preferences: {
      auto_scroll_enabled: bag.chatAutoScrollEnabled,
      set_auto_scroll_enabled: bag.setChatAutoScrollEnabled,
    },
    editing: bag.editingTurn,
    ui_state: {
      loading: bag.loading,
      page_error: bag.error,
      submitting: bag.submitting,
      branching: bag.branching,
      switching: bag.switching,
      retrying_token: bag.retryingToken,
    },
    errors: {
      action_error: bag.actionError,
    },
    current_save: bag.currentSave,
    map_graph: {
      nodes: bag.mapGraphNodes,
      edges: bag.mapGraphEdges,
    },
    message_state: {
      active_character_creation_keys: bag.activeCharacterCreationKeys,
      expanded_director_trace_keys: bag.expandedDirectorTraceKeys,
      set_expanded_director_trace_keys: bag.setExpandedDirectorTraceKeys,
      dismissed_proposal_keys: bag.dismissedProposalKeys,
      dismissed_retry_card_keys: bag.dismissedRetryCardKeys,
    },
    chat_messages_ref: bag.chatMessagesRef,
  };
}
