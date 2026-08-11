import React, { useMemo, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { type AgentPersonaConfig, MBTI_TYPES } from "../../types/config";
import { UiIcon } from "../UiIcon";

interface Props {
  config: AgentPersonaConfig;
  onChange: (config: AgentPersonaConfig) => void;
}

export const PersonaConfigForm: React.FC<Props> = ({ config, onChange }) => {
  const selectedMBTI = useMemo(() => {
    return config.mbti_type && MBTI_TYPES[config.mbti_type]
      ? MBTI_TYPES[config.mbti_type]
      : null;
  }, [config.mbti_type]);

  const handlePickWorkspaceDir = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择该 Agent 的 Workspace",
      });
      if (selected != null) {
        onChange({ ...config, workspace_dir: selected as string });
      }
    } catch (error) {
      console.error("选择目录失败:", error);
    }
  }, [config, onChange]);

  const handleClearWorkspaceDir = useCallback(() => {
    onChange({ ...config, workspace_dir: undefined });
  }, [config, onChange]);

  const handleMBTIChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const code = e.target.value;
    if (!code) {
      onChange({ ...config, mbti_type: undefined });
      return;
    }

    const mbti = MBTI_TYPES[code];
    if (mbti) {
      // If there's already a system prompt, we might want to append or confirm replacement
      // For now, let's just append if it's not already there, or replace if empty
      let newPrompt = config.system_prompt || "";
      
      // Simple check to avoid duplicating the template if it's already somewhat present
      if (!newPrompt.includes(mbti.name)) {
        newPrompt = mbti.system_prompt_template + "\n\n" + newPrompt;
      }
      
      onChange({
        ...config,
        mbti_type: code,
        system_prompt: newPrompt.trim(),
      });
    } else {
      onChange({ ...config, mbti_type: undefined });
    }
  };

  return (
    <div className="persona-form">
      <div className="persona-form-grid">
        <div className="persona-field">
          <label className="persona-label" htmlFor="persona-agent-name">
            Agent 名称
          </label>
          <input
            id="persona-agent-name"
            type="text"
            value={config.name}
            onChange={(e) => onChange({ ...config, name: e.target.value })}
            placeholder="omninova"
            className="persona-input"
          />
        </div>

        <div className="persona-field">
          <label className="persona-label" htmlFor="persona-max-tool-iterations">
            最大工具迭代次数
          </label>
          <input
            id="persona-max-tool-iterations"
            type="number"
            value={config.max_tool_iterations || 20}
            onChange={(e) => onChange({ ...config, max_tool_iterations: parseInt(e.target.value) || 20 })}
            className="persona-input"
          />
        </div>
      </div>

      {/* Per-Agent Workspace */}
      <section className="persona-panel" aria-labelledby="persona-workspace-heading">
        <div className="persona-panel-head">
          <h3 className="persona-panel-title" id="persona-workspace-heading">
            <UiIcon name="folder" size={17} />
            Workspace 目录
          </h3>
        </div>
        <div className="persona-inline-row">
          <input
            id="persona-workspace-dir"
            type="text"
            value={config.workspace_dir ?? ""}
            onChange={(e) => onChange({ ...config, workspace_dir: e.target.value || undefined })}
            placeholder="未设置，使用全局 Workspace"
            className="persona-input"
            aria-label="Agent Workspace 目录"
            aria-describedby="persona-workspace-help"
          />
          <button
            type="button"
            onClick={() => void handlePickWorkspaceDir()}
            className="setup-btn setup-btn--secondary"
          >
            选择目录
          </button>
          <button
            type="button"
            onClick={handleClearWorkspaceDir}
            disabled={!config.workspace_dir}
            className="setup-btn setup-btn--ghost"
          >
            清空
          </button>
        </div>
        <p className="persona-help" id="persona-workspace-help">
          未设置时，该 Agent 使用全局 Workspace 目录。设置后可让不同 Agent 操作不同目录。
        </p>
      </section>

      {/* MBTI Selection */}
      <section className="persona-panel" aria-labelledby="persona-mbti-heading">
        <div className="persona-panel-head">
          <h3 className="persona-panel-title" id="persona-mbti-heading">
            <UiIcon name="experiment" size={17} />
            MBTI 人格构建
          </h3>
          <select
            id="persona-mbti-type"
            value={config.mbti_type || ""}
            onChange={handleMBTIChange}
            className="persona-select"
            aria-labelledby="persona-mbti-heading"
          >
            <option value="">自定义 / 无人格</option>
            <optgroup label="Analysts (分析家)">
              <option value="INTJ">INTJ - 战略家</option>
              <option value="INTP">INTP - 逻辑学家</option>
              <option value="ENTJ">ENTJ - 指挥官</option>
              <option value="ENTP">ENTP - 辩论家</option>
            </optgroup>
            <optgroup label="Diplomats (外交家)">
              <option value="INFJ">INFJ - 提倡者</option>
              <option value="INFP">INFP - 调停者</option>
              <option value="ENFJ">ENFJ - 主人公</option>
              <option value="ENFP">ENFP - 竞选者</option>
            </optgroup>
            <optgroup label="Sentinels (守护者)">
              <option value="ISTJ">ISTJ - 物流师</option>
              <option value="ISFJ">ISFJ - 守卫者</option>
              <option value="ESTJ">ESTJ - 总经理</option>
              <option value="ESFJ">ESFJ - 执政官</option>
            </optgroup>
            <optgroup label="Explorers (探险家)">
              <option value="ISTP">ISTP - 鉴赏家</option>
              <option value="ISFP">ISFP - 探险家</option>
              <option value="ESTP">ESTP - 企业家</option>
              <option value="ESFP">ESFP - 表演者</option>
            </optgroup>
          </select>
        </div>

        {selectedMBTI && (
          <div className="persona-detail-grid">
            <div className="persona-detail-card">
              <div className="persona-detail-label">认知栈</div>
              <div className="persona-tags">
                {selectedMBTI.cognitive_stack.map((func: string) => (
                  <span key={func} className="persona-tag">
                    {func}
                  </span>
                ))}
              </div>
            </div>
            <div className="persona-detail-card">
              <div className="persona-detail-label">描述</div>
              <p>{selectedMBTI.description}</p>
            </div>
            <div className="persona-detail-card">
              <div className="persona-detail-label">交互风格</div>
              <p>{selectedMBTI.interaction_style}</p>
            </div>
            <div className="persona-detail-card">
              <div className="persona-detail-label">优势</div>
              <div className="persona-tags">
                {selectedMBTI.strengths.map((s: string) => (
                  <span key={s} className="persona-tag">
                    {s}
                  </span>
                ))}
              </div>
            </div>
          </div>
        )}
      </section>

      <div className="persona-field">
        <label className="persona-label" htmlFor="persona-system-prompt">
          System Prompt (人设/灵魂)
        </label>
        <textarea
          id="persona-system-prompt"
          value={config.system_prompt || ""}
          onChange={(e) => onChange({ ...config, system_prompt: e.target.value })}
          placeholder="You are a helpful AI assistant..."
          rows={12}
          className="persona-textarea"
          aria-describedby="persona-system-prompt-help"
        />
        <p className="persona-help" id="persona-system-prompt-help">
          定义 Agent 的行为、语气和核心指令。选择 MBTI 类型会自动填充建议的 Prompt。
        </p>
      </div>

      <section className="persona-panel">
        <div className="persona-switch-row">
          <div>
            <h3 className="persona-panel-title">Compact Context</h3>
            <p id="persona-compact-context-help">压缩历史上下文以节省 Token</p>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={config.compact_context}
            aria-label="压缩历史上下文"
            aria-describedby="persona-compact-context-help"
            onClick={() => onChange({ ...config, compact_context: !config.compact_context })}
            className={`persona-switch${config.compact_context ? " is-on" : ""}`}
          >
            <span aria-hidden="true" />
          </button>
        </div>
      </section>
    </div>
  );
};
