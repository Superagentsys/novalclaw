import { invokeTauri, isTauriEnvironment } from "./tauri";

/** 通过 Rust 按绝对路径读取（Tauri 拖放 / 系统文件对话框） */
export async function readComposerAttachmentsFromPaths(
  paths: string[]
): Promise<string> {
  if (!paths.length) return "";
  return invokeTauri<string>("read_composer_attachments", { paths });
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
