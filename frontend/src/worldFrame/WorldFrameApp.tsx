import { useCallback, useEffect, useRef, useState } from "react";

import { GameUiPreview } from "../components/GameUiPreview";
import {
  WORLD_FRAME_PROTOCOL_VERSION,
  isWorldFrameConnectMessage,
  isWorldFrameHostMessage,
  type WorldFrameAction,
  type WorldFramePreviewPayload,
} from "./protocol";
import type { WorldFrameRuntimePayload } from "./runtimeSnapshot";
import { WorldFrameRuntimeView } from "./WorldFrameRuntimeView";

type ConnectedFrame = {
  channelId: string;
  port: MessagePort;
};

type FramePayload =
  | { mode: "preview"; value: WorldFramePreviewPayload }
  | { mode: "runtime"; value: WorldFrameRuntimePayload };

type PendingAction = {
  resolve: () => void;
  reject: (error: Error) => void;
};

export function WorldFrameApp() {
  const connectionRef = useRef<ConnectedFrame | null>(null);
  const latestRevisionRef = useRef(-1);
  const pendingActionsRef = useRef(new Map<string, PendingAction>());
  const [payload, setPayload] = useState<FramePayload | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let bootTimer = 0;
    const handleConnect = (event: MessageEvent<unknown>) => {
      if (event.source !== window.parent || !isWorldFrameConnectMessage(event.data)) {
        return;
      }

      const port = event.ports[0];
      if (!port) {
        return;
      }

      window.clearInterval(bootTimer);

      connectionRef.current?.port.close();
      latestRevisionRef.current = -1;
      setPayload(null);
      setError(null);

      const connection: ConnectedFrame = {
        channelId: event.data.channelId,
        port,
      };
      connectionRef.current = connection;

      port.addEventListener("message", (messageEvent: MessageEvent<unknown>) => {
        if (!isWorldFrameHostMessage(messageEvent.data)) {
          return;
        }
        if (messageEvent.data.type === "world-frame/action-result") {
          const pending = pendingActionsRef.current.get(messageEvent.data.requestId);
          if (!pending) {
            return;
          }
          pendingActionsRef.current.delete(messageEvent.data.requestId);
          if (messageEvent.data.ok) {
            pending.resolve();
          } else {
            pending.reject(new Error(messageEvent.data.error || "World UI action failed."));
          }
          return;
        }
        if (
          messageEvent.data.channelId !== connection.channelId
          || messageEvent.data.revision <= latestRevisionRef.current
        ) {
          return;
        }

        latestRevisionRef.current = messageEvent.data.revision;
        setPayload({
          mode: messageEvent.data.type === "world-frame/render-runtime" ? "runtime" : "preview",
          value: messageEvent.data.payload,
        } as FramePayload);
        setError(null);
      });
      port.start();
      port.postMessage({
        type: "world-frame/ready",
        protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
        channelId: connection.channelId,
      });
    };

    const announceBoot = () => {
      window.parent.postMessage({
        type: "world-frame/boot",
        protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      }, "*");
    };

    window.addEventListener("message", handleConnect);
    announceBoot();
    bootTimer = window.setInterval(announceBoot, 250);
    return () => {
      window.clearInterval(bootTimer);
      window.removeEventListener("message", handleConnect);
      connectionRef.current?.port.close();
      connectionRef.current = null;
      for (const pending of pendingActionsRef.current.values()) {
        pending.reject(new Error("World UI frame disconnected."));
      }
      pendingActionsRef.current.clear();
    };
  }, []);

  const sendAction = useCallback((action: WorldFrameAction): Promise<void> => {
    const connection = connectionRef.current;
    if (!connection) {
      return Promise.reject(new Error("World UI frame is not connected."));
    }
    const requestId = createRequestId();
    return new Promise<void>((resolve, reject) => {
      pendingActionsRef.current.set(requestId, { resolve, reject });
      connection.port.postMessage({
        type: "world-frame/action",
        protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
        channelId: connection.channelId,
        requestId,
        action,
      });
    });
  }, []);

  useEffect(() => {
    const handleError = (event: ErrorEvent) => {
      const message = event.message || "World UI frame failed to render.";
      setError(message);
      const connection = connectionRef.current;
      connection?.port.postMessage({
        type: "world-frame/error",
        protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
        channelId: connection.channelId,
        message,
      });
    };

    window.addEventListener("error", handleError);
    return () => window.removeEventListener("error", handleError);
  }, []);

  if (error) {
    return <div className="world-frame-status world-frame-status--error">{error}</div>;
  }
  if (!payload) {
    return <div className="world-frame-status">正在连接世界界面...</div>;
  }

  return payload.mode === "runtime"
    ? <WorldFrameRuntimeView payload={payload.value} sendAction={sendAction} />
    : <GameUiPreview {...payload.value} />;
}

function createRequestId(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `action-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
