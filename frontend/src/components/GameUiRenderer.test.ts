import { describe, expect, it } from "vitest";

import { resolveGameUiPropValue, type GameUiRenderContext } from "./GameUiRenderer";
import { evaluateGameUiExpression } from "../gameUiRuntime/expression";

const context: GameUiRenderContext = {
  state: { selected: ["alpha"], compact: true },
  data: {
    session: { location: "Harbor" },
    attributes: { energy: 7 },
  },
  locals: { item: { name: "Lantern" } },
};

describe("game UI runtime bindings", () => {
  it("resolves component prop bindings without stringifying their type", () => {
    expect(resolveGameUiPropValue("$state.compact", context)).toBe(true);
    expect(resolveGameUiPropValue("$attributes.energy", context)).toBe(7);
    expect(resolveGameUiPropValue(["$item.name", "$state.selected"], context))
      .toEqual(["Lantern", ["alpha"]]);
  });

  it("renders inline templates and evaluates safe conditions", () => {
    expect(resolveGameUiPropValue("At {{ session.location }}", context)).toBe("At Harbor");
    expect(evaluateGameUiExpression(
      "attributes.energy >= 5 && capabilities.supports_hover == true",
      { ...context.data, capabilities: { supports_hover: true } },
    )).toBe(true);
  });
});
