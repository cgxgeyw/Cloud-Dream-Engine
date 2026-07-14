import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";

import { isTauriEnvironment, requestWorldPermissions } from "../data/apiAdapter";
import type { GameUiPlatform } from "../data/gameUi";
import type { GameSessionStateBag } from "../game/useGameSession";
import type { WorldFrameAction } from "../worldFrame/protocol";
import {
  createGameUiRuntimeSnapshot,
  type WorldFrameRuntimePayload,
  type WorldFrameViewportSnapshot,
} from "../worldFrame/runtimeSnapshot";
import { WorldFrameHost } from "./GameUiSandboxPreview";

export function GameUiSandboxRuntime({ bag, platform }: { bag: GameSessionStateBag; platform: GameUiPlatform }) {
  const navigate = useNavigate();
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [microphoneError, setMicrophoneError] = useState<string | null>(null);
  const viewport = useWorldFrameViewport(platform);
  const imageAttachments = useAttachmentSnapshots(bag.inputImages, "image", true);
  const audioAttachments = useAttachmentSnapshots(bag.inputAudios, "audio", false);

  const stopRecording = useCallback(() => {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      recorder.stop();
    }
    setIsRecording(false);
  }, []);

  const startRecording = useCallback(async () => {
    setMicrophoneError(null);
    try {
      if (isTauriEnvironment()) {
        await requestWorldPermissions(["microphone"]);
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
        const mimeType = recorder.mimeType || "audio/webm";
        const extension = mimeType.includes("ogg") ? "ogg" : mimeType.includes("mpeg") ? "mp3" : "webm";
        const file = new File(
          [new Blob(audioChunksRef.current, { type: mimeType })],
          `recording_${Date.now()}.${extension}`,
          { type: mimeType },
        );
        bag.setInputAudios((previous) => [...previous, file]);
        stream.getTracks().forEach((track) => track.stop());
        mediaStreamRef.current = null;
        mediaRecorderRef.current = null;
      };
      recorder.start();
      setIsRecording(true);
    } catch (errorLike) {
      const name = (errorLike as { name?: string }).name;
      setMicrophoneError(
        name === "NotAllowedError"
          ? "\u9ea6\u514b\u98ce\u6743\u9650\u88ab\u62d2\u7edd\u3002"
          : name === "NotFoundError"
            ? "\u672a\u627e\u5230\u53ef\u7528\u9ea6\u514b\u98ce\u3002"
            : "\u542f\u52a8\u5f55\u97f3\u5931\u8d25\u3002",
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

  const snapshot = useMemo(
    () => createGameUiRuntimeSnapshot(bag, platform, {
      images: imageAttachments,
      audios: audioAttachments,
      isRecording,
      microphoneError,
      viewport,
    }),
    [audioAttachments, bag, imageAttachments, isRecording, microphoneError, platform, viewport],
  );
  const payload = useMemo<WorldFrameRuntimePayload>(() => ({
    platform,
    document: bag.parsedGameUi.document,
    stylesheet: bag.themeCustomCss,
    scopeId: bag.gameUiScopeId,
    rootStyle: serializeRootStyle(bag.runtimeBackgroundStyle),
    snapshot,
  }), [bag.gameUiScopeId, bag.parsedGameUi.document, bag.runtimeBackgroundStyle, bag.themeCustomCss, platform, snapshot]);

  const handleAction = useCallback(async (action: WorldFrameAction) => {
    switch (action.type) {
      case "clear-action-error": bag.clearActionError(); return;
      case "set-draft-value": bag.setInputValue(action.value); return;
      case "set-auto-scroll": bag.setChatAutoScrollEnabled(action.enabled); return;
      case "submit-message": await bag.handleSubmitAction(action.options); return;
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
      case "stop-recording": stopRecording(); return;
      case "remove-audio": bag.setInputAudios((previous) => previous.filter((_, index) => index !== action.index)); return;
      case "navigate":
        if (action.target === "back") navigate(-1);
        else if (action.target === "home") navigate("/");
        else if (action.target === "settings") navigate("/settings");
        else if (bag.session?.id) navigate(`/debug/${bag.session.id}`);
    }
  }, [bag, navigate, startRecording, stopRecording]);

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
