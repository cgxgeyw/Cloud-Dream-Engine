export type GameUiPlatform = "desktop" | "mobile";

export type GameUiPlatformCapabilities = {
  platform: GameUiPlatform;
  supports_mic: boolean;
  supports_file_picker: boolean;
  supports_hover: boolean;
  supports_world_records: boolean;
  supports_world_storage: boolean;
};

export function createGameUiPlatformCapabilities(
  platform: GameUiPlatform,
): GameUiPlatformCapabilities {
  return {
    platform,
    supports_mic: typeof navigator !== "undefined" && Boolean(navigator.mediaDevices?.getUserMedia),
    supports_file_picker: typeof document !== "undefined",
    supports_hover: platform === "desktop",
    supports_world_records: true,
    supports_world_storage: true,
  };
}
