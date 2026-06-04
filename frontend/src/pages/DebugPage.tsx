import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { PromptCallCard, TraceBlock } from "../components/PromptTraceView";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { fetchSessionDebug, type SessionDebugResponse } from "../data/apiAdapter";

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

interface DebugPageProps {
  isMobile?: boolean;
}

export function DebugPage({ isMobile = false }: DebugPageProps) {
  const navigate = useNavigate();
  const { sessionId } = useParams();
  const [debugData, setDebugData] = useState<SessionDebugResponse | null>(null);
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

  function handleBackToTop() {
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  return (
    <>
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
    </>
  );
}
