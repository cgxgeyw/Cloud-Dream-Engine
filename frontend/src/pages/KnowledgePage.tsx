import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ScreenLayout, SurfacePanel } from '../components/ScreenLayout';
import { fetchKnowledgeBases, createKnowledgeBase, deleteKnowledgeBase, fetchKnowledgeDocuments, addKnowledgeDocument, deleteKnowledgeDocument, updateKnowledgeDocument, fetchWorlds, getKnowledgeBindings, setKnowledgeBinding } from '../data/apiAdapter';
import type { KnowledgeBase, KnowledgeDocument, WorldResponse } from '../data/types';
import { useIsMobile } from '../components/ResponsiveLayout';
import { MenuButton } from '../components/ScreenLayout';
import './KnowledgePage.css';

export function KnowledgePage() {
  const navigate = useNavigate();
  const isMobile = useIsMobile();
  const [bases, setBases] = useState<KnowledgeBase[]>([]);
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [bindings, setBindings] = useState<string[]>([]);
  const [draft, setDraft] = useState<KnowledgeDocument | null>(null);
  const [name, setName] = useState(''); const [owner, setOwner] = useState('');
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const upload = useRef<HTMLInputElement>(null);
  const reload = async () => { const [b, w] = await Promise.all([fetchKnowledgeBases(), fetchWorlds()]); setBases(b); setWorlds(w); };
  useEffect(() => { void reload().catch(e => setError(String(e))).finally(() => setLoading(false)); }, []);
  useEffect(() => { let active = true; setDocuments([]); setBindings([]); setDraft(null); if (selected) void Promise.all([fetchKnowledgeDocuments(selected), getKnowledgeBindings(selected)]).then(([d, b]) => { if (active) { setDocuments(d); setBindings(b); } }).catch(e => { if (active) setError(String(e)); }); return () => { active = false; }; }, [selected]);
  const run = async (action: () => Promise<void>) => { setBusy(true); setError(''); try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); } };
  const base = bases.find(b => b.id === selected);
  const groups = [{ id: '', name: '公共知识库' }, ...worlds.map(w => ({ id: w.id, name: w.name }))];
  return <ScreenLayout title="知识库" subtitle="管理公共资料和世界自带资料" compactHeader={isMobile} maxWidth={1120} toolbar={<button className="action-btn" onClick={() => navigate('/')}>返回首页</button>}>
    {error && <p role="alert" className="error-text">{error}</p>}
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
          {groups.filter(g => !g.id || bases.some(b => b.world_id === g.id)).map(g => <section key={g.id} style={{ marginBottom: 24 }}>
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
            {!base.world_id && <fieldset style={{ marginBottom: 16 }}><legend>允许使用此知识库的世界</legend>{worlds.length === 0 ? <p>导入世界后可在这里关联。</p> : worlds.map(w => <label key={w.id} style={{ display: 'block', padding: 6 }}><input type="checkbox" checked={bindings.includes(w.id)} disabled={busy} onChange={e => { const enabled = e.target.checked; void run(async () => { await setKnowledgeBinding(base.id, w.id, enabled); setBindings(await getKnowledgeBindings(base.id)); }); }} /> {w.name}</label>)}</fieldset>}
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
  </ScreenLayout>;
}
