import type { KvScope } from "../data/types";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Copy } from "lucide-react";

import {
  createWorldRecord,
  deleteWorldKv,
  deleteWorldRecord,
  getWorldKv,
  invokeWorldPlatformFeature,
  isTauriEnvironment,
  listWorldKv,
  listWorldRecords,
  requestWorldPermissions,
  setWorldKv,
  updateWorldRecord,
} from "../data/apiAdapter";
import type { GameUiPlatform
} from "../data/gameUi";
import type { GameSessionStateBag } from "../game/useGameSession";
import type { WorldFrameAction } from "../worldFrame/protocol";
import { formatInteractionAnswer } from "../gameUiRuntime/interactions";
import {
  createGameUiRuntimeSnapshot,
  type WorldFrameRuntimePayload,
  type WorldFrameViewportSnapshot,
} from "../worldFrame/runtimeSnapshot";
import { WorldFrameHost } from "./GameUiSandboxPreview";
import { invokeWorldLogic } from "../worldFrame/WorldLogicRuntime";

export function GameUiSandboxRuntime({ bag, platform }: { bag: GameSessionStateBag; platform: GameUiPlatform }) {
  const navigate = useNavigate();
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const pendingSendRef = useRef(false);
  const pendingDiscardRef = useRef(false);
  const sendAfterAudioRef = useRef(false);
  const recordStartTsRef = useRef(0);
  // 上滑取消预备状态（ref 供原生桥闭包读取，state 经快照下发给 iframe 横条）。
  const [voiceCancelArmed, setVoiceCancelArmedState] = useState(false);
  const voiceCancelArmedRef = useRef(false);
  const setVoiceCancelArmed = useCallback((armed: boolean) => {
    voiceCancelArmedRef.current = armed;
    setVoiceCancelArmedState(armed);
  }, []);
  const [isRecording, setIsRecording] = useState(false);
  const [voiceMode, setVoiceMode] = useState(false);
  const [microphoneError, setMicrophoneError] = useState<string | null>(null);
  const viewport = useWorldFrameViewport(platform);
  const imageAttachments = useAttachmentSnapshots(bag.inputImages, "image", true);
  const audioAttachments = useAttachmentSnapshots(bag.inputAudios, "audio", false);

  const stopRecording = useCallback((options?: { send?: boolean; discard?: boolean }) => {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      if (options?.send) {
        pendingSendRef.current = true;
      }
      if (options?.discard) {
        pendingDiscardRef.current = true;
      }
      recorder.stop();
    }
    setIsRecording(false);
    setVoiceCancelArmed(false);
  }, [setVoiceCancelArmed]);

  const startRecording = useCallback(async () => {
    setMicrophoneError(null);
    try {
      if (isTauriEnvironment()) {
        // \u539f\u751f\u6865\u6743\u9650\u7533\u8bf7\u5931\u8d25\u4e0d\u963b\u65ad\u5f55\u97f3\uff1agetUserMedia \u65f6 WebView \u7684
        // onPermissionRequest \u4f1a\u81ea\u884c\u53d1\u8d77\u7cfb\u7edf\u7ea7 RECORD_AUDIO \u7533\u8bf7\u3002
        try {
          const statuses = await requestWorldPermissions(["microphone"], true);
          const micStatus = statuses.find((status) => status.permission === "microphone");
          if (micStatus?.granted === false) {
            setMicrophoneError("\u9ea6\u514b\u98ce\u6743\u9650\u88ab\u62d2\u7edd\u3002");
            return;
          }
        } catch (nativePermissionError) {
          console.warn("[audio] failed to request native microphone permission:", nativePermissionError);
        }
      }
      if (!navigator.mediaDevices?.getUserMedia) {
        setMicrophoneError("\u5f53\u524d\u73af\u5883\u4e0d\u652f\u6301\u5f55\u97f3\u3002");
        return;
      }
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      mediaStreamRef.current = stream;
      const recorder = new MediaRecorder(stream);
      mediaRecorderRef.current = recorder;
      audioChunksRef.current = [];
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          audioChunksRef.current.push(event.data);
        }
      };
      recorder.onstop = () => {
        const shouldSend = pendingSendRef.current;
        pendingSendRef.current = false;
        const shouldDiscard = pendingDiscardRef.current;
        pendingDiscardRef.current = false;
        const durationMs = Date.now() - recordStartTsRef.current;
        stream.getTracks().forEach((track) => track.stop());
        mediaStreamRef.current = null;
        mediaRecorderRef.current = null;
        if (shouldDiscard) {
          audioChunksRef.current = [];
          return;
        }
        if (shouldSend && durationMs < 1000) {
          audioChunksRef.current = [];
          setMicrophoneError("\u5f55\u97f3\u65f6\u95f4\u592a\u77ed\u3002");
          return;
        }
        const mimeType = recorder.mimeType || "audio/webm";
        const extension = mimeType.includes("ogg") ? "ogg" : mimeType.includes("mpeg") ? "mp3" : "webm";
        const file = new File(
          [new Blob(audioChunksRef.current, { type: mimeType })],
          `recording_${Date.now()}.${extension}`,
          { type: mimeType },
        );
        Object.assign(file, { durationSecs: durationMs / 1000 });
        bag.setInputAudios((previous) => [...previous, file]);
        if (shouldSend) {
          sendAfterAudioRef.current = true;
        }
      };
      recordStartTsRef.current = Date.now();
      recorder.start();
      setIsRecording(true);
    } catch (errorLike) {
      const errorState = errorLike as { name?: string; message?: string };
      const name = errorState.name;
      const detail = typeof errorLike === "string" ? errorLike : (name && errorState.message ? `${name}: ${errorState.message}` : name || errorState.message) || "\u672a\u77e5\u9519\u8bef";
      setMicrophoneError(
        name === "NotAllowedError"
          ? "\u9ea6\u514b\u98ce\u6743\u9650\u88ab\u62d2\u7edd\u3002"
          : name === "NotFoundError"
            ? "\u672a\u627e\u5230\u53ef\u7528\u9ea6\u514b\u98ce\u3002"
            : `\u542f\u52a8\u5f55\u97f3\u5931\u8d25\uff1a${detail}`,
      );
    }
  }, [bag]);

  useEffect(() => () => {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      recorder.stop();
    }
    mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
  }, []);

  // 按住说话松开后，音频附件先入列；等状态刷新后再提交，确保带上刚录好的音频。
  useEffect(() => {
    if (!sendAfterAudioRef.current || bag.inputAudios.length === 0) {
      return;
    }
    sendAfterAudioRef.current = false;
    void bag.handleSubmitAction({});
  }, [bag]);

  const snapshot = useMemo(
    () => createGameUiRuntimeSnapshot(bag, platform, {
      images: imageAttachments,
      audios: audioAttachments,
      isRecording,
      microphoneError,
      viewport,
      voiceMode,
      voiceCancel: voiceCancelArmed,
    }),
    [audioAttachments, bag, imageAttachments, isRecording, microphoneError, platform, viewport, voiceMode, voiceCancelArmed],
  );
  const payload = useMemo<WorldFrameRuntimePayload>(() => ({
    platform,
    document: bag.parsedGameUi.document,
    stylesheet: bag.themeCustomCss,
    scopeId: bag.gameUiScopeId,
    rootStyle: serializeRootStyle(bag.runtimeBackgroundStyle),
    storage: bag.worldUiEnvelope.storage,
    logic: bag.worldUiEnvelope.logic,
    snapshot,
  }), [bag.gameUiScopeId, bag.parsedGameUi.document, bag.runtimeBackgroundStyle, bag.themeCustomCss, bag.worldUiEnvelope.logic, bag.worldUiEnvelope.storage, platform, snapshot]);

  // 系统 View 层一定能收到 DOWN/MOVE/UP/CANCEL（Oplus 助手也拦不住），Kotlin 侧经
  // evaluateJavascript 转发到这里；这是按住说话松手与上滑取消的最可靠来源。
  useEffect(() => {
    const w = window as unknown as {
      __nativeTouchEnd?: (up: boolean) => void;
      __nativeTouchMove?: (dy: number) => void;
    };
    w.__nativeTouchEnd = (up: boolean) => {
      const recorder = mediaRecorderRef.current;
      if (recorder && recorder.state !== "inactive") {
        if (up && !voiceCancelArmedRef.current) {
          stopRecording({ send: true });
        } else {
          stopRecording({ discard: true });
        }
      }
    };
    w.__nativeTouchMove = (dy: number) => {
      const recorder = mediaRecorderRef.current;
      if (recorder && recorder.state !== "inactive") {
        setVoiceCancelArmed(dy < -150);
      }
    };
    return () => {
      w.__nativeTouchEnd = undefined;
      w.__nativeTouchMove = undefined;
    };
  }, [stopRecording, setVoiceCancelArmed]);

  const handleAction = useCallback(async (action: WorldFrameAction) => {
    switch (action.type) {
      case "clear-action-error": bag.clearActionError(); return;
      case "set-draft-value": bag.setInputValue(action.value); return;
      case "set-auto-scroll": bag.setChatAutoScrollEnabled(action.enabled); return;
      case "submit-message": await bag.handleSubmitAction(action.options); return;
      case "answer-interaction": {
        const result = await bag.handleAnswerInteraction(action.messageId, action.interactionId, action.answer);
        // 第 4 项事件接线：仅首次回答触发，重复提交（幂等）不重复计分。
        if (result?.newlyAnswered) {
          await dispatchWorldEvent("interaction_answered", {
            session_id: bag.session?.id ?? "",
            message_id: action.messageId,
            interaction_id: action.interactionId,
            answer: result.answer,
          });
        }
        const playerReply = result?.interaction
          ? formatInteractionAnswer(result.interaction, result.answer).trim()
          : "";
        if (playerReply) {
          await bag.handleSubmitAction({ mode: "submit", content: playerReply });
        }
        return;
      }
      case "start-editing-turn": bag.startEditingTurn(action.content, action.turnIndex); return;
      case "cancel-editing-turn": bag.cancelEditingTurn(); return;
      case "branch-from-current": await bag.handleBranch(); return;
      case "retry-turn": await bag.handleRetryFailedStep({ retry_token: action.retryToken }); return;
      case "accept-switch-proposal": await bag.handleAcceptSwitchProposal(action.proposal); return;
      case "dismiss-switch-proposal": bag.dismissSwitchProposal(action.proposalKey); return;
      case "dismiss-retry-card": bag.dismissDirectorRetryCard(action.cardKey); return;
      case "copy-text": await bag.handleCopyMessage(action.text); return;
      case "switch-side-tab": bag.setSideTab(action.tabKey); return;
      case "pick-image": imageInputRef.current?.click(); return;
      case "remove-image": bag.setInputImages((previous) => previous.filter((_, index) => index !== action.index)); return;
      case "start-recording": await startRecording(); return;
      case "stop-recording": stopRecording({ send: action.send === true }); return;
      case "voice-mode": setVoiceMode(action.enabled === true); return;
      case "remove-audio": bag.setInputAudios((previous) => previous.filter((_, index) => index !== action.index)); return;
      case "world-record-list": {
        const worldId = requireWorldRecordScope(bag, action.collection);
        return listWorldRecords(worldId, action.collection);
      }
      case "world-record-create": {
        const worldId = requireWorldRecordScope(bag, action.collection);
        return createWorldRecord(worldId, { collection: action.collection, data: action.data });
      }
      case "world-record-update": {
        const worldId = requireWorldRecordScope(bag, action.collection);
        return updateWorldRecord(worldId, action.recordId, {
          collection: action.collection,
          data: action.data,
        });
      }
      case "world-record-delete": {
        const worldId = requireWorldRecordScope(bag, action.collection);
        return deleteWorldRecord(worldId, action.collection, action.recordId);
      }
      case "world-kv-list": {
        const worldId = requireWorldKvScope(bag, action.namespace);
        return listWorldKv(worldId, action.namespace, resolveKvScope(bag, action.scope));
      }
      case "world-kv-get": {
        const worldId = requireWorldKvScope(bag, action.namespace);
        return getWorldKv(worldId, action.namespace, action.key, resolveKvScope(bag, action.scope));
      }
      case "world-kv-set": {
        const worldId = requireWorldKvScope(bag, action.namespace);
        return setWorldKv(worldId, action.namespace, action.key, action.value, resolveKvScope(bag, action.scope));
      }
      case "world-kv-delete": {
        const worldId = requireWorldKvScope(bag, action.namespace);
        return deleteWorldKv(worldId, action.namespace, action.key, resolveKvScope(bag, action.scope));
      }
      case "world-platform-invoke": {
        // 第 12 项：world_id 由宿主注入，世界包无法伪造其它世界的身份；
        // 声明与用户授权在后端命令内统一校验。
        return invokeWorldPlatformFeature(requireWorldId(bag), action.feature, action.params ?? {});
      }
      case "navigate":
        if (action.target === "back") navigate(-1);
        else if (action.target === "home") navigate("/");
        else if (action.target === "settings") navigate("/settings");
        else if (bag.session?.id) navigate(`/debug/${bag.session.id}`);
    }
  }, [bag, navigate, startRecording, stopRecording]);

  // ---- 世界包事件派发（第 4 项）----
  // 世界包在 logic.events 中声明 "事件 → logic.js 处理函数"，
  // 事件发生时按次调用 Worker（沿用按次创建、超时销毁模式），失败只记日志不打断游戏。
  const firedSessionStartRef = useRef<string | null>(null);
  const dispatchWorldEvent = useCallback(async (event: string, payload: Record<string, unknown>) => {
    const logic = bag.worldUiEnvelope.logic;
    const handler = logic.events?.[event as keyof NonNullable<typeof logic.events>];
    if (!handler || logic.runtime !== "sandbox-js-v1") return;
    try {
      await invokeWorldLogic(logic, handler, { event, ...payload }, handleAction);
    } catch (error) {
      console.warn(`[world-event] ${event} handler ${handler} failed:`, error);
    }
  }, [bag.worldUiEnvelope.logic, handleAction]);

  // session_start：每个会话只触发一次。
  useEffect(() => {
    const sessionId = bag.session?.id?.trim();
    if (!sessionId || firedSessionStartRef.current === sessionId) return;
    firedSessionStartRef.current = sessionId;
    void dispatchWorldEvent("session_start", {
      session_id: sessionId,
      world_id: bag.themeWorld?.id ?? "",
    });
  }, [bag.session?.id, bag.themeWorld?.id, dispatchWorldEvent]);

  // turn_completed：回合成功提交（含重发/编辑/重新生成）后触发，
  // 负载含该回合新增消息，便于计分类 handler 直接消费。
  const firedTurnSeqRef = useRef<string | null>(null);
  useEffect(() => {
    const completed = bag.lastCompletedTurn;
    const sessionId = bag.session?.id;
    if (!completed || !sessionId || completed.sessionId !== sessionId) return;
    const fireKey = `${completed.sessionId}:${completed.seq}`;
    if (firedTurnSeqRef.current === fireKey) return;
    firedTurnSeqRef.current = fireKey;
    const turnMessages = (bag.session?.messages ?? []).filter((message) => {
      const raw = message.metadata && (message.metadata as Record<string, unknown>).turn_index;
      return Number(raw) === completed.turnIndex;
    });
    void dispatchWorldEvent("turn_completed", {
      session_id: sessionId,
      turn_index: completed.turnIndex,
      messages: turnMessages,
    });
    // seq 单调递增，重发同一回合也会重新触发；只依赖信号本体。
  }, [bag.lastCompletedTurn, bag.session, dispatchWorldEvent]);

  const activeSessionId = bag.session?.id ?? "";

  return (
    <div className="world-ui-runtime-host">
      <input
        ref={imageInputRef}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={(event) => {
          const files = Array.from(event.target.files ?? []);
          if (files.length > 0) {
            bag.setInputImages((previous) => [...previous, ...files]);
          }
          event.target.value = "";
        }}
      />
      {activeSessionId ? (
        <button
          type="button"
          className="world-session-diagnostic"
          onClick={() => void bag.handleCopyMessage(activeSessionId)}
          title="复制会话 ID"
          aria-label={`复制会话 ID ${activeSessionId}`}
        >
          <span className="world-session-diagnostic-label">会话 ID</span>
          <code>{activeSessionId}</code>
          <Copy size={12} aria-hidden="true" />
        </button>
      ) : null}
      <WorldFrameHost
        mode="runtime"
        payload={payload}
        onAction={handleAction}
        title={platform === "mobile" ? "\u79fb\u52a8\u7aef\u4e16\u754c\u754c\u9762" : "\u684c\u9762\u7aef\u4e16\u754c\u754c\u9762"}
        className="world-ui-sandbox-runtime"
      />
    </div>
  );
}

