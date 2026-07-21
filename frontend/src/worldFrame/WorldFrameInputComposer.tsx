import { useEffect, useState } from "react";
import { Image, Keyboard, Mic, Send, Square, X } from "lucide-react";

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

  // 微信式语音模式：左侧同位按钮在麦克风/键盘间切换，中间大横条按住说话；
  // 松手发送、上滑取消由宿主原生桥驱动（见 GameUiSandboxRuntime）。
  if (runtime.draft_input.voice_mode) {
    const holdLabel = runtime.draft_input.is_recording
      ? runtime.draft_input.voice_cancel
        ? "\u677e\u5f00\u624b\u6307\uff0c\u53d6\u6d88\u53d1\u9001"
        : "\u677e\u5f00\u53d1\u9001"
      : "\u6309\u4f4f\u8bf4\u8bdd";
    return (
      <div className="game-input-area game-ui-panel">
        <div className="game-input-compose">
          <div className="game-input-actions">
            <button
              type="button"
              className="game-input-icon-btn game-ui-button"
              data-variant="ghost"
              onClick={() => actions.setVoiceMode(false)}
              title={"\u8fd4\u56de\u952e\u76d8"}
            >
              <Keyboard size={18} />
            </button>
            <button
              type="button"
              className={`game-voice-hold${runtime.draft_input.is_recording ? " game-voice-hold--recording" : ""}${runtime.draft_input.voice_cancel ? " game-voice-hold--cancel" : ""}`}
              onPointerDown={(event) => {
                event.preventDefault();
                if (submitting || runtime.draft_input.is_recording) {
                  return;
                }
                void actions.startRecording();
              }}
              onContextMenu={(event) => event.preventDefault()}
              style={{ touchAction: "none" }}
            >
              {holdLabel}
            </button>
          </div>
        </div>
        {runtime.errors.action_error ? <div className="game-input-bubble">{runtime.errors.action_error}</div> : null}
        {runtime.draft_input.microphone_error ? <div className="game-input-bubble">{runtime.draft_input.microphone_error}</div> : null}
        <style>{".game-voice-hold{flex:1;height:44px;border-radius:10px;border:1px solid rgba(127,127,127,0.35);background:rgba(255,255,255,0.72);color:inherit;font-size:16px;user-select:none;-webkit-user-select:none;touch-action:none;}.game-voice-hold--recording{animation:gameVoiceHoldPulse 1.1s ease-in-out infinite;border-color:#14b8a6;}.game-voice-hold--cancel{border-color:#dc2626;color:#dc2626;}@keyframes gameVoiceHoldPulse{0%,100%{transform:scale(1)}50%{transform:scale(0.96)}}"}</style>
      </div>
    );
  }

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
                onClick={() => void (runtime.capabilities.platform === "mobile"
                  ? actions.setVoiceMode(true)
                  : runtime.draft_input.is_recording
                    ? actions.stopRecording()
                    : actions.startRecording())}
                title={runtime.draft_input.is_recording ? "\u505c\u6b62\u5f55\u97f3" : runtime.capabilities.platform === "mobile" ? "\u8bed\u97f3\u8f93\u5165" : "\u5f55\u97f3"}
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
