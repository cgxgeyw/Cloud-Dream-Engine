import { describe, expect, it } from "vitest";
import { createPoisonFeastHarness, type LogicView } from "../test-utils/poisonFeastLogicHarness";

async function fixed(overrides: Record<string, unknown> = {}) {
  const h = await createPoisonFeastHarness();
  await h.call("game.newGame");
  h.seed({ tubes: ["poison", "water", "poison", "water"], consumed: [], turn: "player", awaiting_devil: false,
    player_items: {needle: 2, spill: 2, incense: 2, strong: 2, herb: 2},
    devil_items: {needle: 2, spill: 2, incense: 2, strong: 2, herb: 2}, ...overrides });
  return h;
}

describe("authoritative roulette rules", () => {
  it("loads a hidden random batch and equal starting grants", async () => {
    const h = await createPoisonFeastHarness();
    for (let i=0;i<50;i++) {
      const v = await h.call("game.newGame");
      expect(v.round_tubes).toBeGreaterThanOrEqual(2);
      expect(v.round_tubes).toBeLessThanOrEqual(8);
      expect(v.poison_left).toBeGreaterThan(0);
      expect(v.water_left).toBeGreaterThan(0);
      expect(v.poison_left + v.water_left).toBe(v.round_tubes);
      expect(v.turn_code).toBe(1);
      expect(v.tube_slots.every(x => x.kind === "unknown")).toBe(true);
      const state = h.state();
      const count = (bag: unknown) => Object.values(bag as Record<string, number>).reduce((a,b)=>a+b,0);
      expect(count(state.player_items)).toBe(count(state.devil_items));
      expect(count(state.player_items)).toBeGreaterThanOrEqual(2);
      expect(count(state.player_items)).toBeLessThanOrEqual(5);
    }
  });
  it("self water keeps turn, self poison yields, opponent water also yields", async () => {
    const h = await fixed({ tubes: ["water","poison","water","poison"] });
    const safe = await h.call("game.playerAct", {action:"drink_self"});
    expect(safe.turn_code).toBe(1);
    expect(safe.player_candles.filter(Boolean)).toHaveLength(3);
    const hurt = await h.call("game.playerAct", {action:"drink_self"});
    expect(hurt.turn_code).toBe(2);
    expect(hurt.player_candles.filter(Boolean)).toHaveLength(2);
    const result = await h.callRaw("game.devilAct", {action:"drink_opponent"}) as {turn_ended:boolean};
    expect(result.turn_ended).toBe(true);
    expect((await h.call("game.getView")).turn_code).toBe(1);
  });
  it("invalid player and AI actions never consume tubes or items", async () => {
    const h = await fixed();
    const before = h.state();
    await expect(h.call("game.playerAct", {action:"end_turn"})).rejects.toThrow();
    expect(h.state()).toEqual(before);
    h.seed({turn:"devil",awaiting_devil:true,devil_items:{}});
    const devilBefore = h.state();
    const r = await h.callRaw("game.devilAct", {action:"use_needle"}) as {applied:boolean};
    expect(r.applied).toBe(false);
    expect(h.state()).toEqual(devilBefore);
  });
  it("actor identity and turn ownership cannot be supplied by action input", async () => {
    const h = await fixed();
    await expect(h.callRaw("game.devilAct", {action:"drink_self",actor:"agent"}, "player")).rejects.toThrow();
    const before = h.state();
    const r = await h.callRaw("game.devilAct", {action:"drink_self"}) as {applied:boolean};
    expect(r.applied).toBe(false);
    expect(h.state()).toEqual(before);
  });
  it("needle intelligence is private and spill clears intelligence for the next tube", async () => {
    const h = await fixed();
    await h.call("game.playerAct", {action:"use_needle"});
    expect(await h.readDevilView()).not.toContain("current is poison");
    const ai = await h.callRaw("game.devilView") as {state:string};
    expect(ai.state).not.toContain("当前管是【毒药】");
    const spilled = await h.call("game.playerAct", {action:"use_spill"});
    expect(spilled.player_know_code).toBe(0);
    expect(h.state().devil_know).toBeNull();
    expect(spilled.tube_slots.filter(x=>x.status!==0).every(x=>x.kind==="unknown")).toBe(true);
  });
  it("AI can probe then use the result within the same turn", async () => {
    const h = await fixed({turn:"devil",awaiting_devil:true});
    const peek = await h.callRaw("game.devilAct", {action:"use_needle"}) as {turn_ended:boolean,state:string};
    expect(peek.turn_ended).toBe(false);
    expect(peek.state).toContain("当前管是【毒药】");
    expect((await h.call("game.getView")).player_know_code).toBe(0);
    await h.callRaw("game.devilAct", {action:"drink_opponent"});
    expect((await h.call("game.getView")).player_candles.filter(Boolean)).toHaveLength(2);
    expect(h.state().devil_know).toBeNull();
  });
  for (const target of ["self","opponent"]) {
    it("strong doubles poison damage for " + target + " and cannot stack", async () => {
      const h = await fixed();
      await h.call("game.playerAct", {action:"use_strong"});
      await expect(h.call("game.playerAct", {action:"use_strong"})).rejects.toThrow();
      const v = await h.call("game.playerAct", {action:"drink_"+target});
      expect((target==="self"?v.player_candles:v.devil_candles).filter(Boolean)).toHaveLength(1);
      expect(v.strong).toBe(false);
      expect(v.hurt_amount).toBe(2);
    });
  }
  it("water consumes strong and spill does not consume strong", async () => {
    const h = await fixed({tubes:["poison","water","poison"]});
    await h.call("game.playerAct", {action:"use_strong"});
    expect((await h.call("game.playerAct", {action:"use_spill"})).strong).toBe(true);
    const safe = await h.call("game.playerAct", {action:"drink_self"});
    expect(safe.strong).toBe(false);
    expect(safe.turn_code).toBe(1);
    expect(safe.hurt_seq).toBe(0);
  });
  it("restraints skip once, prevent chaining within retained turn, and consume no extra tube", async () => {
    const h = await fixed();
    await h.call("game.playerAct", {action:"use_incense"});
    await expect(h.call("game.playerAct", {action:"use_incense"})).rejects.toThrow();
    const v = await h.call("game.playerAct", {action:"drink_opponent"});
    expect(v.turn_code).toBe(1);
    expect(v.tubes_left).toBe(3);
    expect(v.devil_asleep).toBe(false);
    await expect(h.call("game.playerAct", {action:"use_incense"})).rejects.toThrow();
    await h.call("game.playerAct", {action:"drink_opponent"});
    expect(h.state().incense_used).toBe(false);
  });
  it("reload gives player the first turn, retains old items and caps inventory", async () => {
    const h = await fixed({tubes:["water"],player_items:{needle:7},devil_items:{spill:7}});
    const v = await h.call("game.playerAct", {action:"drink_opponent"});
    expect(v.round).toBe(2);
    expect(v.turn_code).toBe(1);
    const state = h.state();
    expect((state.player_items as {needle:number}).needle).toBeGreaterThanOrEqual(7);
    expect(Object.values(state.player_items as Record<string,number>).reduce((a,b)=>a+b,0)).toBe(8);
    expect(Object.values(state.devil_items as Record<string,number>).reduce((a,b)=>a+b,0)).toBe(8);
  });
  it("healing is capped and lethal hits stop play without reloading", async () => {
    const h = await fixed({player_candles:2,devil_candles:1,tubes:["poison"]});
    const healed = await h.call("game.playerAct", {action:"use_herb"});
    expect(healed.player_candles.filter(Boolean)).toHaveLength(3);
    await h.call("game.playerAct", {action:"use_herb"});
    const over = await h.call("game.playerAct", {action:"drink_opponent"});
    expect(over.phase).toBe("over");
    expect(over.round).toBe(1);
    expect(over.turn_code).toBe(0);
    expect(over.awaiting_devil).toBe(false);
    const before = h.state();
    await h.callRaw("game.devilAct", {action:"drink_opponent"});
    expect(h.state()).toEqual(before);
  });
  it("session start preserves an existing game", async () => {
    const h = await fixed();
    await h.call("game.playerAct", {action:"drink_opponent"});
    const before = h.state();
    await h.callRaw("onSessionStart");
    expect(h.state()).toEqual(before);
  });
});

