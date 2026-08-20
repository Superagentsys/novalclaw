export interface ContractReviewEngineCard {
  id: string;
  name: string;
  description: string;
  reviewFocus: string[];
  clauses: string[];
  riskPolicy: string;
  outputSchema: string[];
  recommended: boolean;
}

// Keep the contract workflow usable while the desktop command is still
// connecting. Tauri replaces this snapshot with the authoritative profiles
// returned by `list_contract_review_engines` as soon as they are available.
export const DEFAULT_CONTRACT_REVIEW_ENGINES: ContractReviewEngineCard[] = [
  {
    id: "omninova-contract-risk",
    name: "OmniNova 合同风险审查",
    description: "通用合同关键条款、缺漏与版本风险初审",
    reviewFocus: ["交易闭环", "权利义务对等", "可执行性", "版本差异"],
    clauses: [],
    riskPolicy: "按事实和合同原文分级；证据不足时标记待人工确认，不作法律结论。",
    outputSchema: [],
    recommended: true,
  },
  {
    id: "ai-contract-risk-officer",
    name: "Ai Contract Risk Officer",
    description: "偏重商业风险、履约风险与可量化责任边界",
    reviewFocus: ["付款安全", "履约保障", "责任上限", "退出机制"],
    clauses: [],
    riskPolicy: "优先识别高损失概率与高影响条款，并给出可直接谈判的修改建议。",
    outputSchema: [],
    recommended: false,
  },
  {
    id: "baichen-legal",
    name: "Baichen Legal",
    description: "偏重中国商事合同完整性、合规性与争议处理",
    reviewFocus: ["主体资格", "条款完备", "合规风险", "争议解决"],
    clauses: [],
    riskPolicy: "区分缺失、歧义、冲突和不利约定；不虚构法律条文。",
    outputSchema: [],
    recommended: false,
  },
  {
    id: "legal-contract-review",
    name: "Legal Contract Review",
    description: "偏重逐条审阅、语言清晰度与版本变更影响",
    reviewFocus: ["逐条审阅", "定义一致", "交叉引用", "版本比对"],
    clauses: [],
    riskPolicy: "对每项结论引用短原文；无法从文本确认时明确说明。",
    outputSchema: [],
    recommended: false,
  },
];

export interface PreparedContractReview {
  prompt: string;
  markdown: string;
  export: unknown;
  mode: "review" | "comparison";
  engine: ContractReviewEngineCard;
}

export type ContractReviewStage =
  | "idle"
  | "preparing"
  | "reviewing"
  | "generating"
  | "completed"
  | "failed"
  | "cancelled";

export interface ContractReviewAttachmentView {
  id: string;
  name: string;
  kind: "image" | "text" | "other";
  note?: string;
}

export interface ContractVersionChangeView {
  fromDocument: string;
  toDocument: string;
  clause: string;
  change: string;
}

export interface ContractReviewExportView {
  tool?: string;
  engine?: string;
  mode?: "review" | "comparison";
  documents: string[];
  missingClauses: string[];
  keywords: string[];
  versionChanges: ContractVersionChangeView[];
  disclaimer?: string;
}

export interface ContractReviewReportSection {
  id: string;
  title: string;
  content: string;
  defaultOpen: boolean;
}

const DEFAULT_OPEN_SECTIONS = new Set([
  "风险发现",
  "修改建议",
  "风控初审结论",
  "版本差异",
]);

export function toContractReviewExportView(value: unknown): ContractReviewExportView | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const strings = (field: string) =>
    Array.isArray(raw[field])
      ? raw[field].filter((item): item is string => typeof item === "string")
      : [];
  const versionChanges = Array.isArray(raw.versionChanges)
    ? raw.versionChanges.flatMap((item) => {
        if (!item || typeof item !== "object") return [];
        const change = item as Record<string, unknown>;
        const fromDocument = typeof change.fromDocument === "string" ? change.fromDocument : "";
        const toDocument = typeof change.toDocument === "string" ? change.toDocument : "";
        const clause = typeof change.clause === "string" ? change.clause : "";
        const description = typeof change.change === "string" ? change.change : "";
        if (!fromDocument || !toDocument || !clause) return [];
        return [{ fromDocument, toDocument, clause, change: description }];
      })
    : [];

  return {
    tool: typeof raw.tool === "string" ? raw.tool : undefined,
    engine: typeof raw.engine === "string" ? raw.engine : undefined,
    mode: raw.mode === "comparison" ? "comparison" : raw.mode === "review" ? "review" : undefined,
    documents: strings("documents"),
    missingClauses: strings("missingClauses"),
    keywords: strings("keywords"),
    versionChanges,
    disclaimer: typeof raw.disclaimer === "string" ? raw.disclaimer : undefined,
  };
}

export function parseContractReviewSections(content: string): ContractReviewReportSection[] {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) return [];

  const sections: ContractReviewReportSection[] = [];
  let currentTitle = "审核结果";
  let currentLines: string[] = [];
  const flush = () => {
    const sectionContent = currentLines.join("\n").trim();
    if (!sectionContent) return;
    const title = currentTitle.replace(/^\d+[.、]\s*/, "").trim();
    sections.push({
      id: `${sections.length}-${title}`,
      title,
      content: sectionContent,
      defaultOpen: DEFAULT_OPEN_SECTIONS.has(title) || sections.length === 0,
    });
  };

  for (const line of normalized.split("\n")) {
    const heading = line.match(/^#{2,3}\s+(.+?)\s*$/);
    if (heading) {
      flush();
      currentTitle = heading[1];
      currentLines = [];
      continue;
    }
    if (/^#\s+/.test(line)) continue;
    currentLines.push(line);
  }
  flush();
  return sections;
}

export function contractRiskCounts(content: string) {
  return {
    high: (content.match(/高风险/g) ?? []).length,
    medium: (content.match(/中风险/g) ?? []).length,
    notice: (content.match(/(?:低风险|提示项|提示)/g) ?? []).length,
  };
}

export function friendlyContractReviewError(reason: unknown): string {
  const raw = reason instanceof Error ? reason.message : String(reason ?? "");
  const lower = raw.toLowerCase();
  if (lower.includes("ocr") || (lower.includes("pdf") && lower.includes("扫描"))) {
    return "无法解析该 PDF。当前版本暂不支持扫描型 PDF OCR，请改用带文字层的 PDF。";
  }
  if (
    lower.includes("provider") ||
    lower.includes("openai") ||
    lower.includes("model") ||
    lower.includes("模型") ||
    lower.includes("connect") ||
    lower.includes("network") ||
    lower.includes("timeout")
  ) {
    return "模型服务暂时不可用，请检查模型配置和网络连接后重试。";
  }
  if (lower.includes("format") || lower.includes("格式") || lower.includes("extension")) {
    return "无法解析该文件，请确认文件格式为 DOCX、文字层 PDF、TXT 或 MD。";
  }
  return "审核未能完成，请检查合同文件后重试。详细技术信息已保留在日志中。";
}
