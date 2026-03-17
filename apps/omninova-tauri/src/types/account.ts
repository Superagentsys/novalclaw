/**
 * 账户管理类型定义
 *
 * 包含账户相关的数据模型和类型
 *
 * [Source: 2-11-local-account-management.md]
 */

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 账户信息（不含敏感数据）
 *
 * 用于前端显示，不包含密码哈希等敏感信息
 */
export interface AccountInfo {
  /** 用户名 */
  username: string;
  /** 是否在启动时要求密码验证 */
  require_password_on_startup: boolean;
  /** 创建时间（Unix 时间戳） */
  created_at: number;
  /** 更新时间（Unix 时间戳） */
  updated_at: number;
}

/**
 * 新账户创建数据
 */
export interface NewAccount {
  /** 用户名 */
  username: string;
  /** 密码（明文，将被后端哈希存储） */
  password: string;
}

/**
 * 账户更新数据
 *
 * 所有字段均为可选，仅更新提供的字段
 */
export interface AccountUpdate {
  /** 新用户名 */
  username?: string;
  /** 新密码（需要先验证当前密码） */
  new_password?: string;
  /** 是否在启动时要求密码验证 */
  require_password_on_startup?: boolean;
}

/**
 * 密码验证结果
 */
export interface PasswordVerifyResult {
  /** 验证是否成功 */
  success: boolean;
  /** 错误信息（如果验证失败） */
  error?: string;
}

/**
 * 账户状态
 */
export interface AccountStatus {
  /** 是否已创建账户 */
  has_account: boolean;
  /** 账户信息（如果存在） */
  account?: AccountInfo;
}