function requireWorldRecordScope(bag: GameSessionStateBag, collection: string): string {
  const normalized = collection.trim().toLowerCase();
  const declared = Object.prototype.hasOwnProperty.call(
    bag.worldUiEnvelope.storage.collections,
    normalized,
  );
  const legacy = Object.keys(bag.worldUiEnvelope.storage.collections).length === 0
    && bag.worldUiEnvelope.capabilities.includes("supports_world_records");
  if (!declared && !legacy) {
    throw new Error(`This world package did not declare storage collection: ${normalized}`);
  }
  return requireWorldId(bag);
}

function requireWorldKvScope(bag: GameSessionStateBag, namespace: string): string {
  const normalized = namespace.trim().toLowerCase();
  if (!bag.worldUiEnvelope.storage.kv_namespaces.includes(normalized)) {
    throw new Error(`This world package did not declare KV namespace: ${normalized}`);
  }
  return requireWorldId(bag);
}

/** session/character 作用域由宿主注入当前会话 id，世界包不能伪造其它存档。 */
function resolveKvScope(bag: GameSessionStateBag, scope?: KvScope): KvScope | undefined {
  if (!scope?.scope || scope.scope === "world") return undefined;
  const sessionId = bag.session?.id?.trim();
  if (!sessionId) {
    throw new Error("当前没有进行中的存档，无法使用 session/character 作用域。");
  }
  return { scope: scope.scope, session_id: sessionId, character_id: scope.character_id };
}

