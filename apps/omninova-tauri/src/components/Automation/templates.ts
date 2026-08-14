import type { UiIconName } from "../../components/UiIcon";

export type AutomationJobStatus = "success" | "failed" | "running" | "skipped";

export interface AutomationJob {
  id: string;
  name: string;
  schedule: string;
  prompt: string;
  command: string;
  description: string;
  template_id?: string | null;
  tz_offset_minutes: number;
  enabled: boolean;
  last_run?: string | null;
  last_status?: AutomationJobStatus | null;
  next_run?: string | null;
  last_error?: string | null;
  created_at: string;
}

export interface AutomationRun {
  id: string;
  job_id: string;
  job_name: string;
  trigger: string;
  status: AutomationJobStatus;
  started_at: string;
  finished_at?: string | null;
  duration_ms?: number | null;
  output?: string | null;
  error?: string | null;
}

export interface AutomationJobInput {
  id?: string;
  name: string;
  schedule: string;
  prompt: string;
  description?: string;
  templateId?: string;
  tzOffsetMinutes?: number;
  enabled?: boolean;
}

export type ScheduleKind = "daily" | "weekdays" | "weekly" | "interval" | "cron";

export interface ScheduleDraft {
  kind: ScheduleKind;
  hour: string;
  minute: string;
  weekday: string;
  intervalValue: string;
  intervalUnit: "m" | "h" | "d";
  cron: string;
}

export interface AutomationTemplate {
  id: string;
  title: string;
  description: string;
  icon: UiIconName;
  prompt: string;
  schedule: ScheduleDraft;
}

export const WEEKDAY_OPTIONS = [
  { value: "1", label: "周一" },
  { value: "2", label: "周二" },
  { value: "3", label: "周三" },
  { value: "4", label: "周四" },
  { value: "5", label: "周五" },
  { value: "6", label: "周六" },
  { value: "0", label: "周日" },
] as const;

export const AUTOMATION_TEMPLATES: AutomationTemplate[] = [
  {
    id: "ai-news",
    title: "每日 AI 资讯推送",
    description: "每天早上汇总 AI 编程与具身智能热点。",
    icon: "global",
    schedule: daily("09", "00"),
    prompt:
      "请检索并总结过去 24 小时内最值得关注的 AI 编程、大模型与具身智能资讯，输出 5 条，每条包含标题、一句话摘要和来源。",
  },
  {
    id: "english-words",
    title: "每日 5 个英语单词",
    description: "推荐高频词，附释义、发音与例句。",
    icon: "writing",
    schedule: daily("08", "00"),
    prompt:
      "请推荐 5 个适合工作场景的高频英语单词。每个词给出音标、中文释义、一个例句，以及一句记忆提示。",
  },
  {
    id: "bedtime-story",
    title: "每日儿童睡前故事",
    description: "生成 3 到 5 分钟、情节完整的睡前故事。",
    icon: "idea",
    schedule: daily("20", "30"),
    prompt:
      "请创作一个适合 4-8 岁儿童的睡前故事，时长约 3 到 5 分钟朗读。要求情节完整、语言温和，结尾给出一句积极的小道理。",
  },
  {
    id: "weekly-report",
    title: "每周工作周报",
    description: "每周五汇总本周进展、风险与下周计划。",
    icon: "fileText",
    schedule: weekly("5", "17", "00"),
    prompt:
      "请根据当前工作区与近期任务，起草一份本周工作周报，包含本周完成事项、进行中事项、风险阻塞和下周计划。用简洁条目列出。",
  },
  {
    id: "movie-pick",
    title: "经典电影推荐",
    description: "推荐一部高分电影并给出看点。",
    icon: "video",
    schedule: weekly("6", "19", "00"),
    prompt:
      "请推荐一部高分经典电影，给出类型、时长、一句话看点、适合什么心情看，以及不要剧透的简介。",
  },
  {
    id: "today-in-history",
    title: "历史上的今天",
    description: "挑选科技、电影或音乐中的有趣事件。",
    icon: "history",
    schedule: daily("07", "30"),
    prompt:
      "请介绍历史上的今天中 3 件有趣事件，优先覆盖科技、电影或音乐。每件包含年份、事件与一句为什么值得记住。",
  },
  {
    id: "daily-why",
    title: "每日一个为什么",
    description: "提出一个有趣问题并给出简明答案。",
    icon: "experiment",
    schedule: daily("12", "00"),
    prompt:
      "请提出一个有趣的「为什么」问题，并给出 200 字以内、准确且好懂的答案，适合午饭时随手看完。",
  },
  {
    id: "call-parents",
    title: "给父母打电话提醒",
    description: "每周日上午提醒联系家人。",
    icon: "team",
    schedule: weekly("0", "10", "00"),
    prompt:
      "请用温暖但不啰嗦的口吻提醒我现在给父母打个电话或发条消息。可以附一句适合今天说的问候语。",
  },
  {
    id: "health-checkup",
    title: "体检预约提醒",
    description: "定期提醒核对体检时间与注意事项。",
    icon: "safety",
    schedule: weekly("1", "09", "00"),
    prompt:
      "请提醒我核对近期体检或复查安排。列出需要提前准备的事项（空腹、报告、证件），并问我是否需要改期。",
  },
  {
    id: "interview-prep",
    title: "面试复习提醒",
    description: "工作日安排 2 小时大模型面试复习。",
    icon: "knowledge",
    schedule: weekdays("21", "00"),
    prompt:
      "请为我安排今晚 2 小时的大模型面试复习计划：包含 3 个重点题目、每题的回答框架，以及 20 分钟的口头演练建议。",
  },
  {
    id: "meeting-prep",
    title: "会前准备提醒",
    description: "工作日早上整理议程、目标与问题。",
    icon: "message",
    schedule: weekdays("08", "30"),
    prompt:
      "请提醒我做会前准备：列出今天可能的会议目标、需要提前同步的材料，以及 3 个值得在会上提出的问题。",
  },
  {
    id: "pet-wallpaper",
    title: "萌宠手机壁纸",
    description: "随机生成竖版萌宠壁纸创意描述。",
    icon: "palette",
    schedule: daily("18", "00"),
    prompt:
      "请设计一张 9:16 竖版萌宠手机壁纸的详细画面描述，从 7 种风格中随机选一种（水彩、像素、赛博、手账、油画、粘土、日系）。给出主体、配色、构图和可直接用于文生图的提示词。",
  },
];

