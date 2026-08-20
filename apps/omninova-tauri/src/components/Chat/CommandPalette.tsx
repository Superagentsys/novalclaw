import { UiIcon } from "../UiIcon";
import {
  paletteRows,
  type CommandPalette,
  type CommandPaletteItem,
} from "./commandPaletteModel";
import "./CommandPalette.css";

interface CommandPaletteProps {
  palette: CommandPalette;
  selectedIndex: number;
  onHover: (index: number) => void;
  onSelect: (item: CommandPaletteItem) => void;
}

export function CommandPalette({
  palette,
  selectedIndex,
  onHover,
  onSelect,
}: CommandPaletteProps) {
  const rows = paletteRows(palette);

  return (
    <div
      className="command-palette"
      role="listbox"
      aria-label="命令面板"
    >
      <section className="command-palette-section" aria-label="系统命令与工具">
        <div className="command-palette-heading">系统命令 / 工具</div>
        {palette.system.length ? (
          palette.system.map((item) => {
            const index = rows.findIndex((row) => row.id === item.id);
            return (
              <PaletteRow
                key={item.id}
                item={item}
                active={index === selectedIndex}
                onHover={() => onHover(index)}
                onSelect={() => onSelect(item)}
              />
            );
          })
        ) : (
          <div className="command-palette-empty">无匹配系统命令</div>
        )}
      </section>
      <section className="command-palette-section" aria-label="已安装技能">
        <div className="command-palette-heading">已安装技能</div>
        {palette.skills.length ? (
          palette.skills.map((item) => {
            const index = rows.findIndex((row) => row.id === item.id);
            return (
              <PaletteRow
                key={item.id}
                item={item}
                active={index === selectedIndex}
                onHover={() => onHover(index)}
                onSelect={() => onSelect(item)}
              />
            );
          })
        ) : (
          <div className="command-palette-empty">
            {palette.skillsEmptyReason ?? "暂无可用技能"}
          </div>
        )}
      </section>
    </div>
  );
}

function PaletteRow({
  item,
  active,
  onHover,
  onSelect,
}: {
  item: CommandPaletteItem;
  active: boolean;
  onHover: () => void;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      className={`command-palette-row${active ? " is-active" : ""}`}
      onMouseEnter={onHover}
      onMouseDown={(event) => {
        event.preventDefault();
        onSelect();
      }}
    >
      <span className="command-palette-icon" aria-hidden>
        <UiIcon name={item.kind === "skill" ? "apps" : "settings"} size={14} />
      </span>
      <span className="command-palette-copy">
        <span className="command-palette-name">{item.displayName}</span>
        <span className="command-palette-desc">{item.description || item.commandAlias}</span>
      </span>
      <span className={`command-palette-badge command-palette-badge--${item.source}`}>
        {item.sourceBadge}
      </span>
    </button>
  );
}