describe("毒酒宴 · 两个药水动画的数据源", () => {
  it("开局就点亮展示动画，且四支试管全部还在架上", async () => {
    const logic = await createPoisonFeastHarness();
    const view = await logic.call("game.newGame");

    // 展示幕靠 show_active 为真而挂上；show_seq 每调一次酒 +1
    expect(view.show_seq).toBe(1);
    expect(view.show_active).toBe(true);
    expect(view.show_fresh).toBe(true);
    // 举杯动画此时还没发生过
    expect(view.serve_seq).toBe(0);
    expect(view.serve_active).toBe(false);
    expect(view.serve_caption).toBe("");
  });

  it("喝第一口后展示幕退场，举杯数据指向正确的方向与药性", async () => {
    const logic = await createPoisonFeastHarness();

    // 随机性在先手与药性上，两条路径都试一遍，避免单局观测不到某个分支。
    for (let attempt = 0; attempt < 60; attempt++) {
      const before = await logic.call("game.newGame");
      if (before.turn_code !== 1) continue;

      // 自己喝 → 推向自己（画面下方）
      let view = await logic.call("game.playerAct", { action: "drink_self" });
      expect(view.show_active).toBe(false);
      expect(view.serve_seq).toBe(before.serve_seq + 1);
      expect(view.serve_active).toBe(true);
      expect(view.serve_index).toBe(1);
      expect(view.serve_to).toBe("player");
      expect(view.serve_to_code).toBe(1);
      expect(view.serve_dy).toBe("150px");
      expect(view.serve_lean).toBe("20deg");
      expect(["poison", "water"]).toContain(view.serve_kind);
      expect(view.serve_kind_code).toBe(view.serve_kind === "poison" ? 1 : 2);
      // 毒药偏红、清水偏冷蓝；文案必须带上序号与去向
      expect(view.serve_accent).toBe(
        view.serve_kind === "poison" ? "#ff8098" : "#7cc6e8",
      );
      expect(view.serve_caption).toContain("第 1 管");
      expect(view.serve_caption).toContain("→ 你");

      // 灌给对方 → 推向小恶魔（画面上方）
      if (view.turn_code === 1 && view.phase === "playing") {
        const next = await logic.call("game.playerAct", { action: "drink_opponent" });
        expect(next.serve_to).toBe("devil");
        expect(next.serve_to_code).toBe(2);
        expect(next.serve_dy).toBe("-150px");
        expect(next.serve_lean).toBe("-20deg");
        expect(next.serve_caption).toContain("→ 小恶魔");
        expect(next.serve_index).toBe(2);
      }
      return;
    }
    throw new Error("连续 60 局都没轮到玩家，行动分支无法观测");
  });

  it("试管喝空换轮后展示幕重新点亮，序号继续推进", async () => {
    const logic = await createPoisonFeastHarness();
    let view = await logic.call("game.newGame");
    logic.seed({tubes:["water"],consumed:[],turn:"player"});
    const startRound = view.round;
    let guard = 0;

    // 一路喝到换轮；换轮时 round/show_seq 都要 +1 且 show_active 回到 true
    while (view.round === startRound && view.phase === "playing" && guard++ < 60) {
      view = view.turn_code === 1
        ? await logic.call("game.playerAct", { action: "drink_self" })
        : await advanceDevil(logic, guard);
    }

    expect(view.round).toBe(startRound + 1);
    expect(view.show_seq).toBeGreaterThan(1);
    expect(view.show_active).toBe(true);
    // 换轮后是新一轮的支数（递增：第 N 轮 min(N+1, 7) 支）
    expect(view.tubes_left).toBe(view.round_tubes);
    expect(view.serve_seq).toBeGreaterThan(0);
  });

  it("泼洒也会退场展示幕，并标为原地泼掉", async () => {
    const logic = await createPoisonFeastHarness();

    for (let attempt = 0; attempt < 200; attempt++) {
      const view = await logic.call("game.newGame");
      if (view.turn_code !== 1) continue;
      if (!view.my_items.some((item) => item.id === "spill")) continue;

      const poured = await logic.call("game.playerAct", { action: "use_spill" });
      expect(poured.show_active).toBe(false);
      expect(poured.serve_active).toBe(true);
      expect(poured.serve_to).toBe("none");
      expect(poured.serve_to_code).toBe(0);
      // 原地泼掉：不位移、不倾倒
      expect(poured.serve_dy).toBe("0px");
      expect(poured.serve_lean).toBe("0deg");
      expect(poured.serve_caption).toContain("泼掉第 1 管");
      return;
    }
    throw new Error("连续 200 局都没拿到泼洒道具，该分支无法观测");
  });

  it("举杯序号逐次递增，UI 靠它切奇偶 class 重播动画", async () => {
    const logic = await createPoisonFeastHarness();
    const seqs: number[] = [];
    let view = await logic.call("game.newGame");
    seqs.push(view.serve_seq);
    let guard = 0;

    while (view.phase === "playing" && guard++ < 40) {
      view = view.turn_code === 1
        ? await logic.call("game.playerAct", { action: "drink_self" })
        : await advanceDevil(logic, guard);
      seqs.push(view.serve_seq);
    }

    // 严格递增，class 一定变化、动画一定重播
    for (let i = 1; i < seqs.length; i++) {
      expect(seqs[i]).toBeGreaterThan(seqs[i - 1]!);
    }
  });
});

async function advanceDevil(h: Awaited<ReturnType<typeof createPoisonFeastHarness>>, step: number): Promise<LogicView> {
  await h.callRaw("game.devilAct", {action:"drink_opponent"});
  return h.call("game.getView");
}
