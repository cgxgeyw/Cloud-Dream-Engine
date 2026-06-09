/* ═══════════════════════════════════════════════════════════════
   theme.ts — Theme management
   Supports two orthogonal axes:
     1. Mode: "light" | "dark"           → data-theme attribute
     2. Style: "default" | "kawaii"      → data-color-scheme attribute
   ═══════════════════════════════════════════════════════════════ */

export type ThemeMode = "light" | "dark";
export type ThemeStyle = "default" | "kawaii";

/** @deprecated Use ThemeMode + ThemeStyle instead. Kept for backward compat. */
export type AppTheme = ThemeMode;

export const ALL_THEME_STYLES: { id: ThemeStyle; label: string; description: string }[] = [
  { id: "default", label: "默认", description: "干净明亮的现代风格" },
  { id: "kawaii", label: "粉色可爱", description: "软糯粉色 · 圆润泡泡 · 糖果梦境" },
];

const MODE_STORAGE_KEY = "app-theme";
const STYLE_STORAGE_KEY = "app-theme-style-v2";

// ─── Mode (light / dark) ───

function isMode(value: string | null): value is ThemeMode {
  return value === "light" || value === "dark";
}

export function resolveInitialMode(): ThemeMode {
  if (typeof window === "undefined") return "light";
  const saved = window.localStorage.getItem(MODE_STORAGE_KEY);
  if (isMode(saved)) return saved;
  return "light";
}

export function applyMode(mode: ThemeMode) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = mode;
  document.documentElement.style.colorScheme = mode;
}

export function persistMode(mode: ThemeMode) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(MODE_STORAGE_KEY, mode);
}

// ─── Style (default / kawaii) ───

function isStyle(value: string | null): value is ThemeStyle {
  return value === "default" || value === "kawaii";
}

export function resolveInitialStyle(): ThemeStyle {
  if (typeof window === "undefined") return "default";
  const saved = window.localStorage.getItem(STYLE_STORAGE_KEY);
  if (isStyle(saved)) return saved;
  return "default";
}

export function applyStyle(style: ThemeStyle) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.colorScheme = style;
}

export function persistStyle(style: ThemeStyle) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(STYLE_STORAGE_KEY, style);
}

// ─── Backward-compatible wrappers (AppTheme) ───

export function resolveInitialTheme(): AppTheme {
  return resolveInitialMode();
}

export function applyTheme(theme: AppTheme) {
  applyMode(theme);
}

export function persistTheme(theme: AppTheme) {
  persistMode(theme);
}
