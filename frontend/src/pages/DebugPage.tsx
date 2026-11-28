import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useIsMobile } from "../components/ResponsiveLayout";
import { TraceBlock } from "../components/PromptTraceView";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import {
  fetchMemoryEntities,
  fetchMemoryRelations,
  fetchSessionDebug,
  fetchSessionGenerationParams,
  queryOagDebug,
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
  OagActor,
  OagQueryRequest,
  OagQueryResponse,
  OagTraverseStep,
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

  const currentTurn = useMemo(() => {
    const journalTurn = Number(debugData?.recovery_state?.latest_turn_index ?? 0);
    return Math.max(0, journalTurn, turnIndexes[turnIndexes.length - 1] ?? 0);
  }, [debugData, turnIndexes]);

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
            <SurfacePanel style={{ padding: 20 }}>
              <h3 style={{ marginTop: 0, fontSize: 22 }}>每回合输入与输出原文</h3>
              <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.6, marginBottom: 14 }}>
                按回合列出世界主控和每个角色的完整输入、模型原始输出。只展示原文，不做加工。
              </div>

              <div style={{ display: "grid", gap: 18 }}>
                {turnIndexes.length === 0 ? (
                  <div style={{ color: "rgba(255,255,255,0.62)" }}>没有捕获到新的调用记录。</div>
                ) : null}

                {turnIndexes.map((turnIndex, turnListIndex) => {
                  const calls = debugData.prompt_calls.filter((item) => item.turn_index === turnIndex);
                  const directorCalls = calls.filter((item) => item.recipient_type === "director");
                  const characterCalls = calls.filter((item) => item.recipient_type === "character");

                  return (
                    <details key={turnIndex} open={turnListIndex === turnIndexes.length - 1} style={rawBlockStyle}>
                      <summary style={{ cursor: "pointer", listStyle: "none" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap", alignItems: "center" }}>
                          <strong>{`第 ${turnIndex} 回合`}</strong>
                          <span style={{ color: "rgba(255,255,255,0.62)" }}>
                            世界主控 {directorCalls.length} 次 / 角色 {characterCalls.length} 次
                          </span>
                        </div>
                      </summary>

                      <div style={{ display: "grid", gap: 14, marginTop: 14 }}>
                        <RawCallSection
                          title="世界主控"
                          calls={directorCalls}
                          fallbackName="世界主控"
                          emptyText="本回合没有世界主控调用。"
                        />
                        <RawCallSection
                          title="角色"
                          calls={characterCalls}
                          fallbackName="角色"
                          emptyText="本回合没有角色调用。"
                        />
                      </div>
                    </details>
                  );
                })}
              </div>
            </SurfacePanel>

            {sessionId ? <SessionGenerationParamsPanel sessionId={sessionId} /> : null}

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

            {sessionId ? <OagQueryPanel sessionId={sessionId} currentTurn={currentTurn} /> : null}

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

function OagQueryPanel({ sessionId, currentTurn }: { sessionId: string; currentTurn: number }) {
  const [op, setOp] = useState<OagQueryRequest["op"]>("find");
  const [actorKind, setActorKind] = useState<OagActor["kind"]>("director");
  const [actorId, setActorId] = useState("");
  const [asOfTurn, setAsOfTurn] = useState("");
  const [maxNodes, setMaxNodes] = useState("200");
  const [entityName, setEntityName] = useState("");
  const [entityId, setEntityId] = useState("");
  const [objectType, setObjectType] = useState("");
  const [attributeKey, setAttributeKey] = useState("");
  const [attributeOp, setAttributeOp] = useState("eq");
  const [attributeValue, setAttributeValue] = useState("");
  const [linkType, setLinkType] = useState("");
  const [linkDirection, setLinkDirection] = useState<"out" | "in" | "both">("out");
  const [linkTarget, setLinkTarget] = useState("");
  const [traverseSteps, setTraverseSteps] = useState<OagTraverseStep[]>([
    { link_type: "", direction: "out" },
  ]);
  const [metric, setMetric] = useState<NonNullable<OagQueryRequest["metric"]>>("count");
  const [metricAttribute, setMetricAttribute] = useState("");
  const [groupBy, setGroupBy] = useState<"" | NonNullable<OagQueryRequest["group_by"]>>("");
  const [groupAttribute, setGroupAttribute] = useState("");
  const [knowledgeTargetKind, setKnowledgeTargetKind] = useState<NonNullable<OagQueryRequest["target"]>["kind"]>("object");
  const [knowledgeTargetId, setKnowledgeTargetId] = useState("");
  const [knowledgeActorId, setKnowledgeActorId] = useState("");
  const [mentioned, setMentioned] = useState("");
  const [fromTurn, setFromTurn] = useState("");
  const [toTurn, setToTurn] = useState("");
  const [eventType, setEventType] = useState("");
  const [participant, setParticipant] = useState("");
  const [location, setLocation] = useState("");
  const [includeHistory, setIncludeHistory] = useState(false);
  const [includeRetired, setIncludeRetired] = useState(false);
  const [limit, setLimit] = useState("20");
  const [response, setResponse] = useState<OagQueryResponse | null>(null);
  const [queryError, setQueryError] = useState<string | null>(null);
  const [querying, setQuerying] = useState(false);

  function optionalNumber(value: string, label: string, maximum?: number): number | undefined {
    const trimmed = value.trim();
    if (!trimmed) return undefined;
    const parsed = Number(trimmed);
    if (!Number.isInteger(parsed) || parsed < 0 || (maximum !== undefined && parsed > maximum)) {
      throw new Error(`${label}必须是 0 到 ${maximum ?? "合理范围"} 之间的整数`);
    }
    return parsed;
  }

  function requiredNumber(value: string, label: string, maximum: number): number {
    const parsed = optionalNumber(value, label, maximum);
    if (parsed === undefined || parsed === 0) throw new Error(`${label}必须是 1 到 ${maximum} 之间的整数`);
    return parsed;
  }

  function entityReference(id: string, name: string): OagQueryRequest["entity"] {
    const stableId = id.trim();
    const displayName = name.trim();
    if (!stableId && !displayName) return undefined;
    return {
      ...(stableId ? { id: stableId } : {}),
      ...(displayName ? { name: displayName } : {}),
    };
  }

  async function runQuery(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setQuerying(true);
    setQueryError(null);
    setResponse(null);
    try {
      const actor: OagActor = actorKind === "director" || actorKind === "author"
        ? { kind: actorKind }
        : { kind: actorKind, id: actorId.trim() };
      if ((actor.kind === "object" || actor.kind === "player") && !actor.id) {
        throw new Error("角色或玩家视角需要填写稳定 ID");
      }
      const reference = entityReference(entityId, entityName);
      if ((op === "resolve" || op === "entity" || op === "traverse") && !reference) {
        throw new Error("解析实体、查看实体和多跳查询需要填写名称或稳定 ID");
      }
      const request: OagQueryRequest = {
        op,
        as_of_turn: optionalNumber(asOfTurn, "查询回合", currentTurn),
        include_history: includeHistory,
        include_retired: includeRetired,
        limit: requiredNumber(limit, "结果数量", 20),
      };
      const usesObjectFilters = op === "find" || op === "aggregate" || op === "absence";
      if ((op === "resolve" || op === "entity" || op === "traverse") && reference) request.entity = reference;
      if (usesObjectFilters && objectType.trim()) request.object_type = objectType.trim();
      if (usesObjectFilters && attributeKey.trim()) {
        let value: unknown = null;
        if (attributeValue.trim()) {
          try {
            value = JSON.parse(attributeValue);
          } catch {
            throw new Error("属性值需要是合法 JSON，例如 12、true 或 \"文本\"");
          }
        }
        request.attribute = { key: attributeKey.trim(), op: attributeOp, value };
      }
      if (usesObjectFilters && linkType.trim()) {
        request.link = {
          link_type: linkType.trim(),
          direction: linkDirection,
          ...(linkTarget.trim() ? { to: { name: linkTarget.trim() } } : {}),
        };
      }
      if (op === "traverse") {
        if (traverseSteps.length === 0 || traverseSteps.length > 3 || traverseSteps.some((step) => !step.link_type.trim())) {
          throw new Error("多跳查询需要填写 1 到 3 个关系步骤");
        }
        request.steps = traverseSteps.map((step) => ({ ...step, link_type: step.link_type.trim() }));
      }
      if (op === "aggregate") {
        request.metric = metric;
        if (["sum", "avg", "min", "max"].includes(metric) && !metricAttribute.trim()) {
          throw new Error("数值聚合需要填写数值属性键");
        }
        if (metricAttribute.trim()) request.metric_attribute = metricAttribute.trim();
        if (groupBy) request.group_by = groupBy;
        if (groupBy === "attribute") {
          if (!groupAttribute.trim()) throw new Error("按属性分组需要填写分组属性键");
          request.group_attribute = groupAttribute.trim();
        }
        if (groupBy === "link_target" && !request.link) {
          throw new Error("按关系目标分组需要先填写关系条件");
        }
      }
      if (op === "absence") {
        if (!objectType.trim()) throw new Error("否定查询需要填写对象类型键");
        if (!request.attribute && !request.link) throw new Error("否定查询至少需要一个属性或关系条件");
      }
      if (op === "knows") {
        if (!knowledgeTargetId.trim()) throw new Error("知识查询需要填写目标稳定 ID");
        request.target = { kind: knowledgeTargetKind, id: knowledgeTargetId.trim() };
        if (knowledgeActorId.trim()) request.knowledge_actor = { id: knowledgeActorId.trim() };
      }
      if (op === "timeline") {
        request.from_turn = optionalNumber(fromTurn, "起始回合");
        request.to_turn = optionalNumber(toTurn, "结束回合");
        if (eventType.trim()) request.event_type = eventType.trim();
        if (participant.trim()) request.participant = { name: participant.trim() };
        if (location.trim()) request.location = { name: location.trim() };
        if (mentioned.trim()) request.mentioned = mentioned.trim();
      }
      const result = await queryOagDebug(
        sessionId,
        actor,
        request,
        optionalNumber(asOfTurn, "查询回合", currentTurn),
        requiredNumber(maxNodes, "节点预算", 200),
      );
      setResponse(result);
    } catch (error) {
      setQueryError(error instanceof Error ? error.message : String(error));
    } finally {
      setQuerying(false);
    }
  }

  const fieldStyle = {
    display: "grid",
    gap: 6,
    minWidth: 0,
    color: "rgba(255,255,255,0.72)",
    fontSize: 12,
    fontWeight: 650,
  };
  const inputStyle = {
    width: "100%",
    minHeight: 40,
    boxSizing: "border-box" as const,
    padding: "9px 11px",
    borderRadius: 10,
    border: "1px solid rgba(255,255,255,0.16)",
    background: "rgba(255,255,255,0.06)",
    color: "#f8fafc",
    font: "inherit",
    fontSize: 13,
  };

  return (
    <SurfacePanel style={{ padding: 20 }}>
      <h3 style={{ marginTop: 0, fontSize: 22 }}>OAG 可见子图查询</h3>
      <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.7, marginBottom: 16, maxWidth: "75ch" }}>
        先按视角、回合和角色知识裁剪信息，再执行查询。切换导演与角色视角，可以检查秘密、耳语和未来信息是否被正确隐藏。
      </div>
      <form onSubmit={(event) => void runQuery(event)} style={{ display: "grid", gap: 16 }}>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))", gap: 12 }}>
          <label style={fieldStyle}>
            查询操作
            <select value={op} onChange={(event) => setOp(event.target.value as OagQueryRequest["op"])} style={inputStyle}>
              <option value="find">查找对象</option>
              <option value="resolve">解析实体</option>
              <option value="entity">查看实体详情</option>
              <option value="traverse">多跳关系查询</option>
              <option value="aggregate">聚合统计</option>
              <option value="absence">否定条件查询</option>
              <option value="knows">查询谁知道</option>
              <option value="timeline">查询事件时间线</option>
            </select>
          </label>
          <label style={fieldStyle}>
            查询视角
            <select value={actorKind} onChange={(event) => setActorKind(event.target.value as OagActor["kind"])} style={inputStyle}>
              <option value="director">导演，可见全图</option>
              <option value="author">作者，可见全图</option>
              <option value="object">具体角色</option>
              <option value="player">玩家角色</option>
            </select>
          </label>
          {(actorKind === "object" || actorKind === "player") ? (
            <label style={fieldStyle}>
              角色稳定 ID
              <input value={actorId} onChange={(event) => setActorId(event.target.value)} placeholder="支持旧角色 ID" style={inputStyle} />
            </label>
          ) : null}
          <label style={fieldStyle}>
            查询回合
            <input type="number" min={0} max={currentTurn} value={asOfTurn} onChange={(event) => setAsOfTurn(event.target.value)} placeholder={`当前，第 ${currentTurn} 回合`} style={inputStyle} />
          </label>
          <label style={fieldStyle}>
            节点预算
            <input type="number" min={1} max={200} value={maxNodes} onChange={(event) => setMaxNodes(event.target.value)} style={inputStyle} />
          </label>
        </div>

        {op === "resolve" || op === "entity" || op === "traverse" ? (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))", gap: 12 }}>
            <label style={fieldStyle}>
              实体名称
              <input value={entityName} onChange={(event) => setEntityName(event.target.value)} placeholder="名称或别名" style={inputStyle} />
            </label>
            <label style={fieldStyle}>
              实体稳定 ID
              <input value={entityId} onChange={(event) => setEntityId(event.target.value)} placeholder="优先于名称" style={inputStyle} />
            </label>
          </div>
        ) : null}

        {op === "find" || op === "aggregate" || op === "absence" ? (
          <details style={rawBlockStyle}>
            <summary style={{ cursor: "pointer", fontSize: 13, fontWeight: 700 }}>
              {op === "absence" ? "至少一个否定条件" : "对象、属性与关系过滤"}
            </summary>
            <div style={{ display: "grid", gap: 12, marginTop: 12 }}>
              <label style={fieldStyle}>
                对象类型键
                <input value={objectType} onChange={(event) => setObjectType(event.target.value)} placeholder="由世界包声明" style={inputStyle} />
              </label>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 10 }}>
                <label style={fieldStyle}>
                  属性键
                  <input value={attributeKey} onChange={(event) => setAttributeKey(event.target.value)} style={inputStyle} />
                </label>
                <label style={fieldStyle}>
                  条件
                  <select value={attributeOp} onChange={(event) => setAttributeOp(event.target.value)} style={inputStyle}>
                    {["eq", "ne", "gt", "gte", "lt", "lte", "in", "contains"].map((value) => <option key={value} value={value}>{value}</option>)}
                  </select>
                </label>
                <label style={fieldStyle}>
                  属性值，JSON
                  <input value={attributeValue} onChange={(event) => setAttributeValue(event.target.value)} placeholder='12 或 "文本"' style={inputStyle} />
                </label>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 10 }}>
                <label style={fieldStyle}>
                  关系类型键
                  <input value={linkType} onChange={(event) => setLinkType(event.target.value)} style={inputStyle} />
                </label>
                <label style={fieldStyle}>
                  方向
                  <select value={linkDirection} onChange={(event) => setLinkDirection(event.target.value as typeof linkDirection)} style={inputStyle}>
                    <option value="out">向外</option>
                    <option value="in">向内</option>
                    <option value="both">双向</option>
                  </select>
                </label>
                <label style={fieldStyle}>
                  目标名称
                  <input value={linkTarget} onChange={(event) => setLinkTarget(event.target.value)} placeholder="可选" style={inputStyle} />
                </label>
              </div>
            </div>
          </details>
        ) : null}

        {op === "traverse" ? (
          <section style={{ ...rawBlockStyle, display: "grid", gap: 12 }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap", alignItems: "center" }}>
              <strong style={{ fontSize: 13 }}>多跳步骤，最多 3 步</strong>
              <button type="button" disabled={traverseSteps.length >= 3} onClick={() => setTraverseSteps((steps) => [...steps, { link_type: "", direction: "out" }])} style={{ ...actionStyle, minHeight: 34, padding: "0 12px", fontSize: 12, opacity: traverseSteps.length >= 3 ? 0.45 : 1 }}>
                添加一步
              </button>
            </div>
            {traverseSteps.map((step, index) => (
              <div key={index} style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) 120px auto", gap: 10, alignItems: "end" }}>
                <label style={fieldStyle}>
                  第 {index + 1} 步关系类型键
                  <input value={step.link_type} onChange={(event) => setTraverseSteps((steps) => steps.map((item, stepIndex) => stepIndex === index ? { ...item, link_type: event.target.value } : item))} placeholder="由世界包声明" style={inputStyle} />
                </label>
                <label style={fieldStyle}>
                  方向
                  <select value={step.direction} onChange={(event) => setTraverseSteps((steps) => steps.map((item, stepIndex) => stepIndex === index ? { ...item, direction: event.target.value as OagTraverseStep["direction"] } : item))} style={inputStyle}>
                    <option value="out">向外</option>
                    <option value="in">向内</option>
                    <option value="both">双向</option>
                  </select>
                </label>
                <button type="button" disabled={traverseSteps.length <= 1} onClick={() => setTraverseSteps((steps) => steps.filter((_, stepIndex) => stepIndex !== index))} style={{ ...actionStyle, minHeight: 40, padding: "0 12px", fontSize: 12, opacity: traverseSteps.length <= 1 ? 0.45 : 1 }}>
                  删除
                </button>
              </div>
            ))}
            <div style={{ color: "rgba(255,255,255,0.52)", fontSize: 12 }}>只沿当前视角可见的关系前进；反向关系必须由世界包声明，路径自动避免环路。</div>
          </section>
        ) : null}

        {op === "aggregate" ? (
          <section style={{ ...rawBlockStyle, display: "grid", gap: 12 }}>
            <strong style={{ fontSize: 13 }}>聚合与分组</strong>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12 }}>
              <label style={fieldStyle}>
                统计指标
                <select value={metric} onChange={(event) => setMetric(event.target.value as typeof metric)} style={inputStyle}>
                  <option value="count">对象计数</option>
                  <option value="count_distinct">去重计数</option>
                  <option value="sum">求和</option>
                  <option value="avg">平均值</option>
                  <option value="min">最小值</option>
                  <option value="max">最大值</option>
                </select>
              </label>
              <label style={fieldStyle}>
                数值属性键
                <input value={metricAttribute} onChange={(event) => setMetricAttribute(event.target.value)} placeholder="求和、平均、最小、最大必填" style={inputStyle} />
              </label>
              <label style={fieldStyle}>
                分组方式
                <select value={groupBy} onChange={(event) => setGroupBy(event.target.value as typeof groupBy)} style={inputStyle}>
                  <option value="">不分组</option>
                  <option value="object_type">按对象类型</option>
                  <option value="attribute">按属性值</option>
                  <option value="link_target">按关系目标</option>
                </select>
              </label>
              {groupBy === "attribute" ? (
                <label style={fieldStyle}>
                  分组属性键
                  <input value={groupAttribute} onChange={(event) => setGroupAttribute(event.target.value)} style={inputStyle} />
                </label>
              ) : null}
            </div>
            <div style={{ color: "rgba(255,255,255,0.52)", fontSize: 12 }}>先应用上方对象过滤，再统计可见对象；隐藏对象不会进入计数。</div>
          </section>
        ) : null}

        {op === "knows" ? (
          <section style={{ ...rawBlockStyle, display: "grid", gap: 12 }}>
            <strong style={{ fontSize: 13 }}>知识目标</strong>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12 }}>
              <label style={fieldStyle}>
                目标类型
                <select value={knowledgeTargetKind} onChange={(event) => setKnowledgeTargetKind(event.target.value as typeof knowledgeTargetKind)} style={inputStyle}>
                  <option value="object">对象</option>
                  <option value="attribute">属性</option>
                  <option value="link">关系</option>
                  <option value="event">事件</option>
                  <option value="memory">记忆</option>
                </select>
              </label>
              <label style={fieldStyle}>
                目标稳定 ID
                <input value={knowledgeTargetId} onChange={(event) => setKnowledgeTargetId(event.target.value)} style={inputStyle} />
              </label>
              <label style={fieldStyle}>
                知识主体稳定 ID，可选
                <input value={knowledgeActorId} onChange={(event) => setKnowledgeActorId(event.target.value)} placeholder="角色视角只能填自己" style={inputStyle} />
              </label>
            </div>
            <div style={{ color: "rgba(255,255,255,0.52)", fontSize: 12 }}>角色只能查询自己的知情记录；导演可查询全图，但结果仍受目标可见性限制。</div>
          </section>
        ) : null}

        {op === "timeline" ? (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12 }}>
            <label style={fieldStyle}>起始回合<input type="number" min={0} value={fromTurn} onChange={(event) => setFromTurn(event.target.value)} style={inputStyle} /></label>
            <label style={fieldStyle}>结束回合<input type="number" min={0} value={toTurn} onChange={(event) => setToTurn(event.target.value)} style={inputStyle} /></label>
            <label style={fieldStyle}>事件类型<input value={eventType} onChange={(event) => setEventType(event.target.value)} style={inputStyle} /></label>
            <label style={fieldStyle}>参与者名称<input value={participant} onChange={(event) => setParticipant(event.target.value)} style={inputStyle} /></label>
            <label style={fieldStyle}>地点名称<input value={location} onChange={(event) => setLocation(event.target.value)} style={inputStyle} /></label>
            <label style={fieldStyle}>正文提及文本<input value={mentioned} onChange={(event) => setMentioned(event.target.value)} placeholder="仅搜索可见正文" style={inputStyle} /></label>
          </div>
        ) : null}

        <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "center", fontSize: 13 }}>
          <label style={{ display: "inline-flex", gap: 7, alignItems: "center" }}>
            <input type="checkbox" checked={includeHistory} onChange={(event) => setIncludeHistory(event.target.checked)} />
            包含已过期事实
          </label>
          <label style={{ display: "inline-flex", gap: 7, alignItems: "center" }}>
            <input type="checkbox" checked={includeRetired} disabled={actorKind !== "director" && actorKind !== "author"} onChange={(event) => setIncludeRetired(event.target.checked)} />
            包含已退休对象
          </label>
          <label style={{ ...fieldStyle, display: "inline-flex", alignItems: "center", gap: 8 }}>
            结果数量
            <input type="number" min={1} max={20} value={limit} onChange={(event) => setLimit(event.target.value)} style={{ ...inputStyle, width: 82 }} />
          </label>
          <button type="submit" disabled={querying} style={{ ...actionStyle, opacity: querying ? 0.6 : 1, cursor: querying ? "wait" : "pointer" }}>
            {querying ? "正在查询" : "执行查询"}
          </button>
        </div>
      </form>

      {queryError ? <div role="alert" style={{ marginTop: 14, color: "#fca5a5", fontSize: 13 }}>{queryError}</div> : null}
      {response ? (
        <div aria-live="polite" style={{ display: "grid", gap: 10, marginTop: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap", fontSize: 13 }}>
            <strong>查到 {response.count} 条{response.truncated ? "，结果已截断" : ""}</strong>
            <span style={{ color: "rgba(255,255,255,0.52)", fontFamily: "ui-monospace, monospace" }}>追踪 {response.trace_id}</span>
          </div>
          <div style={{ color: "rgba(255,255,255,0.62)", fontSize: 13 }}>{response.hint}</div>
          <RawTextBlock label="查询结果" text={rawText(response.results)} />
        </div>
      ) : null}
    </SurfacePanel>
  );
}

