import { useEffect, useState } from "react";
import {
  createAttributeSchema,
  deleteAttributeSchema,
  fetchAttributeSchemas,
  fetchAttributeValues,
  upsertAttributeValue,
  updateAttributeSchema,
  type AttributeSchemaResponse,
  type AttributeValueType,
} from "../data/apiAdapter";
import { GripVertical, Plus } from "lucide-react";
import "./AttributePanel.css";

type Scope = "world" | "character";
type OwnerType = "world" | "character";

type AttributePanelProps = {
  scope: Scope;
  ownerType: OwnerType;
  ownerId?: string;
  worldId?: string;
  onSchemaChange?: () => void;
  hideTitle?: boolean;
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

const valueTypeLabels: Record<AttributeValueType, string> = {
  text: "文本", number: "数值", boolean: "布尔", list: "列表", json: "JSON",
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

export function AttributePanel({ scope, ownerType, ownerId, worldId, onSchemaChange, hideTitle = false }: AttributePanelProps) {
  const [schemas, setSchemas] = useState<AttributeSchemaResponse[]>([]);
  const [valueMap, setValueMap] = useState<Map<string, string>>(new Map());
  const [draft, setDraft] = useState<DraftState>(emptyDraft);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [savingSchema, setSavingSchema] = useState(false);
  const [savingValueId, setSavingValueId] = useState<string | null>(null);
  const [deletingSchemaId, setDeletingSchemaId] = useState<string | null>(null);
  const [addingSchema, setAddingSchema] = useState(false);
  const [draggedSchemaId, setDraggedSchemaId] = useState<string | null>(null);
  const [selectedSchemaId, setSelectedSchemaId] = useState<string | null>(null);
  const [valueDrafts, setValueDrafts] = useState<Record<string, string>>({});
  const [sorting, setSorting] = useState(false);

  async function loadData(options?: { isCancelled?: () => boolean }) {
    const isCancelled = options?.isCancelled ?? (() => false);
    try {
      setLoading(true);
      setError(null);

      const [schemaData, valueData] = await Promise.all([
        fetchAttributeSchemas(scope, worldId),
        ownerId ? fetchAttributeValues({ ownerType, ownerId }) : Promise.resolve([]),
      ]);

      // ownerId 快速切换或组件卸载时丢弃过期结果，避免旧请求覆盖新数据 / 卸载后 setState。
      if (isCancelled()) {
        return;
      }
      const applicableSchemas = schemaData.filter((schema) => schemaAppliesToOwner(schema, scope, worldId));
      setSchemas(applicableSchemas);
      setSelectedSchemaId((current) => applicableSchemas.some((schema) => schema.id === current)
        ? current : applicableSchemas[0]?.id ?? null);
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
    setValueDrafts({});
    setAddingSchema(false);
    setDraft(emptyDraft);
    void loadData({ isCancelled: () => cancelled });
    return () => {
      cancelled = true;
    };
  }, [scope, ownerType, ownerId, worldId]);

  const selectedSchema = schemas.find((schema) => schema.id === selectedSchemaId);

  function updateDraftField<K extends keyof DraftState>(key: K, value: DraftState[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  async function handleCreateSchema() {
    if (!worldId) {
      setError("请先保存世界，再定义该世界的属性。");
      return;
    }
    if (!draft.key.trim() || !draft.label.trim()) {
      setError("属性 key 和标签不能为空。");
      return;
    }

    try {
      setSavingSchema(true);
      setError(null);

      const created = await createAttributeSchema({
        scope,
        world_id: worldId ?? null,
        sort_order: schemas.length,
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
      setSelectedSchemaId(created.id);
      setAddingSchema(false);
      onSchemaChange?.();
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
      setValueDrafts((current) => {
        if (current[schema.id] !== rawValue) return current;
        const next = { ...current };
        delete next[schema.id];
        return next;
      });
      onSchemaChange?.();
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
      onSchemaChange?.();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "属性删除失败");
    } finally {
      setDeletingSchemaId(null);
    }
  }

  async function reorderSchemas(draggedId: string, targetId: string) {
    if (draggedId === targetId || sorting) return;
    const reordered = [...schemas];
    const draggedIndex = reordered.findIndex((item) => item.id === draggedId);
    if (draggedIndex < 0) return;
    const [dragged] = reordered.splice(draggedIndex, 1);
    const targetIndex = reordered.findIndex((item) => item.id === targetId);
    if (!dragged || targetIndex < 0) return;
    reordered.splice(targetIndex, 0, dragged);
    setSchemas(reordered);
    setSorting(true);
    try {
      await Promise.all(reordered.map((schema, sort_order) => updateAttributeSchema(schema.id, {
        scope: schema.scope,
        world_id: schema.world_id,
        sort_order,
        key: schema.key,
        label: schema.label,
        value_type: schema.value_type,
        description: schema.description,
        default_value: schema.default_value,
        enum_options: schema.enum_options,
        display_policy: schema.display_policy,
        access_policy: schema.access_policy,
        mutation_policy: schema.mutation_policy,
        influence_policy: schema.influence_policy,
        projection_policy: schema.projection_policy,
      })));
      onSchemaChange?.();
    } catch (saveError) {
      await loadData();
      setError(saveError instanceof Error ? saveError.message : "属性顺序保存失败");
    } finally {
      setSorting(false);
    }
  }

  return (
    <div className="attribute-panel" aria-busy={loading}>
      {error ? <p className="text-error" role="alert">{error}</p> : null}
      <div className="attribute-panel__layout">
      <nav className="attribute-panel__sidebar" aria-label={scope === "world" ? "世界属性" : "角色共享属性"}>
      <div className="attribute-panel__list-heading">
        <div>
          {hideTitle ? null : <strong>{scope === "world" ? "世界属性" : "角色共享属性"}</strong>}
          <div className="text-muted" style={{ fontSize: 12 }}>{schemas.length} 个属性 · 拖动排序</div>
        </div>
        <button type="button" className="action-btn" onClick={() => setAddingSchema(true)} disabled={!worldId || savingSchema} title="新增属性" aria-label="新增属性" aria-pressed={addingSchema}>
          <Plus size={16} />
        </button>
      </div>
      <div className="attribute-panel__list">
        {loading && schemas.length === 0 ? <p className="attribute-panel__list-empty text-muted" role="status">正在加载属性...</p> : null}
        {!loading && schemas.length === 0 ? <p className="attribute-panel__list-empty text-muted">还没有属性，点击 ＋ 新增。</p> : null}
        {schemas.map((schema) => (
          <div key={schema.id} className="attribute-panel__item" data-selected={!addingSchema && schema.id === selectedSchemaId}
            data-dragging={draggedSchemaId === schema.id}
            onDragOver={(event) => { if (!sorting) event.preventDefault(); }}
            onDrop={(event) => { event.preventDefault(); const dragged = event.dataTransfer.getData("text/plain") || draggedSchemaId; if (dragged) void reorderSchemas(dragged, schema.id); setDraggedSchemaId(null); }}>
            <span className="attribute-panel__grip" draggable={!sorting}
              title="拖动排序" aria-hidden="true"
              onDragStart={(event) => { setDraggedSchemaId(schema.id); event.dataTransfer.setData("text/plain", schema.id); event.dataTransfer.effectAllowed = "move"; }}
              onDragEnd={() => setDraggedSchemaId(null)}><GripVertical size={15} /></span>
            <button type="button" className="attribute-panel__select" aria-pressed={!addingSchema && schema.id === selectedSchemaId}
              onClick={() => { setSelectedSchemaId(schema.id); setAddingSchema(false); }}>
              <span className="attribute-panel__item-name">{schema.label}</span>
              <span className="attribute-panel__item-meta"><span>{schema.key}</span><span>{valueTypeLabels[schema.value_type]}</span></span>
              {Object.prototype.hasOwnProperty.call(valueDrafts, schema.id) ? <span className="attribute-panel__unsaved">未保存</span> : null}
            </button>
          </div>
        ))}
      </div>
      </nav>
      <div className="attribute-panel__detail">
      {/* 新建属性定义 */}
      {addingSchema ? <div>
        <div className="editor-content" style={{ gap: 12 }}>
          <strong style={{ fontSize: 15 }}>{scope === "world" ? "新增世界属性" : "新增角色共享属性"}</strong>
          <p className="text-muted" style={{ marginTop: 2, marginBottom: 0, fontSize: 13 }}>
            {scope === "character" ? "定义绑定当前世界，所有角色使用相同字段，各自保存属性值。" : "定义绑定当前世界，并可由世界主控更新。"}
          </p>

          <div className="attribute-panel__fields">
            <label className="editor-field">
              <span className="editor-field-label">属性 key</span>
              <input autoFocus value={draft.key} onChange={(e) => updateDraftField("key", e.target.value)} className="editor-field-input" />
            </label>
            <label className="editor-field">
              <span className="editor-field-label">显示名称</span>
              <input value={draft.label} onChange={(e) => updateDraftField("label", e.target.value)} className="editor-field-input" />
            </label>
          </div>

          <div className="attribute-panel__fields">
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
            <button type="button" className="action-btn" disabled={savingSchema} onClick={() => setAddingSchema(false)}>取消</button>
          </div>
        </div>
      </div> : null}

      {!addingSchema && selectedSchema ? (
        <AttributeDetails
          schema={selectedSchema}
          value={valueDrafts[selectedSchema.id] ?? valueMap.get(selectedSchema.id) ?? stringifyValue(selectedSchema.default_value)}
          disabled={!ownerId}
          saving={savingValueId === selectedSchema.id}
          deleting={deletingSchemaId === selectedSchema.id}
          onChange={(value) => setValueDrafts((current) => ({ ...current, [selectedSchema.id]: value }))}
          onSave={handleSaveValue}
          onDelete={handleDeleteSchema}
        />
      ) : null}
      {!addingSchema && !selectedSchema ? (
        <div className="attribute-panel__empty">
          <strong>{loading ? "正在加载属性..." : "添加第一个自定义属性"}</strong>
          {!loading ? <p className="text-muted">{worldId ? "点击左侧 ＋ 定义属性，之后在这里查看详情和编辑值。" : "请先保存世界，再添加自定义属性。"}</p> : null}
        </div>
      ) : null}
      </div>
      </div>
    </div>
  );
}

function AttributeDetails({
  schema,
  value,
  disabled,
  saving,
  deleting,
  onChange,
  onSave,
  onDelete,
}: {
  schema: AttributeSchemaResponse;
  value: string;
  disabled: boolean;
  saving: boolean;
  deleting: boolean;
  onChange: (value: string) => void;
  onSave: (schema: AttributeSchemaResponse, rawValue: string) => Promise<void>;
  onDelete: (schema: AttributeSchemaResponse) => Promise<void>;
}) {
  return (
      <div className="editor-content" style={{ gap: 10 }}>
        <div className="attribute-panel__detail-heading">
          <strong>{schema.label}</strong>
          <div className="text-muted" style={{ fontSize: 12, marginTop: 4 }}>{schema.key} · {valueTypeLabels[schema.value_type]}</div>
          {schema.description ? (
            <p className="text-muted" style={{ marginTop: 2, marginBottom: 0, fontSize: 13 }}>
              {schema.description}
            </p>
          ) : null}
        </div>

        <label className="editor-field">
          <span className="editor-field-label">默认值</span>
          <pre className="attribute-panel__default">{stringifyValue(schema.default_value) || "（空）"}</pre>
        </label>

        {!disabled ? <label className="editor-field">
          <span className="editor-field-label">当前值</span>
          <textarea value={value} onChange={(e) => onChange(e.target.value)} className="editor-kv-value" rows={7} disabled={deleting} />
        </label> : <p className="text-muted" style={{ fontSize: 13 }}>每个角色独立保存属性值；请保存角色后，在角色编辑页设置。未设置时使用默认值。</p>}

        <div className="editor-actions">
          {!disabled ? <button
            type="button"
            onClick={() => void onSave(schema, value)}
            disabled={disabled || saving || deleting}
            className="action-btn action-btn--accent"
          >
            {saving ? "保存中..." : "保存属性值"}
          </button> : null}
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
  );
}
