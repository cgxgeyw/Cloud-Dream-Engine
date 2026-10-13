/**
 * 小恶魔的毒酒宴 —— 世界包 UI 回归测试。
 *
 * 这个测试存在的唯一理由：上一轮 UI 用自造原型HTML 验证，全绿；上线后整块牌桌
 * 不渲染。这里直接用宿主真实的 GameUiRenderer + 表达式求值器渲染世界包文档，
 * 把"绑定写错 → 界面空白"这类问题挡在构建期。
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import { parse as parseJsonc } from "jsonc-parser";

import { htmlToText, renderWorldUiDocument } from "../test-utils/worldUiProbe";

const PACKAGE_ROOT = path.resolve(__dirname, "../../../examples/world-packages/poison-feast");

function loadUiDocument(platform: "desktop" | "mobile") {
  const file = path.join(PACKAGE_ROOT, "world", `ui.${platform}.jsonc`);
  return parseJsonc(readFileSync(file, "utf8")) as Record<string, unknown>;
}

function loadLogicSource(): string {
  return readFileSync(path.join(PACKAGE_ROOT, "world", "logic.js"), "utf8");
}

/** 一局进行中、轮到玩家的真实 view（字段与 logic.js 的 buildView 严格一致）。 */
const PLAYER_TURN_VIEW = {
  phase: "playing",
  phase_code: 1,
  turn: "player",
  turn_code: 1,
  winner: "",
  round: 3,
  awaiting_devil: false,
  // 必须与 logic.js 的 buildView 字段一致：缺字段会让 when 条件全部落空
  last_touched_index: 0,
  devil_item_count: 2,
  player_candles: [1, 1, 0],
  devil_candles: [1, 0, 0],
  tubes_left: 2,
  poison_left: 1,
  water_left: 1,
  // 本轮总支数：UI 的"试管架 · N 支"读它。缺字段会让插值变空串。
  round_tubes: 4,
  tube_slots: [
    { index: 1, status: 0, kind: "poison", known: true },
    { index: 2, status: 0, kind: "water", known: true },
    { index: 3, status: 1, kind: "unknown", known: false },
    { index: 4, status: 2, kind: "unknown", known: false },
  ],
  consumed: ["poison", "water"],
  player_know: "poison",
  player_know_code: 1,
  strong: false,
  devil_asleep: false,
  my_items: [{ id: "needle", label: "试毒银针", desc: "偷看当前管", count: 1 }],
  has_items: true,
  // 动画一（开局展示）：本轮已喝过两支，展示幕已退场
  show_seq: 3,
  show_active: false,
  show_fresh: false,
  // 动画二（举杯展示）：最近一次是玩家把第 2 管清水灌给小恶魔
  serve_seq: 2,
  serve_active: true,
  serve_index: 2,
  serve_kind: "water",
  serve_kind_code: 2,
  serve_to: "devil",
  serve_to_code: 2,
  serve_dy: "-150px",
  serve_lean: "-20deg",
  serve_accent: "#7cc6e8",
  serve_caption: "第 2 管 · 清水 → 小恶魔",
  hint: "轮到你行动。",
  last_event: "你灌给小恶魔，它喝下的是清水。",
  log: ["第 3 轮重新调酒。", "小恶魔灌给你，你喝下清水。"],
  over_text: "",
  devil_inventory_text: "试毒银针×1、烈酒×1",
};

const DEVIL_TURN_VIEW = {
  ...PLAYER_TURN_VIEW,
  turn: "devil",
  turn_code: 2,
  awaiting_devil: true,
  hint: "轮到它了，看着就好。",
  last_event: "",
};

const IDLE_VIEW = {
  ...PLAYER_TURN_VIEW,
  phase: "idle",
  phase_code: 0,
  turn: "none",
  turn_code: 0,
  round: 0,
  tubes_left: 0,
  poison_left: 0,
  water_left: 0,
  tube_slots: [],
  player_candles: [0, 0, 0],
  devil_candles: [0, 0, 0],
  my_items: [],
  has_items: false,
  // 未开局：两个动画都不该出现
  show_seq: 0,
  show_active: false,
  show_fresh: false,
  serve_seq: 0,
  serve_active: false,
  serve_caption: "",
  log: [],
  hint: "点下面的按钮，掀开桌布。",
  last_event: "",
};

