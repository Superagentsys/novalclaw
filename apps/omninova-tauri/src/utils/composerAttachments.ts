import { invokeTauri, isTauriEnvironment } from "./tauri";

/** 通过 Rust 按绝对路径读取（Tauri 拖放 / 系统文件对话框） */
export async function readComposerAttachmentsFromPaths(
  paths: string[]
): Promise<string> {
  if (!paths.length) return "";
  return invokeTauri<string>("read_composer_attachments", { paths });
}

export interface PreparedComposerAttachment {
  name: string;
  requestedPath: string;
  originalPath: string;
  workspaceRelativePath: string;
  size: number;
  kind: "image" | "text" | "office" | "file";
  content: string;
  note: string;
}

export interface PrepareComposerAttachmentsResult {
  attachments: PreparedComposerAttachment[];
  skipped: Array<{ path: string; error: string }>;
}

/** 将桌面绝对路径附件复制/挂载进当前 Workspace，并生成 Agent 可读取的上下文。 */
export async function prepareComposerAttachments(
  paths: string[],
  workspacePath: string,
  sessionId: string,
): Promise<PrepareComposerAttachmentsResult> {
  if (!paths.length) return { attachments: [], skipped: [] };
  return invokeTauri<PrepareComposerAttachmentsResult>("prepare_composer_attachments", {
    paths,
    workspacePath,
    sessionId,
  });
}

/** 系统文件选择对话框（桌面 Tauri） */
export async function pickComposerAttachmentPaths(): Promise<string[]> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: true,
    title: "选择附件",
  });
  if (selected == null) return [];
  return Array.isArray(selected) ? selected : [selected];
}

export { isTauriEnvironment };
