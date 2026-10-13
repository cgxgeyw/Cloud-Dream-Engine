/**
 * 世界包 UI 视觉验证页（仅开发用）。
 *
 * 关键点：这里渲染的是**宿主真实的 GameUiRenderer**，并把世界包自己的 CSS 原样注入，
 * 外层再套一层与生产一致的 iframe + CSP（见 harness.html 的 meta）。
 * 之前"自造原型 HTML 验证通过、线上整块空白"的事故，就是因为绕开了这条链路。
 */
import { createRoot } from "react-dom/client";

import { GameUiRenderer } from "../components/GameUiRenderer";
import { evaluateGameUiExpression } from "../gameUiRuntime/expression";
import type { GameUiRenderContext } from "../components/GameUiRenderer";

import poisonFeastDesktopCss from "../../../examples/world-packages/poison-feast/world/ui.desktop.css?raw";
import poisonFeastMobileCss from "../../../examples/world-packages/poison-feast/world/ui.mobile.css?raw";
import poisonFeastDesktopDoc from "../../../examples/world-packages/poison-feast/world/ui.desktop.jsonc?raw";
import poisonFeastMobileDoc from "../../../examples/world-packages/poison-feast/world/ui.mobile.jsonc?raw";

declare global {
  interface Window {
    __setScenario?: (scenario: string) => void;
    __currentScenario?: string;
    __renderErrors?: string[];
  }
}

