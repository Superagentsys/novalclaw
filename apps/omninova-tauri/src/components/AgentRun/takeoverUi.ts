import type { BrowserTakeoverPhase, BrowserTakeoverStateDto } from "./types";

export type TakeoverTone = "idle" | "waiting" | "human" | "warn" | "error";
export type TakeoverPrimaryAction = "take" | "release" | "none";

const PHASES: BrowserTakeoverPhase[] = [
  "agent_controlled",
  "takeover_requested",
  "human_controlled",
  "timed_out",
  "resynchronizing",
  "browser_lost",
];

export function isBrowserTakeoverPhase(value: string): value is BrowserTakeoverPhase {
  return PHASES.includes(value as BrowserTakeoverPhase);
}

export function shouldRefreshTakeoverFromEvent(
  eventType: string,
  eventRunId: string | undefined,
  currentRunId: string | null | undefined
): boolean {
  return (
    eventType === "browser_takeover_state_changed" &&
    Boolean(currentRunId) &&
    eventRunId === currentRunId
  );
}

export function shouldShowTakeoverCard(state: BrowserTakeoverStateDto | null): boolean {
  if (!state) return false;
  if (state.phase === "browser_lost") return true;
  if (state.phase === "agent_controlled") return state.eligible && !state.headless;
  return isBrowserTakeoverPhase(String(state.phase));
}

export function takeoverPrimaryAction(state: BrowserTakeoverStateDto): TakeoverPrimaryAction {
  if (state.phase === "agent_controlled" && state.eligible && !state.headless) {
    return "take";
  }
  if (state.phase === "human_controlled" || state.phase === "timed_out") {
    return "release";
  }
  return "none";
}

export function takeoverStatusCopy(state: BrowserTakeoverStateDto): {
  title: string;
  detail: string;
  tone: TakeoverTone;
} {
  switch (state.phase) {
    case "takeover_requested":
      return {
        title: "正在等待交接",
        detail: "正在等待当前浏览器操作完成，随后才会把控制权交给你。",
        tone: "waiting",
      };
    case "human_controlled":
      return {
        title: "浏览器控制权现在属于你",
        detail: reasonLine(state.reason) || "请在现有的托管 Chrome 窗口中直接操作，完成后交还控制权。",
        tone: "human",
      };
    case "timed_out":
      return {
        title: "人工接管已超时",
        detail: "控制权尚未交还给 Agent。请明确交还或取消接管。",
        tone: "warn",
      };
    case "resynchronizing":
      return {
        title: "正在刷新浏览器状态",
        detail: "OmniNova 正在刷新浏览器状态，随后才会继续。",
        tone: "waiting",
      };
    case "browser_lost":
      return {
        title: "托管浏览器会话已丢失",
        detail: "这不是一次成功的交还。当前托管浏览器已不可用。",
        tone: "error",
      };
    case "agent_controlled":
    default:
      if (state.headless) {
        return {
          title: "无法手动接管",
          detail:
            "当前浏览器会话以无头模式运行，无法手动接管。请启动可交互的浏览器会话后再使用人工接管。",
          tone: "warn",
        };
      }
      return {
        title: "浏览器由 Agent 控制",
        detail: "接管后，你可以直接操作同一个可见的 Chrome 窗口。",
        tone: "idle",
      };
  }
}

export function takeoverErrorCopy(error: string): string {
  if (error.includes("BrowserTakeoverUnsupportedHeadless")) {
    return "当前浏览器会话以无头模式运行，无法手动接管。请启动可交互的浏览器会话后再使用人工接管。";
  }
  if (error.includes("BrowserTakeoverRunNotFound") || error.includes("BrowserTakeoverUnavailable")) {
    return "当前任务已结束，无法继续接管浏览器。";
  }
  if (error.includes("BrowserTakeoverLost")) {
    return "托管浏览器会话已丢失。";
  }
  return error;
}

export function applyAuthoritativeTakeoverState(
  _previous: BrowserTakeoverStateDto | null,
  next: BrowserTakeoverStateDto
): BrowserTakeoverStateDto {
  return next;
}

function reasonLine(reason?: string | null): string {
  switch (reason) {
    case "captcha":
      return "原因：验证码。";
    case "mfa":
      return "原因：多因素验证。";
    case "qr_login":
      return "原因：二维码登录。";
    case "sms_verification":
      return "原因：短信验证。";
    case "sso":
      return "原因：SSO。";
    case "manual_correction":
      return "原因：需要手动修正。";
    case "unexpected_modal":
      return "原因：意外弹窗。";
    case "explicit_user_request":
      return "原因：你请求接管。";
    default:
      return "";
  }
}
