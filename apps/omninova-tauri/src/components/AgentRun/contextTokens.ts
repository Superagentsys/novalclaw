/**
 * Single token display formatter for Context observability.
 * Arithmetic always uses raw integers; this module only formats at the edge.
 */

export type TokenFormatKind = "estimate" | "exact";

export function formatTokenMagnitude(tokens: number): string {
  const value = Math.max(0, Math.round(tokens));
  if (value >= 1_000_000) {
    const millions = Math.round((value / 1_000_000) * 100) / 100;
    const text = millions.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
    return `${text.includes(".") ? text : `${text}.0`}M`;
  }
  if (value >= 1_000) {
    return `${Math.round(value / 1_000)}K`;
  }
  return String(value);
}

export function formatTokenCount(tokens: number, kind: TokenFormatKind = "exact"): string {
  const magnitude = formatTokenMagnitude(tokens);
  return kind === "estimate" ? `~${magnitude}` : magnitude;
}

export function formatEstimatedTokens(tokens: number): string {
  return formatTokenCount(tokens, "estimate");
}

export function formatWindowTokens(tokens: number): string {
  return formatTokenCount(tokens, "exact");
}

export function primaryUsageRatio(estimatedInputTokens: number, maxInputTokens: number | null | undefined): number | null {
  if (maxInputTokens == null || maxInputTokens <= 0) return null;
  return estimatedInputTokens / maxInputTokens;
}

export function primaryUsagePercent(estimatedInputTokens: number, maxInputTokens: number | null | undefined): number | null {
  const ratio = primaryUsageRatio(estimatedInputTokens, maxInputTokens);
  if (ratio == null) return null;
  return Math.round(ratio * 100);
}

export function clampPercent(percent: number): number {
  if (percent < 0) return 0;
  if (percent > 100) return 100;
  return percent;
}
