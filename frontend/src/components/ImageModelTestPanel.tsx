import { assetUrl, type ImageModelTestResult, type ModelConfigResponse } from "../data/apiAdapter";

type ImageModelTestPanelProps = {
  model: ModelConfigResponse;
  isOpen: boolean;
  loading: boolean;
  prompt: string;
  result?: ImageModelTestResult | null;
  onToggle: () => void;
  onPromptChange: (value: string) => void;
  onSubmit: () => void;
};

export function ImageModelTestPanel({
  model,
  isOpen,
  loading,
  prompt,
  result,
  onToggle,
  onPromptChange,
  onSubmit,
}: ImageModelTestPanelProps) {
  const previewPath = result?.asset_path || result?.image_url || "";
  const previewUrl = previewPath ? assetUrl(previewPath) : "";

  return (
    <>
      <button type="button" onClick={onToggle} className="action-btn">
        {isOpen ? "收起测试" : "测试文生图"}
      </button>

      {isOpen ? (
        <div style={{ marginTop: 12, display: "grid", gap: 12 }}>
          <label className="field-label">
            <span className="field-label-text">测试提示词</span>
            <textarea
              value={prompt}
              onChange={(event) => onPromptChange(event.target.value)}
              className="field-input"
              placeholder="例如：雨夜霓虹街道，低机位，电影感光影，潮湿路面反光。"
              style={{ minHeight: 110, resize: "vertical" }}
            />
          </label>

          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
            <button
              type="button"
              onClick={onSubmit}
              disabled={loading || !prompt.trim()}
              className="action-btn action-btn--accent"
            >
              {loading ? "生成中..." : "开始生成"}
            </button>
            <div className="text-muted">结果会显示在当前模型卡片内。</div>
          </div>

          {result ? (
            <div style={{ display: "grid", gap: 10 }}>
              <div className={result.ok ? "text-muted" : "text-error"}>{result.detail}</div>
              {previewUrl ? (
                <img
                  src={previewUrl}
                  alt={`${model.name} test output`}
                  style={{
                    width: "100%",
                    maxWidth: 560,
                    borderRadius: 18,
                    border: "1px solid rgba(148, 163, 184, 0.2)",
                    objectFit: "cover",
                    background: "rgba(15, 23, 42, 0.24)",
                  }}
                />
              ) : null}
              {result.debug_lines.length ? (
                <pre
                  style={{
                    margin: 0,
                    padding: 12,
                    overflowX: "auto",
                    borderRadius: 14,
                    background: "rgba(15, 23, 42, 0.18)",
                    fontSize: 12,
                    lineHeight: 1.5,
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                  }}
                >
                  {result.debug_lines.join("\n")}
                </pre>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
