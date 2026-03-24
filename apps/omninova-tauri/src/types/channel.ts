/**
 * 渠道类型定义
 *
 * 包含渠道状态、渠道信息和活动统计相关的数据模型
 *
 * [Source: architecture.md#channels]
 * [Source: types.rs - ChannelStatus, ChannelInfo]
 */

// ============================================================================
// 渠道状态类型
// ============================================================================

/**
 * 渠道连接状态
 */
export type ChannelStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

/**
 * 渠道类型
 */
export type ChannelKind =
  | 'slack'
  | 'discord'
  | 'email'
  | 'telegram'
  | 'wechat'
  | 'feishu'
  | 'webhook'
  | string; // 支持自定义类型

/**
 * 渠道能力标志位
 *
 * 与 Rust 后端 bitflags 对应
 */
export const ChannelCapabilities = {
  TEXT: 0b0000_0000_0001,
  RICH_TEXT: 0b0000_0000_0010,
  FILES: 0b0000_0000_0100,
  IMAGES: 0b0000_0000_1000,
  THREADS: 0b0000_0001_0000,
  REPLIES: 0b0000_0010_0000,
  EDIT: 0b0000_0100_0000,
  DELETE: 0b0000_1000_0000,
  MENTIONS: 0b0001_0000_0000,
  REACTIONS: 0b0010_0000_0000,
  DIRECT_MESSAGE: 0b0100_0000_0000,
  CHANNEL_MESSAGE: 0b1000_0000_0000,
} as const;

export type ChannelCapability = typeof ChannelCapabilities[keyof typeof ChannelCapabilities];

/**
 * 检查能力标志位
 */
export function hasCapability(capabilities: number, capability: ChannelCapability): boolean {
  return (capabilities & capability) !== 0;
}

// ============================================================================
// 渠道信息接口
// ============================================================================

/**
 * 渠道信息（与 Rust 后端 ChannelInfo 一致）
 */
export interface ChannelInfo {
  /** 渠道实例 ID */
  id: string;
  /** 渠道名称 */
  name: string;
  /** 渠道类型 */
  kind: ChannelKind;
  /** 连接状态 */
  status: ChannelStatus;
  /** 渠道能力标志位 */
  capabilities: number;
  /** 已发送消息数 */
  messagesSent: number;
  /** 已接收消息数 */
  messagesReceived: number;
  /** 最后活动时间（Unix 时间戳） */
  lastActivity: number | null;
  /** 错误信息（仅在错误状态时） */
  errorMessage: string | null;
}

/**
 * 渠道活动统计
 */
export interface ChannelActivityStats {
  /** 渠道 ID */
  channelId: string;
  /** 今日发送消息数 */
  messagesSentToday: number;
  /** 今日接收消息数 */
  messagesReceivedToday: number;
  /** 平均响应时间（毫秒） */
  averageResponseTime: number | null;
  /** 最后错误信息 */
  lastError: string | null;
  /** 最后错误时间（Unix 时间戳） */
  lastErrorTime: number | null;
}

// ============================================================================
// 渠道事件类型
// ============================================================================

/**
 * 渠道事件类型
 */
export type ChannelEventType =
  | 'connected'
  | 'disconnected'
  | 'error'
  | 'reconnecting'
  | 'message_received'
  | 'message_sent'
  | 'created'
  | 'removed'
  | 'agent_bound'
  | 'agent_unbound'
  | 'behavior_changed';

/**
 * 渠道事件基础接口
 */
export interface ChannelEventBase {
  /** 事件类型 */
  type: ChannelEventType;
  /** 渠道 ID */
  channelId: string;
}

/**
 * 连接事件
 */
export interface ChannelConnectedEvent extends ChannelEventBase {
  type: 'connected';
  channelKind: ChannelKind;
}

/**
 * 断开事件
 */
export interface ChannelDisconnectedEvent extends ChannelEventBase {
  type: 'disconnected';
  reason: string | null;
}

/**
 * 错误事件
 */
export interface ChannelErrorEvent extends ChannelEventBase {
  type: 'error';
  error: string;
}

/**
 * 重连事件
 */
export interface ChannelReconnectingEvent extends ChannelEventBase {
  type: 'reconnecting';
  attempt: number;
}

/**
 * 消息接收事件
 */
export interface ChannelMessageReceivedEvent extends ChannelEventBase {
  type: 'message_received';
  messageId: string;
}

/**
 * 消息发送事件
 */
export interface ChannelMessageSentEvent extends ChannelEventBase {
  type: 'message_sent';
  messageId: string;
}

/**
 * 渠道创建事件
 */
export interface ChannelCreatedEvent extends ChannelEventBase {
  type: 'created';
  channelKind: ChannelKind;
}

