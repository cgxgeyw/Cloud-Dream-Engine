import { describe, expect, it } from "vitest";

import { findPendingInteraction } from "./WorldFrameInputComposer";

describe("findPendingInteraction", () => {
  it("uses only an unanswered interaction emitted by the world director", () => {
    const result = findPendingInteraction([
      {
        message_id: "older",
        role: "agent",
        content: "older prompt",
        metadata: {
          interaction: {
            interaction_id: "older-interaction",
            kind: "choice",
            prompt: "Older",
            config: { options: [] },
            status: "pending",
          },
        },
      },
      {
        message_id: "director",
        role: "system",
        content: "Director prompt",
        metadata: {
          message_kind: "world_interaction",
          interaction: {
            interaction_id: "director-interaction",
            kind: "choice",
            prompt: "Choose",
            config: { options: [] },
            status: "pending",
          },
        },
      },
    ]);

    expect(result?.messageId).toBe("director");
    expect(result?.interaction.interaction_id).toBe("director-interaction");
  });

  it("ignores a pending interaction from an older character message", () => {
    expect(findPendingInteraction([
      {
        message_id: "character",
        role: "agent",
        content: "Older prompt",
        metadata: {
          interaction: {
            interaction_id: "character-interaction",
            kind: "choice",
            prompt: "Older",
            config: { options: [] },
            status: "pending",
          },
        },
      },
    ])).toBeNull();
  });

  it("does not surface an answered interaction in the composer", () => {
    expect(findPendingInteraction([
      {
        message_id: "answered",
        role: "system",
        content: "Director prompt",
        metadata: {
          interaction: {
            interaction_id: "answered-interaction",
            kind: "confirm",
            prompt: "Continue",
            config: {},
            status: "answered",
          },
        },
      },
    ])).toBeNull();
  });
});
