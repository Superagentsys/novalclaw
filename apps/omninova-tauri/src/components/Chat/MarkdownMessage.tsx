import {
  Children,
  createContext,
  isValidElement,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import mermaid from "mermaid";
import { useTheme } from "../../theme/themeState";
import { invokeTauri } from "../../utils/tauri";

import "katex/dist/katex.min.css";
import "./MarkdownMessage.css";

function cssThemeValue(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

function configureMermaid() {
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: "base",
    fontFamily: "inherit",
    themeVariables: {
      background: cssThemeValue("--ui-surface-1", "#ffffff"),
      primaryColor: cssThemeValue("--ui-accent-soft", "#e7edff"),
      primaryTextColor: cssThemeValue("--ui-text", "#192132"),
      primaryBorderColor: cssThemeValue("--ui-accent", "#335eea"),
      lineColor: cssThemeValue("--ui-text-muted", "#66758f"),
      secondaryColor: cssThemeValue("--ui-surface-2", "#f7f9fc"),
      tertiaryColor: cssThemeValue("--ui-surface-3", "#eef2f8"),
      noteBkgColor: cssThemeValue("--ui-warning-soft", "#fff4d9"),
      noteTextColor: cssThemeValue("--ui-text", "#192132"),
      noteBorderColor: cssThemeValue("--ui-warning", "#a86100"),
      fontFamily: "inherit",
    },
  });
}

