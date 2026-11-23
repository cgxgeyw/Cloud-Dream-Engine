import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Plus, ChevronRight } from 'lucide-react';
import { ScreenLayout, SurfacePanel } from '../components/ScreenLayout';
import { MobilePageBar } from '../components/MobilePageBar';
import { LibraryTabs } from '../components/LibraryTabs';
import { fetchKnowledgeBases, createKnowledgeBase, deleteKnowledgeBase, fetchKnowledgeDocuments, addKnowledgeDocument, deleteKnowledgeDocument, updateKnowledgeDocument, fetchWorlds } from '../data/apiAdapter';
import type { KnowledgeBase, KnowledgeDocument, WorldResponse } from '../data/types';
import { useIsMobile } from '../components/ResponsiveLayout';
import { KnowledgeChunkingForm } from '../components/KnowledgeChunkingForm';
import { MenuButton } from '../components/ScreenLayout';
import './KnowledgePage.css';

type MobilePane = 'list' | 'detail' | 'create';

export function KnowledgePage() {
  const navigate = useNavigate();
  const isMobile = useIsMobile();
  const [bases, setBases] = useState<KnowledgeBase[]>([]);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [draft, setDraft] = useState<KnowledgeDocument | null>(null);
  const [name, setName] = useState(''); const [owner, setOwner] = useState('');
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [mobilePane, setMobilePane] = useState<MobilePane>('list');
  const upload = useRef<HTMLInputElement>(null);
  const reload = async () => { const [b, w] = await Promise.all([fetchKnowledgeBases(), fetchWorlds()]); setBases(b); setWorlds(w); };
  useEffect(() => { void reload().catch(e => setError(String(e))).finally(() => setLoading(false)); }, []);
  useEffect(() => { let active = true; setDocuments([]); setDraft(null); if (selected) void fetchKnowledgeDocuments(selected).then((d) => { if (active) setDocuments(d); }).catch(e => { if (active) setError(String(e)); }); return () => { active = false; }; }, [selected]);
  const run = async (action: () => Promise<void>) => { setBusy(true); setError(''); try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); } };
  const base = bases.find(b => b.id === selected);
  const groups = [{ id: '', name: '公共知识库' }, ...worlds.map(w => ({ id: w.id, name: w.name }))];
  const visibleGroups = groups.filter(g => !g.id || bases.some(b => b.world_id === g.id));

  const handleMobileBack = () => {
    if (mobilePane === 'list') {
      navigate('/');
      return;
    }
    setDraft(null);
    setMobilePane('list');
  };

  const mobileTitle = mobilePane === 'create'
    ? '新建知识库'
    : mobilePane === 'detail' && base
      ? base.name
      : '图库和知识库';

  return <ScreenLayout title={isMobile ? '' : '图库和知识库'} subtitle={isMobile ? undefined : '知识资料与智能体可发送的表情贴纸都在这里维护'} compactHeader={isMobile} maxWidth={1120} toolbar={isMobile ? undefined : <button className="action-btn" onClick={() => navigate('/')}>返回首页</button>}>
    {error && <p role="alert" className="error-text">{error}</p>}
    <LibraryTabs active="knowledge" />

    {isMobile ? (
      <div className="knowledge-mobile">
        <MobilePageBar
          title={mobileTitle}
          onBack={handleMobileBack}
          actions={mobilePane === 'list' ? (
            <button type="button" className="mobile-editor-more" onClick={() => setMobilePane('create')} aria-label="新建知识库">
              <Plus size={18} />
            </button>
          ) : undefined}
        />

        {mobilePane === 'list' ? (
          loading ? <p role="status" className="empty-text">加载知识库…</p> : (
            <div className="wed-m-list">
              {visibleGroups.map(g => {
                const items = bases.filter(b => (b.world_id || '') === g.id);
                return (
                  <section key={g.id} className="wed-m-group">
                    <div className="wed-m-group-title">{g.name}</div>
                    <div className="wed-m-cards">
                      {items.map(b => (
                        <button key={b.id} type="button" className="wed-m-card" onClick={() => { setSelected(b.id); setMobilePane('detail'); }}>
                          <span className="wed-m-card-copy">
                            <span className="t">{b.name}</span>
                            <span className="s">{g.id ? '世界知识库' : '公共资料'}</span>
                          </span>
                          <ChevronRight size={16} aria-hidden="true" />
                        </button>
                      ))}
                      {items.length === 0 ? <div className="wed-empty">新建知识库后上传资料。</div> : null}
                    </div>
                  </section>
                );
              })}
            </div>
          )
        ) : null}

        {mobilePane === 'create' ? (
          <SurfacePanel className="surface-panel--pad-lg">
            <form onSubmit={e => { e.preventDefault(); void run(async () => { const b = await createKnowledgeBase(name.trim(), '', owner || null); await reload(); setSelected(b.id); setName(''); setMobilePane('detail'); }); }} className="knowledge-mobile-form">
              <label className="editor-field"><span className="editor-field-label">名称</span><input className="editor-field-input" placeholder="输入知识库名称" value={name} onChange={e => setName(e.target.value)} required maxLength={160} disabled={busy} /></label>
              <label className="editor-field"><span className="editor-field-label">归属</span><select className="editor-field-input" value={owner} onChange={e => setOwner(e.target.value)} disabled={busy}><option value="">公共知识库</option>{worlds.map(w => <option key={w.id} value={w.id}>{w.name}</option>)}</select></label>
              <button className="action-btn action-btn--accent" disabled={busy || !name.trim()}>新建知识库</button>
            </form>
          </SurfacePanel>
        ) : null}

        {mobilePane === 'detail' && base ? (
          <div className="wed-pane">
            <div className="wed-pane-head">
              <div>
                <h2 className="wed-pane-title">{base.name}</h2>
                <div className="wed-pane-sub">{base.world_id ? '世界知识库' : '公共知识库'} · {documents.length} 份资料</div>
              </div>
            </div>

            {!base.world_id ? (
              <div className="wed-block">
                <div className="knowledge-mobile-block-title">使用范围</div>
                <div className="empty-text">公共知识库默认不对任何世界开放。请到「世界编辑器 → 世界主控 → 权限」勾选允许使用的公共知识库，再到「角色编辑 → 记忆」为该角色单独收窄范围。</div>
              </div>
            ) : null}

            <div className="wed-block">
              <KnowledgeChunkingForm base={base} disabled={busy} onSaved={saved => { setBases(prev => prev.map(b => b.id === saved.id ? saved : b)); }} onError={setError} />
            </div>

            <div className="wed-block">
              <div className="knowledge-mobile-actions">
                <button className="action-btn action-btn--accent" disabled={busy} onClick={() => upload.current?.click()}>{busy ? '处理中…' : '上传文件'}</button>
              </div>
              <div className="wed-pane-sub">支持 TXT、Markdown、CSV、JSON、YAML、XML、DOCX、文字 PDF，每个文件最多 20MB。</div>
              <input ref={upload} type="file" hidden multiple accept=".txt,.md,.markdown,.csv,.json,.jsonl,.yaml,.yml,.xml,.docx,.pdf" onChange={e => { const files = Array.from(e.target.files || []); e.target.value = ''; const id = base.id; void run(async () => { try { for (const f of files) { if (f.size > 20 * 1024 * 1024) throw new Error(`${f.name} 超过 20MB`); await addKnowledgeDocument(id, f); } } finally { setDocuments(await fetchKnowledgeDocuments(id)); } }); }} />
              {documents.length === 0 ? <div className="empty-text">还没有资料。上传后可以检查、修改提取的正文。</div> : (
                <div className="wed-m-cards knowledge-doc-list">
                  {documents.map(d => (
                    <button key={d.id} type="button" className="wed-m-card" disabled={busy} onClick={() => setDraft({ ...d })}>
                      <span className="wed-m-card-copy">
                        <span className="t">{d.title}</span>
                        <span className="s">{d.content.length.toLocaleString()} 字符</span>
                      </span>
                      <ChevronRight size={16} aria-hidden="true" />
                    </button>
                  ))}
                </div>
              )}
            </div>

            {draft ? (
              <div className="wed-block">
                <div className="knowledge-mobile-block-title">编辑资料</div>
                <form onSubmit={e => { e.preventDefault(); void run(async () => { await updateKnowledgeDocument(draft.id, draft.title, draft.content); setDocuments(await fetchKnowledgeDocuments(base.id)); }); }}>
                  <label className="editor-field"><span className="editor-field-label">标题</span><input className="editor-field-input" value={draft.title} onChange={e => setDraft({ ...draft, title: e.target.value })} required disabled={busy} /></label>
                  <label className="editor-field"><span className="editor-field-label">正文</span><textarea className="editor-field-input editor-field-textarea" value={draft.content} onChange={e => setDraft({ ...draft, content: e.target.value })} required rows={12} disabled={busy} /></label>
                  <p className="text-muted">来源：{draft.source_filename}</p>
                  <div className="knowledge-mobile-actions">
                    <button className="action-btn action-btn--accent" disabled={busy}>保存正文</button>
                    <button className="action-btn action-btn--danger" type="button" disabled={busy} onClick={() => { if (window.confirm(`删除资料“${draft.title}”？`)) void run(async () => { await deleteKnowledgeDocument(draft.id); setDraft(null); setDocuments(await fetchKnowledgeDocuments(base.id)); }); }}>删除资料</button>
                  </div>
                </form>
              </div>
            ) : null}

            <div className="wed-block">
              <button className="action-btn action-btn--danger" disabled={busy} onClick={() => { if (window.confirm(`删除“${base.name}”及全部资料？`)) void run(async () => { await deleteKnowledgeBase(base.id); setSelected(''); await reload(); setMobilePane('list'); }); }}>删除知识库</button>
            </div>
          </div>
        ) : null}
      </div>
    ) : (
      <div className="settings-page-shell knowledge-page">
      <SurfacePanel className="surface-panel--pad-lg">
        <h2 className="settings-section-title">新建知识库</h2>
        <form onSubmit={e => { e.preventDefault(); void run(async () => { const b = await createKnowledgeBase(name.trim(), '', owner || null); await reload(); setSelected(b.id); setName(''); }); }} className="knowledge-create-form">
          <label className="field-label"><span className="field-label-text">名称</span><input className="field-input" placeholder="输入知识库名称" value={name} onChange={e => setName(e.target.value)} required maxLength={160} disabled={busy} /></label>
          <label className="field-label"><span className="field-label-text">归属</span><select className="field-input" value={owner} onChange={e => setOwner(e.target.value)} disabled={busy}><option value="">公共知识库</option>{worlds.map(w => <option key={w.id} value={w.id}>{w.name}</option>)}</select></label>
          <button className="action-btn action-btn--accent" disabled={busy || !name.trim()}>新建知识库</button>
        </form>
      </SurfacePanel>
        {loading ? <p role="status">加载知识库…</p> : <div className="knowledge-columns">
          <SurfacePanel className="surface-panel--pad-lg">
          <nav aria-label="知识库列表" className="knowledge-list">
            {visibleGroups.map(g => <section key={g.id} style={{ marginBottom: 24 }}>
              <h2 className="settings-section-title">{g.name}</h2>
              {bases.filter(b => (b.world_id || '') === g.id).map(b => <div key={b.id} className={`knowledge-list-item${selected === b.id ? ' knowledge-list-item--selected' : ''}`}><MenuButton title={b.name} onClick={() => { if (!busy) setSelected(b.id); }} /></div>)}
              {!bases.some(b => (b.world_id || '') === g.id) && <p style={{ color: 'var(--color-muted)' }}>新建知识库后上传资料。</p>}
            </section>)}
          </nav>
          </SurfacePanel>
          <SurfacePanel className="surface-panel--pad-lg">
          <section className="knowledge-detail">
            {!base ? <p>选择知识库，查看或上传资料。</p> : <>
              <h2 className="settings-section-title">{base.name}</h2>
              {!base.world_id && <div className="text-muted" style={{ marginBottom: 16 }}>公共知识库默认不对任何世界开放。允许哪些世界使用请到「世界编辑器 → 世界主控 → 权限」勾选，再在「角色编辑 → 记忆」按角色收窄。</div>}
              <KnowledgeChunkingForm base={base} disabled={busy} onSaved={saved => { setBases(prev => prev.map(b => b.id === saved.id ? saved : b)); }} onError={setError} />
              <p style={{ color: 'var(--color-muted)' }}>支持 TXT、Markdown、CSV、JSON、YAML、XML、DOCX、文字 PDF，每个文件最多 20MB。</p>
              <div className="settings-form-actions">
                <button className="action-btn action-btn--accent" disabled={busy} onClick={() => upload.current?.click()}>{busy ? '处理中…' : '上传文件'}</button>
                <button className="action-btn action-btn--danger" disabled={busy} onClick={() => { if (window.confirm(`删除“${base.name}”及全部资料？`)) void run(async () => { await deleteKnowledgeBase(base.id); setSelected(''); await reload(); }); }}>删除知识库</button>
              </div>
              <input ref={upload} type="file" hidden multiple accept=".txt,.md,.markdown,.csv,.json,.jsonl,.yaml,.yml,.xml,.docx,.pdf" onChange={e => { const files = Array.from(e.target.files || []); e.target.value = ''; const id = base.id; void run(async () => { try { for (const f of files) { if (f.size > 20 * 1024 * 1024) throw new Error(`${f.name} 超过 20MB`); await addKnowledgeDocument(id, f); } } finally { setDocuments(await fetchKnowledgeDocuments(id)); } }); }} />
              {documents.length === 0 ? <p className="empty-text">还没有资料。上传后可以检查、修改提取的正文。</p> : <ul className="knowledge-documents">{documents.map(d => <li key={d.id}><button className="action-btn" disabled={busy} onClick={() => setDraft({ ...d })}>{d.title}</button><span className="text-muted">{d.content.length.toLocaleString()} 字符</span></li>)}</ul>}
              {draft && <form onSubmit={e => { e.preventDefault(); void run(async () => { await updateKnowledgeDocument(draft.id, draft.title, draft.content); setDocuments(await fetchKnowledgeDocuments(base.id)); }); }}>
                <div className="grid grid--gap-sm">
                <label className="field-label"><span className="field-label-text">标题</span><input className="field-input" value={draft.title} onChange={e => setDraft({ ...draft, title: e.target.value })} required disabled={busy} /></label>
                <label className="field-label"><span className="field-label-text">正文</span><textarea className="field-input" value={draft.content} onChange={e => setDraft({ ...draft, content: e.target.value })} required rows={16} disabled={busy} /></label>
                </div>
                <p className="text-muted">来源：{draft.source_filename}</p>
                <div className="settings-form-actions"><button className="action-btn action-btn--accent" disabled={busy}>保存正文</button> <button className="action-btn action-btn--danger" type="button" disabled={busy} onClick={() => { if (window.confirm(`删除资料“${draft.title}”？`)) void run(async () => { await deleteKnowledgeDocument(draft.id); setDraft(null); setDocuments(await fetchKnowledgeDocuments(base.id)); }); }}>删除资料</button></div>
              </form>}
            </>}
          </section>
          </SurfacePanel>
        </div>}
      </div>
    )}
  </ScreenLayout>;
}
