export interface CommandPaletteItem {
  id: string;
  kind: string;
  displayName: string;
  description: string;
  source: string;
  sourceBadge: string;
  commandAlias: string;
  aliases: string[];
  enabled: boolean;
}

export interface CommandPalette {
  generation: number;
  openSkillsEnabled: boolean;
  system: CommandPaletteItem[];
  skills: CommandPaletteItem[];
  skillsEmptyReason: string | null;
}

export interface SkillInvocationDto {
  skillId: string;
  source: string;
}

export interface SelectedSkill {
  id: string;
  displayName: string;
  commandAlias: string;
}

export interface SelectedSystemTool {
  id: "tool:contract-review";
  displayName: string;
  commandAlias: string;
}

export function commandTokenAt(
  input: string,
  cursor: number
): { start: number; end: number; token: string } | null {
  const clamped = Math.max(0, Math.min(cursor, input.length));
  const before = input.slice(0, clamped);
  let start = 0;
  for (let i = before.length - 1; i >= 0; i -= 1) {
    if (/\s/.test(before[i])) {
      start = i + 1;
      break;
    }
  }
  const after = input.slice(clamped);
  const whitespace = after.search(/\s/);
  const end = whitespace < 0 ? input.length : clamped + whitespace;
  const token = input.slice(start, end);
  if (token.startsWith("/")) {
    return { start, end, token };
  }
  return null;
}

export function filterCommandPalette(palette: CommandPalette, query: string): CommandPalette {
  const needle = query.trim().replace(/^\//, "").toLowerCase();
  const keep = (item: CommandPaletteItem) => {
    if (!needle) return true;
    return (
      item.displayName.toLowerCase().includes(needle) ||
      item.id.toLowerCase().includes(needle) ||
      item.commandAlias.toLowerCase().includes(needle) ||
      item.aliases.some((alias) => alias.toLowerCase().includes(needle)) ||
      item.description.toLowerCase().includes(needle)
    );
  };
  const skills = palette.skills.filter(keep);
  return {
    ...palette,
    system: palette.system.filter(keep),
    skills,
    skillsEmptyReason:
      !palette.openSkillsEnabled
        ? palette.skillsEmptyReason ?? "技能功能已关闭"
        : skills.length === 0
          ? palette.skillsEmptyReason ?? "暂无可用技能"
          : null,
  };
}

export function paletteRows(palette: CommandPalette): CommandPaletteItem[] {
  return [...palette.system, ...palette.skills];
}

export function parseLeadingSlashCommand(
  line: string
): { id: string; rest: string } | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("/")) return null;
  const parts = trimmed.split(/\s+/);
  const token = parts[0] ?? "";
  const restFrom1 = parts.slice(1).join(" ").trim();
  const lower = token.toLowerCase();
  if (lower === "/help" || token === "/?") {
    return { id: "system:help", rest: restFrom1 };
  }
  if (lower === "/skills") {
    return { id: "system:skills", rest: restFrom1 };
  }
  if (lower === "/contract" || token === "/合同" || token === "/合同审核") {
    return { id: "tool:contract-review", rest: restFrom1 };
  }
  if (lower === "/skill") {
    const slug = parts[1]?.trim() ?? "";
    if (!slug) return { id: "system:help", rest: "" };
    return { id: `skill:${slug.replace(/^\//, "")}`, rest: parts.slice(2).join(" ").trim() };
  }
  return { id: `skill:${token.slice(1)}`, rest: restFrom1 };
}

export function emptyCommandPalette(): CommandPalette {
  return {
    generation: 0,
    openSkillsEnabled: true,
    system: [
      {
        id: "system:help",
        kind: "system",
        displayName: "Help",
        description: "Show available slash commands",
        source: "system",
        sourceBadge: "系统",
        commandAlias: "/help",
        aliases: ["/?"],
        enabled: true,
      },
      {
        id: "system:skills",
        kind: "system",
        displayName: "Skills",
        description: "List installed skills in the catalog",
        source: "system",
        sourceBadge: "系统",
        commandAlias: "/skills",
        aliases: [],
        enabled: true,
      },
      {
        id: "tool:contract-review",
        kind: "system_tool",
        displayName: "合同智能审核",
        description: "上传合同进行关键条款审查、风险识别、缺漏检查和版本比对",
        source: "system_tool",
        sourceBadge: "系统工具",
        commandAlias: "/contract",
        aliases: ["/合同", "/合同审核"],
        enabled: true,
      },
    ],
    skills: [],
    skillsEmptyReason: "暂无可用技能",
  };
}

export function resolveComposerSend(
  raw: string,
  selected: SelectedSkill | undefined,
  palette: CommandPalette
): { text: string; invocations: SkillInvocationDto[]; local?: "help" | "skills"; systemTool?: "contract-review" } {
  if (selected) {
    const parsed = parseLeadingSlashCommand(raw);
    const text = parsed && parsed.id === selected.id ? parsed.rest : raw;
    return {
      text,
      invocations: [{ skillId: selected.id, source: "slash_command" }],
    };
  }
  const parsed = parseLeadingSlashCommand(raw);
  if (!parsed) return { text: raw, invocations: [] };
  if (parsed.id === "system:help") {
    return { text: parsed.rest, invocations: [], local: "help" };
  }
  if (parsed.id === "system:skills") {
    return { text: parsed.rest, invocations: [], local: "skills" };
  }
  if (parsed.id === "tool:contract-review") {
    return { text: parsed.rest, invocations: [], systemTool: "contract-review" };
  }
  const slug = parsed.id.replace(/^skill:/, "");
  const found = palette.skills.find(
    (item) => item.id === parsed.id || item.commandAlias === `/${slug}` || item.commandAlias === parsed.id
  );
  if (found) {
    return {
      text: parsed.rest,
      invocations: [{ skillId: found.id, source: "slash_command" }],
    };
  }
  return { text: raw, invocations: [] };
}
