/**
 * 用宿主真实的 logic.js SDK 语义跑《小恶魔的毒酒宴》规则引擎。
 *
 * 之前只对 UI 做过浏览器验证，规则层从没在真实 SDK 下跑过。这里按
 * worldFrame/WorldLogicRuntime.ts 里 createWorkerSource 的等价语义搭一个
 * 内存 KV，让 logic.js 的 handler 在同一套 api 形状下执行。
 */
import { readFileSync } from "node:fs";
import path from "node:path";

export type LogicView = {
  phase: string;
  phase_code: number;
  turn: string;
  turn_code: number;
  winner: string;
  round: number;
  round_tubes: number;
  strong_owner_code: number;
  awaiting_devil: boolean;
  player_candles: number[];
  devil_candles: number[];
  tubes_left: number;
  poison_left: number;
  water_left: number;
  tube_slots: { index: number; status: number; kind: string; known: boolean }[];
  consumed: unknown[];
  player_know: string;
  player_know_code: number;
  strong: boolean;
  devil_asleep: boolean;
  my_items: { id: string; label: string; desc: string; count: number }[];
  has_items: boolean;
  // 开局展示药水动画的数据源
  show_seq: number;
  show_active: boolean;
  show_fresh: boolean;
  // 举杯展示药水动画的数据源
  serve_seq: number;
  serve_active: boolean;
  serve_index: number;
  serve_kind: string;
  serve_kind_code: number;
  serve_to: string;
  serve_to_code: number;
  serve_dy: string;
  serve_lean: string;
  serve_accent: string;
  serve_caption: string;
  // 扣血演出：只在真的熄了烛火时推进
  hurt_seq: number;
  hurt_active: boolean;
  hurt_side_code: number;
  hurt_amount: number;
  // 空杯演出：喝到清水、什么都没发生
  null_seq: number;
  null_active: boolean;
  // 试管移动：刚刚被取走的那一格
  just_moved_index: number;
  hint: string;
  last_event: string;
  log: string[];
  over_text: string;
};

type Handler = (input: unknown, api: unknown, context?: { actor: string }) => Promise<unknown>;

export type LogicHarness = {
  call: (handler: string, input?: unknown) => Promise<LogicView>;
  callRaw: (handler: string, input?: unknown, actor?: string) => Promise<unknown>;
  seed: (overrides: Record<string, unknown>) => void;
  state: () => Record<string, unknown>;
  /** 读取 logic.js 写给大模型的可见文本。 */
  readDevilView: () => Promise<string>;
};

/**
 * 复刻宿主沙箱：注册 handler，提供 records / kv / platform 三个命名空间。
 * KV 用内存 Map 模拟，键为 "namespace/key"。
 */
export async function createPoisonFeastHarness(): Promise<LogicHarness> {
  const source = readFileSync(
    path.resolve(__dirname, "../../../examples/world-packages/poison-feast/world/logic.js"),
    "utf8",
  );

  const handlers = new Map<string, Handler>();
  // 宿主 SDK 形状：api.kv.set(namespace, key, value, options?)。
  // 这里用单个对象承载所有命名空间，键为 "namespace/key"。
  const kvStore = new Map<string, unknown>();

  const api = {
    records: {
      list: async () => [],
      create: async () => ({}),
      update: async () => ({}),
      remove: async () => ({}),
      query: async () => [],
    },
    kv: {
      async list(namespace: string) {
        return [...kvStore.entries()]
          .filter(([key]) => key.startsWith(`${namespace}/`))
          .map(([key, value]) => ({ namespace, key: key.slice(namespace.length + 1), value }));
      },
      async get(namespace: string, key: string, fallback: unknown = null) {
        const full = `${namespace}/${key}`;
        return kvStore.has(full) ? structuredClone(kvStore.get(full)) : fallback;
      },
      async set(namespace: string, key: string, value: unknown) {
        kvStore.set(`${namespace}/${key}`, structuredClone(value));
        return { namespace, key, value };
      },
      async remove(namespace: string, key: string) {
        kvStore.delete(`${namespace}/${key}`);
        return { namespace, key };
      },
    },
    platform: { invoke: async () => ({}) },
  };

  // logic.js 顶层是 `world.register(...)` 调用，用一个最小 world 对象执行。
  const world = {
    register(name: string, handler: Handler) {
      handlers.set(name, handler);
    },
  };
  // eslint-disable-next-line no-new-func
  new Function("world", "globalThis", source)(world, globalThis);

  const callRaw = async (handler: string, input: unknown = {}, actor = handler.startsWith("game.devil") ? "agent" : "player") => {
    const fn = handlers.get(handler);
    if (!fn) {
      throw new Error(`logic.js 未注册 handler: ${handler}`);
    }
    return fn(input, api, { actor });
  };

  return {
    seed: overrides => kvStore.set("game/state", { ...(kvStore.get("game/state") as object), ...structuredClone(overrides) }),
    state: () => structuredClone(kvStore.get("game/state") as Record<string, unknown>),
    callRaw,
    call: async (handler, input = {}) => (await callRaw(handler, input)) as LogicView,
    readDevilView: async () => String(kvStore.get("variables/game_view") ?? ""),
  };
}
