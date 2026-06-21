import {
  applyMode,
  applyStyle,
  persistMode,
  persistStyle,
  resolveInitialMode,
  resolveInitialStyle,
  ALL_THEME_STYLES,
  type ThemeMode,
  type ThemeStyle,
} from "../data/theme";
import { useLanguage } from "../data/i18n/context";
import { useState } from "react";

/**
 * ThemePicker — shared component for selecting theme style, light/dark mode,
 * and platform UI language. Used in settings pages (desktop + mobile).
 */
export function ThemePicker() {
  const { language, setLanguage, t } = useLanguage();
  const [currentStyle, setCurrentStyle] = useState<ThemeStyle>(() => resolveInitialStyle());
  const [currentMode, setCurrentMode] = useState<ThemeMode>(() => resolveInitialMode());

  function handleStyleChange(style: ThemeStyle) {
    if (style === "kawaii" && currentMode === "dark") {
      setCurrentMode("light");
      applyMode("light");
      persistMode("light");
    }
    setCurrentStyle(style);
    applyStyle(style);
    persistStyle(style);
  }

  function handleModeChange(mode: ThemeMode) {
    if (mode === "dark" && currentStyle === "kawaii") {
      setCurrentStyle("default");
      applyStyle("default");
      persistStyle("default");
    }
    setCurrentMode(mode);
    applyMode(mode);
    persistMode(mode);
  }

  return (
    <div className="theme-picker">
      <div className="theme-picker-section">
        <h4 className="theme-picker-heading">{t("theme.appearance")}</h4>
        <div className="theme-picker-grid">
          {ALL_THEME_STYLES.map((style) => (
            <button
              key={style.id}
              type="button"
              className={`theme-picker-card${currentStyle === style.id ? " theme-picker-card--active" : ""}`}
              onClick={() => handleStyleChange(style.id)}
            >
              <div className={`theme-picker-preview theme-picker-preview--${style.id}`}>
                <div className="theme-picker-preview-header" />
                <div className="theme-picker-preview-body">
                  <div className="theme-picker-preview-line" />
                  <div className="theme-picker-preview-line theme-picker-preview-line--short" />
                  <div className="theme-picker-preview-line" />
                </div>
              </div>
              <div className="theme-picker-card-label">
                <strong>{style.label}</strong>
                <span>{style.description}</span>
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="theme-picker-section">
        <h4 className="theme-picker-heading">{t("theme.mode")}</h4>
        <div className="theme-picker-mode-row">
          <button
            type="button"
            className={`theme-picker-mode-btn${currentMode === "light" ? " theme-picker-mode-btn--active" : ""}`}
            onClick={() => handleModeChange("light")}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="5"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/></svg>
            {t("theme.light")}
          </button>
          <button
            type="button"
            className={`theme-picker-mode-btn${currentMode === "dark" ? " theme-picker-mode-btn--active" : ""}`}
            onClick={() => handleModeChange("dark")}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
            {t("theme.dark")}
          </button>
        </div>
      </div>

      <div className="theme-picker-section">
        <h4 className="theme-picker-heading">{t("theme.language")}</h4>
        <div className="theme-picker-mode-row">
          <button
            type="button"
            className={`theme-picker-mode-btn${language === "zh" ? " theme-picker-mode-btn--active" : ""}`}
            onClick={() => setLanguage("zh")}
          >
            中文
          </button>
          <button
            type="button"
            className={`theme-picker-mode-btn${language === "en" ? " theme-picker-mode-btn--active" : ""}`}
            onClick={() => setLanguage("en")}
          >
            English
          </button>
        </div>
      </div>
    </div>
  );
}
