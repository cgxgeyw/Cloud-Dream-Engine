import { describe, expect, it } from "vitest";

import { buildRuntimeAttributeMaps } from "./runtimeContext";

describe("buildRuntimeAttributeMaps", () => {
  it("keeps flat attributes scoped to the session and current player", () => {
    const result = buildRuntimeAttributeMaps(
      {
        session_attributes: [
          {
            owner_type: "session",
            owner_id: "session-1",
            owner_label: "World",
            items: [
              {
                schema_id: "session-status",
                key: "status",
                label: "Status",
                value_type: "text",
                value: "active",
                source: "test",
                display_policy: {},
                influence_policy: {},
              },
            ],
          },
        ],
        character_attributes: [
          {
            owner_type: "session_character",
            owner_id: "session-1:player-1",
            owner_label: "Player",
            items: [
              {
                schema_id: "character-health",
                key: "health",
                label: "Health",
                value_type: "number",
                value: 80,
                source: "test",
                display_policy: {},
                influence_policy: {},
              },
            ],
          },
          {
            owner_type: "session_character",
            owner_id: "session-1:npc-1",
            owner_label: "NPC",
            items: [
              {
                schema_id: "character-health",
                key: "health",
                label: "Health",
                value_type: "number",
                value: 25,
                source: "test",
                display_policy: {},
                influence_policy: {},
              },
            ],
          },
        ],
      },
      "session-1",
      "player-1",
    );

    expect(result.attributes).toEqual({ status: "active", health: 80 });
    expect(result.attributesByOwner.session_character["session-1:npc-1"].health).toBe(25);
  });

  it("does not flatten an arbitrary character when there is no current player", () => {
    const result = buildRuntimeAttributeMaps(
      {
        session_attributes: [],
        character_attributes: [
          {
            owner_type: "session_character",
            owner_id: "session-1:npc-1",
            owner_label: "NPC",
            items: [
              {
                schema_id: "character-health",
                key: "health",
                label: "Health",
                value_type: "number",
                value: 25,
                source: "test",
                display_policy: {},
                influence_policy: {},
              },
            ],
          },
        ],
      },
      "session-1",
    );

    expect(result.attributes).toEqual({});
    expect(result.attributesByOwner.session_character["session-1:npc-1"].health).toBe(25);
  });
});
