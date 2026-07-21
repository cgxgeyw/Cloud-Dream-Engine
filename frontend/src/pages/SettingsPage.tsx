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
import { ImageModelTestPanel } from "../components/ImageModelTestPanel";
import { ConfirmDialog } from "../components/ModalDialog";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { showToast } from "../components/Toast";
import { useSettings } from "../data/SettingsContext";
import { ArrowLeft, ChevronRight } from "lucide-react";
import { ThemePicker } from "../components/ThemePicker";
import { useT } from "../data/i18n/context";

const BUILTIN_EMBEDDING_MODEL_ID = "BAAI/bge-small-zh-v1.5";

const tabIds = [
  { id: "text-model", labelKey: "settings.tabTextModel" },
  { id: "image-model", labelKey: "settings.tabImageModel" },
  { id: "embedding-model", labelKey: "settings.tabEmbeddingModel" },
  { id: "background", labelKey: "settings.tabBackground" },
  { id: "theme", labelKey: "settings.tabTheme" },
  { id: "export", labelKey: "settings.tabExport" },
] as const;

const commonProviderOptions = [
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

const imageProviderOptions = [
  { value: "gpt-image2", label: "gpt-image2" },
  { value: "google nano banana", label: "google nano banana" },
] as const;

const embeddingProviderOptions = [
  { value: "builtin-local", label: "内置本地" },
  ...commonProviderOptions,
] as const;

const providerBaseUrlPresets: Record<string, string> = {
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



type TabId = (typeof tabIds)[number]["id"];
type ModelTabId = Extract<TabId, "text-model" | "image-model" | "embedding-model">;

type ModelFormState = {
  name: string;
  provider: string;
  model_id: string;
  base_url: string;
  api_key: string;
  max_tokens: string;
  streaming_enabled: boolean;
};

const defaultModelForm: ModelFormState = {
  name: "",
  provider: "",
  model_id: "",
  base_url: "",
  api_key: "",
  max_tokens: "1200",
  streaming_enabled: true,
};

function isModelTab(tab: TabId): tab is ModelTabId {
  return tab === "text-model" || tab === "image-model" || tab === "embedding-model";
}

function resolveUploadedAssetPath(result: { url?: string; asset_path?: string; relative_path?: string }) {
  return result.url?.trim() || result.asset_path?.trim() || result.relative_path?.trim() || "";
}

export function SettingsPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const t = useT();
  const { refresh: refreshGlobalSettings } = useSettings();

  // Desktop: activeTab, Mobile: activeSection + section-list navigation
  const [activeTab, setActiveTab] = useState<TabId>("text-model");
  // 移动端「打开某项设置」由 URL 的 ?section= 驱动（见 useSectionParam）：
  // 打开即 push 一条历史，关闭则在有应用内历史时走历史回退、否则清参数回到列表。
  const { activeSection, openSection: openSectionParam, closeSection } = useSectionParam<TabId>(
    tabIds.map((tab) => tab.id),
  );

  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  const [textModels, setTextModels] = useState<ModelConfigResponse[]>([]);
  const [imageModels, setImageModels] = useState<ModelConfigResponse[]>([]);
  const [embeddingModels, setEmbeddingModels] = useState<ModelConfigResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [discoveringModels, setDiscoveringModels] = useState(false);
  const [testingModelId, setTestingModelId] = useState<string | null>(null);
  const [editingModel, setEditingModel] = useState<ModelConfigResponse | null>(null);
  const [isNewModel, setIsNewModel] = useState(false);
  const [modelForm, setModelForm] = useState<ModelFormState>(defaultModelForm);
  const [modelDiscovery, setModelDiscovery] = useState<{ ok: boolean; detail: string; modelIds: string[] } | null>(null);
  const [pendingDeleteModel, setPendingDeleteModel] = useState<ModelConfigResponse | null>(null);
  const [uploading, setUploading] = useState(false);
  const [customProviderMode, setCustomProviderMode] = useState(false);
  const [openImageTestModelId, setOpenImageTestModelId] = useState<string | null>(null);
  const [imageTestModelId, setImageTestModelId] = useState<string | null>(null);
  const [imageTestPrompts, setImageTestPrompts] = useState<Record<string, string>>({});
  const [imageTestResults, setImageTestResults] = useState<Record<string, ImageModelTestResult | null>>({});

  const currentTab = isMobile && activeSection ? activeSection : activeTab;
  const activeModelTab = isModelTab(currentTab) ? currentTab : null;
  const currentModels =
    activeModelTab === "text-model"
      ? textModels
      : activeModelTab === "image-model"
        ? imageModels
        : embeddingModels;

  const providerOptions = useMemo(() => {
    if (activeModelTab === "text-model") return commonProviderOptions;
    if (activeModelTab === "image-model") return imageProviderOptions;
    return embeddingProviderOptions;
  }, [activeModelTab]);

  const providerSelectValue = customProviderMode
    ? "custom"
    : modelForm.provider.trim();

  const showsCustomProviderInput = customProviderMode;
  const isBuiltinLocalProvider = modelForm.provider === "builtin-local";
  const embeddingDefaultMissing = !!settings?.embedding_enabled && !settings.default_embedding_model.trim();
  const supportsDirectoryPicker = isTauriEnvironment() && !isAndroidRuntime();

  useEffect(() => {
    let cancelled = false;

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const [settingsData, allModels] = await Promise.all([fetchSettings(), fetchModels()]);
        if (cancelled) return;
        setSettings(settingsData);
        setTextModels(allModels.filter((model) => model.model_type === "text"));
        setImageModels(allModels.filter((model) => model.model_type === "image"));
        setEmbeddingModels(allModels.filter((model) => model.model_type === "embedding"));
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : t("settings.loadSettingsFailed"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadData();
    return () => {
      cancelled = true;
    };
  }, []);

  function updateDraft(patch: Partial<SettingsResponse>) {
    setSettings((current) => (current ? { ...current, ...patch } : current));
  }

  // Mobile-specific navigation
  function openSection(sectionId: TabId) {
    openSectionParam(sectionId);
    if (isModelTab(sectionId)) {
      setActiveTab(sectionId);
    }
    setError(null);
    closeModelEditor();
  }

  function closeMobileSection() {
    closeModelEditor();
    setError(null);
    closeSection();
  }


  function openNewModel() {
    if (!activeModelTab) return;

    const nextForm =
      activeModelTab === "embedding-model"
        ? {
            ...defaultModelForm,
            provider: "builtin-local",
            model_id: BUILTIN_EMBEDDING_MODEL_ID,
            name: t("settings.builtinEmbeddingName"),
            streaming_enabled: false,
          }
        : {
            ...defaultModelForm,
            streaming_enabled: activeModelTab === "text-model",
          };

    setIsNewModel(true);
    setEditingModel(null);
    setModelForm(nextForm);
    setCustomProviderMode(false);
    setModelDiscovery(null);
    setError(null);
  }

  function openEditModel(model: ModelConfigResponse) {
    setIsNewModel(false);
    setEditingModel(model);
    setModelForm({
      name: model.name,
      provider: model.provider,
      model_id: model.model_id,
      base_url: model.base_url,
      api_key: model.api_key,
      max_tokens: String(model.max_tokens),
      streaming_enabled: model.streaming_enabled,
    });
    setCustomProviderMode(!providerOptions.some((option) => option.value === model.provider));
    setModelDiscovery(null);
    setError(null);
  }

  function closeModelEditor() {
    setEditingModel(null);
    setIsNewModel(false);
    setModelForm(defaultModelForm);
    setCustomProviderMode(false);
    setModelDiscovery(null);
  }

  function patchModelForm(patch: Partial<ModelFormState>, options?: { resetDiscovery?: boolean }) {
    setModelForm((current) => ({ ...current, ...patch }));
    if (options?.resetDiscovery) {
      setModelDiscovery(null);
    }
  }

  function patchProvider(nextProvider: string) {
    setModelForm((current) => {
      const nextPreset = providerBaseUrlPresets[nextProvider] ?? "";
      const nextModelId =
        activeModelTab === "image-model" && nextProvider !== "builtin-local" && isMobile
          ? nextProvider
          : nextProvider === "builtin-local"
            ? BUILTIN_EMBEDDING_MODEL_ID
            : current.model_id === BUILTIN_EMBEDDING_MODEL_ID && current.provider === "builtin-local"
              ? ""
              : current.model_id;
      return {
        ...current,
        provider: nextProvider,
        base_url: nextPreset,
        api_key: nextProvider === "builtin-local" ? "" : current.api_key,
        model_id: nextModelId,
      };
    });
    setCustomProviderMode(false);
    setModelDiscovery(null);
  }

  async function handleSave() {
    if (!settings) return;

    try {
      setSaving(true);
      setError(null);
      const saved = await updateSettings(settings);
      setSettings(saved);
      await refreshGlobalSettings();
      showToast(t("settings.settingsSaved"));
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : t("settings.saveSettingsFailed"));
    } finally {
      setSaving(false);
    }
  }

  async function handleEmbeddingEnabledChange(nextEnabled: boolean) {
    if (!settings) return;

    if (nextEnabled && !settings.default_embedding_model.trim()) {
      setError(t("settings.needDefaultEmbedding"));
      showToast(t("settings.needDefaultEmbedding"), "error");
      return;
    }

    const nextSettings = { ...settings, embedding_enabled: nextEnabled };
    setSettings(nextSettings);

    try {
      setSaving(true);
      setError(null);
      const saved = await updateSettings(nextSettings);
      setSettings(saved);
      await refreshGlobalSettings();
    } catch (saveError) {
      setSettings(settings);
      const message = saveError instanceof Error ? saveError.message : t("settings.saveEmbeddingFailed");
      setError(message);
      showToast(message, "error");
    } finally {
      setSaving(false);
    }
  }

  async function handleSaveModel() {
    if (!activeModelTab) return;

    const modelType =
      activeModelTab === "text-model"
        ? "text"
        : activeModelTab === "image-model"
          ? "image"
          : "embedding";
    const parsedMaxTokens = Number.parseInt(modelForm.max_tokens.trim(), 10);
    const maxTokens = Number.isFinite(parsedMaxTokens) && parsedMaxTokens > 0 ? parsedMaxTokens : 1200;

    try {
      setSaving(true);
      setError(null);
      const payload = {
        name: modelForm.name.trim(),
        model_type: modelType,
        provider: modelForm.provider.trim(),
        model_id: modelForm.model_id.trim(),
        base_url: isBuiltinLocalProvider ? "" : modelForm.base_url.trim(),
        api_key: isBuiltinLocalProvider ? "" : modelForm.api_key,
        max_tokens: maxTokens,
        streaming_enabled: modelType === "text" ? modelForm.streaming_enabled : false,
        is_default: editingModel?.is_default ?? false,
      };

      if (isNewModel) {
        const created = await createModel(payload);
        if (modelType === "text") {
          setTextModels((prev) => [...prev, created]);
        } else if (modelType === "image") {
          setImageModels((prev) => [...prev, created]);
        } else {
          setEmbeddingModels((prev) => [...prev, created]);
        }
      } else if (editingModel) {
        const updated = await updateModel(editingModel.id, payload);
        if (modelType === "text") {
          setTextModels((prev) => prev.map((item) => (item.id === updated.id ? updated : item)));
        } else if (modelType === "image") {
          setImageModels((prev) => prev.map((item) => (item.id === updated.id ? updated : item)));
        } else {
          setEmbeddingModels((prev) => prev.map((item) => (item.id === updated.id ? updated : item)));
        }
      }

      closeModelEditor();
      showToast(t("settings.modelSaved"));
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : t("settings.saveModelFailed"));
    } finally {
      setSaving(false);
    }
  }

  async function handleDeleteModel(model: ModelConfigResponse) {
    try {
      setError(null);
      await deleteModel(model.id);
      setTextModels((prev) => prev.filter((item) => item.id !== model.id));
      setImageModels((prev) => prev.filter((item) => item.id !== model.id));
      setEmbeddingModels((prev) => prev.filter((item) => item.id !== model.id));
      setPendingDeleteModel(null);
      showToast(t("settings.modelDeleted"));
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : t("settings.deleteModelFailed"));
    }
  }

  async function handleSetDefault(model: ModelConfigResponse) {
    try {
      setError(null);
      await setDefaultModel(model.id);

      const markDefault = (items: ModelConfigResponse[]) =>
        items.map((item) => ({ ...item, is_default: item.id === model.id }));

      if (model.model_type === "text") {
        setTextModels(markDefault);
        setSettings((current) =>
          current
            ? { ...current, text_model_provider: model.provider, default_text_model: model.model_id }
            : current,
        );
        await refreshGlobalSettings();
      } else if (model.model_type === "embedding") {
        setEmbeddingModels(markDefault);
        setSettings((current) =>
          current ? { ...current, default_embedding_model: model.model_id } : current,
        );
      } else {
        setImageModels(markDefault);
        setSettings((current) =>
          current
            ? { ...current, image_model_provider: model.provider, default_image_workflow: model.model_id }
            : current,
        );
        await refreshGlobalSettings();
      }

      showToast(t("settings.defaultModelUpdated"));
    } catch (setError_) {
      setError(setError_ instanceof Error ? setError_.message : t("settings.setDefaultModelFailed"));
    }
  }

  async function handleTestModel(model: ModelConfigResponse) {
    try {
      setTestingModelId(model.id);
      showToast(t("settings.modelTesting"));
      const result = await testModel(model.id);
      showToast(result.detail, result.ok ? "success" : "error");
    } catch (testError_) {
      showToast(testError_ instanceof Error ? testError_.message : t("settings.modelTestFailed"), "error");
    } finally {
      setTestingModelId((current) => (current === model.id ? null : current));
    }
  }

  function patchImageTestPrompt(modelId: string, value: string) {
    setImageTestPrompts((current) => ({ ...current, [modelId]: value }));
  }

  async function handleTestImageModel(model: ModelConfigResponse) {
    const prompt = imageTestPrompts[model.id]?.trim() ?? "";
    if (!prompt) {
      showToast(isMobile ? "Please enter a prompt first." : t("settings.enterPromptFirst"), "error");
      return;
    }

    try {
      setImageTestModelId(model.id);
      const result = await testImageModel(model.id, { prompt });
      setImageTestResults((current) => ({ ...current, [model.id]: result }));
      showToast(result.detail, result.ok ? "success" : "error");
    } catch (testError_) {
      const message = testError_ instanceof Error ? testError_.message : "Image model test failed.";
      setImageTestResults((current) => ({
        ...current,
        [model.id]: {
          ok: false,
          detail: message,
          debug_lines: [message],
        },
      }));
      showToast(message, "error");
    } finally {
      setImageTestModelId((current) => (current === model.id ? null : current));
    }
  }

  async function handleDiscoverModels() {
    try {
      setDiscoveringModels(true);
      const result = await discoverModels({
        provider: modelForm.provider,
        base_url: modelForm.base_url,
        api_key: modelForm.api_key,
      });
      setModelDiscovery({ ok: result.ok, detail: result.detail, modelIds: result.model_ids });
      showToast(result.detail, result.ok ? "success" : "error");
    } catch (discoverError) {
      const message = discoverError instanceof Error ? discoverError.message : t("settings.discoverFailed");
      setModelDiscovery({ ok: false, detail: message, modelIds: [] });
      showToast(message, "error");
    } finally {
      setDiscoveringModels(false);
    }
  }

  async function handleFileUpload(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file || !settings) return;

    try {
      setUploading(true);
      setError(null);
      const result = await uploadFile(file);
      const assetPath = resolveUploadedAssetPath(result);
      if (!assetPath) {
        throw new Error(t("settings.uploadNoUrl"));
      }
      const nextSettings = { ...settings, home_background_strategy: assetPath };
      const saved = await updateSettings(nextSettings);
      setSettings(saved);
      await refreshGlobalSettings();
      showToast(t("settings.bgUpdated"));
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : t("settings.bgUploadFailed"));
    } finally {
      setUploading(false);
    }
  }

  async function handleClearBackground() {
    if (!settings) return;

    try {
      setSaving(true);
      setError(null);
      const saved = await updateSettings({ ...settings, home_background_strategy: "" });
      setSettings(saved);
      await refreshGlobalSettings();
      showToast(t("settings.bgCleared"));
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : t("settings.bgClearFailed"));
    } finally {
      setSaving(false);
    }
  }

  async function handlePickExportDirectory() {
    if (!settings) return;

    if (isAndroidRuntime()) {
      try {
        setError(null);
        const selected = await getExportDirectorySuggestion();
        if (typeof selected === "string" && selected.trim()) {
          updateDraft({ export_directory: selected.trim() });
          showToast(t("settings.exportDirPicked"));
        }
      } catch (pickError) {
        setError(pickError instanceof Error ? pickError.message : t("settings.pickExportDirFailed"));
      }
      return;
    }

    if (!supportsDirectoryPicker) {
      showToast(t("settings.noDirPicker"), "error");
      return;
    }

    try {
      setError(null);
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: settings.export_directory || undefined,
      });
      if (typeof selected === "string" && selected.trim()) {
        updateDraft({ export_directory: selected.trim() });
      }
    } catch (pickError) {
      setError(pickError instanceof Error ? pickError.message : t("settings.pickExportDirFailed"));
    }
  }

  // ==================== Mobile-specific rendering ====================
  function renderSectionList() {
    return (
      <div className="settings-page-shell">
        <div className="settings-mobile-overview-head">
          <div className="settings-mobile-kicker">{t("settings.title")}</div>
          <h1 className="settings-mobile-title">{t("settings.mobileTitle")}</h1>
          <p className="settings-mobile-summary">{t("settings.mobileSummary")}</p>
        </div>
        <SurfacePanel className="surface-panel--pad-lg">
          <div className="settings-nav-list">
            {tabIds.map((item) => (
              <button
                key={item.id}
                type="button"
                className="settings-nav-item"
                onClick={() => openSection(item.id)}
              >
                <div className="settings-nav-item-copy">
                  <strong>{t(item.labelKey)}</strong>
                </div>
                <span className="settings-nav-item-arrow"><ChevronRight size={14} /></span>
              </button>
            ))}
          </div>
        </SurfacePanel>
      </div>
    );
  }

  function renderModelEditor() {
    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          <h3 className="settings-section-title">{isNewModel ? t("settings.newModel") : t("settings.editModel").replace("{name}", editingModel?.name ?? "")}</h3>

          <div className="settings-form-grid settings-form-grid--model-editor">
            <label className="field-label">
              <span className="field-label-text">{t("settings.modelName")}</span>
              <input
                value={modelForm.name}
                onChange={(event) => patchModelForm({ name: event.target.value })}
                className="field-input"
                placeholder={t("settings.phMainModel")}
              />
            </label>

            <label className="field-label">
              <span className="field-label-text">{t("settings.provider")}</span>
              <select
                value={providerSelectValue}
                onChange={(event) => {
                  const nextValue = event.target.value;
                  if (nextValue === "custom") {
                    setCustomProviderMode(true);
                    patchModelForm(
                      {
                        provider: providerOptions.some((option) => option.value === modelForm.provider)
                          ? ""
                          : modelForm.provider,
                      },
                      { resetDiscovery: true },
                    );
                    return;
                  }
                  patchProvider(nextValue);
                }}
                className="field-input editor-field-select"
              >
                <option value="">{t("settings.selectPlaceholder")}</option>
                {providerOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.value === "builtin-local" ? t("settings.builtinLocal") : option.label}
                  </option>
                ))}
                <option value="custom">{t("settings.custom")}</option>
              </select>
              {showsCustomProviderInput ? (
                <input
                  value={modelForm.provider}
                  onChange={(event) => {
                    setCustomProviderMode(true);
                    patchModelForm({ provider: event.target.value }, { resetDiscovery: true });
                  }}
                  className="field-input"
                  style={{ marginTop: 8 }}
                  placeholder={t("settings.phProviderName")}
                />
              ) : null}
            </label>

            <label className="field-label">
              <span className="field-label-text">{t("settings.modelId")}</span>
              <input
                value={modelForm.model_id}
                onChange={(event) => patchModelForm({ model_id: event.target.value })}
                className="field-input"
                placeholder={t("settings.phExampleGpt")}
                readOnly={isBuiltinLocalProvider}
              />
            </label>

            <label className="field-label">
              <span className="field-label-text">Base URL</span>
              <input
                value={modelForm.base_url}
                onChange={(event) => patchModelForm({ base_url: event.target.value }, { resetDiscovery: true })}
                className="field-input"
                placeholder={isBuiltinLocalProvider ? t("settings.builtinNoFill") : t("settings.phBaseUrlExample")}
                readOnly={isBuiltinLocalProvider}
              />
            </label>

            <label className="field-label">
              <span className="field-label-text">API Key</span>
              <input
                value={modelForm.api_key}
                onChange={(event) => patchModelForm({ api_key: event.target.value }, { resetDiscovery: true })}
                className="field-input"
                type="text"
                inputMode="text"
                autoComplete="off"
                autoCapitalize="none"
                spellCheck={false}
                placeholder={isBuiltinLocalProvider ? t("settings.builtinNoFill") : t("settings.leaveEmptyNoSet")}
                readOnly={isBuiltinLocalProvider}
              />
            </label>

            {activeModelTab === "text-model" ? (
              <label className="field-label field-label--inline">
                <span className="field-label-text">{t("settings.maxOutputTokens")}</span>
                <input
                  value={modelForm.max_tokens}
                  onChange={(event) => patchModelForm({ max_tokens: event.target.value })}
                  className="field-input"
                  inputMode="numeric"
                  placeholder="1200"
                />
              </label>
            ) : null}

            {activeModelTab === "text-model" ? (
              <label className="field-label field-label--inline">
                <span className="field-label-text">{t("settings.streamingOutput")}</span>
                <div className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={modelForm.streaming_enabled}
                    onChange={(event) => patchModelForm({ streaming_enabled: event.target.checked })}
                  />
                </div>
              </label>
            ) : null}
          </div>

          {activeModelTab !== "image-model" && !isBuiltinLocalProvider ? (
            <div className="settings-model-discovery">
              <div className="settings-model-discovery-header">
                <div className="settings-model-discovery-copy">
                  <div className="field-label-text">{t("settings.modelListLabel")}</div>
                  <div className="text-muted">{t("settings.modelListHint")}</div>
                </div>
              </div>

              <button
                type="button"
                onClick={() => void handleDiscoverModels()}
                disabled={discoveringModels || !modelForm.base_url.trim()}
                className="action-btn action-btn--accent settings-model-discovery-btn"
              >
                {discoveringModels ? t("settings.discovering") : t("settings.fetchModelList")}
              </button>

              {modelDiscovery ? (
                <div className={modelDiscovery.ok ? "text-muted" : "text-error"}>{modelDiscovery.detail}</div>
              ) : null}

              {modelDiscovery?.modelIds.length ? (
                <div className="settings-model-suggestions">
                  {modelDiscovery.modelIds.map((modelId) => (
                    <button
                      key={modelId}
                      type="button"
                      onClick={() => patchModelForm({ model_id: modelId })}
                      className={`action-btn${modelForm.model_id === modelId ? " action-btn--accent" : ""}`}
                    >
                      {modelId}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}

          <div className="settings-form-actions">
            <button
              type="button"
              onClick={() => void handleSaveModel()}
              disabled={saving || !modelForm.name.trim() || !modelForm.provider.trim() || !modelForm.model_id.trim()}
              className="action-btn action-btn--accent"
            >
              {saving ? t("settings.saving") : t("settings.save")}
            </button>
            <button type="button" onClick={closeModelEditor} className="action-btn">
              {t("common.cancel")}
            </button>
          </div>
          {!supportsDirectoryPicker ? <div className="text-muted">{t("settings.noDirPickerShort")}</div> : null}
        </div>
      </SurfacePanel>
    );
  }

  function renderModelList() {
    if (!settings || !activeModelTab) return null;

    const sectionTitle =
      activeModelTab === "text-model"
        ? t("settings.tabTextModel")
        : activeModelTab === "image-model"
          ? t("settings.tabImageModel")
          : t("settings.tabEmbeddingModel");

    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          {activeModelTab === "embedding-model" ? (
            <div className="settings-section">
              <h3 className="settings-section-title">{t("settings.embeddingModelTitle")}</h3>
              <label className="field-label field-label--inline">
                <span className="field-label-text">{t("settings.enableEmbedding")}</span>
                <div className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={settings.embedding_enabled}
                    onChange={(event) => void handleEmbeddingEnabledChange(event.target.checked)}
                    disabled={saving}
                  />
                </div>
              </label>
              {embeddingDefaultMissing ? <div className="text-error">{t("settings.needDefaultEmbedding")}</div> : null}
            </div>
          ) : null}

          <div className="settings-list-header">
            <h3 className="settings-section-title">{sectionTitle}</h3>
            <button type="button" onClick={openNewModel} className="action-btn action-btn--accent">
              {t("settings.add")}
            </button>
          </div>

          {currentModels.length === 0 ? (
            <div className="empty-text">{t("settings.noModels")}</div>
          ) : (
            <div className="settings-model-list">
              {currentModels.map((model) => (
                <div key={model.id} className="settings-model-item">
                  <div className="settings-model-info">
                    <div className="settings-model-name">
                      <span className="settings-model-name-text">{model.name}</span>
                      {model.is_default ? <span className="settings-model-badge">{t("settings.defaultBadge")}</span> : null}
                      {!model.is_default ? (
                        <button
                          type="button"
                          onClick={() => void handleSetDefault(model)}
                          className="action-btn settings-model-default-btn"
                        >
                          {t("settings.setAsDefault")}
                        </button>
                      ) : null}
                    </div>
                    <div className="settings-model-detail">
                      {model.provider} / {model.model_id}
                    </div>
                    {model.base_url ? <div className="settings-model-detail">{model.base_url}</div> : null}
                    {model.model_type === "text" ? (
                      <div className="settings-model-detail">
                        {model.max_tokens} Tokens / {model.streaming_enabled ? "Streaming" : "Non-streaming"}
                      </div>
                    ) : null}
                  </div>

                  <div className="settings-model-actions">
                    {activeModelTab === "text-model" ? (
                      <button
                        type="button"
                        onClick={() => void handleTestModel(model)}
                        className="action-btn"
                        disabled={testingModelId === model.id}
                      >
                        {testingModelId === model.id ? t("settings.testing") : t("settings.test")}
                      </button>
                    ) : null}
                    <button type="button" onClick={() => openEditModel(model)} className="action-btn">
                      {t("settings.edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() => setPendingDeleteModel(model)}
                      className="action-btn action-btn--danger"
                    >
                      {t("common.delete")}
                    </button>
                  </div>
                  {activeModelTab === "image-model" ? (
                    <ImageModelTestPanel
                      model={model}
                      isOpen={openImageTestModelId === model.id}
                      loading={imageTestModelId === model.id}
                      prompt={imageTestPrompts[model.id] ?? ""}
                      result={imageTestResults[model.id]}
                      onToggle={() =>
                        setOpenImageTestModelId((current) => (current === model.id ? null : model.id))
                      }
                      onPromptChange={(value) => patchImageTestPrompt(model.id, value)}
                      onSubmit={() => void handleTestImageModel(model)}
                    />
                  ) : null}
                </div>
              ))}
            </div>
          )}
        </div>
      </SurfacePanel>
    );
  }

  function renderBackgroundSection() {
    if (!settings) return null;

    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          <h3 className="settings-section-title">{t("settings.pageBackground")}</h3>

          {settings.home_background_strategy.trim() ? (
            <div className="settings-bg-preview">
              <div className="settings-model-detail">{settings.home_background_strategy}</div>
              <div className="settings-bg-actions">
                <button
                  type="button"
                  onClick={() => void handleClearBackground()}
                  className="action-btn action-btn--danger"
                  disabled={saving}
                >
                  {t("settings.clearBg")}
                </button>
              </div>
            </div>
          ) : (
            <div className="settings-upload-zone">
              <div className="empty-text">{t("settings.noBgConfigured")}</div>
            </div>
          )}

          <div className="settings-upload-row">
            <label className="action-btn action-btn--accent" style={{ cursor: uploading ? "wait" : "pointer" }}>
              {uploading ? t("settings.uploading") : t("settings.selectFile")}
              <input
                type="file"
                accept="image/*,video/*"
                onChange={(event) => void handleFileUpload(event)}
                disabled={uploading}
                style={{ display: "none" }}
              />
            </label>
          </div>
        </div>
      </SurfacePanel>
    );
  }

  function renderThemeSection() {
    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          <h3 className="settings-section-title">{t("settings.tabTheme")}</h3>
          <ThemePicker />
        </div>
      </SurfacePanel>
    );
  }

  function renderExportSection() {
    if (!settings) return null;

    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          <h3 className="settings-section-title">{t("settings.tabExport")}</h3>
          <label className="field-label">
            <span className="field-label-text">{t("settings.exportDir")}</span>
            <input
              value={settings.export_directory}
              readOnly={!isAndroidRuntime()}
              className="field-input"
              onChange={(event) => updateDraft({ export_directory: event.target.value })}
              placeholder={t("settings.phEnterExportDir")}
            />
            <span className="text-muted">
              {isAndroidRuntime()
                ? t("settings.androidExportHint")
                : t("settings.desktopExportHint")}
            </span>
          </label>

          <div className="settings-form-actions">
            <button
              type="button"
              onClick={() => void handlePickExportDirectory()}
              className="action-btn"
              aria-label={isAndroidRuntime() ? t("settings.useAppDir") : t("settings.selectFolder")}
            >
              {isAndroidRuntime() ? t("settings.useAppDir") : t("settings.selectFolder")}
            </button>
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={saving}
              className="action-btn action-btn--accent"
            >
              {saving ? t("settings.saving") : t("settings.save")}
            </button>
          </div>
        </div>
      </SurfacePanel>
    );
  }

  function renderMobileSectionContent() {
    if (!activeSection) return renderSectionList();

    const currentSectionMeta = tabIds.find((item) => item.id === activeSection);
    return (
      <div className="settings-page-shell">
        <div className="settings-detail-head">
          <div className="settings-detail-head-copy">
            <span>{t("settings.currentGroup")}</span>
            <strong>{currentSectionMeta ? t(currentSectionMeta.labelKey) : ""}</strong>
          </div>
        </div>

        {isModelTab(activeSection) ? (editingModel || isNewModel ? renderModelEditor() : renderModelList()) : null}
        {activeSection === "background" ? renderBackgroundSection() : null}
        {activeSection === "theme" ? renderThemeSection() : null}
        {activeSection === "export" ? renderExportSection() : null}
      </div>
    );
  }

  // ==================== Desktop rendering ====================
  function renderDesktopContent() {
    return (
      <>
        <div className="settings-tabs">
          {tabIds.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => {
                setActiveTab(tab.id);
                closeModelEditor();
              }}
              className={`settings-tab${activeTab === tab.id ? " settings-tab--active" : ""}`}
            >
              {t(tab.labelKey)}
            </button>
          ))}
        </div>

        {isModelTab(activeTab) && settings ? (
          <SurfacePanel className="surface-panel--pad-lg">
            {activeTab === "embedding-model" ? (
              <div className="settings-section">
                <h3 className="settings-section-title">{t("settings.embeddingModelTitle")}</h3>
                <label className="field-label field-label--inline">
                  <span className="field-label-text">{t("settings.enableEmbedding")}</span>
                  <div className="settings-inline-toggle">
                    <input
                      type="checkbox"
                      checked={settings.embedding_enabled}
                      onChange={(event) => void handleEmbeddingEnabledChange(event.target.checked)}
                      disabled={saving}
                    />
                  </div>
                </label>
                {embeddingDefaultMissing ? <div className="text-error">{t("settings.needDefaultEmbedding")}</div> : null}
              </div>
            ) : null}

            {editingModel || isNewModel ? (
              <div className="settings-section">
                <h3 className="settings-section-title">
                  {isNewModel ? t("settings.newModelMobile") : t("settings.editModelShort").replace("{name}", editingModel?.name ?? "")}
                </h3>
                <div className="settings-form-grid settings-form-grid--model-editor">
                  <label className="field-label">
                    <span className="field-label-text">{t("settings.modelName")}</span>
                    <input
                      value={modelForm.name}
                      onChange={(event) => patchModelForm({ name: event.target.value })}
                      className="field-input"
                      placeholder={t("settings.phNarrativeName")}
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">{t("settings.provider")}</span>
                    <select
                      value={providerSelectValue}
                      onChange={(event) => {
                        const nextValue = event.target.value;
                        if (nextValue === "custom") {
                          setCustomProviderMode(true);
                          patchModelForm(
                            {
                              provider: providerOptions.some((option) => option.value === modelForm.provider)
                                ? ""
                                : modelForm.provider,
                            },
                            { resetDiscovery: true },
                          );
                          return;
                        }
                        patchProvider(nextValue);
                      }}
                      className="field-input editor-field-select"
                    >
                      <option value="">{t("settings.selectProvider")}</option>
                      {providerOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.value === "builtin-local" ? t("settings.builtinLocal") : option.label}
                        </option>
                      ))}
                      <option value="custom">{t("settings.custom")}</option>
                    </select>
                    {showsCustomProviderInput ? (
                      <input
                        value={modelForm.provider}
                        onChange={(event) => {
                          setCustomProviderMode(true);
                          patchModelForm({ provider: event.target.value }, { resetDiscovery: true });
                        }}
                        className="field-input"
                        style={{ marginTop: 8 }}
                        placeholder={t("settings.phCustomProvider")}
                      />
                    ) : null}
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">{t("settings.modelId")}</span>
                    <input
                      value={modelForm.model_id}
                      onChange={(event) => patchModelForm({ model_id: event.target.value })}
                      className="field-input"
                      placeholder={t("settings.phExampleGptLocal")}
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">Base URL</span>
                    <input
                      value={modelForm.base_url}
                      onChange={(event) => patchModelForm({ base_url: event.target.value }, { resetDiscovery: true })}
                      className="field-input"
                      placeholder={isBuiltinLocalProvider ? t("settings.builtinNoFill") : t("settings.phBaseUrlExample")}
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">{t("settings.apiKeyOptional")}</span>
                    <input
                      value={modelForm.api_key}
                      onChange={(event) => patchModelForm({ api_key: event.target.value }, { resetDiscovery: true })}
                      className="field-input"
                      type="password"
                      placeholder={isBuiltinLocalProvider ? t("settings.builtinNoFill") : t("settings.leaveEmptyNoSet")}
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  {activeTab === "text-model" ? (
                    <label className="field-label">
                      <span className="field-label-text">{t("settings.maxOutputTokens")}</span>
                      <input
                        value={modelForm.max_tokens}
                        onChange={(event) => patchModelForm({ max_tokens: event.target.value })}
                        className="field-input"
                        inputMode="numeric"
                        placeholder="1200"
                      />
                    </label>
                  ) : null}
                  {activeTab === "text-model" ? (
                    <label className="field-label">
                      <span className="field-label-text">{t("settings.streamingOutput")}</span>
                      <div className="settings-inline-toggle">
                        <input
                          type="checkbox"
                          checked={modelForm.streaming_enabled}
                          onChange={(event) => patchModelForm({ streaming_enabled: event.target.checked })}
                        />
                      </div>
                    </label>
                  ) : null}
                </div>

                {activeTab !== "image-model" && !isBuiltinLocalProvider ? (
                  <div className="settings-model-discovery">
                    <div className="settings-model-discovery-header">
                      <div className="settings-model-discovery-copy">
                        <div className="field-label-text">{t("settings.endpointModelList")}</div>
                        <div className="text-muted">
                          {t("settings.endpointHint")}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => void handleDiscoverModels()}
                        disabled={discoveringModels || !modelForm.base_url.trim()}
                        className="action-btn"
                      >
                        {discoveringModels ? t("settings.discovering") : t("settings.fetchFromEndpoint")}
                      </button>
                    </div>
                    {modelDiscovery ? (
                      <div className={modelDiscovery.ok ? "text-muted" : "text-error"}>{modelDiscovery.detail}</div>
                    ) : null}
                    {modelDiscovery?.modelIds.length ? (
                      <div className="settings-model-suggestions">
                        {modelDiscovery.modelIds.map((modelId) => (
                          <button
                            key={modelId}
                            type="button"
                            onClick={() => patchModelForm({ model_id: modelId })}
                            className={`action-btn${modelForm.model_id === modelId ? " action-btn--accent" : ""}`}
                          >
                            {modelId}
                          </button>
                        ))}
                      </div>
                    ) : null}
                  </div>
                ) : null}

                <div className="settings-form-actions">
                  <button
                    type="button"
                    onClick={() => void handleSaveModel()}
                    disabled={saving || !modelForm.name.trim() || !modelForm.provider.trim() || !modelForm.model_id.trim()}
                    className="action-btn action-btn--accent"
                  >
                    {saving ? t("settings.saving") : t("settings.save")}
                  </button>
                  <button type="button" onClick={closeModelEditor} className="action-btn">
                    {t("common.cancel")}
                  </button>
                </div>
              </div>
            ) : (
              <div className="settings-section">
                <div className="settings-list-header">
                  <h3 className="settings-section-title">
                    {activeModelTab === "text-model"
                      ? t("settings.textModelList")
                      : activeModelTab === "image-model"
                        ? t("settings.imageModelList")
                        : t("settings.embeddingModelList")}
                  </h3>
                  <button type="button" onClick={openNewModel} className="action-btn action-btn--accent">
                    {t("settings.add")}
                  </button>
                </div>
                {currentModels.length === 0 ? (
                  <div className="empty-text">{t("settings.noModelsAdd")}</div>
                ) : (
                  <div className="settings-model-list">
                    {currentModels.map((model) => (
                      <div key={model.id} className="settings-model-item">
                        <div className="settings-model-info">
                          <div className="settings-model-name">
                            {model.name}
                            {model.is_default ? <span className="settings-model-badge">{t("settings.defaultBadge")}</span> : null}
                          </div>
                          <div className="settings-model-detail">
                            {model.provider} / {model.model_id}
                          </div>
                          {model.base_url ? <div className="settings-model-detail">{model.base_url}</div> : null}
                          {model.model_type === "text" ? (
                            <div className="settings-model-detail">
                              {model.max_tokens} Tokens / {model.streaming_enabled ? t("settings.streaming") : t("settings.nonStreaming")}
                            </div>
                          ) : null}
                        </div>
                        <div className="settings-model-actions">
                          {activeModelTab === "text-model" ? (
                            <button
                              type="button"
                              onClick={() => void handleTestModel(model)}
                              className="action-btn"
                              disabled={testingModelId === model.id}
                            >
                              {testingModelId === model.id ? t("settings.testing") : t("settings.test")}
                            </button>
                          ) : null}
                          {!model.is_default ? (
                            <button type="button" onClick={() => void handleSetDefault(model)} className="action-btn">
                              {t("settings.setAsDefault")}
                            </button>
                          ) : null}
                          <button type="button" onClick={() => openEditModel(model)} className="action-btn">
                            {t("settings.edit")}
                          </button>
                          <button
                            type="button"
                            onClick={() => setPendingDeleteModel(model)}
                            className="action-btn action-btn--danger"
                          >
                            {t("common.delete")}
                          </button>
                        </div>
                        {activeModelTab === "image-model" ? (
                          <ImageModelTestPanel
                            model={model}
                            isOpen={openImageTestModelId === model.id}
                            loading={imageTestModelId === model.id}
                            prompt={imageTestPrompts[model.id] ?? ""}
                            result={imageTestResults[model.id]}
                            onToggle={() =>
                              setOpenImageTestModelId((current) => (current === model.id ? null : model.id))
                            }
                            onPromptChange={(value) => patchImageTestPrompt(model.id, value)}
                            onSubmit={() => void handleTestImageModel(model)}
                          />
                        ) : null}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </SurfacePanel>
        ) : null}

        {activeTab === "background" && settings ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="settings-section">
              <h3 className="settings-section-title">{t("settings.pageBackgroundImgVideo")}</h3>
              <p className="text-muted" style={{ marginTop: 4, marginBottom: 16 }}>
                {t("settings.bgUploadHint")}
              </p>

              {settings.home_background_strategy ? (
                <div className="settings-bg-preview">
                  {/\.(mp4|webm|ogg|mov)$/i.test(settings.home_background_strategy) ? (
                    <video
                      src={assetUrl(settings.home_background_strategy)}
                      className="settings-bg-image"
                      autoPlay
                      loop
                      muted
                      playsInline
                    />
                  ) : (
                    <img
                      src={assetUrl(settings.home_background_strategy)}
                      alt={t("settings.altCurrentBg")}
                      className="settings-bg-image"
                    />
                  )}
                  <div className="settings-bg-actions">
                    <button
                      type="button"
                      onClick={() => void handleClearBackground()}
                      className="action-btn action-btn--danger"
                      disabled={saving}
                    >
                      {t("settings.clearBg")}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="settings-upload-zone">
                  <p className="text-muted">{t("settings.noBgYet")}</p>
                </div>
              )}

              <div className="settings-upload-row" style={{ marginTop: 12 }}>
                <label className="action-btn action-btn--accent" style={{ cursor: uploading ? "wait" : "pointer" }}>
                  {uploading ? t("settings.uploading") : t("settings.selectFileUpload")}
                  <input
                    type="file"
                    accept="image/*,video/*"
                    onChange={(event) => void handleFileUpload(event)}
                    disabled={uploading}
                    style={{ display: "none" }}
                  />
                </label>
              </div>
            </div>
          </SurfacePanel>
        ) : null}

        {activeTab === "theme" ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="settings-section">
              <h3 className="settings-section-title">{t("settings.tabTheme")}</h3>
              <p className="text-muted" style={{ marginTop: 4, marginBottom: 16 }}>
                {t("settings.themeHint")}
              </p>
              <ThemePicker />
            </div>
          </SurfacePanel>
        ) : null}

        {activeTab === "export" && settings ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="settings-section">
              <h3 className="settings-section-title">{t("settings.tabExport")}</h3>
              <label className="field-label">
                <span className="field-label-text">{t("settings.exportDir")}</span>
                <input
                  value={settings.export_directory}
                  onChange={(event) => updateDraft({ export_directory: event.target.value })}
                  className="field-input"
                />
              </label>
              <div className="settings-form-actions">
                {supportsDirectoryPicker ? (
                  <button type="button" onClick={() => void handlePickExportDirectory()} className="action-btn">
                    {t("settings.selectFolder")}
                  </button>
                ) : null}
                <button
                  type="button"
                  onClick={() => void handleSave()}
                  disabled={saving}
                  className="action-btn action-btn--primary"
                >
                  {saving ? t("settings.saving") : t("settings.saveExportSettings")}
                </button>
              </div>
            </div>
          </SurfacePanel>
        ) : null}
      </>
    );
  }

  // ==================== Main render ====================
  return (
    <ScreenLayout
      title={t("settings.title")}
      subtitle={isMobile ? undefined : t("settings.subtitle")}
      compactHeader
      toolbar={
        isMobile ? null : (
          <button type="button" onClick={() => navigate(-1)} className="action-btn">
            <ArrowLeft size={14} /> {t("common.back")}
          </button>
        )
      }
      maxWidth={980}
    >
      {loading ? <SurfacePanel className="surface-panel--pad-lg">{t("settings.loadingSettings")}</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">{t("settings.errorPrefix").replace("{error}", error)}</SurfacePanel> : null}

      {!loading && !error && isMobile ? renderMobileSectionContent() : null}
      {!loading && !error && !isMobile ? renderDesktopContent() : null}

      <ConfirmDialog
        open={Boolean(pendingDeleteModel)}
        title={t("settings.deleteModelTitle")}
        description={pendingDeleteModel ? t("settings.deleteModelConfirm").replace("{name}", pendingDeleteModel.name) : ""}
        confirmLabel={t("settings.deleteModelTitle")}
        confirmVariant="danger"
        onClose={() => setPendingDeleteModel(null)}
        onConfirm={() => {
          if (!pendingDeleteModel) return;
          void handleDeleteModel(pendingDeleteModel);
        }}
      />
    </ScreenLayout>
  );
}

/* TODO: SettingsPage 移动端与桌面端 UI 架构差异较大（section-list vs tabs），
 * 当前通过 isMobile 条件渲染两套布局，后续可考虑进一步重构为统一组件。 */
