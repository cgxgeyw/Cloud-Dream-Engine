/**
 * 用真实 Chromium 打开视觉验证页，逐个场景截图 + 收集控制台错误。
 * 跑法：node scripts/shoot-world-ui.mjs
 */
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import path from "node:path";

const BASE = process.env.HARNESS_BASE ?? "http://127.0.0.1:8899";
const OUT = path.resolve("output/world-ui-shots");
mkdirSync(OUT, { recursive: true });

const SCENARIOS = ["deal", "serve", "player", "late", "revealed", "devil", "hurt", "null", "idle", "over"];

const VIEWPORTS = [
  { name: "desktop", platform: "desktop", width: 1280, height: 800 },
  { name: "mobile", platform: "mobile", width: 390, height: 844 },
];

const problems = [];
const browser = await chromium.launch();

for (const viewport of VIEWPORTS) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (error) => consoleErrors.push(String(error)));

  for (const scenario of SCENARIOS) {
    consoleErrors.length = 0;
    await page.goto(
      `${BASE}/world-ui-harness.html?platform=${viewport.platform}&scenario=${scenario}`,
      { waitUntil: "networkidle" },
    );
    await page.waitForFunction(() => typeof window.__setScenario === "function");
    await page.evaluate((name) => window.__setScenario(name), scenario);
    await page.waitForFunction(
      (name) => window.__currentScenario === name,
      scenario,
      { timeout: 5000 },
    );
    await page.waitForTimeout(400);

    const file = path.join(OUT, `${viewport.name}-${scenario}.png`);
    await page.screenshot({ path: file });

    // 关键断言：牌桌核心内容必须真实出现在 DOM 里且可见
    const probe = await page.evaluate(() => {
      const visible = (selector) => {
        const el = document.querySelector(selector);
        if (!el) return null;
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        return {
          w: Math.round(rect.width),
          h: Math.round(rect.height),
          display: style.display,
          backgroundImage: style.backgroundImage.slice(0, 60),
        };
      };
      const text = document.body.innerText.replace(/\s+/g, " ").trim();
      // 用 getComputedStyle 而不是 getAnimations()：
      // 浏览器会把只改 filter/opacity 的关键帧优化掉，不生成 CSSAnimation 对象，
      // getAnimations() 会漏报，导致"动画没跑"的误判。
      const animOf = (selector) => {
        const el = document.querySelector(selector);
        return el ? getComputedStyle(el).animationName : "MISSING";
      };
      const animations = [
        animOf(".pf-candle-lit"),
        animOf(".pf-devil-avatar"),
        animOf(".pf-tube-active"),
        animOf(".pf-tube-known-poison"),
        animOf(".pf-tube-known-water"),
        animOf(".pf-btn-big"),
        animOf(".pf-hint"),
        animOf(".pf-deal"),
        animOf(".pf-deal-tube"),
        animOf(".pf-serve-odd, .pf-serve-even"),
        animOf(".pf-serve-tube"),
        animOf(".pf-serve-caption"),
        // 扣血与空杯演出（动画三 / 动画四）
        animOf(".pf-hurt-flash"),
        animOf(".pf-hurt-burst"),
        animOf(".pf-null-ripple"),
        animOf(".pf-null-caption"),
      ].filter((n) => n && n !== "MISSING" && n !== "none");
      return {
        animations,
        asleep: !!document.querySelector(".pf-devil-avatar.is-asleep"),
        knownPoison: !!document.querySelector(".pf-tube-known-poison"),
        knownWater: !!document.querySelector(".pf-tube-known-water"),
        // 探明光效类必须同时换掉背景图，否则"发光的空管"等于没探明
        knownPoisonImg: (() => {
          const el = document.querySelector(".pf-tube-known-poison");
          return el ? getComputedStyle(el).backgroundImage.includes("data:") : null;
        })(),
        text,
        root: visible(".game-ui-layout"),
        table: visible(".pf-table"),
        tube: visible(".pf-tube-active") ?? visible(".pf-tube"),
        candle: visible(".pf-candle-lit"),
        devil: visible(".pf-devil-avatar"),
        tubeCount: document.querySelectorAll(".pf-tube").length,
        // 四个区域的实际位置，用来验证"左试管 / 中舞台 / 右命烛"的分区
        zones: Object.fromEntries(
          ["devil", "rack", "stage", "candles", "seat"].map((n) => {
            const el = document.querySelector(`.pf-zone-${n}`);
            if (!el) return [n, null];
            const r = el.getBoundingClientRect();
            return [n, { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) }];
          }),
        ),
        // 新动画层的 DOM 事实：扣血与空杯
        hurtLayer: document.querySelectorAll('[class*="pf-hurt-"]').length,
        nullLayer: document.querySelectorAll('[class*="pf-null-"]').length,
        hurt: [...document.querySelectorAll('[class*="pf-hurt-"]')].map((el) => el.className),
        // 蜡烛区上下（桌面）或左右（移动）分属两人：按标签文字定位，
        // 不能用 nth-of-type —— 两端的方向不同，序号对不上。
        devilCandleLit: (() => {
          const sides = [...document.querySelectorAll(".pf-candle-side")];
          const devilSide = sides.find((s) => (s.textContent ?? "").includes("小恶魔"));
          return devilSide ? devilSide.querySelectorAll(".pf-candle-lit").length : 0;
        })(),
        playerCandleLit: (() => {
          const sides = [...document.querySelectorAll(".pf-candle-side")];
          const playerSide = sides.find(
            (s) => !(s.textContent ?? "").includes("小恶魔") && (s.textContent ?? "").includes("你"),
          );
          return playerSide ? playerSide.querySelectorAll(".pf-candle-lit").length : 0;
        })(),
        candleLit: document.querySelectorAll(".pf-candle-lit").length,
        // 两个动画的 DOM 事实：几支展示试管、举杯层的 class 与注入的 CSS 变量
        dealTubeCount: document.querySelectorAll(".pf-deal-tube").length,
        dealBox: visible(".pf-deal"),
        serveTube: visible(".pf-serve-tube"),
        serveCaption: (() => {
          const el = document.querySelector(".pf-serve-caption");
          return el ? el.innerText.trim() : null;
        })(),
        serveVars: (() => {
          const el = document.querySelector(".pf-serve-odd, .pf-serve-even");
          if (!el) return null;
          const s = el.getAttribute("style") ?? "";
          return {
            dy: /--pf-serve-dy:\s*([^;]+)/.exec(s)?.[1]?.trim() ?? null,
            lean: /--pf-serve-lean:\s*([^;]+)/.exec(s)?.[1]?.trim() ?? null,
            accent: /--pf-serve-accent:\s*([^;]+)/.exec(s)?.[1]?.trim() ?? null,
          };
        })(),
        buttons: [...document.querySelectorAll(".pf-btn")].map((b) => b.innerText.trim()),
        docHeight: document.documentElement.scrollHeight,
        viewportHeight: window.innerHeight,
        // 座位区内容是否溢出牌桌：道具卡被裁掉玩家就点不到了
        seatOverflow: (() => {
          const seat = document.querySelector(".pf-zone-seat");
          if (!seat) return null;
          return Math.round(seat.scrollHeight - seat.clientHeight);
        })(),
        rackTubes: document.querySelectorAll(".pf-zone-rack .pf-tube").length,
        // 本轮总支数：读试管架标签里的 N。
        // 不能用 innerText —— 窄列 CSS 把标签设成 display:none，
        // innerText 读不到隐藏元素；也不能读"架上 N"，那是剩余支数。
        roundTubes: (() => {
          const el = document.querySelector(".pf-rack-label");
          const m = el && /(\d+)/.exec(el.textContent ?? "");
          return m ? Number(m[1]) : null;
        })(),
      };
    });

    const label = `${viewport.name}/${scenario}`;
    if (consoleErrors.length) {
      problems.push(`[${label}] 控制台错误: ${consoleErrors.join(" | ")}`);
    }
    if (!probe.table || probe.table.h < 80) {
      problems.push(`[${label}] 牌桌不可见或高度塌陷: ${JSON.stringify(probe.table)}`);
    }
    if (!probe.root || probe.root.h < 200) {
      problems.push(`[${label}] 根容器高度塌陷: ${JSON.stringify(probe.root)}`);
    }
    if (scenario === "player" || scenario === "devil" || scenario === "deal" || scenario === "serve") {
      if (probe.tubeCount !== 4) {
        problems.push(`[${label}] 试管数量应为 4，实际 ${probe.tubeCount}`);
      }
      if (!probe.tube || probe.tube.h < 20) {
        problems.push(`[${label}] 试管未显示: ${JSON.stringify(probe.tube)}`);
      }
      if (probe.candleLit === 0) {
        problems.push(`[${label}] 没有点亮的命烛`);
      }
      if (!probe.devil || probe.devil.h < 20) {
        problems.push(`[${label}] 小恶魔头像未显示: ${JSON.stringify(probe.devil)}`);
      }
    }
    if (scenario === "player") {
      if (!probe.text.includes("自己喝下") || !probe.text.includes("灌给小恶魔")) {
        problems.push(`[${label}] 缺少核心行动按钮: ${JSON.stringify(probe.buttons)}`);
      }
      if (!probe.text.includes("第 3 轮")) {
        problems.push(`[${label}] 轮次数值未渲染: ${probe.text.slice(0, 80)}`);
      }
      if (!probe.animations.includes("pf-tube-pulse")) {
        problems.push(`[${label}] 当前试管呼吸动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.text.includes("试毒银针")) {
        problems.push(`[${label}] 道具卡未渲染`);
      }
    }
    if (scenario === "devil") {
      if (probe.text.includes("自己喝下")) {
        problems.push(`[${label}] 轮到小恶魔时仍显示玩家行动按钮`);
      }
      if (!probe.text.includes("看它怎么选")) {
        problems.push(`[${label}] 缺少推盏按钮`);
      }
    }
    // 动画断言：命烛光焰与小恶魔悬浮必须在跑
    if (scenario === "player" || scenario === "revealed" || scenario === "devil" || scenario === "deal") {
      if (!probe.animations.includes("pf-flame")) {
        problems.push(`[${label}] 命烛光焰动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      // is-thinking 会覆盖 pf-devil-hover（同一元素只能有一个 animation），
      // 三种状态都算"小恶魔有动画在跑"。
      if (
        !probe.animations.some(
          (n) => n === "pf-devil-hover" || n === "pf-devil-sleep" || n === "pf-devil-thinking",
        )
      ) {
        problems.push(`[${label}] 小恶魔悬浮动画未生效: ${JSON.stringify(probe.animations)}`);
      }
    }
    // 布局断言（2026-10-11 重排后）
    //   桌面端：左试管架（竖排）/ 中舞台 / 右蜡烛（上小恶魔、下玩家）
    //   移动端：竖屏放不下三列，改成竖向三段
    //            top(试管架+小恶魔+蜡烛) / stage / seat
    if (["player", "late", "revealed", "devil", "deal", "serve", "hurt", "null"].includes(scenario)) {
      const z = probe.zones;
      const missing = Object.entries(z).filter(([, v]) => !v || v.h < 20).map(([k]) => k);
      if (missing.length) {
        problems.push(`[${label}] 区域缺失或塌陷: ${missing.join(",")} ${JSON.stringify(z)}`);
      } else {
        const { devil, rack, stage, candles, seat } = z;
        if (viewport.name === "desktop") {
          // 左中右三列
          if (!(rack.x + rack.w <= stage.x + 6)) {
            problems.push(`[${label}] 试管架应在舞台左侧: rack.right=${rack.x + rack.w} stage.x=${stage.x}`);
          }
          if (!(stage.x + stage.w <= candles.x + 6)) {
            problems.push(`[${label}] 舞台应在蜡烛栏左侧: stage.right=${stage.x + stage.w} candles.x=${candles.x}`);
          }
          // 上下：小恶魔在上、座位在下
          if (!(devil.y + devil.h <= seat.y + 6)) {
            problems.push(`[${label}] 小恶魔应在玩家座位上方: devil.bottom=${devil.y + devil.h} seat.y=${seat.y}`);
          }
          if (!(seat.y >= rack.y + rack.h - 6)) {
            problems.push(`[${label}] 玩家座位应在试管架下方: seat.y=${seat.y} rack.bottom=${rack.y + rack.h}`);
          }
          // 试管架横排：高度应接近单支试管（竖排时高度是支数倍）
          if (rack.h > probe.tube.h * 1.8) {
            problems.push(`[${label}] 试管架不是横排: rack.h=${rack.h} tube.h=${probe.tube.h}`);
          }
        } else {
          // 竖屏两列：左窄列(上=小恶魔,下=试管架) / 右宽列(上=命烛,中=舞台)，座位区通栏
          // 小恶魔与试管架在同一列上下叠，所以是上下关系不是左右关系
          if (!(rack.y >= devil.y + devil.h - 6)) {
            problems.push(`[${label}] 试管架应在小恶魔下方: rack.y=${rack.y} devil.bottom=${devil.y + devil.h}`);
          }
          // 试管架左列、舞台右列 → 左右分离
          if (!(rack.x + rack.w <= stage.x + 6)) {
            problems.push(`[${label}] 试管架应在舞台左侧: rack.right=${rack.x + rack.w} stage.x=${stage.x}`);
          }
          // 命烛在舞台上方
          if (!(stage.y >= candles.y + candles.h - 6)) {
            problems.push(`[${label}] 舞台应在命烛下方: stage.y=${stage.y} candles.bottom=${candles.y + candles.h}`);
          }
          // 座位区通栏，在最下方
          if (!(seat.y >= Math.max(stage.y + stage.h, rack.y + rack.h) - 6)) {
            problems.push(`[${label}] 座位区应在最下方: seat.y=${seat.y}`);
          }
          // 座位区必须通栏：它横跨两列，宽度应大于 "左窄列 + 中央舞台" 中任一列
          // （用舞台宽度做参照更稳，不受左列宽窄变化影响）
          if (seat.w < stage.w) {
            problems.push(`[${label}] 座位区未通栏: seat.w=${seat.w} stage.w=${stage.w}`);
          }
        }
        // 中央舞台必须真的空出演出空间（两端一致）
        if (stage.w < 80 || stage.h < 60) {
          problems.push(`[${label}] 中央舞台过小，没有演出空间: ${stage.w}x${stage.h}`);
        }
      }
    }
    // 动画三：喝到毒药扣血
    if (scenario === "hurt") {
      if (probe.hurtLayer < 1) {
        problems.push(`[${label}] 扣血演出层缺失: ${JSON.stringify(probe.hurt)}`);
      }
      if (!probe.animations.includes("pf-hurt-flash")) {
        problems.push(`[${label}] 扣血闪屏动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.animations.some((n) => n === "pf-hurt-burst-player")) {
        problems.push(`[${label}] 玩家受击冲击环未生效: ${JSON.stringify(probe.animations)}`);
      }
      // 玩家受击：玩家烛火应当只剩 1 根亮着
      if (probe.playerCandleLit !== 1) {
        problems.push(`[${label}] 玩家受击后命烛数不对: ${probe.playerCandleLit}`);
      }
      // 清水演出不该同时出现
      if (probe.nullLayer > 0) {
        problems.push(`[${label}] 扣血场景里不该出现空杯演出`);
      }
    }
    // 动画四：喝到清水无反应
    if (scenario === "null") {
      if (probe.nullLayer < 1) {
        problems.push(`[${label}] 空杯演出层缺失`);
      }
      if (!probe.animations.includes("pf-null-ripple")) {
        problems.push(`[${label}] 空杯涟漪动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.text.includes("什么都没发生")) {
        problems.push(`[${label}] 空杯演出缺少文案`);
      }
      // 关键：清水绝不扣血，命烛必须全亮
      if (probe.hurtLayer > 0) {
        problems.push(`[${label}] 喝清水却出现了扣血演出`);
      }
      if (probe.playerCandleLit !== 3) {
        problems.push(`[${label}] 喝清水后玩家命烛应仍为 3 根: ${probe.playerCandleLit}`);
      }
    }
    // 非扣血/空杯场景不该残留这两个演出
    // late 是"正在演出扣血"的后期场景，不算残留
    if (scenario !== "hurt" && scenario !== "null" && scenario !== "late") {
      if (probe.hurtLayer > 0 || probe.nullLayer > 0) {
        problems.push(
          `[${label}] 非演出场景残留扣血(${probe.hurtLayer})/空杯(${probe.nullLayer})演出层`,
        );
      }
    }
    // 动画一：开局 / 换轮时向双方展示 4 支药水
if (scenario === "deal") {
      if (probe.dealTubeCount !== 4) {
        problems.push(`[${label}] 开局展示应有 4 支药水，实际 ${probe.dealTubeCount}`);
      }
      if (!probe.dealBox || probe.dealBox.h < 20) {
        problems.push(`[${label}] 开局展示幕不可见: ${JSON.stringify(probe.dealBox)}`);
      }
      if (!probe.animations.includes("pf-deal-in")) {
        problems.push(`[${label}] 开局展示动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.animations.includes("pf-deal-tube-rise")) {
        problems.push(`[${label}] 逐支展示动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.text.includes("小恶魔当众摆出")) {
        problems.push(`[${label}] 缺少"向双方摆出药水"的说明文案`);
      }
      // 核心机制是隐藏信息：展示幕绝不能声称玩家看清了顺序
      if (probe.text.includes("看清了顺序")) {
        problems.push(`[${label}] 展示幕泄露了药水顺序，违反隐藏信息机制`);
      }
      // 开局时不该有举杯动画
      if (probe.serveTube) {
        problems.push(`[${label}] 开局时不该出现举杯层`);
      }
    }
    // 动画二：选择自己喝 / 对方喝时举杯展示该支药水
    if (scenario === "serve") {
      if (!probe.serveTube || probe.serveTube.h < 20) {
        problems.push(`[${label}] 举杯层不可见: ${JSON.stringify(probe.serveTube)}`);
      }
      if (!probe.serveTube?.backgroundImage?.includes("data:")) {
        problems.push(`[${label}] 举杯层没有真实素材背景`);
      }
      // serve_seq=3 是奇数 → 必须落在 is-odd 分支上，否则动画不会重播
      if (!probe.animations.some((n) => n === "pf-serve-odd")) {
        problems.push(`[${label}] 奇数序号的举杯动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (!probe.animations.includes("pf-serve-glow")) {
        problems.push(`[${label}] 举杯光晕动画未生效: ${JSON.stringify(probe.animations)}`);
      }
      if (probe.serveCaption !== "第 1 管 · 毒药 → 你") {
        problems.push(`[${label}] 举杯文案不对: ${probe.serveCaption}`);
      }
      // logic.js 算好的位移/倾角/配色必须真的注入到 CSS 变量
      if (probe.serveVars?.dy !== "150px" || probe.serveVars?.lean !== "20deg") {
        problems.push(`[${label}] 举杯位移/倾角未注入: ${JSON.stringify(probe.serveVars)}`);
      }
      if (probe.serveVars?.accent !== "#ff8098") {
        problems.push(`[${label}] 举杯配色未注入: ${JSON.stringify(probe.serveVars)}`);
      }
      // 喝过第一口后开局展示幕必须退场
      if (probe.dealTubeCount !== 0) {
        problems.push(`[${label}] 喝过第一口后开局展示幕仍在: ${probe.dealTubeCount} 支`);
      }
    }
    // 非开局场景不该残留开局展示幕
    if (scenario !== "deal" && probe.dealTubeCount > 0) {
      problems.push(`[${label}] 非开局场景残留 ${probe.dealTubeCount} 支展示药水`);
    }
    if (scenario === "revealed") {
      if (!probe.asleep) problems.push(`[${label}] 迷魂香生效但小恶魔没有 is-asleep`);
      if (!probe.knownPoison) problems.push(`[${label}] 探明毒药缺少 known-poison 光效`);
      if (!probe.knownWater) problems.push(`[${label}] 探明清水缺少 known-water 光效`);
      if (probe.knownPoisonImg !== true) {
        problems.push(`[${label}] 探明毒药仍是空管图（只有光效没换素材）`);
      }
    }
    if (scenario === "idle" && !probe.text.includes("掀开桌布")) {
      problems.push(`[${label}] 缺少开局按钮`);
    }
    if (scenario === "over") {
      // 结束后牌桌必须整体让位给结算面板，不能还留着试管和命烛
      if (probe.tubeCount !== 0) {
        problems.push(`[${label}] 结束后仍渲染 ${probe.tubeCount} 支试管（进行中分支没有正确让位）`);
      }
      if (probe.candleLit !== 0) {
        problems.push(`[${label}] 结束后仍渲染点亮的命烛`);
      }
      if (!probe.text.includes("再开一局")) {
        problems.push(`[${label}] 缺少重开按钮`);
      }
    }
    if (scenario === "idle" && probe.tubeCount !== 0) {
      problems.push(`[${label}] 未开局却渲染了 ${probe.tubeCount} 支试管`);
    }
    if (false && scenario === "over" && !probe.text.includes("再开一局")) {
      problems.push(`[${label}] 缺少重开按钮`);
    }
    // 背景不应从布局下方漏出
    if (probe.docHeight > probe.viewportHeight + 8) {
      problems.push(
        `[${label}] 文档高度溢出视口: ${probe.docHeight} > ${probe.viewportHeight}（背景会漏出）`,
      );
    }
    // 座位区本身是可滚动的（overflow-y: auto），溢出是正常的滚动内容，
    // 只有在没有滚动能力时才算问题——那种情况下道具卡会永久点不到。
    const seatScrollable = await page.evaluate(() => {
      const seat = document.querySelector(".pf-zone-seat");
      if (!seat) return true;
      const s = getComputedStyle(seat);
      return /auto|scroll/.test(s.overflowY);
    });
    if (!seatScrollable && probe.seatOverflow > 4) {
      problems.push(`[${label}] 座位区溢出 ${probe.seatOverflow}px 且不可滚动（道具卡点不到）`);
    }
    // 试管架必须完整显示 4 支：少一支就等于骗了玩家
    // 试管架必须完整显示本轮全部试管：少一支就等于骗了玩家
    // （支数随轮次递增 2→7，不能写死 4）
    if (scenario !== "idle" && scenario !== "over") {
      if (probe.rackTubes !== probe.roundTubes) {
        problems.push(
          `[${label}] 试管架渲染 ${probe.rackTubes} 支，本轮应为 ${probe.roundTubes} 支`,
        );
      }
    }

    console.log(
      `${label}: tubes=${probe.tubeCount} candles=${probe.candleLit} table=${probe.table?.w}x${probe.table?.h} anims=[${probe.animations.join(",")}]`,
    );
  }
  await context.close();
}

await browser.close();

if (problems.length) {
  console.log("\n=== 问题 ===");
  for (const problem of problems) console.log("- " + problem);
  process.exitCode = 1;
} else {
  console.log("\n全部场景通过：无控制台错误、布局高度正常、核心元素可见、CSS 动画在运行。");
}