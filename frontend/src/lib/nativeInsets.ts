export type NativeSafeAreaInsets = {
  top: number;
  bottom: number;
  left: number;
  right: number;
};

const STORAGE_KEY = "od.nativeSafeAreaInsets";

function normalize(raw: unknown): NativeSafeAreaInsets | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const read = (key: string): number | null => {
    const value = Number(record[key]);
    return Number.isFinite(value) && value >= 0 ? Math.round(value) : null;
  };
  const top = read("top");
  const bottom = read("bottom");
  if (top == null && bottom == null) return null;
  return {
    top: top ?? 0,
    bottom: bottom ?? 0,
    left: read("left") ?? 0,
    right: read("right") ?? 0,
  };
}

function readCached(): NativeSafeAreaInsets | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    return raw ? normalize(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

/**
 * 读取安卓原生注入的安全区（CSS px）。
 * 页面重载后 `__nativeSafeAreaInsets` 会丢失，而原生距离下一次 inset 变化才会再注入，
 * 因此把最近一次真实值缓存到 localStorage，重载后仍能沿用，避免退回大幅猜测值。
 */
export function readNativeSafeAreaInsets(): NativeSafeAreaInsets | null {
  const live = normalize((window as { __nativeSafeAreaInsets?: unknown }).__nativeSafeAreaInsets);
  if (live) {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(live));
    } catch {
      // 隐私模式等场景下缓存失败不影响使用
    }
    return live;
  }
  return readCached();
}
