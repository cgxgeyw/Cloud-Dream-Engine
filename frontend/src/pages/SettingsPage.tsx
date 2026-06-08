import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
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

const BUILTIN_EMBEDDING_MODEL_ID = "BAAI/bge-small-zh-v1.5";

const tabIds = [
  { id: "text-model", label: "LLM 模型" },
  { id: "image-model", label: "绘图模型" },
  { id: "embedding-model", label: "Embedding 模型" },
  { id: "background", label: "背景设置" },
  { id: "theme", label: "主题设置" },
  { id: "export", label: "导出设置" },
] as const;

const commonProviderOptions = [
  { value: "OpenAI", label: "OpenAI" },
  { value: "Claude / Anthropic", label: "Claude / Anthropic" },
  { value: "Gemini", label: "Gemini" },
  { value: "DeepSeek", label: "DeepSeek" },
  { value: "字节火山", label: "字节火山" },
  { value: "阿里百炼", label: "阿里百炼" },
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
  Ollama: "http://127.0.0.1:11434/v1",
  "LM Studio": "http://127.0.0.1:1234/v1",
  "gpt-image2": "https://api.openai.com/v1",
  "google nano banana": "https://generativelanguage.googleapis.com/v1beta/openai",
  "builtin-local": "",
};

const providerDiscoverySupport = new Set(["OpenAI", "Ollama", "LM Studio"]);

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

interface SettingsPageProps {
  isMobile?: boolean;
}