function requireWorldId(bag: GameSessionStateBag): string {
  const worldId = bag.themeWorld?.id?.trim();
  if (!worldId) {
    throw new Error("The current world is unavailable.");
  }
  return worldId;
}

function useAttachmentSnapshots(files: File[], prefix: string, withPreview: boolean) {
  const snapshots = useMemo(() => files.map((file, index) => ({
    id: `${prefix}-${file.name}-${file.lastModified}-${index}`,
    name: file.name,
    size: file.size,
    type: file.type,
    preview_url: withPreview ? URL.createObjectURL(file) : undefined,
  })), [files, prefix, withPreview]);
  useEffect(() => () => {
    for (const item of snapshots) {
      if (item.preview_url) {
        URL.revokeObjectURL(item.preview_url);
      }
    }
  }, [snapshots]);
  return snapshots;
}

function useWorldFrameViewport(platform: GameUiPlatform): WorldFrameViewportSnapshot {
  const [viewport, setViewport] = useState(() => readViewport(platform));
  useEffect(() => {
    const update = () => setViewport(readViewport(platform));
    update();
    window.addEventListener("resize", update);
    window.addEventListener("orientationchange", update);
    window.visualViewport?.addEventListener("resize", update);
    window.visualViewport?.addEventListener("scroll", update);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("orientationchange", update);
      window.visualViewport?.removeEventListener("resize", update);
      window.visualViewport?.removeEventListener("scroll", update);
    };
  }, [platform]);
  return viewport;
}

