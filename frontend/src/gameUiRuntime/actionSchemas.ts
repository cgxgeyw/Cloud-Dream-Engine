export const GAME_UI_ACTION_IDS = [
  "submit_message",
  "edit_turn_start",
  "edit_turn_cancel",
  "branch_from_current",
  "retry_turn",
  "accept_switch_proposal",
  "dismiss_switch_proposal",
  "dismiss_retry_card",
  "copy_text",
  "switch_side_tab",
  "navigate_home",
  "navigate_settings",
  "navigate_debug",
  "pick_image",
  "remove_image",
  "start_recording",
  "stop_recording",
  "remove_audio",
] as const;

export type GameUiActionId = (typeof GAME_UI_ACTION_IDS)[number];

export type GameUiActionSchema = {
  id: GameUiActionId;
  description: string;
  input: Record<string, string>;
};

export const GAME_UI_ACTION_SCHEMAS: readonly GameUiActionSchema[] = [
  { id: "submit_message", description: "Submit current draft input.", input: { mode: "submit|edit|resend", content: "string?", turn_index: "number?" } },
  { id: "edit_turn_start", description: "Start editing a player turn.", input: { content: "string", turn_index: "number" } },
  { id: "edit_turn_cancel", description: "Cancel current editing state.", input: {} },
  { id: "branch_from_current", description: "Create a new branch from current state.", input: {} },
  { id: "retry_turn", description: "Retry a failed model step.", input: { retry_token: "string" } },
  { id: "accept_switch_proposal", description: "Accept a switch-character proposal.", input: { proposal_key: "string" } },
  { id: "dismiss_switch_proposal", description: "Dismiss a switch-character proposal.", input: { proposal_key: "string" } },
  { id: "dismiss_retry_card", description: "Dismiss a retry card.", input: { card_key: "string" } },
  { id: "copy_text", description: "Copy arbitrary text.", input: { text: "string" } },
  { id: "switch_side_tab", description: "Switch active side tab.", input: { tab_key: "string" } },
  { id: "navigate_home", description: "Navigate to home page.", input: {} },
  { id: "navigate_settings", description: "Navigate to settings page.", input: {} },
  { id: "navigate_debug", description: "Navigate to current session debug page.", input: {} },
  { id: "pick_image", description: "Open the image picker or append selected images into the current draft.", input: { files: "File[]?" } },
  { id: "remove_image", description: "Remove an image attachment from the current draft.", input: { index: "number" } },
  { id: "start_recording", description: "Start audio recording for the current draft.", input: {} },
  { id: "stop_recording", description: "Stop audio recording and append the captured audio into the current draft.", input: {} },
  { id: "remove_audio", description: "Remove an audio attachment from the current draft.", input: { index: "number" } },
] as const;
