import { createRef, type RefObject } from "react";

import type { GameUiPlatform, GameUiDocument } from "../data/gameUi";
import type { GameSessionStateBag } from "../game/useGameSession";
import { createGameUiPlatformCapabilities } from "../gameUiRuntime/capabilities";
import {
  createGameUiRuntimeContext,
  type GameUiDraftAttachment,
  type GameUiRuntimeContext,
} from "../gameUiRuntime/runtimeContext";

export type WorldFrameViewportSnapshot = {
  width: number;
  height: number;
  offset_top: number;
  keyboard_height: number;
  safe_area: {
    top: number;
    right: number;
    bottom: number;
    left: number;
  };
};

export type GameUiRuntimeSnapshot = Omit<
  GameUiRuntimeContext,
  "draft_input" | "message_state" | "chat_messages_ref"
> & {
  draft_input: {
    value: string;
    images: Exclude<GameUiDraftAttachment, File>[];
    audios: Exclude<GameUiDraftAttachment, File>[];
    is_recording: boolean;
    microphone_error: string | null;
  };
  message_state: {
    active_character_creation_keys: string[];
    expanded_director_trace_keys: string[];
    dismissed_proposal_keys: string[];
    dismissed_retry_card_keys: string[];
  };
  viewport: WorldFrameViewportSnapshot;
};

export type WorldFrameRuntimePayload = {
  platform: GameUiPlatform;
  document: GameUiDocument;
  stylesheet: string;
  scopeId: string;
  rootStyle: Record<string, string | number>;
  snapshot: GameUiRuntimeSnapshot;
};

export function createGameUiRuntimeSnapshot(
  bag: GameSessionStateBag,
  platform: GameUiPlatform,
  options: {
    images: Exclude<GameUiDraftAttachment, File>[];
    audios: Exclude<GameUiDraftAttachment, File>[];
    isRecording: boolean;
    microphoneError: string | null;
    viewport: WorldFrameViewportSnapshot;
  },
): GameUiRuntimeSnapshot {
  const runtime = createGameUiRuntimeContext(bag, platform);
  return {
    capabilities: createGameUiPlatformCapabilities(platform),
    session: runtime.session,
    world: runtime.world,
    player: runtime.player,
    world_characters: runtime.world_characters,
    attributes: runtime.attributes,
    attributes_by_owner: runtime.attributes_by_owner,
    attribute_items: runtime.attribute_items,
    messages: runtime.messages,
    latest_narration: runtime.latest_narration,
    copyable_dialogue_text: runtime.copyable_dialogue_text,
    scene_focus: runtime.scene_focus,
    visible_characters: runtime.visible_characters,
    side_tabs: runtime.side_tabs,
    active_side_tab: runtime.active_side_tab,
    active_attribute_content: runtime.active_attribute_content,
    draft_input: {
      value: runtime.draft_input.value,
      images: options.images,
      audios: options.audios,
      is_recording: options.isRecording,
      microphone_error: options.microphoneError,
    },
    message_preferences: runtime.message_preferences,
    editing: runtime.editing,
    ui_state: runtime.ui_state,
    errors: runtime.errors,
    current_save: runtime.current_save,
    map_graph: runtime.map_graph,
    message_state: {
      active_character_creation_keys: runtime.message_state.active_character_creation_keys,
      expanded_director_trace_keys: Array.from(runtime.message_state.expanded_director_trace_keys),
      dismissed_proposal_keys: Array.from(runtime.message_state.dismissed_proposal_keys),
      dismissed_retry_card_keys: Array.from(runtime.message_state.dismissed_retry_card_keys),
    },
    viewport: options.viewport,
  };
}

export function hydrateGameUiRuntimeContext(
  snapshot: GameUiRuntimeSnapshot,
  refs: {
    input: RefObject<HTMLTextAreaElement | null>;
    messages: RefObject<HTMLDivElement | null>;
  } = {
    input: createRef<HTMLTextAreaElement>(),
    messages: createRef<HTMLDivElement>(),
  },
): GameUiRuntimeContext {
  return {
    ...snapshot,
    draft_input: {
      ...snapshot.draft_input,
      input_ref: refs.input,
    },
    message_state: {
      active_character_creation_keys: snapshot.message_state.active_character_creation_keys,
      expanded_director_trace_keys: new Set(snapshot.message_state.expanded_director_trace_keys),
      dismissed_proposal_keys: new Set(snapshot.message_state.dismissed_proposal_keys),
      dismissed_retry_card_keys: new Set(snapshot.message_state.dismissed_retry_card_keys),
    },
    chat_messages_ref: refs.messages,
  };
}
