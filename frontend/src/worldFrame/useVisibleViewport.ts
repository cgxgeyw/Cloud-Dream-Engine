import { useEffect, useState } from "react";

export type VisibleViewportRect = {
  /** 当前可见高度（键盘弹出后变小）。这是最终值，调用方不要再减键盘。 */
  height: number;
  offsetTop: number;
  /** 仅供 CSS 变量/调试 */
  keyboardHeight: number;
};

function readNativeKeyboardHeight(): number {
  const raw = (window as { __nativeKeyboardHeight?: unknown }).__nativeKeyboardHeight;
  const value = typeof raw === "number" ? raw : Number(raw);
  return Number.isFinite(value) ? Math.max(0, Math.round(value)) : 0;
}

let maxLayoutHeight = 0;

/**
 * 可见高度只算一次：
 * - 若 window.innerHeight / visualViewport 已经比无键盘时矮（adjustResize 或父层已缩），
 *   直接用当前值，绝不再减键盘；
 * - 仅当布局完全没缩、但原生报了键盘高时，才用 maxLayout − keyboard 兜底。
 */
function readRect(): VisibleViewportRect {
  const visual = window.visualViewport;
  const layoutHeight = Math.round(window.innerHeight || 0);
  if (layoutHeight > maxLayoutHeight) {
    maxLayoutHeight = layoutHeight;
  }
  const visualHeight = Math.round(visual?.height ?? layoutHeight);
  const offsetTop = Math.round(visual?.offsetTop ?? 0);
  const nativeKeyboard = readNativeKeyboardHeight();
  const visualKeyboard = Math.max(0, maxLayoutHeight - visualHeight - offsetTop);
  const keyboardHeight = Math.max(visualKeyboard, nativeKeyboard);

  const alreadyShrunk = layoutHeight < maxLayoutHeight - 8 || visualHeight < maxLayoutHeight - 8;
  let height: number;
  if (alreadyShrunk) {
    height = Math.max(1, Math.min(visualHeight > 0 ? visualHeight : layoutHeight, layoutHeight || maxLayoutHeight));
  } else if (keyboardHeight > 0 && maxLayoutHeight > 0) {
    height = Math.max(1, maxLayoutHeight - keyboardHeight);
  } else {
    height = Math.max(1, visualHeight > 0 ? visualHeight : layoutHeight);
  }

  return { height, offsetTop, keyboardHeight };
}

export function useVisibleViewport(
  enabled: boolean,
  parent?: { height: number; offset_top: number; keyboard_height: number },
): VisibleViewportRect {
  const [rect, setRect] = useState<VisibleViewportRect>(() => readRect());

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let frame = 0;
    let settleTimer = 0;
    const apply = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const local = readRect();
        const parentHeight = parent && parent.height > 0 ? parent.height : 0;
        const parentOffset = parent ? Math.max(0, parent.offset_top || 0) : 0;
        // 父子都可能已反映键盘，只取 min，绝不再减 keyboard_height
        const nextHeight = parentHeight > 0
          ? Math.max(1, Math.min(local.height, parentHeight))
          : local.height;
        const nextOffset = Math.max(local.offsetTop, parentOffset);
        const nextKeyboard = Math.max(local.keyboardHeight, parent ? parent.keyboard_height || 0 : 0);
        setRect((current) => (
          current.height === nextHeight
          && current.offsetTop === nextOffset
          && current.keyboardHeight === nextKeyboard
            ? current
            : { height: nextHeight, offsetTop: nextOffset, keyboardHeight: nextKeyboard }
        ));
      });
    };

    const settle = () => {
      apply();
      window.clearInterval(settleTimer);
      let ticks = 0;
      settleTimer = window.setInterval(() => {
        apply();
        ticks += 1;
        if (ticks >= 6) {
          window.clearInterval(settleTimer);
        }
      }, 50) as unknown as number;
    };

    const onFocusIn = (event: FocusEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT")) {
        settle();
      }
    };
    const onFocusOut = () => settle();

    settle();
    window.visualViewport?.addEventListener("resize", settle);
    window.visualViewport?.addEventListener("scroll", settle);
    window.addEventListener("resize", settle);
    window.addEventListener("orientationchange", settle);
    window.addEventListener("focusin", onFocusIn);
    window.addEventListener("focusout", onFocusOut);
    window.addEventListener("game-keyboard-maybe", settle);
    window.addEventListener("native-keyboard", settle);
    window.addEventListener("native-safe-area", settle);

    return () => {
      window.cancelAnimationFrame(frame);
      window.clearInterval(settleTimer);
      window.visualViewport?.removeEventListener("resize", settle);
      window.visualViewport?.removeEventListener("scroll", settle);
      window.removeEventListener("resize", settle);
      window.removeEventListener("orientationchange", settle);
      window.removeEventListener("focusin", onFocusIn);
      window.removeEventListener("focusout", onFocusOut);
      window.removeEventListener("game-keyboard-maybe", settle);
      window.removeEventListener("native-keyboard", settle);
      window.removeEventListener("native-safe-area", settle);
    };
  }, [enabled, parent?.height, parent?.offset_top, parent?.keyboard_height]);

  return rect;
}

export function notifyKeyboardMaybeOpen(): void {
  window.dispatchEvent(new Event("game-keyboard-maybe"));
}
