import { describe, expect, it } from "vitest";

import {
  WORLD_FRAME_PROTOCOL_VERSION,
  isWorldFrameBootMessage,
  isWorldFrameClientMessage,
  isWorldFrameConnectMessage,
  isWorldFrameHostMessage,
} from "./protocol";

describe("world frame protocol", () => {
  it("accepts a versioned frame boot message", () => {
    expect(isWorldFrameBootMessage({
      type: "world-frame/boot",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
    })).toBe(true);
  });

  it("accepts a versioned connect message", () => {
    expect(isWorldFrameConnectMessage({
      type: "world-frame/connect",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: "frame-1",
    })).toBe(true);
  });

  it("rejects messages from another protocol version", () => {
    expect(isWorldFrameConnectMessage({
      type: "world-frame/connect",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION + 1,
      channelId: "frame-1",
    })).toBe(false);
  });

  it("requires a render payload and monotonic-compatible revision", () => {
    expect(isWorldFrameHostMessage({
      type: "world-frame/render-preview",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: "frame-1",
      revision: 3,
      payload: {},
    })).toBe(true);
    expect(isWorldFrameHostMessage({
      type: "world-frame/render-preview",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: "frame-1",
      revision: -1,
      payload: {},
    })).toBe(false);
  });

  it("only accepts known client responses", () => {
    expect(isWorldFrameClientMessage({
      type: "world-frame/ready",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: "frame-1",
    })).toBe(true);
    expect(isWorldFrameClientMessage({
      type: "world-frame/action",
      protocolVersion: WORLD_FRAME_PROTOCOL_VERSION,
      channelId: "frame-1",
      requestId: "request-1",
      action: { type: "navigate", target: "settings" },
    })).toBe(true);
  });
});
