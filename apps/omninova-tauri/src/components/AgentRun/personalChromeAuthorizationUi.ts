import type { PersonalChromeAuthorizationStatusDto } from "./types";

export type PersonalChromeAuthorizationTone = "idle" | "waiting" | "ready" | "warn" | "error";
export type PersonalChromeAuthorizationAction = "approve" | "revoke" | "none";

export interface PersonalChromeAuthorizationCopy {
  title: string;
  detail: string;
  tone: PersonalChromeAuthorizationTone;
}

export function shouldShowPersonalChromeAuthorization(
  status: PersonalChromeAuthorizationStatusDto | null
): boolean {
  return Boolean(
    status &&
      (status.configured || status.extension_tab_granted || status.desktop_run_granted)
  );
}

export function personalChromeAuthorizationAction(
  status: PersonalChromeAuthorizationStatusDto
): PersonalChromeAuthorizationAction {
  if (status.full_access) return "none";
  if (status.extension_tab_granted && !status.desktop_run_granted) return "approve";
  if (status.extension_tab_granted || status.desktop_run_granted) return "revoke";
  return "none";
}

export function personalChromeAuthorizationCopy(
  status: PersonalChromeAuthorizationStatusDto
): PersonalChromeAuthorizationCopy {
  if (!status.transport_connected) {
    const protocolMismatch = status.state === "protocol_mismatch";
    return {
      title: protocolMismatch ? "Personal Chrome 版本不兼容" : "Personal Chrome 扩展未连接",
      detail: protocolMismatch
        ? "请更新扩展或桌面端，使两端协议版本保持一致。"
        : "请确认扩展已安装并连接到 OmniNova，然后在扩展中允许当前标签页。",
      tone: protocolMismatch ? "error" : "warn",
    };
  }
  if (status.state === "authorization_error") {
    return {
      title: "无法读取 Personal Chrome 授权",
      detail: personalChromeAuthorizationErrorCopy(status.error_code),
      tone: "error",
    };
  }
  if (!status.extension_tab_granted) {
    return {
      title: "等待 Chrome 标签页授权",
      detail: "在 OmniNova 扩展中选择当前标签页并点击“允许当前标签页”。",
      tone: "waiting",
    };
  }
  if (status.full_access && status.ready) {
    return {
      title: "Personal Chrome 已就绪",
      detail: "完全访问模式已免除任务级批准；Chrome 扩展权限和浏览器运行状态仍受保护。",
      tone: "ready",
    };
  }
  if (!status.desktop_run_granted) {
    return {
      title: "标签页已授权",
      detail: "还需允许当前任务使用该标签页；许可只在本次任务存续期间有效。",
      tone: "waiting",
    };
  }
  if (status.state === "authorization_stale") {
    return {
      title: "Personal Chrome 授权已变化",
      detail: "扩展授权已被更新或撤销。请撤销当前任务许可后重新授权。",
      tone: "warn",
    };
  }
  if (!status.production_factory_enabled) {
    return {
      title: "当前任务授权已就绪",
      detail: "授权链路已建立；生产浏览器后端仍处于发布门禁关闭状态。",
      tone: "warn",
    };
  }
  if (status.ready) {
    return {
      title: "Personal Chrome 已授权",
      detail: "仅当前任务可访问扩展明确允许的标签页。",
      tone: "ready",
    };
  }
  return {
    title: "Personal Chrome 授权尚未就绪",
    detail: "请重新授权当前标签页，或撤销后重试。",
    tone: "warn",
  };
}

export function personalChromeAuthorizationErrorCopy(error: string | null): string {
  switch (error) {
    case "PersonalChromeNotAuthorized":
      return "扩展尚未允许任何标签页。";
    case "PersonalChromeAuthorizationAmbiguous":
      return "检测到多个授权标签页。请撤销后只允许一个标签页。";
    case "PersonalChromeAuthorizationInvalid":
      return "扩展返回的授权状态无效，请撤销后重新授权。";
    case "PersonalChromeNotConfigured":
      return "当前浏览器后端未配置为 Personal Chrome。";
    case "PersonalChromeExtensionDisconnected":
      return "扩展连接已断开，请重新连接后重试。";
    default:
      return error || "授权状态暂时不可用，请稍后重试。";
  }
}

export function applyAuthoritativePersonalChromeStatus(
  _previous: PersonalChromeAuthorizationStatusDto | null,
  next: PersonalChromeAuthorizationStatusDto
): PersonalChromeAuthorizationStatusDto {
  return next;
}
