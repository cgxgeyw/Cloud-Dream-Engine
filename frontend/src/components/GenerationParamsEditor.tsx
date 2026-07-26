import type { GenerationParams } from "../data/types";

/**
 * 生成参数（采样参数）编辑器。第 8 项：参数按「应用默认 → 世界 → 存档」三级覆盖，
 * 每级都可以只配自己关心的几项——所以这里每个输入框留空即代表「本级不覆盖」，
 * 而不是「设为 0」。旁边显示继承来的值，让用户看得出留空后实际会用多少。
 */

type ParamKey = keyof GenerationParams;

type NumericFieldSpec = {
  key: Exclude<ParamKey, "stop">;
  label: string;
  hint: string;
  step: string;
  min?: number;
  max?: number;
  integer?: boolean;
};

/** 各参数的中文名与取值范围说明。范围与后端 sanitized() 的夹紧区间一致。 */
const NUMERIC_FIELDS: NumericFieldSpec[] = [
  {
    key: "temperature",
    label: "随机度 (temperature)",
    hint: "0～2，越大越发散。留空则用默认（导演 0.7 / 角色 0.8）",
    step: "0.05",
    min: 0,
    max: 2,
  },
  {
    key: "top_p",
    label: "候选词累积概率 (top_p)",
    hint: "0～1，只从累积概率前 p 的词里挑",
    step: "0.05",
    min: 0,
    max: 1,
  },
  {
    key: "top_k",
    label: "候选词个数 (top_k)",
    hint: "只从概率最高的 k 个词里挑。仅 Claude 支持，OpenAI 兼容端点会过滤掉",
    step: "1",
    min: 1,
    integer: true,
  },
  {
    key: "max_tokens",
    label: "单次最多生成 (max_tokens)",
    hint: "留空则用所选模型连接配置里的上限",
    step: "16",
    min: 1,
    integer: true,
  },
  {
    key: "presence_penalty",
    label: "话题重复惩罚 (presence_penalty)",
    hint: "-2～2，越大越倾向换新话题。Claude 不支持，会被过滤掉",
    step: "0.1",
    min: -2,
    max: 2,
  },
  {
    key: "frequency_penalty",
    label: "字词重复惩罚 (frequency_penalty)",
    hint: "-2～2，越大越少重复用词。Claude 不支持，会被过滤掉",
    step: "0.1",
    min: -2,
    max: 2,
  },
  {
    key: "seed",
    label: "随机种子 (seed)",
    hint: "固定种子可复现同样输出。Claude 不支持，会被过滤掉",
    step: "1",
    integer: true,
  },
];

function stopToText(stop: string[] | undefined): string {
  return (stop ?? []).join("\n");
}

/** 每行一个停止词；全空视作「本级不覆盖」而非「不设停止词」。 */
function textToStop(value: string): string[] | undefined {
  const items = value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  return items.length > 0 ? items : undefined;
}

function formatInherited(value: number | string[] | undefined): string {
  if (value === undefined) return "未设置";
  return Array.isArray(value) ? value.join(" / ") : String(value);
}

export function GenerationParamsEditor({
  value,
  inherited,
  onChange,
  disabled = false,
  inheritedLabel = "继承值",
}: {
  /** 本级的覆盖值。字段缺省即本级不覆盖。 */
  value: GenerationParams;
  /** 不配本级时实际会生效的值，用于提示；不传则不显示继承提示。 */
  inherited?: GenerationParams;
  onChange: (next: GenerationParams) => void;
  disabled?: boolean;
  inheritedLabel?: string;
}) {
  function patch(key: ParamKey, next: number | string[] | undefined) {
    const draft: GenerationParams = { ...value };
    if (next === undefined) {
      delete draft[key];
    } else {
      // key 与值类型一一对应（stop 为字符串数组，其余为数字），这里按 key 分派。
      if (key === "stop") {
        draft.stop = next as string[];
      } else {
        draft[key] = next as number;
      }
    }
    onChange(draft);
  }

  return (
    <div style={{ display: "grid", gap: 12 }}>
      {NUMERIC_FIELDS.map((field) => {
        const current = value[field.key];
        const inheritedValue = inherited?.[field.key];
        return (
          <label key={field.key} className="field-label">
            <span className="field-label-text">{field.label}</span>
            <input
              type="number"
              className="field-input"
              value={current === undefined ? "" : String(current)}
              step={field.step}
              min={field.min}
              max={field.max}
              disabled={disabled}
              placeholder="留空 = 不覆盖"
              onChange={(event) => {
                const raw = event.target.value.trim();
                if (raw === "") {
                  patch(field.key, undefined);
                  return;
                }
                const parsed = field.integer ? Number.parseInt(raw, 10) : Number.parseFloat(raw);
                if (Number.isNaN(parsed)) return;
                patch(field.key, parsed);
              }}
            />
            <span className="text-muted">
              {field.hint}
              {inherited ? ` · ${inheritedLabel}：${formatInherited(inheritedValue)}` : ""}
            </span>
          </label>
        );
      })}

      <label className="field-label">
        <span className="field-label-text">停止词 (stop)</span>
        <textarea
          className="field-input"
          rows={3}
          disabled={disabled}
          value={stopToText(value.stop)}
          placeholder="每行一个，留空 = 不覆盖"
          onChange={(event) => patch("stop", textToStop(event.target.value))}
        />
        <span className="text-muted">
          模型生成到这些文字就停下。每行一个
          {inherited ? ` · ${inheritedLabel}：${formatInherited(inherited.stop)}` : ""}
        </span>
      </label>
    </div>
  );
}

/** 展示某组参数的只读摘要，用于「最终生效值」这类不可编辑的展示位。 */
export function GenerationParamsSummary({ params }: { params: GenerationParams }) {
  const entries: Array<[string, string]> = [];
  for (const field of NUMERIC_FIELDS) {
    const item = params[field.key];
    if (item !== undefined) {
      entries.push([field.label, String(item)]);
    }
  }
  if (params.stop && params.stop.length > 0) {
    entries.push(["停止词 (stop)", params.stop.join(" / ")]);
  }

  if (entries.length === 0) {
    return <div className="text-muted">没有任何参数覆盖，全部用内置默认。</div>;
  }

  return (
    <div style={{ display: "grid", gap: 4 }}>
      {entries.map(([label, item]) => (
        <div key={label} style={{ display: "flex", justifyContent: "space-between", gap: 12, fontSize: 13 }}>
          <span className="text-muted">{label}</span>
          <span style={{ fontWeight: 600 }}>{item}</span>
        </div>
      ))}
    </div>
  );
}
