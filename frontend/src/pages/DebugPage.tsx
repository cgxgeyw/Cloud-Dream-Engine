import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import { PromptCallCard, TraceBlock } from "../components/PromptTraceView";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import {
  fetchMemoryEntities,
  fetchMemoryRelations,
  fetchSessionDebug,
  fetchSessionGenerationParams,
  updateSessionGenerationParams,
  type SessionDebugResponse,
} from "../data/apiAdapter";
import {
  GenerationParamsEditor,
  GenerationParamsSummary,
} from "../components/GenerationParamsEditor";
import { showToast } from "../components/Toast";
import type {
  GenerationParams,
  MemoryEntity,
  MemoryRelation,
  SessionGenerationParamsResponse,
} from "../data/types";

const actionStyle = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: 42,
  padding: "0 16px",
  borderRadius: 14,
  fontWeight: 700,
  textDecoration: "none",
  border: "1px solid rgba(255,255,255,0.16)",
  color: "#ffffff",
  background: "rgba(255,255,255,0.08)",
  cursor: "pointer",
};

const rawBlockStyle = {
  padding: "12px 14px",
  borderRadius: 12,
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(255,255,255,0.08)",
};

const backToTopStyle = {
  position: "fixed" as const,
  right: 24,
  bottom: 24,
  zIndex: 40,
  minWidth: 112,
  minHeight: 46,
  padding: "0 16px",
  borderRadius: 999,
  border: "1px solid rgba(255,255,255,0.18)",
  background: "rgba(24,28,40,0.88)",
  color: "#ffffff",
  fontWeight: 800,
  boxShadow: "0 14px 36px rgba(0,0,0,0.28)",
  cursor: "pointer",
  backdropFilter: "blur(14px)",
};