const VIEW_SCENARIOS: Record<string, Record<string, unknown>> = {
  // 开局那一刻：4 支试管在架上没人动过，展示动画必须挂出来
  deal: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 1,
      awaiting_devil: false,
      player_candles: [1, 1, 1],
      devil_candles: [1, 1, 1],
      tubes_left: 4,
      round_tubes: 4,
      poison_left: 2,
      water_left: 2,
      tube_slots: [
        { index: 1, status: 2, kind: "unknown", known: false },
        { index: 2, status: 1, kind: "unknown", known: false },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [],
      last_touched_index: 0,
      devil_item_count: 2,
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [{ id: "needle", label: "试毒银针", desc: "看穿当前管是毒是水，只有你知道", count: 1 }],
      has_items: true,
      show_seq: 1,
      show_active: true,
      show_fresh: true,
      serve_seq: 0,
      serve_active: false,
      serve_index: 0,
      serve_kind: "",
      serve_kind_code: 0,
      serve_to: "",
      serve_to_code: 0,
      serve_dy: "0px",
      serve_lean: "0deg",
      serve_accent: "#b06adf",
      serve_caption: "",
      hint: "轮到你行动。",
      last_event: "第 1 轮开始。先手：你。",
      log: ["毒酒宴开局：你 3 根命烛，小恶魔 3 根命烛。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 刚把毒药推向自己：举杯动画的"推向玩家"分支
  serve: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 2,
      awaiting_devil: false,
      player_candles: [1, 1, 1],
      devil_candles: [1, 1, 1],
      tubes_left: 3,
      round_tubes: 4,
      poison_left: 1,
      water_left: 2,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: false },
        { index: 2, status: 2, kind: "unknown", known: false },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [{ index: 1, kind: "poison", spilled: false }],
      last_touched_index: 1,
      devil_item_count: 1,
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [{ id: "spill", label: "泼洒", desc: "公开倒掉当前管，不消耗行动权", count: 1 }],
      has_items: true,
      show_seq: 1,
      show_active: false,
      show_fresh: false,
      serve_seq: 3,
      serve_active: true,
      serve_index: 1,
      serve_kind: "poison",
      serve_kind_code: 1,
      serve_to: "player",
      serve_to_code: 1,
      serve_dy: "150px",
      serve_lean: "20deg",
      serve_accent: "#ff8098",
      serve_caption: "第 1 管 · 毒药 → 你",
      hint: "轮到你行动。",
      last_event: "你把第 1 管灌给自己：毒药！熄了 1 根烛火。",
      log: ["你把第 1 管灌给自己：毒药！熄了 1 根烛火。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 刚喝到毒药被扣血：hurt 演出必须挂出来（偶数序号 → player-even 分支）
  hurt: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "devil",
      turn_code: 2,
      winner: "",
      round: 3,
      awaiting_devil: true,
      player_candles: [1, 0, 0],
      devil_candles: [1, 1, 0],
      tubes_left: 3,
      round_tubes: 4,
      poison_left: 2,
      water_left: 1,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: false },
        { index: 2, status: 2, kind: "unknown", known: false },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [{ index: 1, kind: "poison", spilled: false }],
      last_touched_index: 1,
      devil_item_count: 2,
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [{ id: "incense", label: "迷魂香", desc: "对手下一次行动权直接睡过", count: 1 }],
      has_items: true,
      show_seq: 1,
      show_active: false,
      show_fresh: false,
      serve_seq: 4,
      serve_active: true,
      serve_index: 1,
      serve_kind: "poison",
      serve_kind_code: 1,
      serve_to: "player",
      serve_to_code: 1,
      serve_dy: "150px",
      serve_lean: "20deg",
      serve_accent: "#ff8098",
      serve_caption: "第 1 管 · 毒药 → 你",
      // 扣血演出
      hurt_seq: 2,
      hurt_active: true,
      hurt_side_code: 1,
      hurt_amount: 1,
      // 空杯演出：此时不该出现
      null_seq: 0,
      null_active: false,
      just_moved_index: 1,
      hint: "轮到它了，看着就好。",
      last_event: "你把第 1 管灌给自己：毒药！熄了 1 根烛火。",
      log: ["你把第 1 管灌给自己：毒药！熄了 1 根烛火。"],
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 刚喝到清水：什么都没发生的松弛演出（奇数序号 → odd 分支）
  null: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 3,
      awaiting_devil: false,
      player_candles: [1, 1, 1],
      devil_candles: [1, 1, 1],
      tubes_left: 3,
      round_tubes: 4,
      poison_left: 1,
      water_left: 2,
      tube_slots: [
        { index: 1, status: 0, kind: "water", known: false },
        { index: 2, status: 2, kind: "unknown", known: false },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [{ index: 1, kind: "water", spilled: false }],
      last_touched_index: 1,
      devil_item_count: 2,
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [],
      has_items: false,
      show_seq: 1,
      show_active: false,
      show_fresh: false,
      serve_seq: 4,
      serve_active: true,
      serve_index: 1,
      serve_kind: "water",
      serve_kind_code: 2,
      serve_to: "player",
      serve_to_code: 1,
      serve_dy: "150px",
      serve_lean: "20deg",
      serve_accent: "#7cc6e8",
      serve_caption: "第 1 管 · 清水 → 你",
      // 扣血演出：清水不该触发
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      // 空杯演出
      null_seq: 3,
      null_active: true,
      just_moved_index: 1,
      hint: "轮到你行动。",
      last_event: "你喝下第 1 管：清水。行动权还在你手里。",
      log: ["你喝下第 1 管：清水。行动权还在你手里。"],
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 后期轮次：支数递增到上限 7 支，试管架必须装得下且不被裁
  late: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 6,
      awaiting_devil: false,
      player_candles: [1, 0, 0],
      devil_candles: [1, 0, 0],
      tubes_left: 5,
      poison_left: 3,
      water_left: 2,
      round_tubes: 7,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: false },
        { index: 2, status: 0, kind: "water", known: false },
        { index: 3, status: 2, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
        { index: 5, status: 1, kind: "unknown", known: false },
        { index: 6, status: 1, kind: "unknown", known: false },
        { index: 7, status: 1, kind: "unknown", known: false },
      ],
      consumed: [
        { index: 1, kind: "poison", spilled: false },
        { index: 2, kind: "water", spilled: false },
      ],
      last_touched_index: 2,
      devil_item_count: 2,
      player_know: "",
      player_know_code: 0,
      strong: true,
      strong_owner_code: 1,
      devil_asleep: false,
      my_items: [
        { id: "needle", label: "试毒银针", desc: "看穿当前管是毒是水，只有你知道", count: 1 },
        { id: "strong", label: "烈酒", desc: "你的下一口毒药将熄灭两根烛火", count: 1 },
      ],
      has_items: true,
      show_seq: 6,
      show_active: false,
      show_fresh: false,
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
      hurt_seq: 4,
      hurt_active: true,
      hurt_side_code: 1,
      hurt_amount: 1,
      null_seq: 1,
      null_active: false,
      just_moved_index: 2,
      hint: "轮到你行动。",
      last_event: "第 6 轮，桌上 7 支药水——这轮毒药不少了。",
      log: ["第 6 轮：小恶魔重新调了 7 支试管（3 毒 4 清），双方各得 2 件道具。"],
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 玩家回合：有毒有水、银针已探查、持有道具
  player: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 3,
      awaiting_devil: false,
      player_candles: [1, 1, 0],
      devil_candles: [1, 0, 0],
      tubes_left: 2,
      round_tubes: 4,
      poison_left: 1,
      water_left: 1,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: true },
        { index: 2, status: 0, kind: "water", known: true },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 2, kind: "unknown", known: false },
      ],
      consumed: [{ index: 1, kind: "poison" }, { index: 2, kind: "water" }],
      last_touched_index: 2,
      devil_item_count: 2,
      player_know: "poison",
      player_know_code: 1,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [
        { id: "needle", label: "试毒银针", desc: "看穿当前管是毒是水，只有你知道", count: 1 },
        { id: "incense", label: "迷魂香", desc: "对手下一次行动权直接睡过", count: 2 },
        { id: "strong", label: "烈酒", desc: "你的下一口毒药将熄灭两根烛火", count: 1 },
      ],
      has_items: true,
      hint: "轮到你行动。",
      last_event: "你把第 2 管灌给小恶魔：清水……什么也没发生。",
      log: ["第 3 轮重新调酒。先手：你。", "小恶魔灌给你第 1 管：毒药！你熄了 1 根烛火。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 探明毒药 + 小恶魔被熏晕：验证 known-poison / known-water / is-asleep 三种动画
  revealed: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "player",
      turn_code: 1,
      winner: "",
      round: 5,
      awaiting_devil: false,
      player_candles: [1, 1, 1],
      devil_candles: [1, 1, 0],
      tubes_left: 3,
      round_tubes: 4,
      poison_left: 2,
      water_left: 1,
      tube_slots: [
        { index: 1, status: 1, kind: "poison", known: true },
        { index: 2, status: 2, kind: "poison", known: true },
        { index: 3, status: 1, kind: "water", known: true },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [{ index: 0, kind: "unknown" }],
      last_touched_index: 0,
      devil_item_count: 2,
      player_know: "poison",
      player_know_code: 1,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: true,
      my_items: [{ id: "incense", label: "迷魂香", desc: "对手下一次行动权直接睡过", count: 1 }],
      has_items: true,
      hint: "银针告诉你当前管是毒药，而小恶魔正昏睡着。",
      last_event: "银针刺入第 2 管：黑色。",
      log: ["你用银针看穿了第 2 管。", "迷魂香在小恶魔头顶绕了三圈，它打了个哈欠。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 轮到小恶魔：不该出现玩家的行动按钮
  devil: {
    view: {
      phase: "playing",
      phase_code: 1,
      turn: "devil",
      turn_code: 2,
      winner: "",
      round: 4,
      awaiting_devil: true,
      player_candles: [1, 0, 0],
      devil_candles: [1, 1, 1],
      tubes_left: 3,
      round_tubes: 4,
      poison_left: 2,
      water_left: 1,
      tube_slots: [
        { index: 1, status: 1, kind: "unknown", known: false },
        { index: 2, status: 2, kind: "unknown", known: false },
        { index: 3, status: 1, kind: "unknown", known: false },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [{ index: 1, kind: "water" }],
      player_know: "",
      player_know_code: 0,
      strong: true,
      strong_owner_code: 2,
      devil_asleep: false,
      my_items: [{ id: "spill", label: "泼洒", desc: "公开倒掉当前管，不消耗行动权", count: 1 }],
      has_items: true,
      hint: "把酒盏推向小恶魔，看它怎么选。",
      last_event: "小恶魔喝了烈酒，獠牙亮了起来。",
      log: ["小恶魔灌给你第 1 管：清水……什么也没发生。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 未开局
  idle: {
    view: {
      phase: "idle",
      phase_code: 0,
      turn: "none",
      turn_code: 0,
      winner: "",
      round: 0,
      awaiting_devil: false,
      player_candles: [0, 0, 0],
      devil_candles: [0, 0, 0],
      tubes_left: 0,
      poison_left: 0,
      water_left: 0,
      tube_slots: [],
      consumed: [],
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [],
      has_items: false,
      hint: "点下面的按钮，掀开桌布。",
      last_event: "",
      log: [],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "",
    },
    pf_error: "",
    pf_loading: false,
  },
  // 结算
  over: {
    view: {
      phase: "over",
      phase_code: 2,
      turn: "none",
      turn_code: 0,
      winner: "player",
      round: 6,
      awaiting_devil: false,
      player_candles: [1, 0, 0],
      devil_candles: [0, 0, 0],
      tubes_left: 1,
      round_tubes: 4,
      poison_left: 1,
      water_left: 0,
      tube_slots: [
        { index: 1, status: 0, kind: "poison", known: true },
        { index: 2, status: 0, kind: "water", known: true },
        { index: 3, status: 0, kind: "poison", known: true },
        { index: 4, status: 1, kind: "unknown", known: false },
      ],
      consumed: [],
      player_know: "",
      player_know_code: 0,
      strong: false,
      strong_owner_code: 0,
      devil_asleep: false,
      my_items: [],
      has_items: false,
      hint: "你赢了。",
      last_event: "小恶魔的最后一根烛火熄了——你赢了，灵魂归你。",
      log: ["你把第 3 管灌给小恶魔：毒药！它熄了 2 根烛火。"],
      hurt_seq: 0,
      hurt_active: false,
      hurt_side_code: 0,
      hurt_amount: 0,
      null_seq: 0,
      null_active: false,
      just_moved_index: 0,
      over_text: "小恶魔的烛火全熄了，灵魂归你。",
    },
    pf_error: "",
    pf_loading: false,
  },
};

function parseJsonc(source: string): Record<string, unknown> {
  // 验证页只需要世界包这种"无注释或简单注释"的 JSONC，够用即可。
  const stripped = source
    .split("\n")
    .map((line) => line.replace(/^\s*\/\/.*$/, ""))
    .join("\n");
  return JSON.parse(stripped) as Record<string, unknown>;
}

const platform = new URLSearchParams(window.location.search).get("platform") === "mobile"
  ? "mobile"
  : "desktop";
const initialScenario = new URLSearchParams(window.location.search).get("scenario") ?? "player";
const rawDocument = platform === "mobile" ? poisonFeastMobileDoc : poisonFeastDesktopDoc;
const worldCss = platform === "mobile" ? poisonFeastMobileCss : poisonFeastDesktopCss;

const styleTag = document.createElement("style");
styleTag.textContent = worldCss;
document.head.appendChild(styleTag);

const container = document.createElement("div");
container.className = "game-root game-root--session game-root--desktop-session game-ui-root";
document.getElementById("world-frame-root")?.appendChild(container);

/**
 * 占位组件：验证页不连真实会话，但要用真实尺寸的盒子撑开布局，
 * 否则聊天栏高度塌陷会掩盖真实的溢出问题。
 */
const STUB_COMPONENTS: Record<string, () => React.ReactNode> = {
  message_list: () => (
    <div
      style={{
        height: "100%",
        minHeight: 0,
        overflowY: "auto",
        padding: "8px",
        fontSize: 13,
        color: "#cfc0e6",
      }}
    >
      <div style={{ marginBottom: 10 }}>欢迎来到毒酒宴，客人。</div>
      <div style={{ marginBottom: 10, opacity: 0.75 }}>
        桌上摆着随机数量的试管，毒药与清水的数量以牌面为准，顺序我可不会告诉你。
      </div>
      <div style={{ opacity: 0.6 }}>（验证页占位消息）</div>
    </div>
  ),
  input_composer: () => (
    <div style={{ flex: "0 0 auto", paddingTop: 8 }}>
      <div
        style={{
          border: "1px solid rgba(122,92,168,0.55)",
          borderRadius: 12,
          padding: "8px 10px",
          fontSize: 13,
          color: "#a394c4",
          background: "rgba(28,19,43,0.7)",
        }}
      >
        看着它就行，不必说话
      </div>
    </div>
  ),
} as never;

const root = createRoot(container);
const errors: string[] = [];
window.__renderErrors = errors;
window.addEventListener("error", (event) => errors.push(String(event.message)));

function render(scenario: string) {
  const parsed = parseJsonc(rawDocument);
  const scenarioState = VIEW_SCENARIOS[scenario] ?? VIEW_SCENARIOS.player;
  window.__currentScenario = scenario;
  const merged = {
    ...parsed,
    state: { ...((parsed.state as Record<string, unknown>) ?? {}), ...scenarioState },
  };
  root.render(
    <GameUiRenderer
      document={merged as never}
      componentRenderers={STUB_COMPONENTS}
      runtimeData={{}}
      evaluateCondition={(expression: string, context: GameUiRenderContext) =>
        evaluateGameUiExpression(expression, {
          ...context.data,
          ...context.locals,
          state: context.state,
        })
      }
      onAction={() => Promise.resolve(undefined)}
    />,
  );
}

window.__setScenario = render;
window.__currentScenario = "pending";
render(initialScenario);
