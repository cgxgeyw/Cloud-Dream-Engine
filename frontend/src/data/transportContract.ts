import type * as HttpApi from "./api";
import type * as TauriApi from "./tauriApi";

/**
 * Methods that apiAdapter forwards to both transports with the same arguments
 * and return contract. Keep this list aligned with the direct delegates there.
 */
export type DirectTransportMethod =
  | "fetchWorlds"
  | "fetchWorld"
  | "createWorld"
  | "createWorldWithAi"
  | "updateWorld"
  | "deleteWorld"
  | "deleteAllWorlds"
  | "duplicateWorld"
  | "fetchWorldCharacters"
  | "fetchAllCharacters"
  | "fetchCharacter"
  | "deleteWorldCharacter"
  | "exportWorldCharacterTemplate"
  | "createCharacterInWorldFromCharacter"
  | "fetchWorldOpeningPromptPreview"
  | "validateWorldUiDocument"
  | "validateWorldUiBundle"
  | "compileWorldUiDocument"
  | "verifyWorldPackageUiCompatibility"
  | "downloadWorldPackage"
  | "importWorldPackage"
  | "uploadFile"
  | "fetchSession"
  | "createSession"
  | "submitPlayerAction"
  | "streamPlayerAction"
  | "retryFailedLlmStep"
  | "switchPlayerCharacter"
  | "fetchSaves"
  | "branchSave"
  | "deleteSave"
  | "deleteAllSaves"
  | "fetchModels"
  | "fetchModel"
  | "createModel"
  | "updateModel"
  | "deleteModel"
  | "setDefaultModel"
  | "testModel"
  | "testImageModel"
  | "fetchBuiltinEmbeddingModelStatus"
  | "downloadBuiltinEmbeddingModel"
  | "fetchSettings"
  | "updateSettings"
  | "fetchPlugins"
  | "fetchMcpTools"
  | "createMcpTool"
  | "updateMcpTool"
  | "deleteMcpTool"
  | "fetchAttributeSchemas"
  | "createAttributeSchema"
  | "updateAttributeSchema"
  | "deleteAttributeSchema"
  | "fetchAttributeValues"
  | "upsertAttributeValue"
  | "fetchMemories"
  | "fetchSessionDebug"
  | "fetchSessionRuntimeAttributes";

/**
 * Deliberately excluded adapter methods whose transport signatures or behavior
 * differ: character writes reshape the HTTP payload; discoverModels reshapes
 * its arguments; progress, permissions, local-path import, and export-directory
 * helpers have platform fallbacks; snapshot subscription, WebSocket URLs, and
 * asset URLs are implemented differently by each runtime.
 */
export type AdaptedTransportMethod =
  | "createWorldCharacter"
  | "updateWorldCharacter"
  | "discoverModels"
  | "onAiWorldCreateProgress"
  | "requestWorldPermissions"
  | "importWorldPackageFromPath"
  | "getExportDirectorySuggestion"
  | "onSessionSnapshot"
  | "toSessionWebSocketUrl"
  | "assetUrl";

type HttpTransport = Pick<typeof HttpApi, DirectTransportMethod>;
type TauriTransport = Pick<typeof TauriApi, DirectTransportMethod>;

type IsExact<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2)
    ? (<T>() => T extends B ? 1 : 2) extends
        (<T>() => T extends A ? 1 : 2)
      ? true
      : false
    : false;

export type TransportSignatureMismatch = {
  [Method in DirectTransportMethod]: IsExact<
    HttpTransport[Method],
    TauriTransport[Method]
  > extends true
    ? never
    : Method;
}[DirectTransportMethod];

type AssertNoMismatch<Mismatch extends never> = Mismatch;

/** Shared compile-time contract; this file emits no runtime implementation. */
export type TransportContract = HttpTransport;

// A signature mismatch resolves to its method name and fails this constraint.
export type TransportContractAssertion =
  AssertNoMismatch<TransportSignatureMismatch>;
