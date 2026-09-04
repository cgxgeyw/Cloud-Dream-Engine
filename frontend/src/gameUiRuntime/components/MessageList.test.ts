import { describe, expect, it } from "vitest";

import { formatMessageTimestamp, shouldShowMessageTimestamp } from "./MessageList";

describe("message timestamps", () => {
  it("shows the first timestamp and timestamps after a five-minute gap", () => {
    expect(shouldShowMessageTimestamp("2026-08-22T08:00:00.000Z", undefined)).toBe(true);
    expect(shouldShowMessageTimestamp("2026-08-22T08:04:59.000Z", "2026-08-22T08:00:00.000Z")).toBe(false);
    expect(shouldShowMessageTimestamp("2026-08-22T08:05:00.000Z", "2026-08-22T08:00:00.000Z")).toBe(true);
  });

  it("shows a timestamp when messages cross a local calendar day", () => {
    const previous = new Date(2026, 7, 22, 23, 59, 0).toISOString();
    const current = new Date(2026, 7, 23, 0, 1, 0).toISOString();
    expect(shouldShowMessageTimestamp(current, previous)).toBe(true);
  });

  it("does not render timestamps for missing or invalid persisted values", () => {
    expect(shouldShowMessageTimestamp(undefined, undefined)).toBe(false);
    expect(shouldShowMessageTimestamp("not-a-date", undefined)).toBe(false);
    expect(formatMessageTimestamp("not-a-date")).toBeNull();
  });

  it("uses WeChat-style labels for recent and older messages", () => {
    const now = new Date(2026, 7, 22, 16, 0, 0);
    expect(formatMessageTimestamp(new Date(2026, 7, 22, 9, 3, 0).toISOString(), now)).toBe("今天 09:03");
    expect(formatMessageTimestamp(new Date(2026, 7, 21, 18, 20, 0).toISOString(), now)).toBe("昨天 18:20");
    expect(formatMessageTimestamp(new Date(2026, 5, 1, 8, 30, 0).toISOString(), now)).toBe("6月1日 08:30");
    expect(formatMessageTimestamp(new Date(2025, 11, 31, 8, 30, 0).toISOString(), now)).toBe("2025年12月31日 08:30");
  });
});
