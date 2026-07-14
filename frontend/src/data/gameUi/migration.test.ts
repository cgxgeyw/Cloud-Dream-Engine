import { describe, expect, it } from "vitest";

import poetryDesktop from "../../../../src-tauri/src/db/seeds/assets/poetry-desktop-ui.jsonc?raw";
import poetryMobile from "../../../../src-tauri/src/db/seeds/assets/poetry-mobile-ui.jsonc?raw";
import scheduleDesktop from "../../../../src-tauri/src/db/seeds/assets/schedule-assistant-desktop-ui.jsonc?raw";
import scheduleMobile from "../../../../src-tauri/src/db/seeds/assets/schedule-assistant-mobile-ui.jsonc?raw";
import { migrateWorldUiEnvelopeToV3, parseGameUiDocument } from "./parser";

const MIGRATION_FIXTURES = [
  { name: "poetry", desktop: poetryDesktop, mobile: poetryMobile },
  { name: "schedule-assistant", desktop: scheduleDesktop, mobile: scheduleMobile },
];

describe("world UI v3 migration fixtures", () => {
  it.each(MIGRATION_FIXTURES)("preserves both $name UI documents byte-for-byte", (fixture) => {
    const migrated = migrateWorldUiEnvelopeToV3({
      desktop_file: fixture.desktop,
      mobile_file: fixture.mobile,
      assets: {},
    });

    expect(migrated.runtime_version).toBe(3);
    expect(migrated.entries.desktop.document).toBe(fixture.desktop);
    expect(migrated.entries.mobile.document).toBe(fixture.mobile);
    expect(migrated.entries.desktop.document).not.toBe(migrated.entries.mobile.document);
    expect(parseGameUiDocument(migrated.entries.desktop.document, "desktop").error).toBeNull();
    expect(parseGameUiDocument(migrated.entries.mobile.document, "mobile").error).toBeNull();
  });

  it("keeps desktop and mobile v3 stylesheets independent", () => {
    const migrated = migrateWorldUiEnvelopeToV3({
      runtime_version: 3,
      entries: {
        desktop: { document: poetryDesktop, stylesheet: ".desktop-only { display: grid; }" },
        mobile: { document: poetryMobile, stylesheet: ".mobile-only { display: flex; }" },
      },
    });

    expect(migrated.entries.desktop.stylesheet).toContain(".desktop-only");
    expect(migrated.entries.mobile.stylesheet).toContain(".mobile-only");
  });
});
