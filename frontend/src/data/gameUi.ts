// NOTE: This file has been split into gameUi/ sub-modules as part of P2 refactoring.
// It remains as a barrel re-export for backward compatibility.
// New code should import from the specific sub-modules directly.
// See: docs/architecture-review-20260524.md

export type {
  GameUiPlatform,
  GameUiMountId,
  UiAssetConfig,
  WorldUiEnvelope,
  WorldUiEntry,
  WorldUiEnvelopeV3,
  GameUiStyleRecord,
  GameUiComponentStyleDefinition,
  GameUiMountOptions,
  GameUiNodeBase,
  GameUiGridNode,
  GameUiStackNode,
  GameUiAbsoluteNode,
  GameUiAnchor,
  GameUiPrimitivePropValue,
  GameUiBindingValue,
  GameUiActionReference,
  GameUiPropValue,
  GameUiGridNodeV2,
  GameUiStackNodeV2,
  GameUiAbsoluteNodeV2,
  GameUiComponentNode,
  GameUiSlotNode,
  GameUiWhenNode,
  GameUiForEachNode,
  GameUiTextNode,
  GameUiImageNode,
  GameUiBadgeNode,
  GameUiButtonNode,
  GameUiCheckboxNode,
  GameUiLayoutNodeV2,
  GameUiDocumentBase,
  GameUiDocumentV2,
  GameUiLayoutNode,
  GameUiDocument,
  ParsedGameUiDocument,
} from "./gameUi/types";

export {
  GAME_UI_MOUNT_IDS,
  DEFAULT_GAME_UI_SCHEMA_VERSION,
  LATEST_SUPPORTED_GAME_UI_SCHEMA_VERSION,
  SUPPORTED_GAME_UI_SCHEMA_VERSIONS,
  SUPPORTED_GAME_UI_SCHEMA_VERSION,
  DEFAULT_UI_ASSET_CONFIG,
} from "./gameUi/types";

export { DEFAULT_DESKTOP_DOCUMENT, DEFAULT_MOBILE_DOCUMENT } from "./gameUi/defaults";

export {
  DEFAULT_DESKTOP_UI_FILE,
  DEFAULT_MOBILE_UI_FILE,
  stringifyGameUiDocument,
  defaultGameUiDocument,
  defaultGameUiFile,
  normalizeAssetConfig,
  normalizeWorldUiEnvelope,
  resolveUiFile,
  resolveUiStylesheet,
  migrateWorldUiEnvelopeToV3,
  parseGameUiDocument,
  buildGameUiStylesheet,
  normalizeGameUiScopeId,
  createGameUiScopeSelector,
  resolveSidePanelTabOrder,
  styleRecordToInlineStyle,
  normalizeAssetList,
  resolvePreviewBackgroundAsset,
} from "./gameUi/parser";
