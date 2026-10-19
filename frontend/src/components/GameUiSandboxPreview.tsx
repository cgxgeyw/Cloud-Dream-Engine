import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Bug, ChevronDown, ChevronUp, Copy } from "lucide-react";

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
import { createHtmlWorldDocument } from '../worldFrame/htmlDocument';

type FrameConnection = {
  channelId: string;
  port: MessagePort;
};

export type WorldFrameDiagnostic = {
  timestamp: number;
  level: "info" | "warn" | "error";
  phase: "boot" | "connect" | "render" | "action" | "runtime";
  message: string;
  requestId?: string;
  revision?: number;
};

type WorldFrameHostProps = {
  mode: "preview" | "runtime";
  payload: WorldFramePreviewPayload | WorldFrameRuntimePayload;
  title: string;
  className?: string;
  onAction?: (action: WorldFrameAction) => unknown | Promise<unknown>;
  onDiagnostic?: (diagnostic: WorldFrameDiagnostic) => void;
};

export function WorldFrameHost({ mode, payload, title, className, onAction, onDiagnostic }: WorldFrameHostProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const connectionRef = useRef<FrameConnection | null>(null);
  const revisionRef = useRef(0);
  const latestPayloadRef = useRef(payload);
  const onActionRef = useRef(onAction);
  const onDiagnosticRef = useRef(onDiagnostic);
  const [diagnostics, setDiagnostics] = useState<WorldFrameDiagnostic[]>([]);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [frameState, setFrameState] = useState<"connecting" | "ready" | "error">("connecting");
  const [frameError, setFrameError] = useState<string | null>(null);
  const htmlDocument = useMemo(() => createHtmlWorldDocument(payload.document.html || '', payload.stylesheet), [payload.document.html, payload.stylesheet]);

  latestPayloadRef.current = payload;
  onActionRef.current = onAction;
  onDiagnosticRef.current = onDiagnostic;

  const recordDiagnostic = (entry: Omit<WorldFrameDiagnostic, "timestamp">) => {
    const diagnostic = { ...entry, timestamp: Date.now() };
    setDiagnostics((current) => [...current.slice(-99), diagnostic]);
    onDiagnosticRef.current?.(diagnostic);
  };

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
    recordDiagnostic({ level: "info", phase: "connect", message: "正在连接世界界面。" });

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
        recordDiagnostic({ level: "info", phase: "connect", message: "世界界面已连接。" });
        sendLatestPayload();
        return;
      }
      if (event.data.type === "world-frame/error") {
        setFrameState("error");
        setFrameError(event.data.message);
        recordDiagnostic({ level: "error", phase: "runtime", message: event.data.message });
        return;
      }
      if (event.data.type === "world-frame/diagnostic") {
        recordDiagnostic({ level: event.data.level, phase: event.data.phase, message: event.data.message, requestId: event.data.requestId, revision: event.data.revision });
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
        srcDoc={htmlDocument}
        title={title}
        sandbox="allow-scripts"
        onLoad={() => {
          setFrameState("connecting");
          setFrameError(null);
          recordDiagnostic({ level: "info", phase: "boot", message: "iframe 已加载，等待 SDK 握手。" });
        }}
        className="world-ui-sandbox-preview-frame"
      />
      <div className="world-ui-sandbox-debug">
        <button type="button" className="world-ui-sandbox-debug-toggle" onClick={() => setShowDiagnostics((value) => !value)} title="查看世界界面运行时诊断">
          <Bug size={14} aria-hidden="true" />
          <span>诊断 {diagnostics.length}</span>
          {showDiagnostics ? <ChevronUp size={14} aria-hidden="true" /> : <ChevronDown size={14} aria-hidden="true" />}
        </button>
        {showDiagnostics ? (
          <div className="world-ui-sandbox-debug-panel">
            <div className="world-ui-sandbox-debug-actions">
              <button type="button" onClick={() => void copyDiagnostics(diagnostics)} title="复制诊断信息"><Copy size={13} aria-hidden="true" />复制</button>
              <button type="button" onClick={() => setDiagnostics([])}>清空</button>
            </div>
            {diagnostics.length === 0 ? <div className="text-muted">暂无运行时事件。</div> : diagnostics.slice().reverse().map((entry, index) => (
              <div key={`${entry.timestamp}-${index}`} className={`world-ui-sandbox-debug-entry world-ui-sandbox-debug-entry--${entry.level}`}>
                <code>{new Date(entry.timestamp).toLocaleTimeString()}</code>
                <span>[{entry.phase}] {entry.message}</span>
              </div>
            ))}
          </div>
        ) : null}
      </div>
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

async function copyDiagnostics(entries: WorldFrameDiagnostic[]) {
  const text = entries.map((entry) => `${new Date(entry.timestamp).toISOString()} [${entry.level}/${entry.phase}] ${entry.message}`).join("\n");
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

async function handleFrameAction(
  connection: FrameConnection,
  requestId: string,
  action: WorldFrameAction,
  handler: WorldFrameHostProps["onAction"],
) {
  try {
    if (!handler) {
      throw new Error("该预览不包含运行时操作。");
    }
    const result = await handler(action);
    connection.port.postMessage({
      type: "world-frame/action-result",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: connection.channelId,
      requestId,
      ok: true,
      result,
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
