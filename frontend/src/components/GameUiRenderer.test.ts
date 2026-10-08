import { describe, expect, it } from "vitest";

import {
  buildNodeStyle,
  resolveGameUiPropValue,
  resolveStyleRecord,
  type GameUiRenderContext,
} from "./GameUiRenderer";
import type { GameUiLayoutNodeV2 } from "../data/gameUi";
import { evaluateGameUiExpression } from "../gameUiRuntime/expression";

const context: GameUiRenderContext = {
  state: { selected: ["alpha"], compact: true },
  data: {
    session: { location: "Harbor" },
    attributes: { energy: 7 },
  },
  locals: { item: { name: "Lantern" } },
};

const styleContext: GameUiRenderContext = {
  state: { chart_mode: "week" },
  data: { gutter: 6 },
  locals: { bar: { h_burn: "74%", units: 96, selected: true } },
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

describe("node style bindings", () => {
  it("resolves {{ }} templates inside style records", () => {
    expect(resolveStyleRecord({ height: "{{ bar.h_burn }}", width: "100%" }, styleContext))
      .toEqual({ height: "74%", width: "100%" });
  });

  it("keeps the resolved type so React can append px", () => {
    expect(resolveStyleRecord({ height: "{{ bar.units }}", opacity: "{{ bar.selected }}" }, styleContext))
      .toEqual({ height: 96, opacity: true });
  });

  it("resolves templates embedded in longer style values", () => {
    expect(resolveStyleRecord({ width: "calc(100% - {{ gutter }}px)" }, styleContext))
      .toEqual({ width: "calc(100% - 6px)" });
  });

  it("leaves style records without templates untouched", () => {
    expect(resolveStyleRecord({ height: "96px", display: "flex" }, styleContext))
      .toEqual({ height: "96px", display: "flex" });
    expect(resolveStyleRecord(undefined, styleContext)).toBeUndefined();
  });

  it("resolves the common size fields alongside the style record", () => {
    const node: GameUiLayoutNodeV2 = {
      type: "stack",
      height: "{{ bar.h_burn }}",
      style: { min_height: "{{ bar.units }}", width: "100%" },
    };
    expect(buildNodeStyle(node, styleContext)).toMatchObject({
      height: "74%",
      minHeight: 96,
      width: "100%",
    });
  });
});