/**
 * 渠道移除事件
 */
export interface ChannelRemovedEvent extends ChannelEventBase {
  type: 'removed';
}

/**
 * 代理绑定事件
 */
export interface ChannelAgentBoundEvent extends ChannelEventBase {
  type: 'agent_bound';
  agentId: string;
}

/**
 * 代理解绑事件
 */
export interface ChannelAgentUnboundEvent extends ChannelEventBase {
  type: 'agent_unbound';
}

/**
 * 行为配置变更事件
 */
export interface ChannelBehaviorChangedEvent extends ChannelEventBase {
  type: 'behavior_changed';
}

/**
 * 所有渠道事件的联合类型
 */
export type ChannelEvent =
  | ChannelConnectedEvent
  | ChannelDisconnectedEvent
  | ChannelErrorEvent
  | ChannelReconnectingEvent
  | ChannelMessageReceivedEvent
  | ChannelMessageSentEvent
  | ChannelCreatedEvent
  | ChannelRemovedEvent
  | ChannelAgentBoundEvent
  | ChannelAgentUnboundEvent
  | ChannelBehaviorChangedEvent;

// ============================================================================
// 状态颜色常量
// ============================================================================

/**
 * 渠道状态颜色配置
 */
export const CHANNEL_STATUS_COLORS = {
  connected: {
    bg: 'bg-green-500',
    text: 'text-green-500',
    border: 'border-green-500',
    badge: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300',
  },
  disconnected: {
    bg: 'bg-gray-500',
    text: 'text-gray-500',
    border: 'border-gray-500',
    badge: 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300',
  },
  connecting: {
    bg: 'bg-yellow-500',
    text: 'text-yellow-500',
    border: 'border-yellow-500',
    badge: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-300',
  },
  error: {
    bg: 'bg-red-500',
    text: 'text-red-500',
    border: 'border-red-500',
    badge: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-300',
  },
} as const;

/**
 * 渠道状态显示文本
 */
export const CHANNEL_STATUS_LABELS: Record<ChannelStatus, string> = {
  connected: '已连接',
  disconnected: '已断开',
  connecting: '连接中',
  error: '错误',
};

/**
 * 渠道类型显示文本
 */
export const CHANNEL_KIND_LABELS: Record<string, string> = {
  slack: 'Slack',
  discord: 'Discord',
  email: '邮件',
  telegram: 'Telegram',
  wechat: '微信',
  feishu: '飞书',
  webhook: 'Webhook',
};

/**
 * 获取渠道类型显示文本
 */
export function getChannelKindLabel(kind: ChannelKind): string {
  return CHANNEL_KIND_LABELS[kind] ?? kind;
}

/**
 * 格式化时间差
 */
export function formatTimeAgo(timestamp: number | null): string {
  if (timestamp === null) {
    return '无';
  }

  const now = Date.now();
  const diff = now - timestamp * 1000; // 转换为毫秒

  if (diff < 60000) {
    return '刚刚';
  } else if (diff < 3600000) {
    return `${Math.floor(diff / 60000)} 分钟前`;
  } else if (diff < 86400000) {
    return `${Math.floor(diff / 3600000)} 小时前`;
  } else {
    return `${Math.floor(diff / 86400000)} 天前`;
  }
}

// ============================================================================
// 渠道配置类型 (Story 6.8)
// ============================================================================

/**
 * 响应风格类型
 */
export type ResponseStyleType = 'formal' | 'casual' | 'detailed' | 'concise';

/**
 * 触发关键词匹配类型
 */
export type MatchType = 'exact' | 'prefix' | 'contains' | 'regex';

/**
 * 触发关键词配置
 */
export interface TriggerKeyword {
  /** 关键词 */
  keyword: string;
  /** 匹配类型 */
  matchType: MatchType;
  /** 是否区分大小写 */
  caseSensitive: boolean;
}

/**
 * 响应延迟类型
 */
export type ResponseDelayType = 'none' | 'fixed' | 'random' | 'typing';

/**
 * 响应延迟配置
 */
export interface ResponseDelay {
  /** 延迟类型 */
  type: ResponseDelayType;
  /** 固定延迟（毫秒） */
  fixedMs?: number;
  /** 随机延迟最小值（毫秒） */
  minMs?: number;
  /** 随机延迟最大值（毫秒） */
  maxMs?: number;
  /** 打字速度（字符/秒） */
  typingSpeed?: number;
}

/**
 * 时间段配置
 */
export interface TimeSlot {
  /** 开始时间（HH:MM 格式） */
  start: string;
  /** 结束时间（HH:MM 格式） */
  end: string;
}

/**
 * 工作时间配置
 */
