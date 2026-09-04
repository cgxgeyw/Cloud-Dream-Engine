import { describe, expect, it } from "vitest";

import { parseAgentToolActivity, resolvePlayerActionMode } from "./utils";
import type { ChatMessageResponse } from "../data/types";

describe("resolvePlayerActionMode", () => {
  it.each([
    [undefined, "submit"],
    ["submit", "submit"],
    ["edit", "edit"],
    ["resend", "resend"],
    ["unsupported", "submit"],
  ] as const)("maps %s to %s", (mode, expected) => {
    expect(resolvePlayerActionMode(mode)).toBe(expected);
  });
});

describe("parseAgentToolActivity", () => {
  const agentMessage = (metadata: Record<string, unknown> | null): ChatMessageResponse => ({
    role: "agent",
    content: "",
    speaker: "研究员",
    metadata,
  });

  it("parses calling activity with display name and args preview", () => {
    const view = parseAgentToolActivity(
      agentMessage({
        tool_activity: {
          status: "calling",
          tools: [{ id: "stock_quote", name: "A股实时行情", args_preview: "1.600519, 60" }],
        },
      }),
    );
    expect(view).toEqual({
      status: "calling",
      tools: [{ id: "stock_quote", name: "A股实时行情", argsPreview: "1.600519, 60" }],
    });
  });

  it("parses done activity without args preview", () => {
    const view = parseAgentToolActivity(
      agentMessage({
        tool_activity: {
          status: "done",
          tools: [{ id: "stock_quote", name: "A股实时行情" }],
        },
      }),
    );
    expect(view?.status).toBe("done");
    expect(view?.tools).toHaveLength(1);
    expect(view?.tools[0].argsPreview).toBe("");
  });

  it("returns null for non-agent messages", () => {
    const message: ChatMessageResponse = {
      role: "system",
      content: "",
      metadata: { tool_activity: { status: "calling", tools: [{ name: "x" }] } },
    };
    expect(parseAgentToolActivity(message)).toBeNull();
  });

  it.each([
    ["missing metadata", null],
    ["unknown status", { tool_activity: { status: "pending", tools: [{ name: "x" }] } }],
    ["empty tools", { tool_activity: { status: "calling", tools: [] } }],
    ["tools without name", { tool_activity: { status: "calling", tools: [{ id: "x" }] } }],
    ["non-object tool_activity", { tool_activity: "calling" }],
  ])("returns null when %s", (_label, metadata) => {
    expect(parseAgentToolActivity(agentMessage(metadata))).toBeNull();
  });
});
