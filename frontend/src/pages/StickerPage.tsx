import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ImagePlus, Trash2, Upload } from "lucide-react";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { LibraryTabs } from "../components/LibraryTabs";
import {
  assetUrl,
  createSticker,
  createStickerPack,
  deleteSticker,
  deleteStickerPack,
  fetchStickerPacks,
  fetchStickers,
  fetchWorlds,
  getStickerWorldBindings,
  reorderStickers,
  setStickerWorldBinding,
  updateSticker,
  updateStickerPack,
  uploadFile,
} from "../data/apiAdapter";
import type { StickerItem, StickerPack, WorldResponse } from "../data/types";
import "./StickerPage.css";

const MAX_STICKER_BYTES = 2 * 1024 * 1024;

function uploadedAssetPath(uploaded: { asset_path?: string; relative_path?: string }) {
  const path = uploaded.asset_path || uploaded.relative_path || "";
  if (!path) throw new Error("上传失败：没有返回资源路径");
  return path;
}

function labelFromFilename(name: string) {
  return name.replace(/\.[^.]+$/, "").trim().slice(0, 40);
}

function keyFromFilename(name: string) {
  const base = name
    .replace(/\.[^.]+$/, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 24);
  return base || "sticker";
}

function uniqueKey(base: string, used: Set<string>) {
  if (!used.has(base)) return base;
  let index = 2;
  while (used.has(`${base}-${index}`)) index += 1;
  return `${base}-${index}`;
}

