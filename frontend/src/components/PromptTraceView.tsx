import type { ReactNode } from "react";

export type PromptCallRecordItem = {
  turn_index?: number;
  step?: string;
  recipient_type?: string;
  recipient_name?: string;
  prompt_call?: Record<string, unknown>;
};

export type DirectorPromptTraceItem = {
  turn_index?: number;
  step?: string;
  prompt_trace: Record<string, unknown>;
};

export type CharacterPromptTraceItem = {
  turn_index?: number;
  step?: string;
  speaker?: string | null;
  prompt_trace: Record<string, unknown>;
};

export type LlmCallTraceItem = {
  turn_index: number;
  step: string;
  speaker: string;
  provider?: string;
  model_id?: string;
  status?: string;
  latency_ms?: number;
  input_payload: Record<string, unknown>;
  output_payload: unknown;
  raw_input_payload?: Record<string, unknown>;
  raw_output_payload?: unknown;
};

const blockStyle = {
  padding: "12px 14px",
  borderRadius: 8,
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(255,255,255,0.08)",
};

const badgeStyle = {
  display: "inline-flex",
  alignItems: "center",
  minHeight: 24,
  padding: "0 9px",
  borderRadius: 999,
  background: "rgba(255,255,255,0.08)",
  color: "rgba(255,255,255,0.78)",
  fontSize: 12,
  fontWeight: 700,
};

function formatJsonBlock(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value ?? "");
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function summarizeValue(value: unknown): string {
  if (Array.isArray(value)) return `${value.length} 项`;
  if (value && typeof value === "object") return `${Object.keys(value).length} 个字段`;
  const text = String(value ?? "").trim();
  return text.length > 60 ? `${text.slice(0, 60)}...` : text || "空";
}

function resolveCall(item: PromptCallRecordItem | Record<string, unknown>) {
  const wrapper = asRecord(item);
  const call = asRecord(wrapper.prompt_call ?? item);
  return { wrapper, call };
}

function recipientLabel(type: string) {
  if (type === "director") return "世界主控";
  if (type === "character") return "角色";
  if (type === "tool") return "工具";
  return "LLM";
}

function resolveRecipientName(rawName: unknown, rawType: unknown): string {
  const name = String(rawName ?? "").trim();
  const type = String(rawType ?? "").trim();
  if (type === "director" && (!name || name === "world_director")) {
    return "世界主控";
  }
  if (type === "character" && !name) {
    return "角色";
  }
  return name || "未知接收方";
}

function textValue(value: unknown): string {
  if (Array.isArray(value)) return value.length ? value.map(String).join(" / ") : "空";
  if (value && typeof value === "object") return formatJsonBlock(value);
  return String(value ?? "").trim() || "空";
}

function isDirectorCall(wrapper: Record<string, unknown>, call: Record<string, unknown>) {
  return String(wrapper.recipient_type ?? call.recipient_type ?? "") === "director";
}

