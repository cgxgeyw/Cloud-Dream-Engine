import { useEffect, useState } from 'react';
import { updateKnowledgeChunking, rebuildKnowledgeEmbeddings } from '../data/apiAdapter';
import type { KnowledgeBase, KnowledgeChunkStrategy } from '../data/types';

export const CHUNK_MAX_CHARS_MIN = 100;
export const CHUNK_MAX_CHARS_MAX = 8000;
export const CHUNK_OVERLAP_MAX = 2000;
export const CHUNK_DELIMITER_MAX = 16;

interface Props {
  base: KnowledgeBase;
  disabled: boolean;
  onSaved: (base: KnowledgeBase) => void;
  onError: (message: string) => void;
}

export function KnowledgeChunkingForm({ base, disabled, onSaved, onError }: Props) {
  const [strategy, setStrategy] = useState<KnowledgeChunkStrategy>(base.chunk_strategy);
  const [maxChars, setMaxChars] = useState(String(base.chunk_max_chars));
  const [overlapChars, setOverlapChars] = useState(String(base.chunk_overlap_chars));
  const [delimiter, setDelimiter] = useState(base.chunk_delimiter);
  const [saving, setSaving] = useState(false);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildMessage, setRebuildMessage] = useState('');

  useEffect(() => {
    setStrategy(base.chunk_strategy);
    setMaxChars(String(base.chunk_max_chars));
    setOverlapChars(String(base.chunk_overlap_chars));
    setDelimiter(base.chunk_delimiter);
  }, [base.id, base.chunk_strategy, base.chunk_max_chars, base.chunk_overlap_chars, base.chunk_delimiter]);

  const max = Number(maxChars);
  const overlap = Number(overlapChars);
  const validationError = strategy === 'length' || strategy === 'delimiter'
    ? !Number.isInteger(max) || max < CHUNK_MAX_CHARS_MIN || max > CHUNK_MAX_CHARS_MAX
      ? `单块字数上限需在 ${CHUNK_MAX_CHARS_MIN} 到 ${CHUNK_MAX_CHARS_MAX} 之间`
      : !Number.isInteger(overlap) || overlap < 0 || overlap > CHUNK_OVERLAP_MAX || overlap >= max
        ? `重叠字数需大于等于 0、小于单块上限，且不超过 ${CHUNK_OVERLAP_MAX}`
        : strategy === 'delimiter' && (delimiter.length === 0 || [...delimiter].length > CHUNK_DELIMITER_MAX)
          ? `分隔符不能为空，且最多 ${CHUNK_DELIMITER_MAX} 个字符`
          : ''
    : '';

  const dirty = strategy !== base.chunk_strategy
    || max !== base.chunk_max_chars
    || overlap !== base.chunk_overlap_chars
    || delimiter !== base.chunk_delimiter;

  const save = async () => {
    if (validationError) return;
    setSaving(true);
    try {
      const saved = await updateKnowledgeChunking(base.id, strategy, max, overlap, delimiter);
      onSaved(saved);
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const rebuild = async () => {
    setRebuilding(true);
    setRebuildMessage('');
    try {
      const result = await rebuildKnowledgeEmbeddings(base.id);
      setRebuildMessage(`已开始重建「${result.base_name}」的向量索引，可在右下角进度条查看进度，或切到其他页面后台继续。`);
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setRebuilding(false);
    }
  };

  return (
    <form className="knowledge-chunking" onSubmit={e => { e.preventDefault(); void save(); }}>
      <div className="knowledge-chunking-title">分块方式</div>
      <label className="editor-field">
        <span className="editor-field-label">切分依据</span>
        <select className="editor-field-input" value={strategy} onChange={e => setStrategy(e.target.value as KnowledgeChunkStrategy)} disabled={disabled || saving}>
          <option value="length">按字数（超过上限时按语义边界递归切分）</option>
          <option value="delimiter">按符号（先按符号分段，超长段再按字数切分）</option>
        </select>
      </label>
      {strategy === 'delimiter' ? (
        <label className="editor-field">
          <span className="editor-field-label">分隔符</span>
          <input className="editor-field-input" value={delimiter} onChange={e => setDelimiter(e.target.value)} placeholder="例如 ### 或 ---" maxLength={CHUNK_DELIMITER_MAX} disabled={disabled || saving} />
        </label>
      ) : null}
      <div className="knowledge-chunking-row">
        <label className="editor-field">
          <span className="editor-field-label">单块字数上限</span>
          <input className="editor-field-input" type="number" inputMode="numeric" min={CHUNK_MAX_CHARS_MIN} max={CHUNK_MAX_CHARS_MAX} step={50} value={maxChars} onChange={e => setMaxChars(e.target.value)} disabled={disabled || saving} />
        </label>
        <label className="editor-field">
          <span className="editor-field-label">相邻块重叠字数</span>
          <input className="editor-field-input" type="number" inputMode="numeric" min={0} max={CHUNK_OVERLAP_MAX} step={10} value={overlapChars} onChange={e => setOverlapChars(e.target.value)} disabled={disabled || saving} />
        </label>
      </div>
      {validationError ? <p role="alert" className="error-text">{validationError}</p> : null}
      <p className="text-muted">修改后立即对检索生效，无需重新上传资料。</p>
      <p className="text-muted">检索时会把切片和查询一起做向量比对，再与关键词命中混合排序；向量模型未启用或未下载时自动退回纯关键词匹配。</p>
      {rebuildMessage ? <p className="text-muted" role="status">{rebuildMessage}</p> : null}
      <div className="knowledge-mobile-actions">
        <button className="action-btn action-btn--accent" disabled={disabled || saving || !dirty || Boolean(validationError)}>{saving ? '保存中…' : '保存分块设置'}</button>
        <button className="action-btn" type="button" disabled={disabled || rebuilding} onClick={() => void rebuild()}>{rebuilding ? '启动中…' : '重建向量索引'}</button>
      </div>
    </form>
  );
}
