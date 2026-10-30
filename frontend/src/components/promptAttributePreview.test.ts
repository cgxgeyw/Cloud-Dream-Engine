import { describe, expect, it } from "vitest";
import type { AttributeSchemaResponse } from "../data/types";
import { buildDirectorAttributePreview } from "./promptAttributePreview";

function schema(defaultValue: unknown): AttributeSchemaResponse {
  return {
    id: "health-schema", scope: "world", world_id: "world-1", sort_order: 0,
    key: "health", label: "Health", value_type: "json", description: "Current health",
    default_value: defaultValue, enum_options: [], display_policy: {}, access_policy: {},
    mutation_policy: {}, influence_policy: {}, projection_policy: {},
  };
}

describe("director attribute preview", () => {
  it.each([0, false, "", [], { hp: 100 }, null].map((value) => ({ value })))("shows missing attributes and their defaults ($value)", ({ value }) => {
    const { modulesById } = buildDirectorAttributePreview([], [schema(value)]);
    expect(modulesById.get("world_attribute:health")).toMatchObject({
      preview_only: true, content: { default_value: value, description: "Current health" },
    });
    expect(modulesById.get("attributes")).toMatchObject({
      preview_only: true, content: [{ attribute: "Health", value_source: "默认值" }],
    });
    // The same collection drives the rendered list, including synthesized entries.
    expect(Array.from(modulesById.values()).map((module) => module.id)).toContain("world_attribute:health");
  });

  it("keeps actual prompt values instead of replacing them with defaults", () => {
    const actual = { id: "world_attribute:health", content: "[{\"current_value\":7}]", sent: true };
    const { modulesById } = buildDirectorAttributePreview([actual], [schema(100)]);
    expect(modulesById.get(actual.id)).toBe(actual);
    expect(modulesById.get("attributes")?.content).toEqual([
      { attribute: "Health", value_source: "Prompt 预览值", content: actual.content },
    ]);
  });

  it("explains when no attributes exist instead of leaving the section missing", () => {
    const { modulesById } = buildDirectorAttributePreview([], []);
    expect(modulesById.get("attributes")?.content).toContain("没有可预览的自定义属性");
  });
});
