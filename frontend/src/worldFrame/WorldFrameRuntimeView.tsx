import { useMemo, useRef } from "react";

import { GameUiRenderer, type GameUiRenderContext } from "../components/GameUiRenderer";
import type { GameUiActionReference, GameUiPropValue } from "../data/gameUi";
import { parseSwitchProposal, resolvePlayerActionMode } from "../game/utils";
import type { GameUiRuntimeActions } from "../gameUiRuntime/actions";
import { evaluateGameUiExpression } from "../gameUiRuntime/expression";
import { createGameUiComponentRenderers } from "../gameUiRuntime/registry";
import { hydrateGameUiRuntimeContext, type WorldFrameRuntimePayload } from "./runtimeSnapshot";
import type { WorldFrameAction } from "./protocol";
import { WorldFrameInputComposer } from "./WorldFrameInputComposer";

type Props = {
  payload: WorldFrameRuntimePayload;
  sendAction: (action: WorldFrameAction) => Promise<void>;
};

export function WorldFrameRuntimeView({ payload, sendAction }: Props) {
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const messagesRef = useRef<HTMLDivElement | null>(null);
  const runtime = useMemo(
    () => hydrateGameUiRuntimeContext(payload.snapshot, { input: inputRef, messages: messagesRef }),
    [payload.snapshot],
  );
  const actions = useMemo(() => createFrameActions(sendAction), [sendAction]);
  const componentRenderers = useMemo(() => ({
    ...createGameUiComponentRenderers(runtime, actions),
    input_composer: ({ node }: Parameters<NonNullable<ReturnType<typeof createGameUiComponentRenderers>[string]>>[0]) => (
      <WorldFrameInputComposer runtime={runtime} actions={actions} node={node} />
    ),
  }), [actions, runtime]);
  const runtimeData = useMemo(() => ({
    session: runtime.session,
    world: runtime.world,
    player: runtime.player,
    attributes: runtime.attributes,
    attributes_by_owner: runtime.attributes_by_owner,
    attribute_items: runtime.attribute_items,
    messages: runtime.messages,
    visible_characters: runtime.visible_characters,
    capabilities: runtime.capabilities,
    ui_state: runtime.ui_state,
    errors: runtime.errors,
    side_tabs: runtime.side_tabs,
    active_side_tab: runtime.active_side_tab,
    active_attribute_content: runtime.active_attribute_content,
    scene_focus: runtime.scene_focus,
    latest_narration: runtime.latest_narration,
    draft_input: payload.snapshot.draft_input,
    viewport: payload.snapshot.viewport,
  }), [runtime]);

  const handleDslAction = async (action: GameUiActionReference, context: GameUiRenderContext) => {
    await dispatchDslAction(action, context, runtime, actions, sendAction);
  };

  const viewportStyle = {
    ...payload.rootStyle,
    "--game-visual-viewport-height": `${payload.snapshot.viewport.height}px`,
    "--world-safe-area-top": `${payload.snapshot.viewport.safe_area.top}px`,
    "--world-safe-area-right": `${payload.snapshot.viewport.safe_area.right}px`,
    "--world-safe-area-bottom": `${payload.snapshot.viewport.safe_area.bottom}px`,
    "--world-safe-area-left": `${payload.snapshot.viewport.safe_area.left}px`,
    height: payload.platform === "mobile" ? `${payload.snapshot.viewport.height}px` : "100%",
  } as React.CSSProperties;

  return (
    <div
      className={`game-root game-root--session game-root--${payload.platform}-session game-ui-root`}
      data-game-ui-scope={payload.scopeId}
      data-world-frame-runtime="3"
      style={viewportStyle}
    >
      {payload.stylesheet ? <style>{payload.stylesheet}</style> : null}
      {runtime.ui_state.loading ? <div className="game-loading">{"\u6b63\u5728\u52a0\u8f7d\u4f1a\u8bdd..."}</div> : null}
      {runtime.ui_state.page_error ? <div className="game-loading game-error-text">{runtime.ui_state.page_error}</div> : null}
      {!runtime.ui_state.loading && !runtime.ui_state.page_error && runtime.session ? (
        <GameUiRenderer
          document={payload.document}
          componentRenderers={componentRenderers}
          runtimeData={runtimeData}
          evaluateCondition={(expression, context) => evaluateGameUiExpression(expression, {
            ...context.data,
            ...context.locals,
            state: context.state,
            capabilities: runtime.capabilities,
            ui_state: runtime.ui_state,
            errors: runtime.errors,
          })}
          onAction={handleDslAction}
        />
      ) : null}
    </div>
  );
}

