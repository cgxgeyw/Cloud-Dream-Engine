import { useEffect, useState } from "react";
import { Image, Mic, Send, Square, X } from "lucide-react";

import type { GameUiComponentNode } from "../data/gameUi";
import type { GameUiRuntimeActions } from "../gameUiRuntime/actions";
import type { GameUiRuntimeContext } from "../gameUiRuntime/runtimeContext";

type Props = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
};

export function WorldFrameInputComposer({ runtime, actions, node }: Props) {
  const [draftValue, setDraftValue] = useState(runtime.draft_input.value);

  useEffect(() => {
    setDraftValue(runtime.draft_input.value);
  }, [runtime.draft_input.value]);

  if (!runtime.session) {
    return null;
  }

  const editing = Boolean(runtime.editing);
  const submitting = runtime.ui_state.submitting;
  const showImageButton = readBooleanProp(node, "show_image_button", true)
    && runtime.capabilities.supports_file_picker;
  const showAudioButton = readBooleanProp(node, "show_audio_button", true)
    && runtime.capabilities.supports_mic;
  const showSessionMeta = readBooleanProp(node, "show_session_meta", true);
  const enterToSubmit = readBooleanProp(node, "enter_to_submit", true);
  const placeholder = readStringProp(node, "placeholder", "\u8f93\u5165\u6d88\u606f\u6216\u884c\u52a8...");
  const submitLabel = readStringProp(node, "submit_label", "\u53d1\u9001");
  const canSubmit = !submitting && (
    draftValue.trim().length > 0
    || runtime.draft_input.images.length > 0
    || runtime.draft_input.audios.length > 0
  );

  const submit = () => {
    if (!canSubmit) {
      return;
    }
    void actions.submitMessage({ mode: editing ? "edit" : "submit", content: draftValue });
  };

  return (
    <div className="game-input-area game-ui-panel">
      {editing ? (
        <div className="game-input-mode">
          <div className="game-input-mode-copy">
            <div className="game-input-mode-title">
              {`\u6b63\u5728\u7f16\u8f91\u7b2c ${runtime.editing?.turnIndex ?? ""} \u8f6e`}
            </div>
          </div>
          <button
            type="button"
            className="game-message-action-btn game-ui-button"
            data-variant="ghost"
            disabled={submitting}
            onClick={actions.cancelEditingTurn}
          >
            {"\u53d6\u6d88"}
          </button>
        </div>
      ) : null}

      <div className="game-input-compose">
        <textarea
          ref={runtime.draft_input.input_ref}
          value={draftValue}
          onChange={(event) => {
            const value = event.target.value;
            setDraftValue(value);
            actions.setDraftValue(value);
            if (runtime.errors.action_error) {
              actions.clearActionError();
            }
          }}
          onKeyDown={(event) => {
            if (enterToSubmit && event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          placeholder={placeholder}
          className="game-textarea game-ui-textarea"
        />

        <div className={runtime.capabilities.platform === "mobile" ? "game-input-actions" : "game-input-toolbar"}>
          <div className={runtime.capabilities.platform === "mobile" ? undefined : "game-input-toolbar-left"}>
            {showImageButton ? (
              <button
                type="button"
                className={runtime.capabilities.platform === "mobile" ? "game-input-icon-btn game-ui-button" : "game-input-attach-btn game-ui-button"}
                data-variant="ghost"
                disabled={submitting}
                onClick={() => actions.pickImage()}
                title={"\u6dfb\u52a0\u56fe\u7247"}
              >
                <Image size={18} />
              </button>
            ) : null}
            {showAudioButton ? (
              <button
                type="button"
                className={runtime.capabilities.platform === "mobile" ? "game-input-icon-btn game-ui-button" : "game-input-attach-btn game-ui-button"}
                data-variant="ghost"
                disabled={submitting && !runtime.draft_input.is_recording}
                onClick={() => void (runtime.draft_input.is_recording ? actions.stopRecording() : actions.startRecording())}
                title={runtime.draft_input.is_recording ? "\u505c\u6b62\u5f55\u97f3" : "\u5f55\u97f3"}
              >
                {runtime.draft_input.is_recording ? <Square size={18} /> : <Mic size={18} />}
              </button>
            ) : null}
          </div>
          {runtime.capabilities.platform === "mobile" ? <div className="game-input-actions-spacer" /> : null}
          <button
            type="button"
            className={`game-submit-btn game-ui-button${runtime.capabilities.platform === "desktop" ? " game-submit-btn--inline" : ""}`}
            data-variant="primary"
            disabled={!canSubmit}
            onClick={submit}
          >
            <Send size={17} />
            <span>{submitting ? "\u53d1\u9001\u4e2d..." : submitLabel}</span>
          </button>
        </div>
      </div>

      {runtime.draft_input.images.length > 0 ? (
        <div className="game-input-images">
          {runtime.draft_input.images.map((file, index) => (
            <div key={"id" in file ? file.id : `${file.name}-${index}`} className="game-input-image-preview">
              {"preview_url" in file && file.preview_url ? <img src={file.preview_url} alt={file.name} /> : <span>{file.name}</span>}
              <button type="button" className="game-input-image-remove" onClick={() => actions.removeImage(index)} title={"\u79fb\u9664\u56fe\u7247"}>
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
      ) : null}

      {runtime.draft_input.audios.length > 0 ? (
        <div className="game-input-audios">
          {runtime.draft_input.audios.map((file, index) => (
            <div key={"id" in file ? file.id : `${file.name}-${index}`} className="game-input-audio-preview">
              <span className="game-input-audio-name">{file.name}</span>
              <button type="button" className="game-input-audio-remove" onClick={() => actions.removeAudio(index)} title={"\u79fb\u9664\u5f55\u97f3"}>
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
      ) : null}

      {runtime.errors.action_error ? <div className="game-input-bubble">{runtime.errors.action_error}</div> : null}
      {runtime.draft_input.microphone_error ? <div className="game-input-bubble">{runtime.draft_input.microphone_error}</div> : null}

      {showSessionMeta ? (
        <div className={`game-session-meta${runtime.capabilities.platform === "mobile" ? " game-session-meta--compact" : ""}`}>
          {runtime.current_save ? <span className="game-session-meta-id">{`\u5b58\u6863 ${runtime.current_save.id}`}</span> : null}
          <span className="game-session-meta-id">{`\u4f1a\u8bdd ${runtime.session.id}`}</span>
        </div>
      ) : null}
    </div>
  );
}

function readBooleanProp(node: GameUiComponentNode | undefined, key: string, fallback: boolean): boolean {
  const value = node?.props?.[key];
  return typeof value === "boolean" ? value : fallback;
}

function readStringProp(node: GameUiComponentNode | undefined, key: string, fallback: string): string {
  const value = node?.props?.[key];
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}