function readViewport(platform: GameUiPlatform): WorldFrameViewportSnapshot {
  const visual = window.visualViewport;
  const width = Math.round(visual?.width ?? window.innerWidth);
  const height = Math.round(visual?.height ?? window.innerHeight);
  const offsetTop = Math.round(visual?.offsetTop ?? 0);
  return {
    width,
    height,
    offset_top: offsetTop,
    keyboard_height: platform === "mobile" ? Math.max(0, Math.round(window.innerHeight - height - offsetTop)) : 0,
    safe_area: measureSafeAreaInsets(),
  };
}

function measureSafeAreaInsets() {
  const probe = document.createElement("div");
  probe.style.cssText = "position:fixed;visibility:hidden;pointer-events:none;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)";
  document.body.appendChild(probe);
  const style = getComputedStyle(probe);
  const result = {
    top: parseFloat(style.paddingTop) || 0,
    right: parseFloat(style.paddingRight) || 0,
    bottom: parseFloat(style.paddingBottom) || 0,
    left: parseFloat(style.paddingLeft) || 0,
  };
  probe.remove();
  return result;
}

function serializeRootStyle(style: React.CSSProperties & Record<string, string>): Record<string, string | number> {
  const result: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(style)) {
    if (typeof value === "string" || typeof value === "number") {
      result[key] = value;
    }
  }
  return result;
}