export function StickerPage() {
  const navigate = useNavigate();
  const [packs, setPacks] = useState<StickerPack[]>([]);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [selected, setSelected] = useState("");
  const [stickers, setStickers] = useState<StickerItem[]>([]);
  const [bindings, setBindings] = useState<string[]>([]);
  const [draft, setDraft] = useState<StickerItem | null>(null);
  const [newName, setNewName] = useState("");
  const [newOwner, setNewOwner] = useState("");
  const [packName, setPackName] = useState("");
  const [packDescription, setPackDescription] = useState("");
  const [packEnabled, setPackEnabled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [dragId, setDragId] = useState("");
  const upload = useRef<HTMLInputElement>(null);
  const replaceUpload = useRef<HTMLInputElement>(null);

  const reload = async () => {
    const [nextPacks, nextWorlds] = await Promise.all([fetchStickerPacks(), fetchWorlds()]);
    setPacks(nextPacks);
    setWorlds(nextWorlds);
    setSelected((current) => {
      if (current && nextPacks.some((pack) => pack.id === current)) return current;
      return nextPacks[0]?.id ?? "";
    });
  };

  useEffect(() => {
    void reload()
      .catch((reason) => setError(String(reason)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    let active = true;
    setStickers([]);
    setBindings([]);
    setDraft(null);
    if (!selected) return () => { active = false; };
    void Promise.all([fetchStickers(selected), getStickerWorldBindings(selected)])
      .then(([nextStickers, nextBindings]) => {
        if (!active) return;
        setStickers(nextStickers);
        setBindings(nextBindings);
      })
      .catch((reason) => {
        if (active) setError(String(reason));
      });
    return () => { active = false; };
  }, [selected]);

  const pack = packs.find((item) => item.id === selected) ?? null;
  useEffect(() => {
    setPackName(pack?.name ?? "");
    setPackDescription(pack?.description ?? "");
    setPackEnabled(pack?.enabled ?? true);
    setDraft(null);
  }, [pack?.id, pack?.name, pack?.description, pack?.enabled]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const uploadStickers = (files: File[]) => {
    if (!pack || files.length === 0) return;
    void run(async () => {
      const used = new Set(stickers.map((sticker) => sticker.key));
      for (const file of files) {
        if (file.size > MAX_STICKER_BYTES) {
          throw new Error(`${file.name} 超过 2MB，请先压缩`);
        }
        const uploaded = await uploadFile(file);
        const key = uniqueKey(keyFromFilename(file.name), used);
        used.add(key);
        await createSticker(pack.id, key, labelFromFilename(file.name), "", uploadedAssetPath(uploaded));
      }
      setStickers(await fetchStickers(pack.id));
      await reload();
    });
  };

  const moveSticker = (index: number, offset: number) => {
    if (!pack) return;
    const next = [...stickers];
    const target = index + offset;
    if (target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target], next[index]];
    setStickers(next);
    void run(async () => {
      await reorderStickers(pack.id, next.map((sticker) => sticker.id));
    });
  };

  const dropSticker = (targetId: string) => {
    if (!pack || !dragId || dragId === targetId) return;
    const next = [...stickers];
    const from = next.findIndex((sticker) => sticker.id === dragId);
    const to = next.findIndex((sticker) => sticker.id === targetId);
    if (from < 0 || to < 0) return;
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    setStickers(next);
    setDragId("");
    void run(async () => {
      await reorderStickers(pack.id, next.map((sticker) => sticker.id));
    });
  };

  const publicWorlds = worlds.filter((world) => world.id);
  const groups = [
    { id: "", name: "公共图库" },
    ...publicWorlds.map((world) => ({ id: world.id, name: world.name })),
  ];
  const visibleGroups = groups.filter((group) => !group.id || packs.some((item) => item.world_id === group.id));

  return (
    <ScreenLayout
      title="图库和知识库"
      subtitle="图库：管理智能体可查找和发送的图片，改动会在下一回合生效。"
      maxWidth={1120}
      toolbar={<button className="action-btn" onClick={() => navigate("/")}>返回首页</button>}
    >
      {error ? <p role="alert" className="error-text">{error}</p> : null}
      <LibraryTabs active="stickers" />

      <div className="sticker-page">
        <SurfacePanel className="surface-panel--pad-lg">
          <h2 className="settings-section-title">新建图库</h2>
          <form
            className="sticker-create-form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(async () => {
                const created = await createStickerPack(newName.trim(), "", newOwner || null);
                setNewName("");
                await reload();
                setSelected(created.id);
              });
            }}
          >
            <label className="field-label">
              <span className="field-label-text">名称</span>
              <input
                className="field-input"
                value={newName}
                onChange={(event) => setNewName(event.target.value)}
                placeholder="例如：大肥鱼表情包库"
                maxLength={60}
                disabled={busy}
                required
              />
            </label>
            <label className="field-label">
              <span className="field-label-text">归属</span>
              <select
                className="field-input"
                value={newOwner}
                onChange={(event) => setNewOwner(event.target.value)}
                disabled={busy}
              >
                <option value="">公共图库</option>
                {publicWorlds.map((world) => (
                  <option key={world.id} value={world.id}>{world.name}</option>
                ))}
              </select>
            </label>
            <button className="action-btn action-btn--accent" disabled={busy || !newName.trim()}>新建</button>
          </form>

          {loading ? <p role="status" className="empty-text">加载图库中…</p> : (
            <div className="sticker-pack-list">
              {visibleGroups.map((group) => {
                const items = packs.filter((item) => (item.world_id || "") === group.id);
                if (items.length === 0 && group.id) return null;
                return (
                  <section key={group.id} className="sticker-pack-group">
                    <div className="sticker-pack-group-title">{group.name}</div>
                    {items.length === 0 ? <p className="empty-text">还没有图库。</p> : items.map((item) => (
                      <button
                        key={item.id}
                        type="button"
                        className={`sticker-pack-item${selected === item.id ? " sticker-pack-item--selected" : ""}`}
                        onClick={() => { if (!busy) setSelected(item.id); }}
                      >
                        <span className="sticker-pack-name">{item.name}</span>
                        <span className="sticker-pack-meta">
                          {item.sticker_count} 张{item.enabled ? "" : " · 已停用"}
                        </span>
                      </button>
                    ))}
                  </section>
                );
              })}
            </div>
          )}
        </SurfacePanel>

        <SurfacePanel className="surface-panel--pad-lg">
          {!pack ? <p className="empty-text">选择或新建一个图库，然后上传图片。</p> : (
            <div className="sticker-detail">
              <form
                className="sticker-pack-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void run(async () => {
                    await updateStickerPack(pack.id, packName.trim(), packDescription.trim(), packEnabled);
                    await reload();
                  });
                }}
              >
                <label className="field-label">
                  <span className="field-label-text">名称</span>
                  <input className="field-input" value={packName} onChange={(event) => setPackName(event.target.value)} maxLength={60} disabled={busy} required />
                </label>
                <label className="field-label">
                  <span className="field-label-text">说明</span>
                  <input className="field-input" value={packDescription} onChange={(event) => setPackDescription(event.target.value)} placeholder="给这个图库一句话描述" disabled={busy} />
                </label>
                <label className="sticker-toggle">
                  <input type="checkbox" checked={packEnabled} onChange={(event) => setPackEnabled(event.target.checked)} disabled={busy} />
                  <span>启用</span>
                </label>
                <button className="action-btn" disabled={busy || !packName.trim()}>保存图库</button>
                <button
                  type="button"
                  className="action-btn action-btn--danger"
                  disabled={busy}
                  onClick={() => {
                    if (!window.confirm(`删除“${pack.name}”及其中全部图片？图片文件会保留，历史消息不受影响。`)) return;
                    void run(async () => {
                      await deleteStickerPack(pack.id);
                      setSelected("");
                      await reload();
                    });
                  }}
                >
                  删除图库
                </button>
              </form>

              {!pack.world_id ? (
                <div className="sticker-bind-block">
                  <div className="sticker-block-title">允许使用此图库的世界</div>
                  {publicWorlds.length === 0 ? <p className="empty-text">世界创建后可在这里关联。</p> : (
                    <div className="sticker-bind-list">
                      {publicWorlds.map((world) => (
                        <label key={world.id} className="sticker-bind-item">
                          <input
                            type="checkbox"
                            checked={bindings.includes(world.id)}
                            disabled={busy}
                            onChange={(event) => {
                              const enabled = event.target.checked;
                              void run(async () => {
                                await setStickerWorldBinding(pack.id, world.id, enabled);
                                setBindings(await getStickerWorldBindings(pack.id));
                              });
                            }}
                          />
                          <span>{world.name}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>
              ) : (
                <p className="text-muted">世界专属图库只对所属世界生效，无需绑定。</p>
              )}

              <div className="sticker-upload-row">
                <button className="action-btn action-btn--accent" disabled={busy} onClick={() => upload.current?.click()}>
                  <Upload size={14} /> 上传图片
                </button>
                <span className="text-muted">支持 png / jpg / webp / gif，单张 ≤2MB；文件名会变成 key，可再修改。</span>
                <input
                  ref={upload}
                  type="file"
                  hidden
                  multiple
                  accept="image/png,image/jpeg,image/webp,image/gif"
                  onChange={(event) => {
                    const files = Array.from(event.target.files || []);
                    event.target.value = "";
                    uploadStickers(files);
                  }}
                />
              </div>

              {stickers.length === 0 ? <p className="empty-text">还没有图片。上传后模型会先查找、再发送合适的图片。</p> : (
                <div className="sticker-grid">
                  {stickers.map((sticker, index) => (
                    <article
                      key={sticker.id}
                      className={`sticker-card${sticker.enabled ? "" : " sticker-card--disabled"}`}
                      draggable
                      onDragStart={() => setDragId(sticker.id)}
                      onDragOver={(event) => event.preventDefault()}
                      onDrop={() => dropSticker(sticker.id)}
                    >
                      <img src={assetUrl(sticker.asset_path)} alt={sticker.label} loading="lazy" />
                      <div className="sticker-card-body">
                        <code className="sticker-key">{sticker.key}</code>
                        <strong className="sticker-label">{sticker.label}</strong>
                        <span className="sticker-desc">{sticker.description || "无描述"}</span>
                      </div>
                      <div className="sticker-card-actions">
                        <button type="button" className="sticker-icon-btn" disabled={busy || index === 0} onClick={() => moveSticker(index, -1)} aria-label="上移">↑</button>
                        <button type="button" className="sticker-icon-btn" disabled={busy || index === stickers.length - 1} onClick={() => moveSticker(index, 1)} aria-label="下移">↓</button>
                        <button type="button" className="sticker-icon-btn" disabled={busy} onClick={() => setDraft({ ...sticker })}>编辑</button>
                        <button
                          type="button"
                          className="sticker-icon-btn sticker-icon-btn--danger"
                          disabled={busy}
                          onClick={() => {
                            if (!window.confirm(`删除图片“${sticker.label}”？历史消息里的图片会保留。`)) return;
                            void run(async () => {
                              await deleteSticker(sticker.id);
                              setStickers(await fetchStickers(pack.id));
                              await reload();
                            });
                          }}
                          aria-label="删除"
                        >
                          <Trash2 size={13} />
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              )}

              {draft ? (
                <form
                  className="sticker-edit-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void run(async () => {
                      await updateSticker(
                        draft.id,
                        draft.key,
                        draft.label,
                        draft.description,
                        draft.asset_path,
                        draft.enabled,
                      );
                      setDraft(null);
                      setStickers(await fetchStickers(pack.id));
                      await reload();
                    });
                  }}
                >
                  <div className="sticker-block-title">编辑图片</div>
                  <div className="sticker-edit-preview">
                    <img src={assetUrl(draft.asset_path)} alt={draft.label} />
                    <button type="button" className="action-btn" disabled={busy} onClick={() => replaceUpload.current?.click()}>
                      <ImagePlus size={14} /> 换图
                    </button>
                    <input
                      ref={replaceUpload}
                      type="file"
                      hidden
                      accept="image/png,image/jpeg,image/webp,image/gif"
                      onChange={(event) => {
                        const file = event.target.files?.[0];
                        event.target.value = "";
                        if (!file) return;
                        void run(async () => {
                          if (file.size > MAX_STICKER_BYTES) throw new Error(`${file.name} 超过 2MB，请先压缩`);
                          const uploaded = await uploadFile(file);
                          setDraft((current) => current ? { ...current, asset_path: uploadedAssetPath(uploaded) } : current);
                        });
                      }}
                    />
                  </div>
                  <label className="field-label">
                    <span className="field-label-text">key（模型调用时使用）</span>
                    <input
                      className="field-input"
                      value={draft.key}
                      onChange={(event) => setDraft({ ...draft, key: event.target.value })}
                      maxLength={32}
                      pattern="[a-z0-9_-]+"
                      disabled={busy}
                      required
                    />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">名称</span>
                    <input className="field-input" value={draft.label} onChange={(event) => setDraft({ ...draft, label: event.target.value })} maxLength={40} disabled={busy} />
                  </label>
                  <label className="field-label">
                    <span className="field-label-text">描述（模型据此判断何时发送）</span>
                    <input className="field-input" value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} placeholder="例如：看到饭时兴奋地端碗" disabled={busy} />
                  </label>
                  <label className="sticker-toggle">
                    <input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} disabled={busy} />
                    <span>启用这张图片</span>
                  </label>
                  <div className="sticker-edit-actions">
                    <button className="action-btn action-btn--accent" disabled={busy}>保存图片</button>
                    <button type="button" className="action-btn" disabled={busy} onClick={() => setDraft(null)}>取消</button>
                  </div>
                </form>
              ) : null}
            </div>
          )}
        </SurfacePanel>
      </div>
    </ScreenLayout>
  );
}
