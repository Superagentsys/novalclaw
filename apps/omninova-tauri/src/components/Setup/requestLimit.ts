export type RequestLimitInputStatus = "blank" | "valid" | "invalid";

export interface RequestLimitParseResult {
  status: RequestLimitInputStatus;
  value?: number;
  error?: string;
}

export function formatTokenCount(value: number | null | undefined): string {
  if (value == null || value <= 0) return "";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}K`;
  return String(value);
}

/**
 * Parses the raw Settings field for 单次请求最大输出.
 *
 * blank -> unset (valid)
 * positive integer within known model max -> valid
 * anything else -> invalid, and the caller must NOT mutate persisted config.
 */
export function parseRequestLimitInput(raw: string, modelMax?: number | null): RequestLimitParseResult {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { status: "blank" };
  }
  if (!/^\d+$/.test(trimmed)) {
    return { status: "invalid", error: "请输入正整数 Token 数。" };
  }
  const parsed = Number(trimmed);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    return { status: "invalid", error: "请输入大于 0 的 Token 数；留空表示使用 OmniNova 默认策略。" };
  }
  if (modelMax != null && modelMax > 0 && parsed > modelMax) {
    return {
      status: "invalid",
      error: `不能超过模型最大输出上限 ${formatTokenCount(modelMax)}。`,
    };
  }
  return { status: "valid", value: parsed };
}

export function requestLimitError(raw: string, modelMax?: number | null): string | null {
  return parseRequestLimitInput(raw, modelMax).error ?? null;
}

/**
 * Applies a raw field edit to the persisted provider value.
 *
 * Invalid input returns the existing value unchanged so failed validation can
 * never erase a previously configured limit.
 */
export function applyRequestLimitInput(
  current: number | null | undefined,
  raw: string,
  modelMax?: number | null
): number | null | undefined {
  const parsed = parseRequestLimitInput(raw, modelMax);
  if (parsed.status === "blank") return undefined;
  if (parsed.status === "valid") return parsed.value;
  return current;
}

export function formatRequestLimitInput(value: number | null | undefined): string {
  return value == null ? "" : String(value);
}