function daily(hour: string, minute: string): ScheduleDraft {
  return emptyDraft({ kind: "daily", hour, minute });
}

function weekdays(hour: string, minute: string): ScheduleDraft {
  return emptyDraft({ kind: "weekdays", hour, minute });
}

function weekly(weekday: string, hour: string, minute: string): ScheduleDraft {
  return emptyDraft({ kind: "weekly", weekday, hour, minute });
}

export function emptyDraft(overrides: Partial<ScheduleDraft> = {}): ScheduleDraft {
  return {
    kind: "daily",
    hour: "09",
    minute: "00",
    weekday: "1",
    intervalValue: "30",
    intervalUnit: "m",
    cron: "0 9 * * *",
    ...overrides,
  };
}

export function localTzOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

export function pad2(value: string | number): string {
  return String(value).padStart(2, "0");
}

export function compileSchedule(draft: ScheduleDraft): string {
  const hour = clampNumber(draft.hour, 0, 23);
  const minute = clampNumber(draft.minute, 0, 59);
  switch (draft.kind) {
    case "daily":
      return `${minute} ${hour} * * *`;
    case "weekdays":
      return `${minute} ${hour} * * 1-5`;
    case "weekly":
      return `${minute} ${hour} * * ${draft.weekday || "1"}`;
    case "interval": {
      const value = Math.max(1, Number.parseInt(draft.intervalValue, 10) || 1);
      return `every ${value}${draft.intervalUnit}`;
    }
    case "cron":
      return draft.cron.trim();
  }
}

export function parseSchedule(spec: string): ScheduleDraft {
  const trimmed = spec.trim();
  const interval = /^every\s+(\d+)\s*(m|h|d|min|mins|minute|minutes|hour|hours|day|days)$/i.exec(
    trimmed,
  );
  if (interval) {
    const rawUnit = interval[2].toLowerCase();
    const intervalUnit: ScheduleDraft["intervalUnit"] = rawUnit.startsWith("d")
      ? "d"
      : rawUnit.startsWith("h")
        ? "h"
        : "m";
    return emptyDraft({
      kind: "interval",
      intervalValue: interval[1],
      intervalUnit,
      cron: trimmed,
    });
  }

  const fields = trimmed.split(/\s+/);
  if (fields.length === 5) {
    const [minute, hour, day, month, weekday] = fields;
    if (day === "*" && month === "*") {
      if (weekday === "*") {
        return emptyDraft({ kind: "daily", hour: pad2(hour), minute: pad2(minute), cron: trimmed });
      }
      if (weekday === "1-5") {
        return emptyDraft({
          kind: "weekdays",
          hour: pad2(hour),
          minute: pad2(minute),
          cron: trimmed,
        });
      }
      if (/^[0-7]$/.test(weekday)) {
        return emptyDraft({
          kind: "weekly",
          weekday,
          hour: pad2(hour),
          minute: pad2(minute),
          cron: trimmed,
        });
      }
    }
    return emptyDraft({ kind: "cron", cron: trimmed, hour: pad2(hour), minute: pad2(minute) });
  }

  return emptyDraft({ kind: "cron", cron: trimmed || "0 9 * * *" });
}

export function describeSchedule(spec: string): string {
  const draft = parseSchedule(spec);
  const time = `${pad2(draft.hour)}:${pad2(draft.minute)}`;
  switch (draft.kind) {
    case "daily":
      return `每天 ${time}`;
    case "weekdays":
      return `工作日 ${time}`;
    case "weekly": {
      const weekday = WEEKDAY_OPTIONS.find((item) => item.value === draft.weekday)?.label ?? "指定日";
      return `每${weekday} ${time}`;
    }
    case "interval":
      return `每 ${draft.intervalValue}${
        draft.intervalUnit === "m" ? " 分钟" : draft.intervalUnit === "h" ? " 小时" : " 天"
      }`;
    case "cron":
      return spec;
  }
}

export function formatDateTime(value?: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatDuration(ms?: number | null): string {
  if (ms == null) return "—";
  if (ms < 1000) return `${ms} ms`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const remain = seconds % 60;
  return remain ? `${minutes} 分 ${remain} 秒` : `${minutes} 分钟`;
}

export function jobStatusLabel(status?: AutomationJobStatus | null): string {
  switch (status) {
    case "success":
      return "成功";
    case "failed":
      return "失败";
    case "running":
      return "运行中";
    case "skipped":
      return "已跳过";
    default:
      return "未运行";
  }
}

function clampNumber(raw: string, min: number, max: number): number {
  const parsed = Number.parseInt(raw, 10);
  if (Number.isNaN(parsed)) return min;
  return Math.min(max, Math.max(min, parsed));
}
