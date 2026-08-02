import React, { useState } from "react";
import type { MessageInteraction } from "../../data/types";
import type { GameUiRuntimeActions } from "../actions";
import { formatInteractionAnswer, readInteractionFields, readInteractionOptions } from "../interactions";

type InteractionBlockProps = {
  messageId: string | undefined;
  interaction: MessageInteraction;
  locked: boolean;
  actions: GameUiRuntimeActions;
};

/** 交互消息（第 5 项）的宿主渲染器：pending 渲染控件，answered 渲染只读结果。 */
export function InteractionBlock({ messageId, interaction, locked, actions }: InteractionBlockProps) {
  const [selected, setSelected] = useState<string[]>([]);
  const [formValues, setFormValues] = useState<Record<string, string>>({});
  const [sliderValue, setSliderValue] = useState<number>(
    typeof interaction.config.default === "number" ? interaction.config.default : 0,
  );
  const [submitting, setSubmitting] = useState(false);
  const disabled = locked || submitting || !messageId;

  const submit = (answer: unknown) => {
    if (disabled || !messageId) return;
    setSubmitting(true);
    void actions
      .answerInteraction(messageId, interaction.interaction_id, answer)
      .finally(() => setSubmitting(false));
  };

  const containerStyle: React.CSSProperties = {
    marginTop: 8,
    padding: "10px 12px",
    border: "1px solid rgba(128, 128, 128, 0.35)",
    borderRadius: 8,
    display: "flex",
    flexDirection: "column",
    gap: 8,
  };

  if (interaction.status === "answered") {
    return (
      <div className="game-interaction game-interaction--answered" style={containerStyle} data-kind={interaction.kind}>
        {interaction.prompt ? <div style={{ opacity: 0.85 }}>{interaction.prompt}</div> : null}
        <div style={{ fontWeight: 600 }}>已回答：{formatInteractionAnswer(interaction) || "（空）"}</div>
      </div>
    );
  }

  const options = readInteractionOptions(interaction);
  const fields = readInteractionFields(interaction);

  return (
    <div className="game-interaction" style={containerStyle} data-kind={interaction.kind}>
      {interaction.prompt ? <div style={{ fontWeight: 600 }}>{interaction.prompt}</div> : null}

      {interaction.kind === "choice"
        ? options.map((option) => (
            <button
              key={option.id}
              type="button"
              className="game-ui-button"
              data-variant="ghost"
              disabled={disabled}
              onClick={() => submit(option.id)}
            >
              {option.label}
            </button>
          ))
        : null}

      {interaction.kind === "multi_choice" ? (
        <>
          {options.map((option) => (
            <label key={option.id} style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <input
                type="checkbox"
                disabled={disabled}
                checked={selected.includes(option.id)}
                onChange={(event) => {
                  setSelected((prev) =>
                    event.target.checked
                      ? [...prev, option.id]
                      : prev.filter((id) => id !== option.id),
                  );
                }}
              />
              <span>{option.label}</span>
            </label>
          ))}
          <button
            type="button"
            className="game-ui-button"
            disabled={disabled || selected.length === 0}
            onClick={() => submit(selected)}
          >
            提交
          </button>
        </>
      ) : null}

      {interaction.kind === "form" ? (
        <>
          {fields.map((field) => (
            <label key={field.id} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={{ opacity: 0.85 }}>
                {field.label}
                {field.required ? " *" : ""}
              </span>
              {field.input === "textarea" ? (
                <textarea
                  className="editor-field-input"
                  disabled={disabled}
                  value={formValues[field.id] ?? ""}
                  onChange={(event) =>
                    setFormValues((prev) => ({ ...prev, [field.id]: event.target.value }))
                  }
                />
              ) : (
                <input
                  className="editor-field-input"
                  type={field.input === "number" ? "number" : "text"}
                  disabled={disabled}
                  value={formValues[field.id] ?? ""}
                  onChange={(event) =>
                    setFormValues((prev) => ({ ...prev, [field.id]: event.target.value }))
                  }
                />
              )}
            </label>
          ))}
          <button
            type="button"
            className="game-ui-button"
            disabled={disabled || fields.some((field) => field.required && !(formValues[field.id] ?? "").trim())}
            onClick={() => submit(formValues)}
          >
            提交
          </button>
        </>
      ) : null}

      {interaction.kind === "confirm" ? (
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="game-ui-button"
            disabled={disabled}
            onClick={() => submit(true)}
          >
            {readStringProp(interaction.config.confirm_label, "确认")}
          </button>
          <button
            type="button"
            className="game-ui-button"
            data-variant="ghost"
            disabled={disabled}
            onClick={() => submit(false)}
          >
            {readStringProp(interaction.config.cancel_label, "取消")}
          </button>
        </div>
      ) : null}

      {interaction.kind === "slider" ? (
        <>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <input
              type="range"
              style={{ flex: 1 }}
              min={readNumberProp(interaction.config.min, 0)}
              max={readNumberProp(interaction.config.max, 100)}
              step={readNumberProp(interaction.config.step, 1)}
              disabled={disabled}
              value={sliderValue}
              onChange={(event) => setSliderValue(Number(event.target.value))}
            />
            <span style={{ minWidth: 32, textAlign: "right" }}>{sliderValue}</span>
          </div>
          <button
            type="button"
            className="game-ui-button"
            disabled={disabled}
            onClick={() => submit(sliderValue)}
          >
            提交
          </button>
        </>
      ) : null}
    </div>
  );
}

function readStringProp(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim() ? value : fallback;
}

function readNumberProp(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