const OVER_VIEW = {
  ...PLAYER_TURN_VIEW,
  phase: "over",
  phase_code: 2,
  winner: "player",
  player_candles: [0, 0, 0],
  over_text: "小恶魔的烛火全熄了，灵魂归你。",
};

/**
 * 数一数真正画出来的试管。
 * 注意：不能用 `\bpf-tube\b` —— 连字符也是单词边界，`pf-tube-cell` 会被误计。
 * 这里要求 class 名单里出现独立的 `pf-tube` 词条。
 */
function countTubeElements(html: string): number {
  return [...html.matchAll(/class="([^"]*)"/g)].filter((match) =>
    (match[1] ?? "").split(/\s+/).includes("pf-tube"),
  ).length;
}

describe("毒酒宴世界包 · 桌面端 UI", () => {
  const document = loadUiDocument("desktop");

  it("状态绑定必须走 state.* —— view.round 这类裸键取不到值", () => {
    // 这是上一轮线上空白的直接原因：裸 `view` 会被解析成 runtimeData.view（不存在），
    // 于是所有插值变成空串、所有 when 条件为假，牌桌整块不渲染。
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("第 3 轮");
    expect(probe.text).not.toMatch(/第\s+轮/);
  });

  it("轮到玩家时，牌桌主体与两个核心行动按钮都在", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("试管架");
    expect(probe.text).toContain("自己喝下");
    expect(probe.text).toContain("灌给小恶魔");
    expect(probe.text).toContain("小恶魔");
    expect(probe.text).toContain("轮到你行动。");
  });

  it("四支试管全部渲染，且当前管有独立标识", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    // 编号 1..4 各出现一次
    for (const index of [1, 2, 3, 4]) {
      expect(probe.text).toContain(String(index));
    }
    expect(probe.html).toMatch(/pf-tube-active/);
    expect((probe.html.match(/pf-tube-idle/g) ?? []).length).toBeGreaterThan(0);
  });

  it("每个试管格子只渲染一支试管（when 分支不能重叠）", () => {
    // 曾经catch-all 的 `status == 0` 与 `status == 0 && kind == 'poison'` 同时命中，
    // 一格画出两支试管，牌架上出现 6 支。这里锁死"一格一支"。
    // 只看进行中的局面：结束后牌桌被结算面板替换，本来就不该有试管。
    for (const view of [PLAYER_TURN_VIEW, DEVIL_TURN_VIEW]) {
      const probe = renderWorldUiDocument(document, { view });
      expect(countTubeElements(probe.html)).toBe(view.tube_slots.length);
    }
  });

  it("带 last_touched_index 的真实局面下依然一格一支", () => {
    // 加入"刚刚倒下/刚刚泼掉"的一次性动画分支后，兜底分支没有加`!=`排除，
    // 同一个格子会同时命中两个 when，4 个格子画出 5 支试管。
    // 上一条测试漏掉它，是因为夹具没带 last_touched_index（undefined 永远不等于任何数字）。
    const view = {
      ...PLAYER_TURN_VIEW,
      last_touched_index: 2,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: true },
        { index: 2, status: 0, kind: "water", known: true },
        { index: 3, status: 2, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
    };
    const probe = renderWorldUiDocument(document, { view });
    expect(countTubeElements(probe.html)).toBe(4);
    // 刚刚动过的那支应当拿到一次性动画
    expect((probe.html.match(/pf-tube-just-poured/g) ?? []).length).toBe(1);
  });

  it("探明的毒药/清水管分别带上对应的光效 class", () => {
    const view = {
      ...PLAYER_TURN_VIEW,
      tube_slots: [
        { index: 1, status: 2, kind: "poison", known: true },
        { index: 2, status: 1, kind: "water", known: true },
      ],
    };
    const probe = renderWorldUiDocument(document, { view });
    expect(probe.html).toMatch(/pf-tube-known-poison/);
    expect(probe.html).toMatch(/pf-tube-known-water/);
    expect(countTubeElements(probe.html)).toBe(2);
  });

  it("未探查的已倒试管也只画一支（kind 为 unknown）", () => {
    const view = {
      ...PLAYER_TURN_VIEW,
      tube_slots: [
        { index: 1, status: 0, kind: "unknown", known: false },
        { index: 2, status: 0, kind: "unknown", known: false },
        { index: 3, status: 2, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
    };
    const probe = renderWorldUiDocument(document, { view });
    expect(countTubeElements(probe.html)).toBe(4);
  });

  it("双方各三根命烛按存活数渲染", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    // 玩家 [1,1,0] → 2 亮 1 灭；小恶魔 [1,0,0] → 1 亮 2 灭
    expect((probe.html.match(/pf-candle-lit/g) ?? []).length).toBe(3);
    expect((probe.html.match(/pf-candle-out/g) ?? []).length).toBe(3);
  });

  it("银针情报与道具卡在拥有时才出现", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("当前管是毒药");
    expect(probe.text).toContain("试毒银针");

    const bare = renderWorldUiDocument(document, {
      view: { ...PLAYER_TURN_VIEW, my_items: [], has_items: false, player_know_code: 0, player_know: "", devil_inventory_text: "无" },
    });
    expect(bare.text).not.toContain("试毒银针");
    expect(bare.text).not.toContain("当前管是毒药");
  });

  it("轮到小恶魔时，不再显示玩家的行动按钮", () => {
    const probe = renderWorldUiDocument(document, { view: DEVIL_TURN_VIEW });
    expect(probe.text).not.toContain("自己喝下");
    expect(probe.text).not.toContain("灌给小恶魔");
    expect(probe.text).toContain("轮到它了，看着就好。");
  });

  it("聊天区只有只读消息列表，没有输入框和发送按钮", () => {
    // 世界包不允许玩家打字：玩家的行动权由 logic.js 本地结算，
    // 聊天区只用来展示小恶魔台词，输入框与发送按钮一律不渲染。
    for (const platform of ["desktop", "mobile"] as const) {
      const doc = loadUiDocument(platform);
      expect(JSON.stringify(doc)).not.toContain("input_composer");
      expect(JSON.stringify(doc)).not.toContain("看着它就行，不必说话");
      const probe = renderWorldUiDocument(doc, { view: PLAYER_TURN_VIEW });
      expect(probe.html).toContain("message_list");
      expect(probe.html).not.toContain("<textarea");
    }
  });

  it("轮到小恶魔时，按钮写的是「让小恶魔继续」", () => {
    const probe = renderWorldUiDocument(document, { view: DEVIL_TURN_VIEW });
    expect(probe.text).toContain("让小恶魔继续");
    // 旧文案都不能再出现
    expect(probe.text).not.toContain("跳过回合");
    expect(probe.text).not.toContain("把酒盏");
    expect(probe.text).not.toContain("让它来");
  });

  it("未开局只显示开局按钮，结束后显示结果与重开", () => {
    const idle = renderWorldUiDocument(document, { view: IDLE_VIEW });
    expect(idle.text).toContain("掀开桌布");
    expect(idle.text).not.toContain("自己喝下");

    const over = renderWorldUiDocument(document, { view: OVER_VIEW });
    expect(over.text).toContain("小恶魔的烛火全熄了");
    expect(over.text).toContain("再开一局");
    expect(over.text).not.toContain("自己喝下");
  });

  it("战报逐条渲染", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("第 3 轮重新调酒。");
    expect(probe.text).toContain("小恶魔灌给你");
  });

  it("核心行动按钮绑定到正确的 logic handler", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    // React 不会把 action 序列化进 HTML，所以直接从文档树断言动作定义。
    const actionArgs: string[] = [];
    const walk = (node: unknown) => {
      if (Array.isArray(node)) {
        node.forEach(walk);
        return;
      }
      if (!node || typeof node !== "object") return;
      const record = node as Record<string, unknown>;
      const action = record.action as { id?: string; args?: unknown } | undefined;
      if (action?.id === "logic.run") {
        actionArgs.push(JSON.stringify(action.args ?? {}));
      }
      for (const key of ["children", "child", "empty"]) {
        if (record[key]) walk(record[key]);
      }
    };
    walk((document as { layout: { root: unknown } }).layout.root);

    expect(actionArgs.some((raw) => raw.includes('"drink_self"'))).toBe(true);
    expect(actionArgs.some((raw) => raw.includes('"drink_opponent"'))).toBe(true);
    expect(actionArgs.some((raw) => raw.includes("game.newGame"))).toBe(true);
    // 道具按钮必须把 item.id 拼进 use_xxx
    expect(actionArgs.some((raw) => raw.includes("use_{{ item.id }}"))).toBe(true);
    // 所有 logic.run 都必须写回 view，否则界面永远停在旧状态
    expect(actionArgs.every((raw) => raw.includes("game."))).toBe(true);
  });

  it("牌桌区占据主要宽度，聊天区为辅助窄栏", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    // 根布局：两列，右侧聊天栏为固定窄宽
    expect(probe.html).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\)\s+3\d\dpx/);
    // 聊天区在 DOM 里位于牌桌之后，且桌面端不限制牌桌高度
    expect(probe.html.indexOf("pf-table")).toBeLessThan(probe.html.indexOf("pf-chat"));
  });

  it("四个对局区域按“左试管 / 中舞台 / 右命烛”分区，座位区通栏", () => {
    // 布局重排（2026-10-11）：试管架移到左侧竖排，小恶魔保持上方居中，
    // 蜡烛仍占右侧一栏（上小恶魔、下玩家），桌面中央整块空出来挂动画。
    // 座位区跨三列 —— 不跨列的话道具与按钮会把第一列撑到 700px，
    // 反过来把中间舞台列挤成 0（实测 cols = 716px 0px 84px）。
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    for (const zone of [
      "pf-zone-devil",
      "pf-zone-rack",
      "pf-zone-stage",
      "pf-zone-candles",
      "pf-zone-seat",
    ]) {
      expect(probe.html).toContain(zone);
    }
    // 直接验证每个区域节点带的 grid-area，比断言序列化字符串稳。
    const zoneArea = (zone: string) =>
      new RegExp(`class="[^"]*${zone}"[^>]*grid-area:\\s*([a-z]+)`).exec(probe.html)?.[1];
    expect(zoneArea("pf-zone-rack")).toBe("rack");
    expect(zoneArea("pf-zone-stage")).toBe("stage");
    expect(zoneArea("pf-zone-candles")).toBe("candles");
    expect(zoneArea("pf-zone-seat")).toBe("seat");
    expect(zoneArea("pf-zone-devil")).toBe("devil");
    // 试管架必须横排。支数逐轮递增到上限 7 支后，竖排高度是 7 倍，
    // 已经超出牌桌可用高度（实测首尾两支被裁）。class 名本身不足以证明方向，
    // 要直接断言 flex-direction。
    const rackRow = /class="[^"]*pf-rack-row"[^>]*style="([^"]*)"/.exec(probe.html)?.[1] ?? "";
    expect(rackRow).toContain("flex-direction:row");
    // 小恶魔不与玩家争同一格：玩家侧只有文字标识，没有头像素材
    expect(probe.html).toContain("你面前的道具");
    // 蜡烛栏上下分属两人，标签都要在
    expect(probe.text).toContain("小恶魔");
    expect(probe.text).toContain("你");
  });

  it("座位区跨三列，不会把中央舞台挤没", () => {
    // 座位区的道具与行动按钮需要 400px+ 宽度。若它只占第一列，
    // 第一列会被撑到 700px，中间舞台列就被算成 0。
    const doc = (document as { layout: { root: { children: unknown[] } } }).layout.root;
    const grid = (function find(node: unknown): Record<string, unknown> | null {
      if (Array.isArray(node)) {
        for (const item of node) {
          const hit = find(item);
          if (hit) return hit;
        }
        return null;
      }
      if (!node || typeof node !== "object") return null;
      const rec = node as Record<string, unknown>;
      if (rec.class_name === "pf-table-grid") return rec;
      for (const key of Object.keys(rec)) {
        const hit = find(rec[key]);
        if (hit) return hit;
      }
      return null;
    })(doc);

    expect(grid).not.toBeNull();
    const areas = grid?.areas as string[][];
    // 座位区所在行必须横跨所有列
    const seatRow = areas.findIndex((row) => row.includes("seat"));
    expect(seatRow).toBeGreaterThanOrEqual(0);
    expect(new Set(areas[seatRow]).size).toBe(1);
    // 中央列必须有最小宽度保障
    const columns = grid?.columns as string[];
    expect(columns[1]).toMatch(/minmax\(\d+px/);
    void doc;
  });

  it("小恶魔道具和原版桌面库存一样公开", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("对面道具：试毒银针×1、烈酒×1");
    const meta = /class="[^"]*pf-devil-meta"[^>]*>(.*?)<\/div>/.exec(probe.html);
    expect(meta?.[1]).toContain("试毒银针×1、烈酒×1");
  });

  it("进行中分支与结算分支互斥（结束后不残留牌桌）", () => {
    // 曾经 pf-board 没有包在 when 里，导致结束后仍渲染试管与命烛。
    const probe = renderWorldUiDocument(document, { view: OVER_VIEW });
    expect(countTubeElements(probe.html)).toBe(0);
    expect(probe.text).toContain("再开一局");
  });
});

