import { parse } from "jsonc-parser";
import type { GameUiDocument, GameUiPlatform } from "./types";
import defaultDesktopUiSource from "../../../../src-tauri/src/db/seeds/assets/default-desktop-ui.jsonc?raw";
import defaultMobileUiSource from "../../../../src-tauri/src/db/seeds/assets/default-mobile-ui.jsonc?raw";

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

function parseDefaultGameUiDocument(source: string, platform: GameUiPlatform): GameUiDocument {
  const parsed = parse(source) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error(`Default ${platform} game UI source is not an object.`);
  }
  return parsed as GameUiDocument;
}
