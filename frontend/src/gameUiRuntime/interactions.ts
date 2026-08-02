import type { MessageInteraction } from "../data/types";

export function readInteractionOptions(
  interaction: MessageInteraction,
): Array<{ id: string; label: string }> {
  const raw = interaction.config.options;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    const id = typeof record.id === "string" ? record.id : "";
    const label = typeof record.label === "string" ? record.label : "";
    return id && label ? [{ id, label }] : [];
  });
}

export function readInteractionFields(
  interaction: MessageInteraction,
): Array<{ id: string; label: string; input: string; required: boolean }> {
  const raw = interaction.config.fields;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    const id = typeof record.id === "string" ? record.id : "";
    const label = typeof record.label === "string" ? record.label : "";
    return id && label
      ? [{
          id,
          label,
          input: typeof record.input === "string" ? record.input : "text",
          required: record.required === true,
        }]
      : [];
  });
}

export function formatInteractionAnswer(
  interaction: MessageInteraction,
  answer: unknown = interaction.answer,
): string {
  if (answer === null || answer === undefined) return "";
  switch (interaction.kind) {
    case "choice": {
      const option = readInteractionOptions(interaction).find((item) => item.id === answer);
      return option?.label ?? String(answer);
    }
    case "multi_choice": {
      if (!Array.isArray(answer)) return String(answer);
      const options = readInteractionOptions(interaction);
      return answer
        .map((id) => options.find((item) => item.id === id)?.label ?? String(id))
        .join("、");
    }
    case "form": {
      if (typeof answer !== "object" || Array.isArray(answer)) return String(answer);
      const fields = readInteractionFields(interaction);
      return Object.entries(answer as Record<string, unknown>)
        .map(([id, value]) => `${fields.find((field) => field.id === id)?.label ?? id}：${String(value)}`)
        .join("；");
    }
    case "confirm":
      return answer === true
        ? readString(interaction.config.confirm_label, "确认")
        : readString(interaction.config.cancel_label, "取消");
    default:
      return String(answer);
  }
}

function readString(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim() ? value : fallback;
}
