import type { RefObject } from "react";
import type {
  CharacterResponse,
  RuntimeAttributeItem,
  SaveResponse,
  SessionRuntimeAttributesResponse,
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

export type RuntimeAttributesByOwner = Record<
  string,
  Record<string, Record<string, unknown>>
>;

export type GameUiDraftAttachment = File | {
  id: string;
  name: string;
  size: number;
  type: string;
  preview_url?: string;
};

function isCurrentPlayerAttributeOwner(
  ownerId: string,
  sessionId: string,
  playerCharacterId: string,
): boolean {
  return ownerId === playerCharacterId
    || ownerId === `session_character:${playerCharacterId}`
    || ownerId === `${sessionId}:${playerCharacterId}`
    || ownerId === `session_character:${sessionId}:${playerCharacterId}`;
}

export function buildRuntimeAttributeMaps(
  runtimeAttributes: SessionRuntimeAttributesResponse,
  sessionId: string,
  playerCharacterId?: string,
): {
  attributes: Record<string, unknown>;
  attributesByOwner: RuntimeAttributesByOwner;
} {
  const attributes: Record<string, unknown> = {};
  const attributesByOwner: RuntimeAttributesByOwner = {};
  const groups = [
    ...runtimeAttributes.session_attributes,
    ...runtimeAttributes.character_attributes,
  ];

  for (const group of groups) {
    const ownerType = group.owner_type.trim();
    const ownerId = group.owner_id.trim();
    let ownerAttributes: Record<string, unknown> | null = null;
    if (ownerType && ownerId) {
      const ownersOfType = attributesByOwner[ownerType] ?? {};
      ownerAttributes = ownersOfType[ownerId] ?? {};
      attributesByOwner[ownerType] = ownersOfType;
      ownersOfType[ownerId] = ownerAttributes;
    }

    for (const item of group.items) {
      if (ownerAttributes) {
        ownerAttributes[item.key] = item.value;
      }
    }
  }

  const flatGroups = [...runtimeAttributes.session_attributes];
  if (playerCharacterId) {
    flatGroups.push(
      ...runtimeAttributes.character_attributes.filter((group) =>
        isCurrentPlayerAttributeOwner(
          group.owner_id.trim(),
          sessionId,
          playerCharacterId,
        )
      ),
    );
  }
  for (const group of flatGroups) {
    for (const item of group.items) {
      attributes[item.key] = item.value;
    }
  }

  return { attributes, attributesByOwner };
}

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
  attributes: Record<string, unknown>;
  attributes_by_owner: RuntimeAttributesByOwner;
  attribute_items: RuntimeAttributeItem[];
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
    images: GameUiDraftAttachment[];
    audios: GameUiDraftAttachment[];
    is_recording: boolean;
    microphone_error: string | null;
    input_ref: RefObject<HTMLTextAreaElement | null>;
  };
  message_preferences: {
    auto_scroll_enabled: boolean;
  };
  editing: EditingTurnState | null;
  ui_state: {
    loading: boolean;
    page_error: string | null;
    submitting: boolean;
    streaming_response_active: boolean;
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
    dismissed_proposal_keys: Set<string>;
    dismissed_retry_card_keys: Set<string>;
  };
  chat_messages_ref: RefObject<HTMLDivElement | null>;
};

export function createGameUiRuntimeContext(
  bag: GameSessionStateBag,
  platform: GameUiPlatform,
): GameUiRuntimeContext {
  const { attributes, attributesByOwner } = buildRuntimeAttributeMaps(
    bag.runtimeAttributes,
    bag.session?.id ?? bag.sessionId,
    bag.playerCharacter?.id,
  );

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
    attributes,
    attributes_by_owner: attributesByOwner,
    attribute_items: [...bag.runtimeAttributes.session_attributes, ...bag.runtimeAttributes.character_attributes]
      .flatMap((group) => group.items),
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
      images: bag.inputImages,
      audios: bag.inputAudios,
      is_recording: false,
      microphone_error: null,
      input_ref: bag.inputRef,
    },
    message_preferences: {
      auto_scroll_enabled: bag.chatAutoScrollEnabled,
    },
    editing: bag.editingTurn,
    ui_state: {
      loading: bag.loading,
      page_error: bag.error,
      submitting: bag.submitting,
      streaming_response_active: bag.streamingResponseActive,
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
      dismissed_proposal_keys: bag.dismissedProposalKeys,
      dismissed_retry_card_keys: bag.dismissedRetryCardKeys,
    },
    chat_messages_ref: bag.chatMessagesRef,
  };
}
