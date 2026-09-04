import React from "react";
import { CodeBlock as StreamdownCodeBlock, Streamdown } from "streamdown";
import { cjk } from "@streamdown/cjk";
import { math } from "@streamdown/math";
import { mermaid } from "@streamdown/mermaid";
import "streamdown/styles.css";
import "katex/dist/katex.min.css";

// streamdown 自带的复制/下载/全屏控件是 Tailwind 工具类写的，沙箱 iframe 的
// 构建不过 Tailwind，那些类全部失效 → 排版错乱。这里关掉全部内置控件，
// 代码块用自己的极简头部（语言名 + 复制），样式走 theme.css。

function CodeBlockCopyButton({ code }: { code: string }) {
  const [copied, setCopied] = React.useState(false);
  React.useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);
  return (
    <button
      type="button"
      className="game-md-codeblock-copy"
      onClick={() => {
        void navigator.clipboard?.writeText(code).then(() => setCopied(true));
      }}
    >
      {copied ? "已复制" : "复制"}
    </button>
  );
}

function MarkdownCode({
  className,
  children,
}: {
  className?: string;
  children?: React.ReactNode;
}) {
  const match = /language-([A-Za-z0-9_-]+)/.exec(className || "");
  const code = String(children ?? "").replace(/\n$/, "");
  const isBlock = Boolean(match) || code.includes("\n");
  if (!isBlock) {
    return <code className={className}>{children}</code>;
  }
  const language = match?.[1] ?? "";
  return (
    <div className="game-md-codeblock">
      <div className="game-md-codeblock-header">
        <span className="game-md-codeblock-lang">{language || "text"}</span>
        <CodeBlockCopyButton code={code} />
      </div>
      <StreamdownCodeBlock code={code} language={language} lineNumbers={false} />
    </div>
  );
}

const MARKDOWN_COMPONENTS = {
  // 默认 pre 容器带控件布局，拆掉只留内容，代码块由上面的 code 覆盖渲染。
  pre: ({ children }: { children?: React.ReactNode }) => <>{children}</>,
  code: MarkdownCode,
};

const INLINE_LATEX_REGEX = /\\\((.+?)\\\)/g;
const BLOCK_LATEX_REGEX = /\\\[(.+?)\\\]/gs;
const CODE_BLOCK_REGEX = /```[\s\S]*?```|`[^`\n]*`/g;

// 模型常输出 \(...\) / \[...\] 形式的公式，remark-math 只认 $...$ / $$...$$。
// 统一转换，跳过代码块（对齐 rikkahub web-ui 的预处理）。
function preProcessMathDelimiters(content: string): string {
  if (!content.includes("\\(") && !content.includes("\\[")) {
    return content;
  }
  const codeBlocks: Array<{ start: number; end: number }> = [];
  const codeBlockRegex = new RegExp(CODE_BLOCK_REGEX.source, "g");
  let match: RegExpExecArray | null;
  while ((match = codeBlockRegex.exec(content)) !== null) {
    codeBlocks.push({ start: match.index, end: match.index + match[0].length });
  }
  const isInCodeBlock = (position: number): boolean =>
    codeBlocks.some((range) => position >= range.start && position < range.end);

  return content
    .replace(new RegExp(INLINE_LATEX_REGEX.source, "g"), (text, group1, offset) =>
      isInCodeBlock(offset) ? text : `$${group1}$`,
    )
    .replace(new RegExp(BLOCK_LATEX_REGEX.source, "gs"), (text, group1, offset) =>
      isInCodeBlock(offset) ? text : `$$${group1}$$`,
    );
}

/**
 * 消息正文的 Markdown 渲染。支持：GFM 表格、代码高亮（shiki）、KaTeX 公式、
 * Mermaid 图、内嵌 HTML（经 sanitize+harden，脚本与危险协议被剔除）。
 * streaming 时启用半截 Markdown 容错与逐词淡入。
 */
export function MarkdownMessageContent({
  text,
  streaming = false,
}: {
  text: string;
  streaming?: boolean;
}) {
  const processed = React.useMemo(() => preProcessMathDelimiters(text), [text]);
  if (!processed.trim()) {
    return null;
  }
  return (
    <Streamdown
      className="game-markdown"
      mode={streaming ? "streaming" : "static"}
      isAnimating={streaming}
      animated={streaming ? { animation: "fadeIn", sep: "word", duration: 100 } : false}
      plugins={{ cjk, math, mermaid }}
      controls={false}
      components={MARKDOWN_COMPONENTS}
    >
      {processed}
    </Streamdown>
  );
}
