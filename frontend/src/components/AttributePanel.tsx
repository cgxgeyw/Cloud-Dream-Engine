import { useEffect, useMemo, useState } from "react";
import {
  createAttributeSchema,
  deleteAttributeSchema,
  fetchAttributeSchemas,
  fetchAttributeValues,
  upsertAttributeValue,
  type AttributeSchemaResponse,
  type AttributeValueType,
} from "../data/apiAdapter";

type Scope = "world" | "character";
type OwnerType = "world" | "character";

type AttributePanelProps = {
  scope: Scope;
  ownerType: OwnerType;
  ownerId?: string;
};

type DraftState = {
  key: string;
  label: string;
  valueType: AttributeValueType;
  description: string;
  defaultValue: string;
  enumOptions: string;
};

const emptyDraft: DraftState = {
  key: "",
  label: "",
  valueType: "text",
  description: "",
  defaultValue: "",
  enumOptions: "",
};

function stringifyValue(value: unknown) {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  return JSON.stringify(value, null, 2);
}

function parseEnumOptions(raw: string) {
  return raw
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseValue(valueType: AttributeValueType, raw: string) {
  const text = raw.trim();

  if (valueType === "number") {
    return text ? Number(text) : 0;
  }
  if (valueType === "boolean") {
    return text.toLowerCase() === "true";
  }
  if (valueType === "list") {
    return raw
      .split("\n")
      .map((item) => item.trim())
      .filter(Boolean);
  }
  if (valueType === "json") {
    return text ? JSON.parse(text) : {};
  }
  return raw;
}

function buildDisplayPolicy() {
  return {
    editor_visible: true,
    game_visible: false,
    debug_visible: true,
  };
}

function buildAccessPolicy(scope: Scope) {
  return {
    creator_read: true,
    player_read: false,
    agent_self_read: scope === "character",
    agent_other_read: false,
    director_read: true,
    plugin_read: true,
  };
}

function buildMutationPolicy() {
  return {
    creator_write: true,
    allowed_ops: ["set"],
  };
}

function buildInfluencePolicy() {
  return {
    "prompt.director": { enabled: true, mode: "raw" },
    "ui.status_panel": { enabled: true, mode: "text" },
  };
}

function buildProjectionPolicy(scope: Scope) {
  return {
    inherit_to_session: true,
    session_owner_type: scope === "world" ? "session" : "session_character",
    mutable_in_session: true,
  };
}

function schemaAppliesToOwner(schema: AttributeSchemaResponse, scope: Scope, ownerId?: string) {
  const applicableWorldIds = schema.display_policy?.applicable_world_ids;
  if (scope !== "world" || !ownerId || !Array.isArray(applicableWorldIds) || applicableWorldIds.length === 0) {
    return true;
  }
  return applicableWorldIds.map(String).includes(ownerId);
}

export function AttributePanel({ scope, ownerType, ownerId }: AttributePanelProps) {
  const [schemas, setSchemas] = useState<AttributeSchemaResponse[]>([]);
  const [valueMap, setValueMap] = useState<Map<string, string>>(new Map());
  const [draft, setDraft] = useState<DraftState>(emptyDraft);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [savingSchema, setSavingSchema] = useState(false);
  const [savingValueId, setSavingValueId] = useState<string | null>(null);
  const [deletingSchemaId, setDeletingSchemaId] = useState<string | null>(null);

  async function loadData(options?: { isCancelled?: () => boolean }) {
    const isCancelled = options?.isCancelled ?? (() => false);
    try {
      setLoading(true);
      setError(null);

      const [schemaData, valueData] = await Promise.all([
        fetchAttributeSchemas(scope),
        ownerId ? fetchAttributeValues({ ownerType, ownerId }) : Promise.resolve([]),
      ]);

      // ownerId 快速切换或组件卸载时丢弃过期结果，避免旧请求覆盖新数据 / 卸载后 setState。
      if (isCancelled()) {
        return;
      }
      setSchemas(schemaData.filter((schema) => schemaAppliesToOwner(schema, scope, ownerId)));
      setValueMap(new Map(valueData.map((item) => [item.schema_id, stringifyValue(item.value)])));
    } catch (loadError) {
      if (isCancelled()) {
        return;
      }
      setError(loadError instanceof Error ? loadError.message : "属性加载失败");
    } finally {
      if (!isCancelled()) {
        setLoading(false);
      }
    }
  }

  useEffect(() => {
    let cancelled = false;
    void loadData({ isCancelled: () => cancelled });
    return () => {
      cancelled = true;
    };
  }, [scope, ownerType, ownerId]);

  const canEditValues = useMemo(() => Boolean(ownerId), [ownerId]);

  function updateDraftField<K extends keyof DraftState>(key: K, value: DraftState[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  async function handleCreateSchema() {
    if (!draft.key.trim() || !draft.label.trim()) {
      setError("属性 key 和标签不能为空。");
      return;
    }

    try {
      setSavingSchema(true);
      setError(null);

      const created = await createAttributeSchema({
        scope,
        key: draft.key.trim(),
        label: draft.label.trim(),
        value_type: draft.valueType,
        description: draft.description.trim(),
        default_value: parseValue(draft.valueType, draft.defaultValue),
        enum_options: parseEnumOptions(draft.enumOptions),
        display_policy: buildDisplayPolicy(),
        access_policy: buildAccessPolicy(scope),
        mutation_policy: buildMutationPolicy(),
        influence_policy: buildInfluencePolicy(),
        projection_policy: buildProjectionPolicy(scope),
      });

      if (ownerId) {
        await upsertAttributeValue({
          schema_id: created.id,
          owner_type: ownerType,
          owner_id: ownerId,
          value: parseValue(draft.valueType, draft.defaultValue),
          source: "manual",
        });
      }

      setDraft(emptyDraft);
      await loadData();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "属性创建失败");
    } finally {
      setSavingSchema(false);
    }
  }

  async function handleSaveValue(schema: AttributeSchemaResponse, rawValue: string) {
    if (!ownerId) {
      setError("请先保存当前对象，再编辑属性值。");
      return;
    }

    try {
      setSavingValueId(schema.id);
      setError(null);
      await upsertAttributeValue({
        schema_id: schema.id,
        owner_type: ownerType,
        owner_id: ownerId,
        value: parseValue(schema.value_type, rawValue),
        source: "manual",
      });
      await loadData();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "属性值保存失败");
    } finally {
      setSavingValueId(null);
    }
  }

  async function handleDeleteSchema(schema: AttributeSchemaResponse) {
    const confirmed = window.confirm(
      `删除属性「${schema.label}」将同时清除其在所有角色 / 世界 / 会话上的已存值，且不可恢复。确定删除？`,
    );
    if (!confirmed) {
      return;
    }

    try {
      setDeletingSchemaId(schema.id);
      setError(null);
      await deleteAttributeSchema(schema.id);
      await loadData();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "属性删除失败");
    } finally {
      setDeletingSchemaId(null);
    }
  }

  return (
    <div className="editor-content">
      {/* 新建属性定义 */}
      <div className="surface-panel surface-panel--pad-lg" style={{ background: "var(--color-subtle-bg)" }}>
        <div className="editor-content" style={{ gap: 12 }}>
          <div>
            <strong style={{ fontSize: 15 }}>新增自定义属性</strong>
            <p className="text-muted" style={{ marginTop: 2, marginBottom: 0, fontSize: 13 }}>
              先创建属性定义，再为当前角色或世界保存属性值。
            </p>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="editor-field">
              <span className="editor-field-label">属性 key</span>
              <input value={draft.key} onChange={(e) => updateDraftField("key", e.target.value)} className="editor-field-input" />
            </label>
            <label className="editor-field">
              <span className="editor-field-label">显示名称</span>
              <input value={draft.label} onChange={(e) => updateDraftField("label", e.target.value)} className="editor-field-input" />
            </label>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="editor-field">
              <span className="editor-field-label">值类型</span>
              <select
                value={draft.valueType}
                onChange={(e) => updateDraftField("valueType", e.target.value as AttributeValueType)}
                className="editor-field-input editor-field-select"
              >
                <option value="text">文本</option>
                <option value="number">数值</option>
                <option value="boolean">布尔</option>
                <option value="list">列表</option>
                <option value="json">JSON</option>
              </select>
            </label>
            <label className="editor-field">
              <span className="editor-field-label">默认值</span>
              <input
                value={draft.defaultValue}
                onChange={(e) => updateDraftField("defaultValue", e.target.value)}
                className="editor-field-input"
              />
            </label>
          </div>

          <label className="editor-field">
            <span className="editor-field-label">枚举选项（每行一个，可选）</span>
            <textarea
              value={draft.enumOptions}
              onChange={(e) => updateDraftField("enumOptions", e.target.value)}
              className="editor-field-input editor-field-textarea"
            />
          </label>

          <label className="editor-field">
            <span className="editor-field-label">说明</span>
            <textarea
              value={draft.description}
              onChange={(e) => updateDraftField("description", e.target.value)}
              className="editor-field-input editor-field-textarea"
            />
          </label>

          <div className="editor-actions">
            <button
              type="button"
              onClick={() => void handleCreateSchema()}
              disabled={savingSchema}
              className="action-btn action-btn--accent"
            >
              {savingSchema ? "创建中..." : "创建属性"}
            </button>
            {!ownerId ? (
              <span className="text-muted" style={{ alignSelf: "center", fontSize: 13 }}>
                请先保存当前对象，属性值才能绑定到具体记录。
              </span>
            ) : null}
          </div>
        </div>
      </div>

      {/* 已有属性列表 */}
      <div className="editor-content">
        <div>
          <strong style={{ fontSize: 15 }}>已定义属性</strong>
          <p className="text-muted" style={{ marginTop: 2, marginBottom: 0, fontSize: 13 }}>
            当前 scope: {scope}，共 {schemas.length} 个属性。
          </p>
        </div>

        {loading ? <p className="text-muted">正在加载属性...</p> : null}
        {error ? <p className="text-error">{error}</p> : null}

        {!loading && schemas.length === 0 ? (
          <p className="text-muted">还没有可用属性，先创建一个吧。</p>
        ) : null}

        {!loading && schemas.length > 0
          ? schemas.map((schema) => {
              const currentValue = valueMap.get(schema.id) ?? stringifyValue(schema.default_value);
              return (
                <AttributeValueCard
                  key={schema.id}
                  schema={schema}
                  initialValue={currentValue}
                  disabled={!canEditValues}
                  saving={savingValueId === schema.id}
                  deleting={deletingSchemaId === schema.id}
                  onSave={handleSaveValue}
                  onDelete={handleDeleteSchema}
                />
              );
            })
          : null}
      </div>
    </div>
  );
}

