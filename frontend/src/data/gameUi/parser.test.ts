import { describe, expect, it } from "vitest";

import { parseGameUiDocument } from "./parser";

const MINIMAL_V2_DOCUMENT = {
  schema_version: 2,
  layout: {
    root: {
      type: "stack",
      children: [],
    },
  },
};

describe("parseGameUiDocument", () => {
  it("accepts an explicitly versioned v2 document", () => {
    const result = parseGameUiDocument(JSON.stringify(MINIMAL_V2_DOCUMENT), "desktop");

    expect(result.usedFallback).toBe(false);
    expect(result.error).toBeNull();
    expect(result.document.schema_version).toBe(2);
  });

  it("rejects a document with no schema version", () => {
    const { schema_version: _schemaVersion, ...unversioned } = MINIMAL_V2_DOCUMENT;
    const result = parseGameUiDocument(JSON.stringify(unversioned), "desktop");

    expect(result.usedFallback).toBe(true);
    expect(result.error).toContain("schema_version is missing");
  });

  it("rejects a non-numeric schema version", () => {
    const result = parseGameUiDocument(
      JSON.stringify({ ...MINIMAL_V2_DOCUMENT, schema_version: "2" }),
      "mobile",
    );

    expect(result.usedFallback).toBe(true);
    expect(result.error).toBe("schema_version must be a number.");
  });

  it("normalizes button actions without weakening the action type", () => {
    const result = parseGameUiDocument(
      JSON.stringify({
        ...MINIMAL_V2_DOCUMENT,
        layout: {
          root: {
            type: "button",
            label: "Continue",
            action: {
              id: "@submit_message",
              args: { nested: { count: 2 }, enabled: true },
              content_template: "Continue as {{player.name}}",
              mode: "resend",
            },
          },
        },
      }),
      "desktop",
    );

    expect(result.usedFallback).toBe(false);
    expect(result.document.layout.root).toMatchObject({
      type: "button",
      action: {
        id: "@submit_message",
        args: { nested: { count: 2 }, enabled: true },
        content_template: "Continue as {{player.name}}",
        mode: "resend",
      },
    });
  });
});
