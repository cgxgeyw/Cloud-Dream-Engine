import { describe, expect, it } from "vitest";

import {
  GAME_UI_EXPRESSION_FUNCTION_IDS,
  evaluateGameUiExpression,
} from "./expression";

const scope = {
  state: { hp: 30, max_hp: 100, name: "阿宁", flags: { brave: true } },
  attributes: { energy: 7 },
};

describe("expression arithmetic", () => {
  it("evaluates the four operations with correct precedence", () => {
    expect(evaluateGameUiExpression("1 + 2 * 3 == 7", scope)).toBe(true);
    expect(evaluateGameUiExpression("(1 + 2) * 3 == 9", scope)).toBe(true);
    expect(evaluateGameUiExpression("10 - 4 - 3 == 3", scope)).toBe(true);
    expect(evaluateGameUiExpression("10 % 4 == 2", scope)).toBe(true);
  });

  it("reads state paths inside arithmetic", () => {
    expect(evaluateGameUiExpression("state.hp + 10 <= state.max_hp", scope)).toBe(true);
    expect(evaluateGameUiExpression("attributes.energy * 2 > 10", scope)).toBe(true);
  });

  it("keeps comparison semantics that existed before arithmetic", () => {
    expect(evaluateGameUiExpression("state.hp > 0 && attributes.energy >= 7", scope)).toBe(true);
    expect(evaluateGameUiExpression("state.hp > 100 || attributes.energy > 100", scope)).toBe(false);
    expect(evaluateGameUiExpression('state.name == "阿宁"', scope)).toBe(true);
  });

  it("treats division by zero as Infinity rather than throwing", () => {
    expect(evaluateGameUiExpression("state.hp / 0 > 0", scope)).toBe(true);
    expect(evaluateGameUiExpression("state.hp % 0 > 0", scope)).toBe(false);
  });

  it("parses unary minus", () => {
    expect(evaluateGameUiExpression("-5 + state.hp == 25", scope)).toBe(true);
    expect(evaluateGameUiExpression("state.hp > -1", scope)).toBe(true);
  });
});

describe("expression math functions", () => {
  it("clamps, floors and rounds", () => {
    expect(evaluateGameUiExpression("math_clamp(150, 0, 100) == 100", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_floor(2.9) == 2", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_ceil(2.1) == 3", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_round(2.5) == 3", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_abs(0 - 7) == 7", scope)).toBe(true);
  });

  it("picks the smaller and larger of several values", () => {
    expect(evaluateGameUiExpression("math_min(3, 1, 2) == 1", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_max(3, 1, 2) == 3", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_min(state.hp, state.max_hp) == 30", scope)).toBe(true);
  });

  it("computes a health percentage for gauges", () => {
    expect(
      evaluateGameUiExpression(
        "math_round(state.hp * 100 / state.max_hp) == 30",
        scope,
      ),
    ).toBe(true);
  });

  it("interpolates with a clamped t", () => {
    expect(evaluateGameUiExpression("math_lerp(0, 10, 0.5) == 5", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_lerp(0, 10, 2) == 10", scope)).toBe(true);
    expect(evaluateGameUiExpression("math_lerp(0, 10, -1) == 0", scope)).toBe(true);
  });
});

describe("expression random functions", () => {
  it("math_random stays within 0..1 across many draws", () => {
    for (let i = 0; i < 200; i += 1) {
      expect(evaluateGameUiExpression("math_random() >= 0 && math_random() <= 1", scope)).toBe(true);
    }
  });

  it("math_random_int stays within the requested inclusive range", () => {
    for (let i = 0; i < 200; i += 1) {
      expect(
        evaluateGameUiExpression("math_random_int(1, 6) >= 1 && math_random_int(1, 6) <= 6", scope),
      ).toBe(true);
    }
  });

  it("math_random_int with one argument returns 0..n", () => {
    for (let i = 0; i < 100; i += 1) {
      expect(evaluateGameUiExpression("math_random_int(4) >= 0 && math_random_int(4) <= 4", scope)).toBe(
        true,
      );
    }
  });

  it("can drive a probability gate", () => {
    // 掷一次骰子得 6 就触发；不要求必然为真，只要求两种结果都出现过。
    let hits = 0;
    for (let i = 0; i < 300; i += 1) {
      if (evaluateGameUiExpression("math_random_int(1, 6) == 6", scope)) {
        hits += 1;
      }
    }
    expect(hits).toBeGreaterThan(0);
    expect(hits).toBeLessThan(300);
  });
});

describe("expression function safety", () => {
  it("rejects any function outside the whitelist", () => {
    expect(() => evaluateGameUiExpression("fetch(1)", scope)).toThrow(/不支持调用/);
    expect(() => evaluateGameUiExpression("eval(1)", scope)).toThrow(/不支持调用/);
    expect(() => evaluateGameUiExpression("constructor(1)", scope)).toThrow(/不支持调用/);
  });

  it("rejects unknown members of the math prefix too", () => {
    expect(() => evaluateGameUiExpression("math_pow(2, 3)", scope)).toThrow(/不支持调用/);
  });

  it("still evaluates an all-whitelist function list", () => {
    expect(GAME_UI_EXPRESSION_FUNCTION_IDS).toEqual([
      "math_min",
      "math_max",
      "math_abs",
      "math_floor",
      "math_ceil",
      "math_round",
      "math_random",
      "math_random_int",
      "math_clamp",
      "math_lerp",
    ]);
  });
});