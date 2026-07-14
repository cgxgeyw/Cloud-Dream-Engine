import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";

import type { GameUiPreviewProps } from "./GameUiPreview";
import {
  WORLD_FRAME_PROTOCOL_VERSION,
  isWorldFrameBootMessage,
  isWorldFrameClientMessage,
  type WorldFrameAction,
  type WorldFrameHostMessage,
  type WorldFramePreviewPayload,
} from "../worldFrame/protocol";
import type { WorldFrameRuntimePayload } from "../worldFrame/runtimeSnapshot";
export { createWorldFrameDocument } from "../worldFrame/frameDocument";

type FrameConnection = {
  channelId: string;
  port: MessagePort;
};

type WorldFrameHostProps = {
  mode: "preview" | "runtime";
  payload: WorldFramePreviewPayload | WorldFrameRuntimePayload;
  title: string;
  className?: string;
  onAction?: (action: WorldFrameAction) => void | Promise<void>;
};

export function WorldFrameHost({ mode, payload, title, className, onAction }: WorldFrameHostProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const connectionRef = useRef<FrameConnection | null>(null);
  const revisionRef = useRef(0);
  const latestPayloadRef = useRef(payload);
  const onActionRef = useRef(onAction);
  const [frameState, setFrameState] = useState<"connecting" | "ready" | "error">("connecting");
  const [frameError, setFrameError] = useState<string | null>(null);
  const frameUrl = useMemo(
    () => resolveWorldFrameAssetUrl(import.meta.env.DEV ? "frame.dev.html" : "frame.html"),
    [],
  );

  latestPayloadRef.current = payload;
  onActionRef.current = onAction;

  const sendLatestPayload = () => {
    const connection = connectionRef.current;
    if (!connection) {
      return;
    }
    revisionRef.current += 1;
    const message: WorldFrameHostMessage = {
      type: mode === "runtime" ? "world-frame/render-runtime" : "world-frame/render-preview",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: connection.channelId,
      revision: revisionRef.current,
      payload: latestPayloadRef.current,
    } as WorldFrameHostMessage;
    connection.port.postMessage(message);
  };

  const connectFrame = () => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (!frameWindow) {
      return;
    }
    connectionRef.current?.port.close();
    setFrameState("connecting");
    setFrameError(null);

    const channel = new MessageChannel();
    const channelId = createChannelId();
    const connection: FrameConnection = { channelId, port: channel.port1 };
    connectionRef.current = connection;

    channel.port1.addEventListener("message", (event: MessageEvent<unknown>) => {
      if (!isWorldFrameClientMessage(event.data) || event.data.channelId !== channelId) {
        return;
      }
      if (event.data.type === "world-frame/ready") {
        setFrameState("ready");
        sendLatestPayload();
        return;
      }
      if (event.data.type === "world-frame/error") {
        setFrameState("error");
        setFrameError(event.data.message);
        return;
      }
      void handleFrameAction(connection, event.data.requestId, event.data.action, onActionRef.current);
    });
    channel.port1.start();

    frameWindow.postMessage({
      type: "world-frame/connect",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId,
    }, "*", [channel.port2]);
  };

  useEffect(() => {
    const handleFrameBoot = (event: MessageEvent<unknown>) => {
      if (event.source === iframeRef.current?.contentWindow && isWorldFrameBootMessage(event.data)) {
        connectFrame();
      }
    };
    window.addEventListener("message", handleFrameBoot);
    return () => window.removeEventListener("message", handleFrameBoot);
  });

  useEffect(() => {
    if (frameState === "ready") {
      sendLatestPayload();
    }
  }, [frameState, mode, payload]);

  useEffect(() => () => {
    connectionRef.current?.port.close();
    connectionRef.current = null;
  }, []);

  return (
    <div className={["world-ui-sandbox-preview", className].filter(Boolean).join(" ")}>
      <iframe
        ref={iframeRef}
        src={frameUrl}
        title={title}
        sandbox="allow-scripts"
        onLoad={() => {
          setFrameState("connecting");
          setFrameError(null);
        }}
        className="world-ui-sandbox-preview-frame"
      />
      {frameState === "connecting" ? (
        <div className="world-ui-sandbox-preview-status">{"\u6b63\u5728\u542f\u52a8\u9694\u79bb\u754c\u9762..."}</div>
      ) : null}
      {frameState === "error" ? (
        <div className="world-ui-sandbox-preview-status world-ui-sandbox-preview-status--error">
          {frameError || "\u9694\u79bb\u754c\u9762\u52a0\u8f7d\u5931\u8d25\u3002"}
        </div>
      ) : null}
    </div>
  );
}

export function GameUiSandboxPreview(props: GameUiPreviewProps) {
  const payload = useMemo<WorldFramePreviewPayload>(() => ({
    ...props,
    rootStyle: serializeRootStyle(props.rootStyle),
  }), [props]);
  return (
    <WorldFrameHost
      mode="preview"
      payload={payload}
      title={`${props.platform === "mobile" ? "\u79fb\u52a8\u7aef" : "\u684c\u9762\u7aef"}\u4e16\u754c\u754c\u9762\u9884\u89c8`}
    />
  );
}

async function handleFrameAction(
  connection: FrameConnection,
  requestId: string,
  action: WorldFrameAction,
  handler: WorldFrameHostProps["onAction"],
) {
  try {
    if (!handler) {
      throw new Error("This frame does not expose runtime actions.");
    }
    await handler(action);
    connection.port.postMessage({
      type: "world-frame/action-result",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: connection.channelId,
      requestId,
      ok: true,
    });
  } catch (errorLike) {
    connection.port.postMessage({
      type: "world-frame/action-result",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: connection.channelId,
      requestId,
      ok: false,
      error: errorLike instanceof Error ? errorLike.message : String(errorLike),
    });
  }
}

export function resolveWorldFrameAssetUrl(assetName: string, href = window.location.href): string {
  const currentUrl = new URL(href);
  return currentUrl.protocol === "file:"
    ? new URL(`./world-frame/${assetName}`, currentUrl).toString()
    : new URL(`/world-frame/${assetName}`, currentUrl).toString();
}

function createChannelId(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `world-frame-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function serializeRootStyle(style?: CSSProperties): Record<string, string | number> | undefined {
  return style
    ? Object.fromEntries(Object.entries(style).filter((entry): entry is [string, string | number] =>
        typeof entry[1] === "string" || typeof entry[1] === "number"))
    : undefined;
}
