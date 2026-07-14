import { describe, expect, it } from "vitest";

import { resolvePlayerActionMode } from "./utils";

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