describe("毒酒宴世界包 · 移动端 UI", () => {
  const document = loadUiDocument("mobile");

  it("同样走 state.* 绑定，牌桌正常渲染", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    expect(probe.text).toContain("第 3 轮");
    expect(probe.text).toContain("自己喝下");
    expect(probe.text).toContain("灌给小恶魔");
    expect(probe.text).toContain("试管架");
  });

  it("竖屏单列布局，聊天区在牌桌下方且限高", () => {
    const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
    // 单列
    expect(probe.html).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\)/);
    // 聊天区限高 26vh，且位于牌桌之后
    // 移除输入框后根布局只剩 table/chat 两行
    expect(probe.html).toMatch(/grid-template-rows:[^;]*26vh/);
    // 根布局只剩 table/chat 两行两区。
    // 注意不能对整个 root 做 `"input"` 字符串断言——logic.run 的 args 里
    // 本来就有 `"input": {}`，会把断言误伤。
    const root = (document.layout as { root: { rows: string[]; areas: string[][] } }).root;
    expect(root.rows).toHaveLength(2);
    expect(root.areas.map((row) => row[0])).toEqual(["table", "chat"]);
    expect(probe.html.indexOf("pf-table")).toBeLessThan(probe.html.indexOf("pf-chat"));
  });
});