function DirectorFieldGuide({ writtenResult }: { writtenResult: unknown }) {
  const result = asRecord(writtenResult);
  const rows = [
    {
      key: "next_location",
      label: "地点 / 地图节点",
      value: result.next_location,
      desc: "偏向“角色当前在哪个地图节点”。规则、触发器、记忆位置通常看它。",
    },
    {
      key: "next_scene_name",
      label: "场景名",
      value: result.next_scene_name,
      desc: "偏向“当前演出场景叫什么”。不切大地图时，它经常和地点相同。",
    },
    {
      key: "next_scene_background_hint",
      label: "场景背景描述",
      value: result.next_scene_background_hint,
      desc: "会写入场景运行时，用于背景图匹配/生成、场景气氛和调试查看。",
    },
    {
      key: "scene_visible_characters",
      label: "在场可见角色",
      value: result.scene_visible_characters,
      desc: "表示这个场景里有哪些 NPC 可见/可被后续选择，不包含玩家当前操控角色。",
    },
    {
      key: "planned_speakers",
      label: "本回合发言角色",
      value: result.planned_speakers,
      desc: "表示这一回合实际要调用哪些角色模型。它通常是在场可见角色的子集。",
    },
    {
      key: "character_visual_directives",
      label: "角色视觉指令",
      value: result.character_visual_directives,
      desc: "用于选择或生成角色立绘，比如 portrait_asset_path 或 generation_prompt。",
    },
  ].filter((row) => result[row.key] !== undefined && result[row.key] !== null);

  if (!rows.length) return null;

  return (
    <details open style={blockStyle}>
      <summary style={{ cursor: "pointer", fontWeight: 700 }}>世界主控写入摘要</summary>
      <div style={{ display: "grid", gap: 8, marginTop: 12 }}>
        {rows.map((row) => (
          <div
            key={row.key}
            style={{
              display: "grid",
              gap: 4,
              padding: "10px 12px",
              borderRadius: 8,
              background: "rgba(0,0,0,0.18)",
              border: "1px solid rgba(255,255,255,0.08)",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
              <strong>{row.label}</strong>
              <span style={{ color: "rgba(255,255,255,0.78)" }}>{textValue(row.value)}</span>
            </div>
            <div style={{ color: "rgba(255,255,255,0.62)", fontSize: 12, lineHeight: 1.5 }}>{row.desc}</div>
          </div>
        ))}
      </div>
    </details>
  );
}

export function TraceBlock({
  title,
  value,
  defaultOpen = false,
}: {
  title: string;
  value: unknown;
  defaultOpen?: boolean;
}) {
  return (
    <details open={defaultOpen} style={blockStyle}>
      <summary style={{ cursor: "pointer", fontWeight: 700 }}>
        {title}
        <span style={{ marginLeft: 8, color: "rgba(255,255,255,0.55)", fontSize: 12 }}>{summarizeValue(value)}</span>
      </summary>
      <pre
        style={{
          margin: "12px 0 0",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontSize: 12,
          lineHeight: 1.55,
          color: "rgba(255,255,255,0.84)",
          background: "rgba(0,0,0,0.22)",
          border: "1px solid rgba(255,255,255,0.08)",
          borderRadius: 8,
          padding: 14,
          overflowX: "auto",
        }}
      >
        {formatJsonBlock(value)}
      </pre>
    </details>
  );
}

export function PromptSendPreviewCard({
  item,
  defaultOpen = false,
}: {
  item: PromptCallRecordItem | Record<string, unknown>;
  index?: number;
  defaultOpen?: boolean;
}) {
  const { wrapper, call } = resolveCall(item);
  const recipientType = String(wrapper.recipient_type ?? call.recipient_type ?? "");
  const recipientName = resolveRecipientName(
    wrapper.recipient_name ?? call.recipient_name,
    recipientType,
  );
  const stage = String(call.stage ?? wrapper.step ?? "预览");

  return (
    <details open={defaultOpen} style={{ ...blockStyle, display: "grid" }}>
      <summary style={{ cursor: "pointer", listStyle: "none" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
          <strong>{`发给 ${recipientName}`}</strong>
          <span style={badgeStyle}>{`${recipientLabel(recipientType)} / ${stage}`}</span>
        </div>
      </summary>
      <pre
        style={{
          margin: "12px 0 0",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontSize: 12,
          lineHeight: 1.6,
          color: "rgba(255,255,255,0.88)",
          background: "rgba(0,0,0,0.24)",
          border: "1px solid rgba(255,255,255,0.08)",
          borderRadius: 8,
          padding: 14,
          overflowX: "auto",
        }}
      >
        {String(call.final_sent_content ?? "")}
      </pre>
    </details>
  );
}

export function PromptCallCard({
  item,
  index = 0,
  defaultOpen = true,
}: {
  item: PromptCallRecordItem | Record<string, unknown>;
  index?: number;
  defaultOpen?: boolean;
}) {
  const { wrapper, call } = resolveCall(item);
  const recipientType = String(wrapper.recipient_type ?? call.recipient_type ?? "");
  const recipientName = resolveRecipientName(
    wrapper.recipient_name ?? call.recipient_name,
    recipientType,
  );
  const stage = String(call.stage ?? wrapper.step ?? "调用");
  const purpose = String(call.purpose ?? "");
  const directorCall = isDirectorCall(wrapper, call);

  return (
    <details open={defaultOpen} style={{ ...blockStyle, display: "grid" }}>
      <summary style={{ cursor: "pointer", listStyle: "none" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
          <strong>{`调用 ${index + 1}：发给 ${recipientName}`}</strong>
          <span style={badgeStyle}>{`${recipientLabel(recipientType)} / ${stage}`}</span>
        </div>
        {purpose ? <div style={{ marginTop: 6, color: "rgba(255,255,255,0.62)", fontSize: 13 }}>{purpose}</div> : null}
      </summary>
      <div style={{ display: "grid", gap: 12, marginTop: 12 }}>
        <TraceBlock title={`最终发送内容（发给 ${recipientName}）`} value={call.final_sent_content ?? ""} defaultOpen />
        <TraceBlock title="模型原始返回" value={call.raw_model_return ?? "尚未记录返回"} defaultOpen={call.raw_model_return !== undefined && call.raw_model_return !== null} />
        <TraceBlock title="正则处理" value={call.return_processing ?? "没有返回处理记录"} />
        <TraceBlock title="正则处理后返回" value={call.processed_model_return ?? "尚未记录处理后返回"} defaultOpen={call.processed_model_return !== undefined && call.processed_model_return !== null} />
        {directorCall ? <DirectorFieldGuide writtenResult={call.written_result} /> : null}
        <TraceBlock title="写入游戏的结果（原始 JSON）" value={call.written_result ?? "尚未记录写入结果"} defaultOpen={!directorCall && call.written_result !== undefined && call.written_result !== null} />
        <TraceBlock title="提示词模块" value={call.modules ?? []} />
        <TraceBlock title="实际 messages" value={call.messages ?? []} />
        <TraceBlock title="原始调试数据" value={call.raw_debug ?? call} />
      </div>
    </details>
  );
}

export function DirectorPromptCard({ item, index }: { item: DirectorPromptTraceItem; index: number }) {
  return <PromptCallCard item={{ ...item, recipient_type: "director", recipient_name: "世界主控", prompt_call: item.prompt_trace }} index={index} />;
}

export function CharacterPromptCard({ item, index }: { item: CharacterPromptTraceItem; index: number }) {
  return <PromptCallCard item={{ ...item, recipient_type: "character", recipient_name: item.speaker || "角色", prompt_call: item.prompt_trace }} index={index} />;
}

export function LlmCallCard({ item, index }: { item: LlmCallTraceItem; index: number }) {
  return (
    <PromptCallCard
      item={{
        turn_index: item.turn_index,
        step: item.step,
        recipient_name: item.speaker,
        prompt_call: {
          ...item.input_payload,
          recipient_name: item.speaker,
          purpose: `${item.provider || "model"} / ${item.model_id || "unknown"} / ${item.status || "completed"} / ${item.latency_ms ?? 0}ms`,
          raw_model_return: item.output_payload,
          processed_model_return: item.output_payload,
          raw_debug: {
            provider: item.provider,
            model_id: item.model_id,
            status: item.status,
            latency_ms: item.latency_ms,
            input_payload: item.raw_input_payload ?? item.input_payload,
            output_payload: item.raw_output_payload ?? item.output_payload,
          },
        },
      }}
      index={index}
      defaultOpen={false}
    />
  );
}

export function PromptTraceSection({
  title,
  emptyText,
  children,
}: {
  title: string;
  emptyText?: string;
  children: ReactNode;
}) {
  return (
    <section style={{ display: "grid", gap: 12 }}>
      <h4 style={{ margin: 0, fontSize: 18 }}>{title}</h4>
      {children || (emptyText ? <div style={{ color: "rgba(255,255,255,0.62)" }}>{emptyText}</div> : null)}
    </section>
  );
}