function AttributeValueCard({
  schema,
  initialValue,
  disabled,
  saving,
  deleting,
  onSave,
  onDelete,
}: {
  schema: AttributeSchemaResponse;
  initialValue: string;
  disabled: boolean;
  saving: boolean;
  deleting: boolean;
  onSave: (schema: AttributeSchemaResponse, rawValue: string) => Promise<void>;
  onDelete: (schema: AttributeSchemaResponse) => Promise<void>;
}) {
  const [value, setValue] = useState(initialValue);

  useEffect(() => {
    setValue(initialValue);
  }, [initialValue]);

  return (
    <div className="surface-panel surface-panel--pad-lg" style={{ background: "var(--color-subtle-bg)" }}>
      <div className="editor-content" style={{ gap: 10 }}>
        <div>
          <strong style={{ fontSize: 14 }}>{schema.label}</strong>
          <span className="text-muted" style={{ fontSize: 12, marginLeft: 8 }}>
            {schema.key} / {schema.value_type}
          </span>
          {schema.description ? (
            <p className="text-muted" style={{ marginTop: 2, marginBottom: 0, fontSize: 13 }}>
              {schema.description}
            </p>
          ) : null}
        </div>

        <label className="editor-field">
          <span className="editor-field-label">当前值</span>
          <textarea value={value} onChange={(e) => setValue(e.target.value)} className="editor-kv-value" rows={3} />
        </label>

        <div className="editor-actions">
          <button
            type="button"
            onClick={() => void onSave(schema, value)}
            disabled={disabled || saving || deleting}
            className="action-btn action-btn--accent"
          >
            {saving ? "保存中..." : "保存属性值"}
          </button>
          <button
            type="button"
            onClick={() => void onDelete(schema)}
            disabled={saving || deleting}
            className="action-btn"
          >
            {deleting ? "删除中..." : "删除属性"}
          </button>
        </div>
      </div>
    </div>
  );
}
