/* ═══════════════════════════════════════════════════════════════
   language.ts — Platform UI language (中文 / English)
   Controls the platform chrome language only; world/in-game content
   is unaffected. Mirrors the persistence pattern in theme.ts.
   ═══════════════════════════════════════════════════════════════ */

export type AppLanguage = "zh" | "en";

const LANGUAGE_STORAGE_KEY = "app-language";

function isLanguage(value: string | null): value is AppLanguage {
  return value === "zh" || value === "en";
}

export function resolveInitialLanguage(): AppLanguage {
  if (typeof window === "undefined") return "zh";
  const saved = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
  if (isLanguage(saved)) return saved;
  return "zh";
}

export function applyLanguage(language: AppLanguage) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
  document.documentElement.dataset.lang = language;
}

export function persistLanguage(language: AppLanguage) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
}