export function SettingsPage({ isMobile = false }: SettingsPageProps) {
  const navigate = useNavigate();
  const { refresh: refreshGlobalSettings } = useSettings();

  // Desktop: activeTab, Mobile: activeSection + section-list navigation
  const [activeTab, setActiveTab] = useState<TabId>("text-model");
  const [activeSection, setActiveSection] = useState<TabId | null>(null);

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
  const supportsModelDiscovery =
    activeModelTab !== "image-model" &&
    !isBuiltinLocalProvider &&
    providerDiscoverySupport.has(modelForm.provider.trim());

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
          setError(loadError instanceof Error ? loadError.message : "加载设置失败");
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
    setActiveSection(sectionId);
    if (isModelTab(sectionId)) {
      setActiveTab(sectionId);
    }
    setError(null);
    closeModelEditor();
  }


  function openNewModel() {
    if (!activeModelTab) return;

    const nextForm =
      activeModelTab === "embedding-model"
        ? {
            ...defaultModelForm,
            provider: "builtin-local",
            model_id: BUILTIN_EMBEDDING_MODEL_ID,
            name: "内置 Embedding",
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
      showToast("设置已保存");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存设置失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleEmbeddingEnabledChange(nextEnabled: boolean) {
    if (!settings) return;

    if (nextEnabled && !settings.default_embedding_model.trim()) {
      setError("请先设置默认 Embedding 模型");
      showToast("请先设置默认 Embedding 模型", "error");
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
      const message = saveError instanceof Error ? saveError.message : "保存 Embedding 设置失败";
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
      showToast("模型已保存");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存模型失败");
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
      showToast("模型已删除");
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除模型失败");
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

      showToast("默认模型已更新");
    } catch (setError_) {
      setError(setError_ instanceof Error ? setError_.message : "设置默认模型失败");
    }
  }

  async function handleTestModel(model: ModelConfigResponse) {
    try {
      setTestingModelId(model.id);
      showToast("模型测试中...");
      const result = await testModel(model.id);
      showToast(result.detail, result.ok ? "success" : "error");
    } catch (testError_) {
      showToast(testError_ instanceof Error ? testError_.message : "模型测试失败", "error");
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
      showToast(isMobile ? "Please enter a prompt first." : "请先输入测试提示词", "error");
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
    if (!supportsModelDiscovery) {
      const message = "当前提供商不支持拉取模型列表。";
      setModelDiscovery({ ok: false, detail: message, modelIds: [] });
      showToast(message, "error");
      return;
    }

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
      const rawMessage = discoverError instanceof Error ? discoverError.message : "读取模型列表失败";
      const message =
        rawMessage.includes("Model discovery not supported for provider")
          ? "当前提供商不支持拉取模型列表。"
          : rawMessage;
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
        throw new Error("上传成功，但没有返回可用的资源地址");
      }
      const nextSettings = { ...settings, home_background_strategy: assetPath };
      const saved = await updateSettings(nextSettings);
      setSettings(saved);
      await refreshGlobalSettings();
      showToast("背景已更新");
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : "上传背景失败");
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
      showToast("背景已清除");
    } catch (clearError) {
      setError(clearError instanceof Error ? clearError.message : "清除背景失败");
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
          showToast(isMobile ? "已选择应用导出目录" : "已选择应用导出目录");
        }
      } catch (pickError) {
        setError(pickError instanceof Error ? pickError.message : "选择导出目录失败");
      }
      return;
    }

    if (!supportsDirectoryPicker) {
      showToast("当前环境不支持目录选择，请直接填写导出目录", "error");
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
      setError(pickError instanceof Error ? pickError.message : "选择导出目录失败");
    }
  }

  // ==================== Mobile-specific rendering ====================
  function renderSectionList() {
    return (
      <div className="settings-page-shell">
        <div className="settings-mobile-overview-head">
          <div className="settings-mobile-kicker">设置</div>
          <h1 className="settings-mobile-title">{"\u8bbe\u7f6e\u4e2d\u5fc3"}</h1>
          <p className="settings-mobile-summary">{"\u7ba1\u7406\u6a21\u578b\u3001\u4e3b\u9898\u3001\u80cc\u666f\u548c\u5bfc\u51fa\u504f\u597d\u3002"}</p>
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
                  <strong>{item.label}</strong>
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
          <h3 className="settings-section-title">{isNewModel ? "新增模型" : `编辑模型：${editingModel?.name ?? ""}`}</h3>

          <div className="settings-form-grid settings-form-grid--model-editor">
            <label className="field-label">
              <span className="field-label-text">模型名称</span>
              <input
                value={modelForm.name}
                onChange={(event) => patchModelForm({ name: event.target.value })}
                className="field-input"
                placeholder="例如：主对话模型"
              />
            </label>

            <label className="field-label">
              <span className="field-label-text">提供商</span>
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
                <option value="">请选择</option>
                {providerOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
                <option value="custom">自定义</option>
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
                  placeholder="填写 provider 名称"
                />
              ) : null}
            </label>

            <label className="field-label">
              <span className="field-label-text">模型 ID</span>
              <input
                value={modelForm.model_id}
                onChange={(event) => patchModelForm({ model_id: event.target.value })}
                className="field-input"
                placeholder="例如：gpt-4.1"
                readOnly={isBuiltinLocalProvider}
              />
            </label>

            <label className="field-label">
              <span className="field-label-text">Base URL</span>
              <input
                value={modelForm.base_url}
                onChange={(event) => patchModelForm({ base_url: event.target.value }, { resetDiscovery: true })}
                className="field-input"
                placeholder={isBuiltinLocalProvider ? "内置本地模型无需填写" : "例如：http://127.0.0.1:1234/v1"}
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
                placeholder={isBuiltinLocalProvider ? "内置本地模型无需填写" : "留空则不设置"}
                readOnly={isBuiltinLocalProvider}
              />
            </label>

            {activeModelTab === "text-model" ? (
              <label className="field-label field-label--inline">
                <span className="field-label-text">最大输出 Tokens</span>
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
                <span className="field-label-text">流式输出</span>
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
                  <div className="field-label-text">模型列表</div>
                  {supportsModelDiscovery ? (
                    <div className="text-muted">从当前接口读取可用模型 ID，点选后会自动填入模型 ID。</div>
                  ) : null}
                </div>
              </div>

              {supportsModelDiscovery ? (
                <button
                  type="button"
                  onClick={() => void handleDiscoverModels()}
                  disabled={discoveringModels || !modelForm.base_url.trim()}
                  className="action-btn action-btn--accent settings-model-discovery-btn"
                >
                  {discoveringModels ? "读取中..." : "获取模型列表"}
                </button>
              ) : null}

              {!supportsModelDiscovery ? (
                <div className="text-muted">当前 provider 不支持拉取模型列表，请手动填写模型 ID</div>
              ) : null}

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
              {saving ? "保存中..." : "保存"}
            </button>
            <button type="button" onClick={closeModelEditor} className="action-btn">
              取消
            </button>
          </div>
          {!supportsDirectoryPicker ? <div className="text-muted">当前环境不支持目录选择器。</div> : null}
        </div>
      </SurfacePanel>
    );
  }

  function renderModelList() {
    if (!settings || !activeModelTab) return null;

    const sectionTitle =
      activeModelTab === "text-model"
        ? "LLM 模型"
        : activeModelTab === "image-model"
          ? "绘图模型"
          : "Embedding 模型";

    return (
      <SurfacePanel className="surface-panel--pad-lg">
        <div className="settings-section">
          {activeModelTab === "embedding-model" ? (
            <div className="settings-section">
              <h3 className="settings-section-title">嵌入模型</h3>
              <label className="field-label field-label--inline">
                <span className="field-label-text">启用嵌入模型</span>
                <div className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={settings.embedding_enabled}
                    onChange={(event) => void handleEmbeddingEnabledChange(event.target.checked)}
                    disabled={saving}
                  />
                </div>
              </label>
              {embeddingDefaultMissing ? <div className="text-error">请先设置默认 Embedding 模型</div> : null}
            </div>
          ) : null}

          <div className="settings-list-header">
            <h3 className="settings-section-title">{sectionTitle}</h3>
            <button type="button" onClick={openNewModel} className="action-btn action-btn--accent">
              + 新增
            </button>
          </div>

          {currentModels.length === 0 ? (
            <div className="empty-text">暂无模型</div>
          ) : (
            <div className="settings-model-list">
              {currentModels.map((model) => (
                <div key={model.id} className="settings-model-item">
                  <div className="settings-model-info">
                    <div className="settings-model-name">
                      <span className="settings-model-name-text">{model.name}</span>
                      {model.is_default ? <span className="settings-model-badge">默认</span> : null}
                      {!model.is_default ? (
                        <button
                          type="button"
                          onClick={() => void handleSetDefault(model)}
                          className="action-btn settings-model-default-btn"
                        >
                          设为默认
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
                        {testingModelId === model.id ? "测试中..." : "测试"}
                      </button>
                    ) : null}
                    <button type="button" onClick={() => openEditModel(model)} className="action-btn">
                      编辑
                    </button>
                    <button
                      type="button"
                      onClick={() => setPendingDeleteModel(model)}
                      className="action-btn action-btn--danger"
                    >
                      删除
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
          <h3 className="settings-section-title">页面背景</h3>

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
                  清除背景
                </button>
              </div>
            </div>
          ) : (
            <div className="settings-upload-zone">
              <div className="empty-text">尚未配置背景。</div>
            </div>
          )}

          <div className="settings-upload-row">
            <label className="action-btn action-btn--accent" style={{ cursor: uploading ? "wait" : "pointer" }}>
              {uploading ? "上传中..." : "选择文件"}
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
          <h3 className="settings-section-title">主题设置</h3>
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
          <h3 className="settings-section-title">导出设置</h3>
          <label className="field-label">
            <span className="field-label-text">导出目录</span>
            <input
              value={settings.export_directory}
              readOnly={!isAndroidRuntime()}
              className="field-input"
              onChange={(event) => updateDraft({ export_directory: event.target.value })}
              placeholder="输入导出目录"
            />
            <span className="text-muted">
              {isAndroidRuntime()
                ? "安卓端默认使用应用导出目录，也可以手动修改路径。"
                : "桌面端通过系统目录选择器设置导出目录。"}
            </span>
          </label>

          <div className="settings-form-actions">
            <button
              type="button"
              onClick={() => void handlePickExportDirectory()}
              className="action-btn"
              aria-label={isAndroidRuntime() ? "使用应用目录" : "选择文件夹"}
            >
              {isAndroidRuntime() ? "使用应用目录" : "选择文件夹"}
            </button>
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={saving}
              className="action-btn action-btn--accent"
            >
              {saving ? "保存中..." : "保存"}
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
            <span>{"\u5f53\u524d\u5206\u7ec4"}</span>
            <strong>{currentSectionMeta?.label}</strong>
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
              {tab.label}
            </button>
          ))}
        </div>

        {isModelTab(activeTab) && settings ? (
          <SurfacePanel className="surface-panel--pad-lg">
            {activeTab === "embedding-model" ? (
              <div className="settings-section">
                <h3 className="settings-section-title">嵌入模型</h3>
                <label className="field-label field-label--inline">
                  <span className="field-label-text">启用嵌入模型</span>
                  <div className="settings-inline-toggle">
                    <input
                      type="checkbox"
                      checked={settings.embedding_enabled}
                      onChange={(event) => void handleEmbeddingEnabledChange(event.target.checked)}
                      disabled={saving}
                    />
                  </div>
                </label>
                {embeddingDefaultMissing ? <div className="text-error">请先设置默认 Embedding 模型</div> : null}
              </div>
            ) : null}

            {editingModel || isNewModel ? (
              <div className="settings-section">
                <h3 className="settings-section-title">
                  {isNewModel ? "新建模型" : `编辑：${editingModel?.name}`}
                </h3>
                <div className="settings-form-grid settings-form-grid--model-editor">
                  <label className="field-label">
                    <span className="field-label-text">模型名称</span>
                    <input
                      value={modelForm.name}
                      onChange={(event) => patchModelForm({ name: event.target.value })}
                      className="field-input"
                      placeholder="例如：GPT-4.1 叙事增强"
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">提供商</span>
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
                      <option value="">请选择 provider</option>
                      {providerOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                      <option value="custom">自定义</option>
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
                        placeholder="输入自定义 provider 名称"
                      />
                    ) : null}
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">模型 ID</span>
                    <input
                      value={modelForm.model_id}
                      onChange={(event) => patchModelForm({ model_id: event.target.value })}
                      className="field-input"
                      placeholder="例如：gpt-4.1-narrative-local"
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">Base URL</span>
                    <input
                      value={modelForm.base_url}
                      onChange={(event) => patchModelForm({ base_url: event.target.value }, { resetDiscovery: true })}
                      className="field-input"
                      placeholder={isBuiltinLocalProvider ? "内置本地模型无需填写" : "例如：http://127.0.0.1:1234/v1"}
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">API Key（可选）</span>
                    <input
                      value={modelForm.api_key}
                      onChange={(event) => patchModelForm({ api_key: event.target.value }, { resetDiscovery: true })}
                      className="field-input"
                      type="password"
                      placeholder={isBuiltinLocalProvider ? "内置本地模型无需填写" : "留空则不设置"}
                      readOnly={isBuiltinLocalProvider}
                    />
                  </label>
                  {activeTab === "text-model" ? (
                    <label className="field-label">
                      <span className="field-label-text">最大输出 Tokens</span>
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
                      <span className="field-label-text">流式输出</span>
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
                        <div className="field-label-text">端点模型列表</div>
                        {supportsModelDiscovery ? (
                          <div className="text-muted">
                            读取当前 Base URL 返回的可用模型。支持 OpenAI 兼容 `/models`，Ollama 会额外尝试 `/api/tags`。
                          </div>
                        ) : (
                          <div className="text-muted">当前 provider 不支持拉取模型列表，请手动填写模型 ID</div>
                        )}
                      </div>
                      {supportsModelDiscovery ? (
                        <button
                          type="button"
                          onClick={() => void handleDiscoverModels()}
                          disabled={discoveringModels || !modelForm.base_url.trim()}
                          className="action-btn"
                        >
                          {discoveringModels ? "读取中..." : "从端点拉取"}
                        </button>
                      ) : null}
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
                    {saving ? "保存中..." : "保存"}
                  </button>
                  <button type="button" onClick={closeModelEditor} className="action-btn">
                    取消
                  </button>
                </div>
              </div>
            ) : (
              <div className="settings-section">
                <div className="settings-list-header">
                  <h3 className="settings-section-title">
                    {activeModelTab === "text-model"
                      ? "LLM 模型列表"
                      : activeModelTab === "image-model"
                        ? "文生图模型列表"
                        : "Embedding 模型列表"}
                  </h3>
                  <button type="button" onClick={openNewModel} className="action-btn action-btn--accent">
                    + 新增
                  </button>
                </div>
                {currentModels.length === 0 ? (
                  <div className="empty-text">暂无模型，点击“新增”开始添加。</div>
                ) : (
                  <div className="settings-model-list">
                    {currentModels.map((model) => (
                      <div key={model.id} className="settings-model-item">
                        <div className="settings-model-info">
                          <div className="settings-model-name">
                            {model.name}
                            {model.is_default ? <span className="settings-model-badge">默认</span> : null}
                          </div>
                          <div className="settings-model-detail">
                            {model.provider} / {model.model_id}
                          </div>
                          {model.base_url ? <div className="settings-model-detail">{model.base_url}</div> : null}
                          {model.model_type === "text" ? (
                            <div className="settings-model-detail">
                              {model.max_tokens} Tokens / {model.streaming_enabled ? "流式" : "非流式"}
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
                              {testingModelId === model.id ? "测试中..." : "测试"}
                            </button>
                          ) : null}
                          {!model.is_default ? (
                            <button type="button" onClick={() => void handleSetDefault(model)} className="action-btn">
                              设为默认
                            </button>
                          ) : null}
                          <button type="button" onClick={() => openEditModel(model)} className="action-btn">
                            编辑
                          </button>
                          <button
                            type="button"
                            onClick={() => setPendingDeleteModel(model)}
                            className="action-btn action-btn--danger"
                          >
                            删除
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
              <h3 className="settings-section-title">页面背景图 / 视频</h3>
              <p className="text-muted" style={{ marginTop: 4, marginBottom: 16 }}>
                上传后立即生效，作为所有非游戏页面的背景。
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
                      alt="当前背景"
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
                      清除背景
                    </button>
                  </div>
                </div>
              ) : (
                <div className="settings-upload-zone">
                  <p className="text-muted">当前还没有背景，使用下方按钮上传即可。</p>
                </div>
              )}

              <div className="settings-upload-row" style={{ marginTop: 12 }}>
                <label className="action-btn action-btn--accent" style={{ cursor: uploading ? "wait" : "pointer" }}>
                  {uploading ? "上传中..." : "选择文件并上传"}
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
              <h3 className="settings-section-title">主题设置</h3>
              <p className="text-muted" style={{ marginTop: 4, marginBottom: 16 }}>
                选择你喜欢的界面外观风格，即时预览并切换。
              </p>
              <ThemePicker />
            </div>
          </SurfacePanel>
        ) : null}

        {activeTab === "export" && settings ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <div className="settings-section">
              <h3 className="settings-section-title">导出设置</h3>
              <label className="field-label">
                <span className="field-label-text">导出目录</span>
                <input
                  value={settings.export_directory}
                  onChange={(event) => updateDraft({ export_directory: event.target.value })}
                  className="field-input"
                />
              </label>
              <div className="settings-form-actions">
                {supportsDirectoryPicker ? (
                  <button type="button" onClick={() => void handlePickExportDirectory()} className="action-btn">
                    选择文件夹
                  </button>
                ) : null}
                <button
                  type="button"
                  onClick={() => void handleSave()}
                  disabled={saving}
                  className="action-btn action-btn--primary"
                >
                  {saving ? "保存中..." : "保存导出设置"}
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
      title="设置"
      subtitle={isMobile ? undefined : "配置文本模型、图片模型、背景与导出选项。"}
      compactHeader
      toolbar={
        isMobile ? null : (
          <button type="button" onClick={() => navigate(-1)} className="action-btn">
            <ArrowLeft size={14} /> 返回
          </button>
        )
      }
      maxWidth={980}
    >
      {loading ? <SurfacePanel className="surface-panel--pad-lg">正在加载设置...</SurfacePanel> : null}
      {error ? <SurfacePanel className="surface-panel--pad-lg text-error">错误：{error}</SurfacePanel> : null}

      {!loading && !error && isMobile ? renderMobileSectionContent() : null}
      {!loading && !error && !isMobile ? renderDesktopContent() : null}

      <ConfirmDialog
        open={Boolean(pendingDeleteModel)}
        title="删除模型"
        description={pendingDeleteModel ? `确定删除模型“${pendingDeleteModel.name}”吗？` : ""}
        confirmLabel="删除模型"
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