export interface WorkingHours {
  /** 时间段列表 */
  slots: TimeSlot[];
  /** 时区 */
  timezone: string;
  /** 工作日（0=周日, 1=周一, ..., 6=周六） */
  weekdays: number[];
}

/**
 * 渠道行为配置
 */
export interface ChannelBehaviorConfig {
  /** 响应风格 */
  responseStyle: ResponseStyleType;
  /** 触发关键词 */
  triggerKeywords: TriggerKeyword[];
  /** 最大响应长度（0=无限制） */
  maxResponseLength: number;
  /** 响应延迟 */
  responseDelay: ResponseDelay;
  /** 工作时间 */
  workingHours: WorkingHours | null;
}

/**
 * Slack 渠道凭据
 */
export interface SlackCredentials {
  /** Bot Token (xoxb-...) */
  botToken: string;
  /** App Token (xapp-...)，可选 */
  appToken?: string;
  /** 监听的频道列表 */
  channels?: string[];
}

/**
 * Discord 渠道凭据
 */
export interface DiscordCredentials {
  /** Bot Token */
  botToken: string;
  /** 服务器 ID */
  guildId?: string;
  /** 监听的频道列表 */
  channels?: string[];
}

/**
 * 邮件渠道凭据
 */
export interface EmailCredentials {
  /** IMAP 服务器地址 */
  imapHost: string;
  /** IMAP 端口 */
  imapPort: number;
  /** SMTP 服务器地址 */
  smtpHost: string;
  /** SMTP 端口 */
  smtpPort: number;
  /** 用户名 */
  username: string;
  /** 密码 */
  password: string;
  /** 是否使用 TLS */
  useTls: boolean;
}

/**
 * Telegram 渠道凭据
 */
export interface TelegramCredentials {
  /** Bot Token */
  botToken: string;
}

/**
 * 渠道凭据联合类型
 */
export type ChannelCredentials =
  | { kind: 'slack'; data: SlackCredentials }
  | { kind: 'discord'; data: DiscordCredentials }
  | { kind: 'email'; data: EmailCredentials }
  | { kind: 'telegram'; data: TelegramCredentials }
  | { kind: 'webhook'; data: Record<string, unknown> }
  | { kind: string; data: Record<string, unknown> };

/**
 * 渠道完整配置
 */
export interface ChannelConfig {
  /** 渠道 ID */
  id: string;
  /** 渠道名称 */
  name: string;
  /** 渠道类型 */
  kind: ChannelKind;
  /** 是否启用 */
  enabled: boolean;
  /** 行为配置 */
  behavior: ChannelBehaviorConfig;
  /** 绑定的代理 ID */
  agentId?: string;
}

/**
 * 配置字段定义
 */
export interface ConfigField {
  /** 字段名 */
  name: string;
  /** 显示标签 */
  label: string;
  /** 字段类型 */
  type: 'text' | 'password' | 'number' | 'select' | 'checkbox' | 'textarea';
  /** 是否必填 */
  required: boolean;
  /** 占位符 */
  placeholder?: string;
  /** 帮助文本 */
  helpText?: string;
  /** 默认值 */
  defaultValue?: string | number | boolean;
  /** 选项（用于 select 类型） */
  options?: { value: string; label: string }[];
  /** 最小值（用于 number 类型） */
  min?: number;
  /** 最大值（用于 number 类型） */
  max?: number;
}

/**
 * 渠道类型定义
 */
export interface ChannelTypeDefinition {
  /** 渠道类型 */
  kind: ChannelKind;
  /** 显示名称 */
  name: string;
  /** 描述 */
  description: string;
  /** 图标名称（lucide-react） */
  icon: string;
  /** 功能特性 */
  features: string[];
  /** 配置字段 */
  configFields: ConfigField[];
}

// ============================================================================
// 渠道类型定义常量
// ============================================================================

/**
 * 渠道类型定义列表
 */
