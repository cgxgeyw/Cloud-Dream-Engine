// Extracted from SettingsPage.tsx — tabs, provider presets, form state
import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import { useSectionParam } from "../hooks/useSectionParam";
import { open } from "@tauri-apps/plugin-dialog";
import {
  assetUrl,
  createModel,
  deleteModel,
  discoverModels,
  fetchModels,
  fetchSettings,
  getExportDirectorySuggestion,
  isAndroidRuntime,
  isTauriEnvironment,
  setDefaultModel,
  testImageModel,
  testModel,
  updateModel,
  updateSettings,
  uploadFile,
  type ImageModelTestResult,
  type ModelConfigResponse,
  type SettingsResponse,
} from "../data/apiAdapter";
import { GenerationParamsEditor } from "../components/GenerationParamsEditor";
import { ImageModelTestPanel } from "../components/ImageModelTestPanel";
import { ConfirmDialog } from "../components/ModalDialog";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { useSettings } from "../data/SettingsContext";
import { ArrowLeft, ChevronRight } from "lucide-react";
import { ThemePicker } from "../components/ThemePicker";
import { WorldPermissionsPanel } from "../components/WorldPermissionsPanel";
import { useT } from "../data/i18n/context";


export const BUILTIN_EMBEDDING_MODEL_ID = "BAAI/bge-small-zh-v1.5";

export const tabIds = [
  { id: "text-model", labelKey: "settings.tabTextModel" },
  { id: "image-model", labelKey: "settings.tabImageModel" },
  { id: "embedding-model", labelKey: "settings.tabEmbeddingModel" },
  { id: "generation-params", labelKey: "settings.tabGenerationParams" },
  { id: "world-permissions", labelKey: "settings.tabWorldPermissions" },
  { id: "background", labelKey: "settings.tabBackground" },
  { id: "theme", labelKey: "settings.tabTheme" },
  { id: "export", labelKey: "settings.tabExport" },
] as const;

export const commonProviderOptions = [
  { value: "OpenAI", label: "OpenAI" },
  { value: "Claude / Anthropic", label: "Claude / Anthropic" },
  { value: "Gemini", label: "Gemini" },
  { value: "DeepSeek", label: "DeepSeek" },
  { value: "字节火山", label: "字节火山" },
  { value: "阿里百炼", label: "阿里百炼" },
  { value: "Kimi", label: "Kimi" },
  { value: "MiniMax", label: "MiniMax" },
  { value: "智谱", label: "智谱" },
  { value: "Ollama", label: "Ollama" },
  { value: "LM Studio", label: "LM Studio" },
] as const;

export const imageProviderOptions = [
  { value: "gpt-image2", label: "gpt-image2" },
  { value: "google nano banana", label: "google nano banana" },
] as const;

export const embeddingProviderOptions = [
  { value: "builtin-local", label: "内置本地" },
  ...commonProviderOptions,
] as const;

export const providerBaseUrlPresets: Record<string, string> = {
  OpenAI: "https://api.openai.com/v1",
  "Claude / Anthropic": "https://api.anthropic.com",
  Gemini: "https://generativelanguage.googleapis.com/v1beta/openai",
  DeepSeek: "https://api.deepseek.com/v1",
  "字节火山": "https://ark.cn-beijing.volces.com/api/v3",
  "阿里百炼": "https://dashscope.aliyuncs.com/compatible-mode/v1",
  Kimi: "https://api.moonshot.cn/v1",
  MiniMax: "https://api.minimaxi.com/v1",
  "智谱": "https://open.bigmodel.cn/api/paas/v4",
  Ollama: "http://127.0.0.1:11434/v1",
  "LM Studio": "http://127.0.0.1:1234/v1",
  "gpt-image2": "https://api.openai.com/v1",
  "google nano banana": "https://generativelanguage.googleapis.com/v1beta/openai",
  "builtin-local": "",
};



export type TabId = (typeof tabIds)[number]["id"];
export type ModelTabId = Extract<TabId, "text-model" | "image-model" | "embedding-model">;

export type ModelFormState = {
  name: string;
  provider: string;
  model_id: string;
  base_url: string;
  api_key: string;
  max_tokens: string;
  streaming_enabled: boolean;
  supports_image_input: boolean;
  supports_audio_input: boolean;
};

export const defaultModelForm: ModelFormState = {
  name: "",
  provider: "",
  model_id: "",
  base_url: "",
  api_key: "",
  max_tokens: "1200",
  streaming_enabled: true,
  supports_image_input: false,
  supports_audio_input: false,
};

export function isModelTab(tab: TabId): tab is ModelTabId {
  return tab === "text-model" || tab === "image-model" || tab === "embedding-model";
}

export function resolveUploadedAssetPath(result: { url?: string; asset_path?: string; relative_path?: string }) {
  return result.url?.trim() || result.asset_path?.trim() || result.relative_path?.trim() || "";
}