function rawText(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === undefined || value === null) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function RawTextBlock({ label, text }: { label: string; text: string }) {
  return (
    <div style={{ display: "grid", gap: 6 }}>
      <div style={{ color: "rgba(255,255,255,0.5)", fontSize: 12, fontWeight: 700, letterSpacing: 0.4 }}>{label}</div>
      <pre
        style={{
          margin: 0,
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
          fontSize: 12.5,
          lineHeight: 1.6,
          color: "rgba(255,255,255,0.88)",
          background: "rgba(0,0,0,0.24)",
          border: "1px solid rgba(255,255,255,0.08)",
          borderRadius: 8,
          padding: 14,
          maxHeight: 420,
          overflow: "auto",
        }}
      >
        {text || "（未记录）"}
      </pre>
    </div>
  );
}

function RawCallSection({
  title,
  calls,
  fallbackName,
  emptyText,
}: {
  title: string;
  calls: SessionDebugResponse["prompt_calls"];
  fallbackName: string;
  emptyText: string;
}) {
  return (
    <section style={{ display: "grid", gap: 10 }}>
      <h4 style={{ margin: 0, fontSize: 16 }}>{title}</h4>
      {calls.length === 0 ? (
        <div style={{ color: "rgba(255,255,255,0.62)" }}>{emptyText}</div>
      ) : (
        calls.map((item, index) => {
          const call = item.prompt_call ?? {};
          const rawName = String(item.recipient_name ?? call.recipient_name ?? "").trim();
          const name = item.recipient_type === "director" && (!rawName || rawName === "world_director")
            ? "世界主控"
            : rawName || fallbackName;
          const stage = String(call.stage ?? item.stage ?? item.step ?? "").trim();

          return (
            <div key={`${item.step ?? "call"}-${index}`} style={{ display: "grid", gap: 10 }}>
              <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
                <strong>{`${index + 1}. ${name}`}</strong>
                <span style={{ color: "rgba(255,255,255,0.55)", fontSize: 12 }}>{stage}</span>
              </div>
              <RawTextBlock label="输入" text={rawText(call.final_sent_content)} />
              <RawTextBlock label="输出" text={rawText(call.raw_model_return)} />
            </div>
          );
        })
      )}
    </section>
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

  // 不配会话层时实际生效的值 = 应用层（角色路径的默认更高，取它做提示）。
  const inheritedFromUpperLayers: GenerationParams = { ...data.app };

  return (
    <SurfacePanel style={{ padding: 20 }}>
      <h3 style={{ marginTop: 0, fontSize: 22 }}>本局生成参数</h3>
      <div style={{ color: "rgba(255,255,255,0.70)", fontSize: 13, lineHeight: 1.6, marginBottom: 14 }}>
        采样参数按「应用设置 → 本局存档」两级覆盖，这里改的是优先级最高的本局一层。
        留空即沿用上一级。改动只影响这个存档，下一个回合生效。所连服务不支持的参数会被过滤，
        每回合的「采样/模式参数」卡片里能看到被过滤的项和原因。
      </div>

      <div className="debug-params-grid" style={{ display: "grid", gap: 16 }}>
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