export const CHANNEL_TYPE_DEFINITIONS: ChannelTypeDefinition[] = [
  {
    kind: 'slack',
    name: 'Slack',
    description: '连接到 Slack 工作区，让代理在频道和私信中响应消息',
    icon: 'MessageSquare',
    features: ['频道消息', '私信', '线程回复', '富文本'],
    configFields: [
      {
        name: 'botToken',
        label: 'Bot Token',
        type: 'password',
        required: true,
        placeholder: 'xoxb-...',
        helpText: '在 Slack 应用配置页面获取的 Bot User OAuth Token',
      },
      {
        name: 'appToken',
        label: 'App Token',
        type: 'password',
        required: false,
        placeholder: 'xapp-...',
        helpText: '可选，用于 Socket Mode 连接',
      },
    ],
  },
  {
    kind: 'discord',
    name: 'Discord',
    description: '连接到 Discord 服务器，让代理在服务器频道中响应消息',
    icon: 'Hash',
    features: ['服务器消息', '@提及', '嵌入消息', '表情反应'],
    configFields: [
      {
        name: 'botToken',
        label: 'Bot Token',
        type: 'password',
        required: true,
        placeholder: 'Bot Token...',
        helpText: '在 Discord 开发者门户创建 Bot 后获取的 Token',
      },
      {
        name: 'guildId',
        label: '服务器 ID',
        type: 'text',
        required: false,
        placeholder: '123456789012345678',
        helpText: '可选，指定要监听的服务器',
      },
    ],
  },
  {
    kind: 'email',
    name: '电子邮件',
    description: '连接到邮件服务器，让代理通过邮件收发消息',
    icon: 'Mail',
    features: ['收发邮件', '线程追踪', '附件支持', 'HTML 格式'],
    configFields: [
      {
        name: 'imapHost',
        label: 'IMAP 服务器',
        type: 'text',
        required: true,
        placeholder: 'imap.example.com',
      },
      {
        name: 'imapPort',
        label: 'IMAP 端口',
        type: 'number',
        required: true,
        defaultValue: '993',
        min: 1,
        max: 65535,
      },
      {
        name: 'smtpHost',
        label: 'SMTP 服务器',
        type: 'text',
        required: true,
        placeholder: 'smtp.example.com',
      },
      {
        name: 'smtpPort',
        label: 'SMTP 端口',
        type: 'number',
        required: true,
        defaultValue: '587',
        min: 1,
        max: 65535,
      },
      {
        name: 'username',
        label: '用户名',
        type: 'text',
        required: true,
        placeholder: 'your@email.com',
      },
      {
        name: 'password',
        label: '密码',
        type: 'password',
        required: true,
        helpText: '应用专用密码（如果启用了两步验证）',
      },
      {
        name: 'useTls',
        label: '使用 TLS',
        type: 'checkbox',
        required: false,
        defaultValue: true,
      },
    ],
  },
  {
    kind: 'telegram',
    name: 'Telegram',
    description: '连接到 Telegram Bot，让代理在聊天中响应消息',
    icon: 'Send',
    features: ['私聊', '群组消息', '内联键盘', '文件'],
    configFields: [
      {
        name: 'botToken',
        label: 'Bot Token',
        type: 'password',
        required: true,
        placeholder: '123456:ABC-DEF...',
        helpText: '在 BotFather 创建 Bot 后获取的 Token',
      },
    ],
  },
  {
    kind: 'webhook',
    name: 'Webhook',
    description: '通过 HTTP Webhook 接收和发送消息',
    icon: 'Webhook',
    features: ['HTTP 请求', '自定义格式', 'API 集成'],
    configFields: [
      {
        name: 'endpoint',
        label: 'Webhook URL',
        type: 'text',
        required: true,
        placeholder: 'https://example.com/webhook',
      },
      {
        name: 'secret',
        label: '签名密钥',
        type: 'password',
        required: false,
        helpText: '可选，用于验证请求签名',
      },
    ],
  },
];

/**
 * 根据渠道类型获取配置定义
 */
export function getChannelTypeDefinition(kind: ChannelKind): ChannelTypeDefinition | undefined {
  return CHANNEL_TYPE_DEFINITIONS.find((def) => def.kind === kind);
}

/**
 * 响应风格显示文本
 */
export const RESPONSE_STYLE_LABELS: Record<ResponseStyleType, string> = {
  formal: '正式',
  casual: '随意',
  detailed: '详细',
  concise: '简洁',
};

/**
 * 匹配类型显示文本
 */
export const MATCH_TYPE_LABELS: Record<MatchType, string> = {
  exact: '精确匹配',
  prefix: '前缀匹配',
  contains: '包含匹配',
  regex: '正则表达式',
};

/**
 * 创建默认渠道行为配置
 */
export function createDefaultBehaviorConfig(): ChannelBehaviorConfig {
  return {
    responseStyle: 'detailed',
    triggerKeywords: [],
    maxResponseLength: 0,
    responseDelay: {
      type: 'none',
    },
    workingHours: null,
  };
}

/**
 * 创建默认触发关键词
 */
export function createDefaultTriggerKeyword(keyword: string = ''): TriggerKeyword {
  return {
    keyword,
    matchType: 'contains',
    caseSensitive: false,
  };
}

/**
 * 掩码敏感值
 */
export function maskSensitiveValue(value: string, visibleChars: number = 4): string {
  if (value.length <= visibleChars) {
    return '*'.repeat(value.length);
  }
  const prefix = value.slice(0, visibleChars);
  const masked = '*'.repeat(Math.min(value.length - visibleChars, 20));
  return `${prefix}${masked}`;
}