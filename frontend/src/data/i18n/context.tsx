/* ═══════════════════════════════════════════════════════════════
   i18n/context.tsx — Platform language context + hooks
   Switching language updates React state (re-renders consumers) AND
   persists + applies the <html lang> attribute via language.ts.
   ═══════════════════════════════════════════════════════════════ */
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  applyLanguage,
  persistLanguage,
  resolveInitialLanguage,
  type AppLanguage,
} from "../language";
import { dictionaries } from "./dict";

type TranslateFn = (key: string, fallback?: string) => string;

interface LanguageContextValue {
  language: AppLanguage;
  setLanguage: (language: AppLanguage) => void;
  t: TranslateFn;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<AppLanguage>(() => resolveInitialLanguage());

  const setLanguage = useCallback((next: AppLanguage) => {
    setLanguageState(next);
    applyLanguage(next);
    persistLanguage(next);
  }, []);

  const t = useCallback<TranslateFn>(
    (key, fallback) => {
      const table = dictionaries[language] ?? {};
      return table[key] ?? fallback ?? key;
    },
    [language],
  );

  const value = useMemo<LanguageContextValue>(
    () => ({ language, setLanguage, t }),
    [language, setLanguage, t],
  );

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) {
    throw new Error("useLanguage must be used within a LanguageProvider");
  }
  return ctx;
}

/** Convenience hook when only the translate function is needed. */
export function useT(): TranslateFn {
  return useLanguage().t;
}
