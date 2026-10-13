/**
 * 用宿主**真实**的 GameUiRenderer 渲染世界包 UI 文档，产出 HTML 字符串。
 *
 * 为什么必须这么做：之前用自造的原型 HTML 验证过一轮，全部通过，但那是自己
 * 实现的渲染逻辑，和宿主组件的绑定语义（state / data / locals 根、for_each
 * 返回 Fragment、when 不加包装）完全不同，结果线上整块牌桌不渲染。
 * 这里直接复用 components/GameUiRenderer.tsx，杜绝"验证通过但线上空白"。
 */
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";

import { GameUiRenderer } from "../components/GameUiRenderer";
import type { GameUiActionReference } from "../data/gameUi";
import { evaluateGameUiExpression } from "../gameUiRuntime/expression";
import type { GameUiRenderContext } from "../components/GameUiRenderer";

export type RenderProbeResult = {
  html: string;
  text: string;
  /** 收集到的按钮（标签 / 文本内容），便于断言可点击目标。 */
  buttons: { label: string; text: string; className: string }[];
  /** 收集到的图片 src。 */
  images: string[];
  /** 执行过的动作。 */
  actions: GameUiActionReference[];
};

/** 对象深合并；数组按整体覆盖（测试要的就是"换成这局的试管"）。 */
function mergeDeep(
  base: Record<string, unknown>,
  patch: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(patch)) {
    const previous = result[key];
    if (
      previous &&
      typeof previous === "object" &&
      !Array.isArray(previous) &&
      value &&
      typeof value === "object" &&
      !Array.isArray(value)
    ) {
      result[key] = mergeDeep(previous as Record<string, unknown>, value as Record<string, unknown>);
    } else {
      result[key] = value;
    }
  }
  return result;
}

/** 剥掉标签，得到纯文本，用于断言"某段文字是否真的渲染出来了"。 */
export function htmlToText(html: string): string {
  return html
    .replace(/<style[\s\S]*?<\/style>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * 渲染世界包 UI 文档。
 *
 * state 的注入方式很关键：GameUiRenderer 用 `document.state` 作为 useState 初值，
 * 宿主则是靠 action 的 `result_state` 回写后再整体重挂载。因此这里必须把测试用的
 * state 合并进 document.state，否则渲染的永远是文档里的默认占位值。
 *
 * @param document 世界包里的 ui.*.jsonc 解析结果
 * @param state    宿主 ui state（对应 result_state 写入的树）
 * @param options.componentRenderers 内置组件渲染器（message_list / input_composer 等）
 */
export function renderWorldUiDocument(
  document: Record<string, unknown>,
  state: Record<string, unknown>,
  options: {
    componentRenderers?: Record<string, (context: never) => unknown>;
    data?: Record<string, unknown>;
  } = {},
): RenderProbeResult {
  const actions: GameUiActionReference[] = [];
  const evaluateCondition = (expression: string, context: GameUiRenderContext) =>
    evaluateGameUiExpression(expression, {
      ...context.data,
      ...context.locals,
      state: context.state,
    });

  const mergedDocument = {
    ...document,
    // 深合并：文档自带的 state.view 是占位空值，只做顶层覆盖会整块丢掉测试数据。
    state: mergeDeep((document.state as Record<string, unknown>) ?? {}, state),
  };

  const html = renderToStaticMarkup(
    createElement(GameUiRenderer as never, {
      document: mergedDocument,
      componentRenderers: options.componentRenderers,
      runtimeData: options.data ?? {},
      evaluateCondition,
      onAction: (action: GameUiActionReference) => {
        actions.push(action);
        return Promise.resolve(undefined);
      },
    } as never),
  );

  const buttons = [...html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)].map((match) => ({
    label: (match[1].match(/aria-label="([^"]*)"/)?.[1] ?? "").trim(),
    text: htmlToText(match[2]),
    className: (match[1].match(/class="([^"]*)"/)?.[1] ?? "").trim(),
  }));
  const images = [...html.matchAll(/<img\b[^>]*\bsrc="([^"]*)"/g)].map((match) => match[1]);

  return { html, text: htmlToText(html), buttons, images, actions };
}