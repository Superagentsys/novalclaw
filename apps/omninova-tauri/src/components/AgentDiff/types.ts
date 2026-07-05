export type AgentDiffChangeType = "added" | "modified" | "deleted" | "unknown";

export type AgentDiffFileStatus = "pending" | "active" | "completed" | "failed" | "interrupted";

export type AgentDiffLineType = "context" | "add" | "remove";

export interface AgentDiffLine {
  type: AgentDiffLineType;
  oldLine?: number;
  newLine?: number;
  content: string;
}

export interface AgentDiffHunk {
  id: string;
  path: string;
  source?: "patch" | "file_write";
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  additions: number;
  deletions: number;
  summary: string;
  oldText?: string;
  newText?: string;
  textTruncated?: boolean;
  contentTotalChars?: number;
  contentPreviewChars?: number;
  lines: AgentDiffLine[];
}

export interface AgentChangedFile {
  path: string;
  changeType: AgentDiffChangeType;
  additions: number;
  deletions: number;
  status: AgentDiffFileStatus;
  hunks: AgentDiffHunk[];
  lastEventAt: number;
  toolCallIds: string[];
  summaryOnly?: boolean;
}

export interface AgentDiffRunState {
  runId: string;
  files: Record<string, AgentChangedFile>;
  orderedPaths: string[];
  totals: {
    files: number;
    additions: number;
    deletions: number;
  };
  activePath?: string;
  terminalStatus?: "completed" | "failed" | "cancelled";
}