/** Renders a Mermaid diagram from its source, with graceful fallback to code. */
function MermaidDiagram({ code }: { code: string }) {
  const { resolvedTheme } = useTheme();
  const [svg, setSvg] = useState<string>("");
  const [failed, setFailed] = useState(false);
  const [renderId] = useState(
    () => `mermaid-${Math.random().toString(36).slice(2)}`,
  );

  useEffect(() => {
    let cancelled = false;
    configureMermaid();
    mermaid
      .render(`${renderId}-${resolvedTheme}`, code)
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
  }, [code, renderId, resolvedTheme]);

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

const COLLAPSE_LINE_THRESHOLD = 14;
const WorkspacePathContext = createContext<string | null>(null);

function collectNodeText(node: React.ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(collectNodeText).join("");
  if (isValidElement(node)) {
    return collectNodeText(
      (node.props as { children?: React.ReactNode }).children,
    );
  }
  return "";
}

function looksLikeFilePath(text: string): boolean {
  const line = text.replace(/\s+$/, "").trim();
  if (!line || line.includes("\n") || line.length > 260) return false;
  if (/[;{}()<>|"'`=*?]/.test(line)) return false;
  for (let i = 0; i < line.length; i += 1) {
    if (line.charCodeAt(i) < 0x20) return false;
  }
  const name = line.split(/[\\/]/).pop() ?? line;
  // Require a stem plus a letter-initial extension so numbers like 3.14 are excluded.
  return /^.+\.[A-Za-z][A-Za-z0-9]{0,11}$/.test(name);
}

function countLines(text: string): number {
  if (!text) return 0;
  return text.replace(/\n$/, "").split("\n").length;
}

function fileBasename(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** Fenced code: copy, collapse long blocks, and preview workspace file paths. */
function CodeBlock({
  className,
  children,
}: {
  className?: string;
  children?: React.ReactNode;
}) {
  const workspacePath = useContext(WorkspacePathContext);
  const [copied, setCopied] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewText, setPreviewText] = useState<string | null>(null);
  const [previewImage, setPreviewImage] = useState<string | null>(null);
  const source = collectNodeText(children).replace(/\n$/, "");
  const match = /language-([\w-]+)/.exec(className ?? "");
  const lang = match?.[1];
  const filePath = looksLikeFilePath(source) ? source.trim() : null;
  const lineCount = countLines(source);
  const collapsible = !filePath && lineCount > COLLAPSE_LINE_THRESHOLD;
  const collapsed = collapsible && !expanded;

  if (lang === "mermaid") {
    return <MermaidDiagram code={source} />;
  }

  const onCopy = () => {
    const text = previewing && previewText ? previewText : source;
    navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  const loadPreview = async () => {
    if (!filePath) return;
    if (previewText || previewImage) {
      setPreviewing(true);
      return;
    }
    setPreviewLoading(true);
    setPreviewError(null);
    try {
      const preview = await invokeTauri<{
        textPreview?: string | null;
        dataUrl?: string | null;
      }>("task_artifact_preview", {
        path: filePath,
        workspacePath: workspacePath || undefined,
      });
      setPreviewText(preview.textPreview ?? null);
      setPreviewImage(preview.dataUrl ?? null);
      if (!preview.textPreview && !preview.dataUrl) {
        setPreviewError("该文件类型不提供内嵌预览，可改用「打开文件」。");
      }
      setPreviewing(true);
    } catch (reason) {
      setPreviewError(String(reason));
      setPreviewing(true);
    } finally {
      setPreviewLoading(false);
    }
  };

  const toggleFilePreview = () => {
    if (previewing) {
      setPreviewing(false);
      return;
    }
    void loadPreview();
  };

  const openFile = (reveal: boolean) => {
    if (!filePath) return;
    void invokeTauri("open_task_artifact", {
      path: filePath,
      workspacePath: workspacePath || undefined,
      reveal,
    }).catch((reason) => {
      setPreviewError(String(reason));
      setPreviewing(true);
    });
  };

  const hasPreviewContent = Boolean(previewText || previewImage);
  const showPreview = Boolean(filePath && previewing && (hasPreviewContent || previewError));
  const hidePre = Boolean(filePath && previewing && hasPreviewContent);

  return (
    <div className={`md-code-wrap${filePath ? " md-code-wrap--file" : ""}`}>
      <div className="md-code-header">
        <span
          className={`md-code-lang${filePath ? " is-file" : ""}`}
          title={filePath ?? lang ?? "text"}
        >
          {filePath ? fileBasename(filePath) : lang ?? "text"}
        </span>
        <div className="md-code-header-actions">
          {filePath ? (
            <>
              <button
                type="button"
                className="md-code-copy md-code-copy--primary"
                onClick={toggleFilePreview}
                disabled={previewLoading}
              >
                {previewLoading ? "读取中…" : previewing ? "收起预览" : "展开预览"}
              </button>
              <button
                type="button"
                className="md-code-copy"
                onClick={() => openFile(false)}
              >
                打开文件
              </button>
            </>
          ) : null}
          {collapsible ? (
            <button
              type="button"
              className="md-code-copy md-code-copy--primary"
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? "收起" : `展开全部（${lineCount} 行）`}
            </button>
          ) : null}
          <button type="button" className="md-code-copy" onClick={onCopy}>
            {copied ? "已复制" : "复制"}
          </button>
        </div>
      </div>
      {hidePre ? null : (
        <pre
          className={`md-code-block${collapsed ? " md-code-block--collapsed" : ""}${
            filePath || collapsed ? " md-code-block--clickable" : ""
          }`}
          onClick={
            filePath
              ? toggleFilePreview
              : collapsed
                ? () => setExpanded(true)
                : undefined
          }
        >
          <code className={className}>{children}</code>
        </pre>
      )}
      {showPreview ? (
        <div className="md-code-preview">
          {previewError ? (
            <div className="md-code-preview-error">{previewError}</div>
          ) : null}
          {previewImage ? (
            <img
              className="md-code-preview-image"
              src={previewImage}
              alt={filePath ?? ""}
            />
          ) : null}
          {previewText ? <pre className="md-code-preview-text">{previewText}</pre> : null}
        </div>
      ) : null}
    </div>
  );
}

function PreBlock({ children }: { children?: React.ReactNode }) {
  const codeEl = Children.toArray(children).find((child) => isValidElement(child));
  if (!isValidElement(codeEl)) {
    return <CodeBlock>{children}</CodeBlock>;
  }
  const props = codeEl.props as { className?: string; children?: React.ReactNode };
  return <CodeBlock className={props.className}>{props.children}</CodeBlock>;
}

const components: Components = {
  pre: PreBlock as Components["pre"],
  code: ({ className, children }) => (
    <code className={className || "md-inline-code"}>{children}</code>
  ),
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
export function MarkdownMessage({
  content,
  workspacePath,
}: {
  content: string;
  workspacePath?: string | null;
}) {
  const remarkPlugins = useMemo(() => [remarkGfm, remarkMath], []);
  const rehypePlugins = useMemo(
    () => [rehypeKatex, [rehypeHighlight, { detect: true, ignoreMissing: true }]],
    [],
  );

  return (
    <WorkspacePathContext.Provider value={workspacePath ?? null}>
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
    </WorkspacePathContext.Provider>
  );
}

export default MarkdownMessage;