export function DebugPage() {
  const isMobile = useIsMobile();
  const navigate = useNavigate();
  const { sessionId } = useParams();
  const [debugData, setDebugData] = useState<SessionDebugResponse | null>(null);
  const [factEntities, setFactEntities] = useState<MemoryEntity[]>([]);
  const [factRelations, setFactRelations] = useState<MemoryRelation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);

  useEffect(() => {
    if (!sessionId) {
      setError("缺少会话 ID");
      setLoading(false);
      return;
    }

    let cancelled = false;
    const stableSessionId = sessionId;

    async function loadDebug() {
      try {
        setLoading(true);
        setError(null);
        const data = await fetchSessionDebug(stableSessionId);
        if (!cancelled) {
          setDebugData(data);
        }
        try {
          const [entities, relations] = await Promise.all([
            fetchMemoryEntities(stableSessionId),
            fetchMemoryRelations(stableSessionId),
          ]);
          if (!cancelled) {
            setFactEntities(entities);
            setFactRelations(relations);
          }
        } catch {
          // 事实卡片加载失败不影响主调试数据展示
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "调试数据加载失败");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadDebug();
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    function syncBackToTopVisibility() {
      setShowBackToTop(window.scrollY > 320);
    }

    syncBackToTopVisibility();
    window.addEventListener("scroll", syncBackToTopVisibility, { passive: true });
    return () => {
      window.removeEventListener("scroll", syncBackToTopVisibility);
    };
  }, []);

  const turnIndexes = useMemo(() => {
    if (!debugData) return [];
    const indexes = new Set<number>();
    debugData.prompt_calls.forEach((item) => {
      if (typeof item.turn_index === "number") {
        indexes.add(item.turn_index);
      }
    });
    return Array.from(indexes).sort((left, right) => left - right);
  }, [debugData]);

  const entityNameById = useMemo(
    () => new Map(factEntities.map((entity) => [entity.id, entity.name])),
    [factEntities],
  );

  function handleBackToTop() {
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  return (
    <div className="debug-page-shell">
      <ScreenLayout
        title="调试面板"
        subtitle="按回合查看每次 LLM 调用到底发给了谁、发了什么、模型返回后如何处理并写入游戏。"
        toolbar={(
          <>
            <button type="button" onClick={() => navigate(`/game/${sessionId}`)} style={actionStyle}>返回游戏</button>
            {!isMobile ? (
              <button type="button" onClick={() => navigate("/")} style={actionStyle}>返回首页</button>
            ) : null}
          </>
        )}
        maxWidth={1320}
        compactHeader
      >
        {loading ? <SurfacePanel style={{ padding: 20 }}>正在加载调试数据...</SurfacePanel> : null}
        {error ? <SurfacePanel style={{ padding: 20, color: "#fca5a5" }}>加载失败：{error}</SurfacePanel> : null}

        {!loading && !error && debugData ? (
          <div style={{ display: "grid", gap: 16 }}>
            {sessionId ? <SessionGenerationParamsPanel sessionId={sessionId} /> : null}

            <SurfacePanel style={{ padding: 20 }}>
              <h3 style={{ marginTop: 0, fontSize: 22 }}>按回合查看 LLM 调用</h3>
              <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.6, marginBottom: 14 }}>
                每个回合可以整体折叠。回合内按世界主控、工具调用链、角色调用分组；每张卡片都会优先展示最终发送内容、模型返回、处理后返回和写入结果。
              </div>

              <div style={{ display: "grid", gap: 18 }}>
                {turnIndexes.length === 0 ? (
                  <div style={{ color: "rgba(255,255,255,0.62)" }}>没有捕获到新的 PromptCall 调试记录。</div>
                ) : null}

                {turnIndexes.map((turnIndex, turnListIndex) => {
                  const calls = debugData.prompt_calls.filter((item) => item.turn_index === turnIndex);
                  const directorCalls = calls.filter((item) => item.recipient_type === "director");
                  const characterCalls = calls.filter((item) => item.recipient_type === "character");
                  const toolCalls = calls.filter((item) => {
                    const modules = item.prompt_call?.modules;
                    return Array.isArray(modules) && modules.some((module) => String((module as Record<string, unknown>).name ?? "").includes("工具"));
                  });

                  return (
                    <details key={turnIndex} open={turnListIndex === turnIndexes.length - 1} style={rawBlockStyle}>
                      <summary style={{ cursor: "pointer", listStyle: "none" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap", alignItems: "center" }}>
                          <strong>{`第 ${turnIndex} 回合`}</strong>
                          <span style={{ color: "rgba(255,255,255,0.62)" }}>
                            世界主控 {directorCalls.length} 次 / 工具链 {toolCalls.length} 条 / 角色 {characterCalls.length} 次
                          </span>
                        </div>
                      </summary>

                      <div style={{ display: "grid", gap: 14, marginTop: 14 }}>
                        <section style={{ display: "grid", gap: 10 }}>
                          <h4 style={{ margin: 0, fontSize: 16 }}>世界主控调用</h4>
                          {directorCalls.length
                            ? directorCalls.map((item, index) => <PromptCallCard key={`${item.step}-${index}`} item={item} index={index} />)
                            : <div style={{ color: "rgba(255,255,255,0.62)" }}>本回合没有世界主控调用。</div>}
                        </section>

                        <section style={{ display: "grid", gap: 10 }}>
                          <h4 style={{ margin: 0, fontSize: 16 }}>工具调用链</h4>
                          {toolCalls.length
                            ? toolCalls.map((item, index) => <PromptCallCard key={`tool-${item.step}-${index}`} item={item} index={index} defaultOpen={false} />)
                            : <div style={{ color: "rgba(255,255,255,0.62)" }}>本回合没有工具资料或工具结果。</div>}
                        </section>

                        <section style={{ display: "grid", gap: 10 }}>
                          <h4 style={{ margin: 0, fontSize: 16 }}>角色调用列表</h4>
                          {characterCalls.length
                            ? characterCalls.map((item, index) => <PromptCallCard key={`${item.step}-${item.recipient_name}-${index}`} item={item} index={index} />)
                            : <div style={{ color: "rgba(255,255,255,0.62)" }}>本回合没有角色调用。</div>}
                        </section>
                      </div>
                    </details>
                  );
                })}
              </div>
            </SurfacePanel>

            <SurfacePanel style={{ padding: 20 }}>
              <h3 style={{ marginTop: 0, fontSize: 22 }}>事实卡片</h3>
              <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.6, marginBottom: 14 }}>
                NPC 每回合从对话中提取的结构化事实。关系带时效:被推翻的事实会标记失效回合并保留历史。
              </div>
              {factEntities.length === 0 && factRelations.length === 0 ? (
                <div style={{ color: "rgba(255,255,255,0.62)" }}>还没有提取到事实卡片。玩几个回合后再来看看。</div>
              ) : (
                <div style={{ display: "grid", gap: 18 }}>
                  <section style={{ display: "grid", gap: 8 }}>
                    <h4 style={{ margin: 0, fontSize: 16 }}>实体({factEntities.length})</h4>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                      {factEntities.map((entity) => (
                        <span key={entity.id} style={{ padding: "4px 12px", borderRadius: 999, background: "rgba(255,255,255,0.08)", border: "1px solid rgba(255,255,255,0.12)", fontSize: 13 }}>
                          {entity.name}{entity.entity_type ? ` · ${entity.entity_type}` : ""}
                          <span style={{ color: "rgba(255,255,255,0.5)" }}>(提及 {entity.mention_count} 次)</span>
                        </span>
                      ))}
                    </div>
                  </section>
                  <section style={{ display: "grid", gap: 8 }}>
                    <h4 style={{ margin: 0, fontSize: 16 }}>关系({factRelations.filter((relation) => relation.invalid_at_turn == null).length} 条生效 / {factRelations.length} 条总计)</h4>
                    <div style={{ display: "grid", gap: 6 }}>
                      {factRelations.map((relation) => {
                        const subjectName = entityNameById.get(relation.subject_entity_id) ?? relation.subject_entity_id;
                        const objectName = (relation.object_entity_id && entityNameById.get(relation.object_entity_id)) || relation.object_text;
                        const expired = relation.invalid_at_turn != null;
                        return (
                          <div key={relation.id} style={{ ...rawBlockStyle, opacity: expired ? 0.45 : 1, fontSize: 13 }}>
                            <strong>{subjectName}</strong>
                            <span style={{ color: "rgba(255,255,255,0.62)" }}> —{relation.predicate}→ </span>
                            <strong>{objectName}</strong>
                            <span style={{ color: "rgba(255,255,255,0.5)", marginLeft: 8 }}>
                              {expired
                                ? `第 ${relation.valid_from_turn} 回合生效,第 ${relation.invalid_at_turn} 回合失效`
                                : `第 ${relation.valid_from_turn} 回合起生效`}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </section>
                </div>
              )}
            </SurfacePanel>

            <SurfacePanel style={{ padding: 20 }}>
              <h3 style={{ marginTop: 0, fontSize: 22 }}>原始调试数据</h3>
              <TraceBlock title="会话快照" value={debugData.session} />
              <TraceBlock
                title="旧调试记录（开发期仅用于排错）"
                value={{
                  director_prompt_traces: debugData.director_prompt_traces,
                  character_prompt_traces: debugData.character_prompt_traces,
                  llm_calls: debugData.llm_calls,
                }}
              />
            </SurfacePanel>
          </div>
        ) : null}
      </ScreenLayout>

      {showBackToTop ? (
        <button type="button" onClick={handleBackToTop} style={backToTopStyle} aria-label="回到顶部">
          回到顶部
        </button>
      ) : null}
    </div>
  );
}

/**
 * 本局生成参数（第 8 项）。放在调试面板而非游戏界面里：游戏界面由世界包的 UI 文档
 * 渲染，宿主往里塞控件会改动世界包外观；调试面板是宿主自己的页面，且旁边就是
 * 「本回合实际用了什么参数、什么被过滤掉」的记录，改完立刻能对照。
 */
function SessionGenerationParamsPanel({ sessionId }: { sessionId: string }) {
  const [data, setData] = useState<SessionGenerationParamsResponse | null>(null);
  const [draft, setDraft] = useState<GenerationParams>({});
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const loaded = await fetchSessionGenerationParams(sessionId);
        if (!cancelled) {
          setData(loaded);
          setDraft(loaded.session);
        }
      } catch (error) {
        if (!cancelled) {
          setLoadError(error instanceof Error ? error.message : String(error));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  async function save() {
    setSaving(true);
    try {
      const saved = await updateSessionGenerationParams(sessionId, draft);
      setData(saved);
      // 后端会夹紧越界值，回填保存后的真实值而不是用户输入。
      setDraft(saved.session);
      showToast("本局生成参数已保存，下个回合生效", "success");
    } catch (error) {
      showToast(`保存失败：${String(error)}`, "error");
    } finally {
      setSaving(false);
    }
  }

  if (loadError) {
    return (
      <SurfacePanel style={{ padding: 20, color: "#fca5a5" }}>
        生成参数加载失败：{loadError}
      </SurfacePanel>
    );
  }
  if (!data) {
    return <SurfacePanel style={{ padding: 20 }}>正在加载生成参数...</SurfacePanel>;
  }

  // 不配会话层时实际生效的值 = 应用层叠世界层（角色路径的默认更高，取它做提示）。
  const inheritedFromUpperLayers: GenerationParams = { ...data.app, ...data.world };

  return (
    <SurfacePanel style={{ padding: 20 }}>
      <h3 style={{ marginTop: 0, fontSize: 22 }}>本局生成参数</h3>
      <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.6, marginBottom: 14 }}>
        采样参数按「应用设置 → 世界包 → 本局存档」三级覆盖，这里改的是优先级最高的本局一层。
        留空即沿用上一级。改动只影响这个存档，下一个回合生效。所连服务不支持的参数会被过滤，
        每回合的「采样/模式参数」卡片里能看到被过滤的项和原因。
      </div>

      <div style={{ display: "grid", gap: 16, gridTemplateColumns: "minmax(0, 1fr) minmax(0, 320px)" }}>
        <div>
          <GenerationParamsEditor
            value={draft}
            inherited={inheritedFromUpperLayers}
            inheritedLabel="上级值"
            disabled={saving}
            onChange={setDraft}
          />
          <div style={{ display: "flex", gap: 10, marginTop: 14 }}>
            <button type="button" style={actionStyle} disabled={saving} onClick={() => void save()}>
              {saving ? "保存中..." : "保存本局参数"}
            </button>
            <button
              type="button"
              style={actionStyle}
              disabled={saving || Object.keys(draft).length === 0}
              onClick={() => setDraft({})}
            >
              清空本局覆盖
            </button>
          </div>
        </div>

        <div style={{ display: "grid", gap: 12, alignContent: "start" }}>
          <div style={rawBlockStyle}>
            <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 13 }}>应用设置这一层</div>
            <GenerationParamsSummary params={data.app} />
          </div>
          <div style={rawBlockStyle}>
            <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 13 }}>世界包这一层</div>
            <GenerationParamsSummary params={data.world} />
          </div>
          <div style={rawBlockStyle}>
            <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 13 }}>世界主控最终生效</div>
            <GenerationParamsSummary params={data.effective_director} />
          </div>
          <div style={rawBlockStyle}>
            <div style={{ fontWeight: 700, marginBottom: 8, fontSize: 13 }}>角色最终生效</div>
            <GenerationParamsSummary params={data.effective_character} />
          </div>
        </div>
      </div>
    </SurfacePanel>
  );
}