async function dispatchDslAction(
  action: GameUiActionReference,
  context: GameUiRenderContext,
  runtime: ReturnType<typeof hydrateGameUiRuntimeContext>,
  actions: GameUiRuntimeActions,
  sendAction: (action: WorldFrameAction) => Promise<void>,
) {
  const actionId = action.id.replace(/^@/, "");
  const args = resolveActionArgs(action.args ?? {}, context);
  const contentTemplate = action.content_template ?? action.content ?? readStringArg(args, "content");
  const content = renderActionText(contentTemplate, context).trim();

  switch (actionId) {
    case "submit_message":
      await actions.submitMessage({
        mode: resolvePlayerActionMode(action.mode || readStringArg(args, "mode")),
        content: content || undefined,
        turnIndex: readNumberArg(args, "turn_index"),
      });
      return;
    case "edit_turn_start":
      actions.startEditingTurn(content, readNumberArg(args, "turn_index") ?? 0);
      return;
    case "edit_turn_cancel": actions.cancelEditingTurn(); return;
    case "branch_from_current": await actions.branchFromCurrent(); return;
    case "retry_turn": await actions.retryTurn(readStringArg(args, "retry_token")); return;
    case "accept_switch_proposal": {
      const proposalKey = readStringArg(args, "proposal_key");
      const proposal = runtime.messages
        .map((message) => parseSwitchProposal(message))
        .find((item) => item?.key === proposalKey);
      if (!proposal) {
        throw new Error(`Switch proposal not found: ${proposalKey}`);
      }
      await actions.acceptSwitchProposal(proposal);
      return;
    }
    case "dismiss_switch_proposal": actions.dismissSwitchProposal(readStringArg(args, "proposal_key")); return;
    case "dismiss_retry_card": actions.dismissRetryCard(readStringArg(args, "card_key")); return;
    case "copy_text": await actions.copyText(readStringArg(args, "text") || content); return;
    case "switch_side_tab": actions.switchSideTab(readStringArg(args, "tab_key")); return;
    case "navigate_back": actions.navigateBack(); return;
    case "navigate_home": await sendAction({ type: "navigate", target: "home" }); return;
    case "navigate_settings": actions.navigateSettings(); return;
    case "navigate_debug": actions.navigateDebug(); return;
    case "pick_image": actions.pickImage(); return;
    case "remove_image": actions.removeImage(readNumberArg(args, "index") ?? -1); return;
    case "start_recording": await actions.startRecording(); return;
    case "stop_recording": actions.stopRecording(); return;
    case "remove_audio": actions.removeAudio(readNumberArg(args, "index") ?? -1); return;
  }
}

function createFrameActions(send: (action: WorldFrameAction) => Promise<void>): GameUiRuntimeActions {
  return {
    clearActionError: () => void send({ type: "clear-action-error" }),
    setDraftValue: (value) => void send({ type: "set-draft-value", value }),
    setAutoScrollEnabled: (enabled) => void send({ type: "set-auto-scroll", enabled }),
    submitMessage: (options = {}) => send({ type: "submit-message", options }),
    startEditingTurn: (content, turnIndex) => void send({ type: "start-editing-turn", content, turnIndex }),
    cancelEditingTurn: () => void send({ type: "cancel-editing-turn" }),
    branchFromCurrent: () => send({ type: "branch-from-current" }),
    retryTurn: (retryToken) => send({ type: "retry-turn", retryToken }),
    acceptSwitchProposal: (proposal) => send({ type: "accept-switch-proposal", proposal }),
    dismissSwitchProposal: (proposalKey) => void send({ type: "dismiss-switch-proposal", proposalKey }),
    dismissRetryCard: (cardKey) => void send({ type: "dismiss-retry-card", cardKey }),
    copyText: (text) => send({ type: "copy-text", text }),
    switchSideTab: (tabKey) => void send({ type: "switch-side-tab", tabKey }),
    navigateBack: () => void send({ type: "navigate", target: "back" }),
    navigateSettings: () => void send({ type: "navigate", target: "settings" }),
    navigateDebug: () => void send({ type: "navigate", target: "debug" }),
    pickImage: () => void send({ type: "pick-image" }),
    removeImage: (index) => void send({ type: "remove-image", index }),
    startRecording: () => send({ type: "start-recording" }),
    stopRecording: () => void send({ type: "stop-recording" }),
    removeAudio: (index) => void send({ type: "remove-audio", index }),
    addAudio: () => undefined,
    attachInputComposerBridge: () => undefined,
  };
}

function renderActionText(template: string, context: GameUiRenderContext): string {
  return template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, expression: string) => {
    const value = resolveTemplatePath(context, expression.trim());
    return Array.isArray(value)
      ? value.map((item) => String(item)).filter(Boolean).join("\u3001")
      : value == null ? "" : String(value);
  });
}

function resolveActionArgs(
  args: Record<string, GameUiPropValue>,
  context: GameUiRenderContext,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(args).map(([key, value]) => [key, resolveActionValue(value, context)]),
  );
}

function resolveActionValue(value: GameUiPropValue, context: GameUiRenderContext): unknown {
  if (typeof value === "string") {
    return value.startsWith("$")
      ? resolveTemplatePath(context, value)
      : renderActionText(value, context);
  }
  if (Array.isArray(value)) {
    return value.map((item) => resolveActionValue(item, context));
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, resolveActionValue(item, context)]),
    );
  }
  return value;
}

function readStringArg(args: Record<string, unknown>, key: string): string {
  const value = args[key];
  return value == null ? "" : String(value);
}

function readNumberArg(args: Record<string, unknown>, key: string): number | undefined {
  const value = Number(args[key]);
  return Number.isFinite(value) ? value : undefined;
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
