import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import mermaid from "mermaid";

import "katex/dist/katex.min.css";
import "highlight.js/styles/github-dark.css";
import "./MarkdownMessage.css";

let mermaidInitialized = false;

function ensureMermaid() {
  if (mermaidInitialized) return;
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: "dark",
    fontFamily: "inherit",
  });
  mermaidInitialized = true;
}

/** Renders a Mermaid diagram from its source, with graceful fallback to code. */
function MermaidDiagram({ code }: { code: string }) {
  const [svg, setSvg] = useState<string>("");
  const [failed, setFailed] = useState(false);
  const [renderId] = useState(
    () => `mermaid-${Math.random().toString(36).slice(2)}`,
  );

  useEffect(() => {
    let cancelled = false;
    ensureMermaid();
    mermaid
      .render(renderId, code)
      .then(({ svg }) => {
        if (!cancelled) {
          setSvg(svg);
          setFailed(false);
        }
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [code, renderId]);

  if (failed) {
    return (
      <pre className="md-code-block">
        <code>{code}</code>
      </pre>
    );
  }

  return (
    <div
      className="md-mermaid"
      // Mermaid returns sanitized SVG (securityLevel: strict).
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}

/** Code block with language label and copy button; delegates fenced `mermaid`. */
function CodeBlock({
  inline,
  className,
  children,
}: {
  inline?: boolean;
  className?: string;
  children?: React.ReactNode;
}) {
  const [copied, setCopied] = useState(false);
  const raw = String(children ?? "");
  const match = /language-([\w-]+)/.exec(className ?? "");
  const lang = match?.[1];

  if (inline) {
    return <code className="md-inline-code">{children}</code>;
  }

  if (lang === "mermaid") {
    return <MermaidDiagram code={raw.replace(/\n$/, "")} />;
  }

  const onCopy = () => {
    navigator.clipboard?.writeText(raw.replace(/\n$/, "")).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  return (
    <div className="md-code-wrap">
      <div className="md-code-header">
        <span className="md-code-lang">{lang ?? "text"}</span>
        <button type="button" className="md-code-copy" onClick={onCopy}>
          {copied ? "已复制" : "复制"}
        </button>
      </div>
      <pre className="md-code-block">
        <code className={className}>{children}</code>
      </pre>
    </div>
  );
}

const components: Components = {
  code: CodeBlock as Components["code"],
  a: ({ href, children }) => (
    <a href={href} target="_blank" rel="noreferrer noopener">
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="md-table-wrap">
      <table>{children}</table>
    </div>
  ),
};

/** Renders assistant/agent message content as rich Markdown. */
export function MarkdownMessage({ content }: { content: string }) {
  const remarkPlugins = useMemo(() => [remarkGfm, remarkMath], []);
  const rehypePlugins = useMemo(
    () => [rehypeKatex, [rehypeHighlight, { detect: true, ignoreMissing: true }]],
    [],
  );

  return (
    <div className="md-content">
      <ReactMarkdown
        remarkPlugins={remarkPlugins}
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        rehypePlugins={rehypePlugins as any}
        components={components}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}

export default MarkdownMessage;
