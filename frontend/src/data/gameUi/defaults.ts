import type { GameUiDocument, GameUiPlatform } from "./types";
import defaultDesktopUiSource from "../../../../src-tauri/src/db/seeds/assets/default-world-ui.html?raw";
import defaultMobileUiSource from "../../../../src-tauri/src/db/seeds/assets/default-world-ui.html?raw";

export const DEFAULT_DESKTOP_UI_SOURCE = ensureDefaultGameUiSource(
  defaultDesktopUiSource,
  "desktop",
);
export const DEFAULT_MOBILE_UI_SOURCE = ensureDefaultGameUiSource(
  defaultMobileUiSource,
  "mobile",
);

export const DEFAULT_DESKTOP_DOCUMENT: GameUiDocument = parseDefaultGameUiDocument(
  DEFAULT_DESKTOP_UI_SOURCE,
  "desktop",
);
export const DEFAULT_MOBILE_DOCUMENT: GameUiDocument = parseDefaultGameUiDocument(
  DEFAULT_MOBILE_UI_SOURCE,
  "mobile",
);

function ensureDefaultGameUiSource(source: string, platform: GameUiPlatform): string {
  const trimmed = source.trim();
  if (!trimmed) {
    throw new Error(`Default ${platform} game UI source is empty.`);
  }
  return `${trimmed}\n`;
}

function parseDefaultGameUiDocument(source: string, _platform: GameUiPlatform): GameUiDocument {
  return { html: source };
}
