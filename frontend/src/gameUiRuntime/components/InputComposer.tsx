import { useEffect, useId, useRef, useState } from "react";
import { Image, Mic, Send, Square } from "lucide-react";
import type { GameUiComponentNode } from "../../data/gameUi";
import type { GameUiRuntimeActions } from "../actions";
import type { GameUiRuntimeContext } from "../runtimeContext";

type InputComposerComponentProps = {
  runtime: GameUiRuntimeContext;
  actions: GameUiRuntimeActions;
  node?: GameUiComponentNode;
};

function readBooleanProp(
  node: GameUiComponentNode | undefined,
  key: string,
  fallback: boolean,
): boolean {
  const value = node?.props?.[key];
  return typeof value === "boolean" ? value : fallback;
}

function readStringProp(
  node: GameUiComponentNode | undefined,
  key: string,
  fallback: string,
): string {
  const value = node?.props?.[key];
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}

export function InputComposerComponent({ runtime, actions, node }: InputComposerComponentProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [micError, setMicError] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const imageInputId = useId();

  if (!runtime.session) {
    return null;
  }

  const checkMicPermission = async (): Promise<boolean> => {
    try {
      if (navigator.permissions?.query) {
        const status = await navigator.permissions.query({ name: "microphone" as PermissionName });
        if (status.state === "denied") {
          setMicError("麦克风权限被拒绝。");
          return false;
        }
      }

      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((track) => track.stop());
      setMicError(null);
      return true;
    } catch (errorLike) {
      const errorState = errorLike as { name?: string };
      if (errorState.name === "NotAllowedError") {
        setMicError("麦克风权限被拒绝。");
      } else if (errorState.name === "NotFoundError") {
        setMicError("未找到可用麦克风。");
      } else {
        setMicError(`麦克风不可用：${errorState.name || "未知错误"}`);
      }
      return false;
    }
  };

  const startRecordingInternal = async () => {
    setMicError(null);
    const allowed = await checkMicPermission();
    if (!allowed) {
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mediaRecorder = new MediaRecorder(stream);
      mediaRecorderRef.current = mediaRecorder;
      audioChunksRef.current = [];
      mediaRecorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          audioChunksRef.current.push(event.data);
        }
      };
      mediaRecorder.onstop = () => {
        const blob = new Blob(audioChunksRef.current, { type: mediaRecorder.mimeType });
        const extension = mediaRecorder.mimeType.includes("webm")
          ? "webm"
          : mediaRecorder.mimeType.includes("ogg")
            ? "ogg"
            : "mp3";
        const file = new File(
          [blob],
          `recording_${new Date().toISOString().slice(0, 19).replace(/[:\-T]/g, "")}.${extension}`,
          { type: mediaRecorder.mimeType },
        );
        runtime.draft_input.set_audios((previous) => [...previous, file]);
        stream.getTracks().forEach((track) => track.stop());
      };
      mediaRecorder.start();
      setIsRecording(true);
    } catch {
      setMicError("启动录音失败。");
    }
  };

  const stopRecordingInternal = () => {
    mediaRecorderRef.current?.stop();
    setIsRecording(false);
  };

  useEffect(() => {
    actions.attachInputComposerBridge({
      openImagePicker: () => {
        document.getElementById(imageInputId)?.click();
      },
      startRecording: startRecordingInternal,
      stopRecording: stopRecordingInternal,
    });

    return () => {
      actions.attachInputComposerBridge(null);
    };
  }, [actions, imageInputId]);

  const submitMode = runtime.editing ? "edit" : "submit";
  const placeholder = readStringProp(node, "placeholder", "输入消息或行动...");
  const submitLabel = readStringProp(node, "submit_label", "发送");
  const editingSubmitLabel = readStringProp(node, "editing_submit_label", "保存并重试");
  const showImageButton = readBooleanProp(node, "show_image_button", true)
    && runtime.capabilities.supports_file_picker;
  const showAudioButton = readBooleanProp(node, "show_audio_button", true)
    && runtime.capabilities.supports_mic;
  const showSessionMeta = readBooleanProp(node, "show_session_meta", true);
  const enterToSubmit = readBooleanProp(node, "enter_to_submit", true);

  return (
    <div className="game-input-area game-ui-panel">
      {runtime.editing ? (
        <div className="game-input-mode">
          <div className="game-input-mode-copy">
            <div className="game-input-mode-title">{`正在编辑第 ${runtime.editing.turnIndex} 轮`}</div>
            <div className="game-input-mode-text">提交后会回退到这一轮，并重新生成后续内容。</div>
          </div>
          <div className="game-input-mode-actions">
            <button
              type="button"
              className="game-message-action-btn game-message-action-btn--confirm game-ui-button"
              data-variant="primary"
              disabled={runtime.ui_state.submitting || !runtime.draft_input.value.trim()}
              onClick={() => void actions.submitMessage({ mode: "edit" })}
            >
              {editingSubmitLabel}
            </button>
            <button
              type="button"
              className="game-message-action-btn game-message-action-btn--ghost game-ui-button"
              data-variant="ghost"
              disabled={runtime.ui_state.submitting}
              onClick={actions.cancelEditingTurn}
            >
              取消
            </button>
          </div>
        </div>
      ) : null}

      <div className="game-input-compose">
        <textarea
          ref={runtime.draft_input.input_ref}
          value={runtime.draft_input.value}
          onChange={(event) => {
            runtime.draft_input.set_value(event.target.value);
            if (runtime.errors.action_error) {
              actions.clearActionError();
            }
          }}
          onKeyDown={(event) => {
            if (enterToSubmit && event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void actions.submitMessage({ mode: submitMode });
            }
          }}
          placeholder={placeholder}
          className="game-textarea game-ui-textarea"
        />

        {runtime.capabilities.platform === "mobile" ? (
          <div className="game-input-actions">
            <input
              type="file"
              accept="image/*"
              multiple
              id={imageInputId}
              style={{ display: "none" }}
              onChange={(event) => {
                const files = Array.from(event.target.files || []);
                actions.pickImage(files);
                event.target.value = "";
              }}
            />
            {showAudioButton ? (
              <button type="button" className={`game-input-icon-btn game-ui-button${isRecording ? " game-input-icon-btn--recording" : ""}`} data-variant="ghost" onClick={() => void (isRecording ? actions.stopRecording() : actions.startRecording())} title={isRecording ? "停止录音" : "录音"}>
                {isRecording ? <Square size={20} /> : <Mic size={20} />}
              </button>
            ) : null}
            {showImageButton ? (
              <button type="button" className="game-input-icon-btn game-ui-button" data-variant="ghost" onClick={() => actions.pickImage()} title="添加图片">
                <Image size={20} />
              </button>
            ) : null}
            <div className="game-input-actions-spacer" />
            <button
              type="button"
              onClick={() => void actions.submitMessage({ mode: "submit" })}
              disabled={runtime.ui_state.submitting || (!runtime.draft_input.value.trim() && runtime.draft_input.images.length === 0 && runtime.draft_input.audios.length === 0)}
              className="game-submit-btn game-ui-button"
              data-variant="primary"
            >
              <Send size={18} />
              <span>{runtime.ui_state.submitting ? "\u53d1\u9001\u4e2d..." : submitLabel}</span>
            </button>
          </div>
        ) : (
          <div className="game-input-toolbar">
            <div className="game-input-toolbar-left">
              <input
                type="file"
                accept="image/*"
                multiple
                id={imageInputId}
                style={{ display: "none" }}
                onChange={(event) => {
                  const files = Array.from(event.target.files || []);
                  actions.pickImage(files);
                  event.target.value = "";
                }}
              />
              {showImageButton ? (
                <button type="button" className="game-input-attach-btn game-ui-button" data-variant="ghost" onClick={() => actions.pickImage()} title="添加图片">
                  <Image size={18} />
                </button>
              ) : null}
              {showAudioButton ? (
                <button type="button" className={`game-input-attach-btn game-ui-button${isRecording ? " game-input-attach-btn--recording" : ""}`} data-variant="ghost" onClick={() => void (isRecording ? actions.stopRecording() : actions.startRecording())} title={isRecording ? "停止录音" : "录音"}>
                  {isRecording ? <Square size={18} /> : <Mic size={18} />}
                </button>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => void actions.submitMessage({ mode: "submit" })}
              disabled={runtime.ui_state.submitting || (!runtime.draft_input.value.trim() && runtime.draft_input.images.length === 0 && runtime.draft_input.audios.length === 0)}
              className="game-submit-btn game-submit-btn--inline game-ui-button"
              data-variant="primary"
            >
              <Send size={16} />
              <span>{runtime.ui_state.submitting ? "发送中..." : submitLabel}</span>
            </button>
          </div>
        )}
      </div>

      {runtime.draft_input.images.length > 0 ? (
        <div className="game-input-images">
          {runtime.draft_input.images.map((file, index) => (
            <div key={`${file.name}-${index}`} className="game-input-image-preview">
              <img src={URL.createObjectURL(file)} alt={`Preview ${file.name}`} />
              <button
                type="button"
                className="game-input-image-remove"
                onClick={() => actions.removeImage(index)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      ) : null}

      {runtime.draft_input.audios.length > 0 ? (
        <div className="game-input-audios">
          {runtime.draft_input.audios.map((file, index) => (
            <div key={`${file.name}-${index}`} className="game-input-audio-preview">
              <span className="game-input-audio-name">{file.name}</span>
              <button
                type="button"
                className="game-input-audio-remove"
                onClick={() => actions.removeAudio(index)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      ) : null}

      {runtime.errors.action_error ? <div className="game-input-bubble">{runtime.errors.action_error}</div> : null}
      {micError ? <div className="game-input-bubble">{micError}</div> : null}

      {showSessionMeta ? (
        <div className={`game-session-meta${runtime.capabilities.platform === "mobile" ? " game-session-meta--compact" : ""}`}>
          {runtime.current_save ? <span className="game-session-meta-id">{`存档 ${runtime.current_save.id}`}</span> : null}
          {runtime.session.id ? <span className="game-session-meta-id">{`会话 ${runtime.session.id}`}</span> : null}
        </div>
      ) : null}
    </div>
  );
}
