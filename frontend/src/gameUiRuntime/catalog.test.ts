import { describe, expect, it } from "vitest";

import { GAME_UI_ACTION_IDS, GAME_UI_ACTION_SCHEMAS } from "./actionSchemas";
import { GAME_UI_CATALOG, getCatalogComponent } from "./catalog";
import { createGameUiPlatformCapabilities } from "./capabilities";
import { createGameUiComponentRegistry } from "./registry";

describe("shared game UI catalog", () => {
  it("has unique ids across components, actions and capabilities", () => {
    for (const entries of [
      GAME_UI_CATALOG.components.map((entry) => entry.id),
      GAME_UI_CATALOG.actions.map((entry) => entry.id),
      GAME_UI_CATALOG.capabilities.map((entry) => entry.id),
    ]) {
      expect(new Set(entries).size).toBe(entries.length);
    }
  });

  it("only references known actions and capabilities", () => {
    const actionIds = new Set(GAME_UI_CATALOG.actions.map((action) => action.id));
    const capabilityIds = new Set(GAME_UI_CATALOG.capabilities.map((capability) => capability.id));

    for (const component of GAME_UI_CATALOG.components) {
      for (const action of component.implicit_actions) {
        expect(actionIds.has(action), `${component.id} -> ${action}`).toBe(true);
      }
      for (const capability of component.implicit_capabilities) {
        expect(capabilityIds.has(capability), `${component.id} -> ${capability}`).toBe(true);
      }
    }
    for (const action of GAME_UI_CATALOG.actions) {
      for (const capability of action.implies_capabilities ?? []) {
        expect(capabilityIds.has(capability), `${action.id} -> ${capability}`).toBe(true);
      }
    }
  });

  it("drives the runtime component registry (props come from the catalog)", () => {
    // 渲染器闭包只在渲染时才会用到 runtime / actions，这里传空对象即可拿到注册表形状。
    const registry = createGameUiComponentRegistry(
      {} as Parameters<typeof createGameUiComponentRegistry>[0],
      {} as Parameters<typeof createGameUiComponentRegistry>[1],
    );

    for (const [id, definition] of Object.entries(registry)) {
      const component = getCatalogComponent(id);
      expect(component, `registry component ${id} missing from catalog`).toBeDefined();
      expect(definition.propsSchema).toEqual(component?.props);
    }
    // 运行时注册表覆盖除 ledger_book（仅世界框架内渲染）外的全部目录组件。
    expect(Object.keys(registry).sort()).toEqual(
      GAME_UI_CATALOG.components
        .map((component) => component.id)
        .filter((id) => id !== "ledger_book")
        .sort(),
    );
  });

  it("exposes every catalog action through the action schema list", () => {
    expect(GAME_UI_ACTION_IDS).toEqual(GAME_UI_CATALOG.actions.map((action) => action.id));
    expect(GAME_UI_ACTION_SCHEMAS.map((schema) => schema.id)).toEqual(
      GAME_UI_CATALOG.actions.map((action) => action.id),
    );
  });

  it("keeps platform capability keys aligned with the catalog", () => {
    for (const platform of ["desktop", "mobile"] as const) {
      const { platform: _platform, ...flags } = createGameUiPlatformCapabilities(platform);
      expect(Object.keys(flags).sort()).toEqual(
        GAME_UI_CATALOG.capabilities.map((capability) => capability.id).sort(),
      );
    }
  });
});
