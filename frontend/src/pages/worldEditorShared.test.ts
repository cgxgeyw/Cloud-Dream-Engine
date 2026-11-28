import { describe, expect, it } from "vitest";
import {
  applyDirectorImageGenerationEnabled,
  DEFAULT_DIRECTOR_MCP_TOOL_IDS,
  defaultDirectorConfig,
  defaultUiThemeConfig,
  DIRECTOR_IMAGE_GENERATION_MCP_TOOL_IDS,
  DIRECTOR_ONLY_MCP_TOOL_IDS,
  normalizeDirectorConfig,
  resolveDirectorImageGenerationEnabled,
} from "./worldEditorShared";

describe("normalizeDirectorConfig memory mode", () => {
  it.each(["history", "embedding", "oag"] as const)("preserves %s", (memoryMode) => {
    expect(normalizeDirectorConfig({ memory_mode: memoryMode }).memory_mode).toBe(memoryMode);
  });

  it("defaults to embedding and supports the legacy OAG flag", () => {
    expect(normalizeDirectorConfig({}).memory_mode).toBe("embedding");
    expect(normalizeDirectorConfig({ oag_enabled: true }).memory_mode).toBe("oag");
  });
});

describe("normalizeDirectorConfig memory retrieval", () => {
  it.each(["lexical_only", "semantic_only", "hybrid"])("drops the obsolete %s retrieval mode", (mode) => {
    const config = normalizeDirectorConfig({ character_memory_retrieval_mode: mode });
    expect(config).not.toHaveProperty("character_memory_retrieval_mode");
  });
});

describe("normalizeDirectorConfig model selection", () => {
  it("preserves the director model and drops world-level generation params", () => {
    const config = normalizeDirectorConfig({
      director_model: " model-1 ",
      generation_params: { temperature: 1.2, max_tokens: 4000 },
    });
    expect(config.director_model).toBe("model-1");
    expect(config).not.toHaveProperty("generation_params");
  });
});

describe("normalizeDirectorConfig MCP tool permissions", () => {
  it("cleans and deduplicates the whitelist", () => {
    const config = normalizeDirectorConfig({
      allowed_mcp_tool_ids: [" custom-tool ", "custom-tool", "", null, 42],
    });
    expect(config.allowed_mcp_tool_ids).toEqual(["custom-tool"]);
  });

  it("grants no tools when the whitelist is missing", () => {
    expect(normalizeDirectorConfig({}).allowed_mcp_tool_ids).toEqual([]);
  });

  it("ignores legacy permission flags and does not emit them", () => {
    const config = normalizeDirectorConfig({
      knowledge_access: true,
      allow_scene_transition: true,
      allow_player_character_switch: true,
    });
    expect(config.allowed_mcp_tool_ids).toEqual([]);
    expect(config).not.toHaveProperty("knowledge_access");
    expect(config).not.toHaveProperty("allow_scene_transition");
    expect(config).not.toHaveProperty("allow_player_character_switch");
  });

  it("grants the four director-only tools to new worlds by default", () => {
    expect(defaultDirectorConfig.allowed_mcp_tool_ids).toEqual(DEFAULT_DIRECTOR_MCP_TOOL_IDS);
    expect(DEFAULT_DIRECTOR_MCP_TOOL_IDS).toHaveLength(4);
    expect(DIRECTOR_ONLY_MCP_TOOL_IDS).toHaveLength(6);
    expect(DIRECTOR_ONLY_MCP_TOOL_IDS).not.toContain("mcp-tool-image-generation");
    expect(defaultDirectorConfig.allowed_mcp_tool_ids).not.toEqual(
      expect.arrayContaining([...DIRECTOR_IMAGE_GENERATION_MCP_TOOL_IDS]),
    );
  });
});

describe("director image generation switch", () => {
  it("enables runtime generation and both specialized tool grants together", () => {
    const next = applyDirectorImageGenerationEnabled(
      defaultDirectorConfig,
      defaultUiThemeConfig,
      true,
    );

    expect(next.uiThemeConfig.runtime_image_generation_enabled).toBe(true);
    expect(next.directorConfig.allowed_mcp_tool_ids).toEqual(
      expect.arrayContaining([...DEFAULT_DIRECTOR_MCP_TOOL_IDS, ...DIRECTOR_IMAGE_GENERATION_MCP_TOOL_IDS]),
    );
    expect(resolveDirectorImageGenerationEnabled(next.directorConfig, next.uiThemeConfig)).toBe(true);
  });

  it("disables runtime generation and removes only the image tool grants", () => {
    const enabled = applyDirectorImageGenerationEnabled(
      defaultDirectorConfig,
      defaultUiThemeConfig,
      true,
    );
    const withCustomTool = {
      ...enabled.directorConfig,
      allowed_mcp_tool_ids: [...enabled.directorConfig.allowed_mcp_tool_ids, "custom-tool"],
    };
    const next = applyDirectorImageGenerationEnabled(withCustomTool, enabled.uiThemeConfig, false);

    expect(next.uiThemeConfig.runtime_image_generation_enabled).toBe(false);
    expect(next.directorConfig.allowed_mcp_tool_ids).toEqual([
      ...DEFAULT_DIRECTOR_MCP_TOOL_IDS,
      "custom-tool",
    ]);
    expect(resolveDirectorImageGenerationEnabled(next.directorConfig, next.uiThemeConfig)).toBe(false);
  });

  it("requires the runtime flag and both tool grants", () => {
    const bothGranted = applyDirectorImageGenerationEnabled(
      defaultDirectorConfig,
      defaultUiThemeConfig,
      true,
    );
    expect(
      resolveDirectorImageGenerationEnabled(bothGranted.directorConfig, defaultUiThemeConfig),
    ).toBe(false);
    expect(
      resolveDirectorImageGenerationEnabled(
        {
          ...bothGranted.directorConfig,
          allowed_mcp_tool_ids: bothGranted.directorConfig.allowed_mcp_tool_ids.filter(
            (id) => id !== DIRECTOR_IMAGE_GENERATION_MCP_TOOL_IDS[0],
          ),
        },
        bothGranted.uiThemeConfig,
      ),
    ).toBe(false);
  });
});
