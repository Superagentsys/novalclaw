/**
 * 指令执行框架类型定义
 *
 * 包含聊天指令相关的数据模型和类型
 *
 * [Source: Story 4.10 - 指令执行框架]
 */

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 指令信息（用于帮助显示和列表）
 */
export interface CommandInfo {
  /** 指令名称（不带 / 前缀） */
  name: string;
  /** 人类可读的描述 */
  description: string;
  /** 使用示例（如 "/help" 或 "/export [format]"） */
  usage: string;
}

/**
 * 指令执行结果
 */
export interface CommandResult {
  /** 是否执行成功 */
  success: boolean;
  /** 显示给用户的消息 */
  message: string;
  /** 可选的结构化数据 */
  data?: Record<string, unknown>;
  /** 可用指令列表（用于 /help 或未知指令错误） */
  availableCommands?: CommandInfo[];
}

/**
 * 指令操作类型（从 data.action 字段解析）
 */
export type CommandAction = 'clear_messages' | 'export_session';

/**
 * 解析后的指令数据
 */
export interface CommandData {
  /** 操作类型 */
  action: CommandAction;
  /** 导出格式（仅 export_session 操作） */
  format?: 'json' | 'markdown' | 'text';
}

/**
 * 检查指令结果是否包含需要前端处理的操作
 */
export function isCommandAction(data: unknown): data is CommandData {
  if (typeof data !== 'object' || data === null) return false;
  const d = data as Record<string, unknown>;
  return typeof d['action'] === 'string';
}

/**
 * 解析指令结果中的操作数据
 */
export function parseCommandData(result: CommandResult): CommandData | null {
  if (result.data && isCommandAction(result.data)) {
    return result.data;
  }
  return null;
}