describe("毒酒宴世界包 · 样式与素材", () => {
  const readCss = (platform: "desktop" | "mobile") =>
    readFileSync(path.join(PACKAGE_ROOT, "world", `ui.${platform}.css`), "utf8");

  it("必须建立根高度链，否则布局塌陷、背景从下方漏出", () => {
    for (const platform of ["desktop", "mobile"] as const) {
      const css = readCss(platform);
      expect(css).toMatch(/html[\s\S]*body[\s\S]*#root/);
      expect(css).toMatch(/\.game-ui-layout/);
      expect(css).toMatch(/height:\s*100%/);
    }
  });

  it("牌桌是视觉主体，聊天区被明确压缩", () => {
    expect(readCss("desktop")).toMatch(/\.pf-chat\b/);
    expect(readCss("mobile")).toMatch(/max-height[^;]*vh|\.pf-chat\b/);
  });

  it("素材走真实图片，不允许退回CSS 绘制图形", () => {
    const css = readCss("desktop");
    // 试管 / 命烛 / 道具 / 头像都必须有真实图片背景
    expect(css).toMatch(/--pf-img-tube-empty:/);
    expect(css).toMatch(/--pf-img-tube-poison:/);
    expect(css).toMatch(/--pf-img-tube-water:/);
    expect(css).toMatch(/--pf-img-candle-lit:/);
    expect(css).toMatch(/--pf-img-candle-out:/);
    expect(css).toMatch(/--pf-img-devil:/);
    expect(css).toMatch(/--pf-img-item-needle:/);
    expect(css).toMatch(/--pf-img-item-spill:/);
    expect(css).toMatch(/--pf-img-item-incense:/);
    expect(css).toMatch(/--pf-img-item-strong:/);
  });

  it("每个道具图标都通过 CSS 变量绑定到真实素材", () => {
    for (const platform of ["desktop", "mobile"] as const) {
      const css = readCss(platform);
      for (const id of ["needle", "spill", "incense", "strong"]) {
        expect(css).toContain(`--pf-img-item-${id}:`);
      }
    }
  });
});

describe("毒酒宴世界包 · 扣血与空杯动画", () => {
  const readCss = (platform: "desktop" | "mobile") =>
    readFileSync(path.join(PACKAGE_ROOT, "world", `ui.${platform}.css`), "utf8");

  const countByClass = (html: string, className: string) =>
    [...html.matchAll(/class="([^"]*)"/g)].filter((match) =>
      (match[1] ?? "").split(/\s+/).includes(className),
    ).length;

  /** 刚喝到毒药：扣血演出应该出现。 */
  const HURT_VIEW = {
    ...PLAYER_TURN_VIEW,
    serve_seq: 4,
    serve_kind: "poison",
    serve_kind_code: 1,
    hurt_seq: 2,
    hurt_active: true,
    hurt_side_code: 1,
    hurt_amount: 1,
    null_seq: 0,
    null_active: false,
  };

  /** 刚喝到清水：什么都没发生。 */
  const NULL_VIEW = {
    ...PLAYER_TURN_VIEW,
    serve_seq: 4,
    serve_kind: "water",
    serve_kind_code: 2,
    hurt_seq: 0,
    hurt_active: false,
    null_seq: 3,
    null_active: true,
  };

  for (const platform of ["desktop", "mobile"] as const) {
    it(`[${platform}] 喝到毒药时播放扣血演出，且定位到受击方`, () => {
      const document = loadUiDocument(platform);
      const probe = renderWorldUiDocument(document, { view: HURT_VIEW });

      // hurt_seq=2 是偶数 → 玩家侧必须落在 player-even 分支
      expect(countByClass(probe.html, "pf-hurt-player-even")).toBe(1);
      expect(countByClass(probe.html, "pf-hurt-devil-even")).toBe(0);
      expect(countByClass(probe.html, "pf-hurt-flash")).toBe(1);
      expect(countByClass(probe.html, "pf-hurt-burst")).toBe(1);
      // 玩家受击配色为红
      expect(probe.html).toContain("--pf-hurt-tint:#ff6b8a");
      // 清水演出此时不该出现
      expect(countByClass(probe.html, "pf-null-ripple")).toBe(0);

      const css = readCss(platform);
      expect(css).toContain("@keyframes pf-hurt-flash");
      expect(css).toContain("@keyframes pf-hurt-burst-player");
      expect(css).toContain("@keyframes pf-hurt-burst-devil");
      // 偏移必须自适应，不能写死 px（窄屏会飞出舞台）
      expect(css).toMatch(/pf-hurt-burst-player[\s\S]*?translateY\(\d+vh\)/);
    });

    it(`[${platform}] 小恶魔受击时演出落在上方（devil 分支）`, () => {
      const document = loadUiDocument(platform);
      const probe = renderWorldUiDocument(document, {
        view: { ...HURT_VIEW, hurt_side_code: 2 },
      });
      expect(countByClass(probe.html, "pf-hurt-devil-even")).toBe(1);
      expect(countByClass(probe.html, "pf-hurt-player-even")).toBe(0);
      expect(probe.html).toContain("--pf-hurt-tint:#b06adf");
    });

    it(`[${platform}] 喝到清水时播放"什么都没发生"，不播扣血`, () => {
      const document = loadUiDocument(platform);
      const probe = renderWorldUiDocument(document, { view: NULL_VIEW });

      // null_seq=3 是奇数 → odd 分支
      expect(countByClass(probe.html, "pf-null-odd")).toBe(1);
      expect(countByClass(probe.html, "pf-null-ripple")).toBe(1);
      expect(probe.text).toContain("什么都没发生");
      // 关键：清水不扣血，绝不能出现受击演出
      expect(countByClass(probe.html, "pf-hurt-flash")).toBe(0);
      expect(countByClass(probe.html, "pf-hurt-burst")).toBe(0);

      const css = readCss(platform);
      expect(css).toContain("@keyframes pf-null-ripple");
      expect(css).toContain("@keyframes pf-null-caption");
    });

    it(`[${platform}] 未发生过的事不残留动画层`, () => {
      const document = loadUiDocument(platform);
      // hurt_seq / null_seq 均为 0：两个演出都不该渲染
      const probe = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
      expect(countByClass(probe.html, "pf-hurt-flash")).toBe(0);
      expect(countByClass(probe.html, "pf-null-ripple")).toBe(0);
      // 开局展示幕仍然是唯一该在的浮层
      expect(countByClass(probe.html, "pf-serve-tube")).toBe(1);
    });

    it(`[${platform}] 扣血与空杯动画都进了 prefers-reduced-motion 降级块`, () => {
      const css = readCss(platform);
      const blocks = [
        ...css.matchAll(/@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\n\}/g),
      ].map((match) => match[0]);
      const all = blocks.join("\n");
      for (const selector of [".pf-hurt-flash", ".pf-hurt-burst", ".pf-null-ripple", ".pf-null-caption"]) {
        expect(all).toContain(selector);
      }
    });
  }
});

describe("毒酒宴世界包 · 两个药水动画", () => {
  const readCss = (platform: "desktop" | "mobile") =>
    readFileSync(path.join(PACKAGE_ROOT, "world", `ui.${platform}.css`), "utf8");

  const countByClass = (html: string, className: string) =>
    [...html.matchAll(/class="([^"]*)"/g)].filter((match) =>
      (match[1] ?? "").split(/\s+/).includes(className),
    ).length;

  /** 开局那一刻的 view：第 1 轮 3 支试管还在架上，没人喝过。 */
  const DEAL_VIEW = {
    ...PLAYER_TURN_VIEW,
    round: 1,
    tubes_left: 3,
    poison_left: 1,
    water_left: 2,
    round_tubes: 3,
    last_touched_index: 0,
    consumed: [],
    player_know: "",
    player_know_code: 0,
    tube_slots: [
      { index: 1, status: 2, kind: "unknown", known: false },
      { index: 2, status: 1, kind: "unknown", known: false },
      { index: 3, status: 1, kind: "unknown", known: false },
    ],
    show_seq: 1,
    show_active: true,
    show_fresh: true,
    serve_seq: 0,
    serve_active: false,
    serve_caption: "",
  };

  for (const platform of ["desktop", "mobile"] as const) {
    it(`[${platform}] 动画一：开局向双方摆出本轮全部药水，喝过第一口后自动退场`, () => {
      const document = loadUiDocument(platform);
      const opening = renderWorldUiDocument(document, { view: DEAL_VIEW });

      // 本轮全部试管逐支"立"起来
      expect(countByClass(opening.html, "pf-deal-tube")).toBe(3);
      expect(opening.text).toContain("小恶魔当众摆出第 1 轮的 3 支药水");
      // 核心机制是隐藏信息：顺序只有小恶魔知道，动画只能演出"摆出瓶子"这个动作，
      // 绝不能宣称双方看清了顺序，否则等于把底牌直接送给玩家。
      expect(opening.text).not.toContain("看清了顺序");
      // 必须挂在 absolute 浮层里：in-flow 会撑高 mid 区、挤扁玩家座位区
      expect(countByClass(opening.html, "pf-overlay")).toBe(1);
      expect(countByClass(opening.html, "pf-deal-layer")).toBe(1);

      // 本轮已喝过（PLAYER_TURN_VIEW.show_active === false）时展示幕必须消失
      const afterDrink = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
      expect(countByClass(afterDrink.html, "pf-deal")).toBe(0);
      expect(afterDrink.text).not.toContain("小恶魔当众摆出");

      // 逐支延迟 + 关键帧都必须存在，否则是"四支一起闪"而不是依次展示
      const css = readCss(platform);
      expect(css).toContain("@keyframes pf-deal-tube-rise");
      expect(css).toContain("@keyframes pf-deal-in");
      // 支数上限 7，错峰延迟必须覆盖到第 7 支
      for (const n of [1, 2, 3, 4, 5, 6, 7]) {
        expect(css).toMatch(
          new RegExp(`\\.pf-deal-tube:nth-child\\(${n}\\)\\s*\\{\\s*animation-delay:`),
        );
      }
      // 浮层定位：内容必须脱离文档流，否则会撑高 pf-zone-mid
      expect(css).toMatch(
        /\.pf-deal-layer,\s*\.pf-serve\s*\{[^}]*position:\s*absolute/,
      );
      // 展示中的 4 支必须是同一张空瓶图：一旦按药性切素材，等于替小恶魔把底牌掀了
      const dealTube = /\.pf-deal-tube\s*\{[^}]*\}/.exec(css)?.[0] ?? "";
      expect(dealTube).toContain("var(--pf-img-tube-empty)");
      expect(dealTube).not.toContain("--pf-img-tube-poison");
      expect(dealTube).not.toContain("--pf-img-tube-water");
      // 定位与动画都写 transform 时会互相覆盖，关键帧必须自带水平居中
      const serveKeyframes = /@keyframes pf-serve-odd \{[\s\S]*?\n\}/.exec(css)?.[0] ?? "";
      expect(serveKeyframes).toMatch(/translate\(-50%, 0\)/);
    });

    it(`[${platform}] 动画二：选择自己喝 / 对方喝时举杯展示该支药水`, () => {
      const document = loadUiDocument(platform);

      // 推向小恶魔的清水
      const toDevil = renderWorldUiDocument(document, { view: PLAYER_TURN_VIEW });
      expect(countByClass(toDevil.html, "pf-serve-tube")).toBe(1);
      expect(countByClass(toDevil.html, "pf-serve-water")).toBe(1);
      // 毒药分支不能同时出现
      expect(countByClass(toDevil.html, "pf-serve-poison")).toBe(0);
      expect(toDevil.text).toContain("第 2 管 · 清水 → 小恶魔");

      // 奇偶交替的 class 必须真的落在 DOM 上——动画重播全靠它
      expect(toDevil.html).toMatch(/pf-serve-(odd|even)/);

      // 推向自己的毒药：素材与文案都要换
      const toPlayer = renderWorldUiDocument(document, {
        view: {
          ...PLAYER_TURN_VIEW,
          serve_seq: 3,
          serve_index: 1,
          serve_kind: "poison",
          serve_kind_code: 1,
          serve_to: "player",
          serve_to_code: 1,
          serve_dy: "150px",
          serve_lean: "20deg",
          serve_accent: "#ff8098",
          serve_caption: "第 1 管 · 毒药 → 你",
        },
      });
      expect(countByClass(toPlayer.html, "pf-serve-poison")).toBe(1);
      expect(countByClass(toPlayer.html, "pf-serve-water")).toBe(0);
      expect(toPlayer.text).toContain("第 1 管 · 毒药 → 你");

      // 未发生过举杯时不渲染
      const idle = renderWorldUiDocument(document, { view: IDLE_VIEW });
      expect(countByClass(idle.html, "pf-serve-tube")).toBe(0);

      // 位移/倾角/光晕色由 logic.js 算好后注入 CSS 变量
      expect(toPlayer.html).toContain("--pf-serve-dy:150px");
      expect(toPlayer.html).toContain("--pf-serve-lean:20deg");
      expect(toPlayer.html).toContain("--pf-serve-accent:#ff8098");

      const css = readCss(platform);
      expect(css).toContain("@keyframes pf-serve-odd");
      expect(css).toContain("@keyframes pf-serve-even");
      expect(css).toContain("@keyframes pf-serve-glow");
      expect(css).toContain("var(--pf-serve-dy");
      expect(css).toContain("var(--pf-serve-lean");
    });

    it(`[${platform}] 两个动画都进了 prefers-reduced-motion 降级块`, () => {
      const css = readCss(platform);
      // 移动端有两个降级块（试管段 + 汇总段），必须全部收集后再断言，
      // 只匹配第一个块会漏掉后面补的类。
      const blocks = [
        ...css.matchAll(/@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\n\}/g),
      ].map((match) => match[0]);
      expect(blocks.length).toBeGreaterThan(0);
      const all = blocks.join("\n");
      for (const selector of [
        ".pf-deal",
        ".pf-deal-tube",
        ".pf-serve-odd",
        ".pf-serve-even",
        ".pf-serve-tube",
        ".pf-serve-caption",
      ]) {
        expect(all).toContain(selector);
      }
    });
  }
});

describe("毒酒宴世界包 · logic.js", () => {
  it("getView 暴露 UI 需要的全部字段", () => {
    const source = loadLogicSource();
    for (const field of [
      "phase_code",
      "turn_code",
      "player_candles",
      "devil_candles",
      "tube_slots",
      "player_know_code",
      "my_items",
      "has_items",
      "over_text",
      "show_active",
      "serve_seq",
      "serve_kind_code",
      "serve_to_code",
      "serve_dy",
      "serve_lean",
      "serve_accent",
      "serve_caption",
    ]) {
      expect(source).toContain(field);
    }
  });
});
