import React, { useMemo } from "react";
import type { AgentPersonaConfig } from "../../types/config";
import { MBTI_TYPES } from "../../types/mbti";

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
    <div className="space-y-6">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="block text-white/70 text-sm font-medium mb-2">
            Agent 名称
          </label>
          <input
            type="text"
            value={config.name}
            onChange={(e) => onChange({ ...config, name: e.target.value })}
            placeholder="omninova"
            className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50"
          />
        </div>
        
        <div>
          <label className="block text-white/70 text-sm font-medium mb-2">
            最大工具迭代次数
          </label>
          <input
            type="number"
            value={config.max_tool_iterations || 20}
            onChange={(e) => onChange({ ...config, max_tool_iterations: parseInt(e.target.value) || 20 })}
            className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50"
          />
        </div>
      </div>

      {/* MBTI Selection */}
      <div className="bg-white/5 rounded-lg border border-white/10 p-4 space-y-4">
        <div className="flex items-center justify-between">
          <h4 className="text-white font-medium flex items-center gap-2">
            <span>🧬</span> MBTI 人格构建
          </h4>
          <select
            value={config.mbti_type || ""}
            onChange={handleMBTIChange}
            className="bg-black/20 border border-white/10 rounded px-3 py-1.5 text-white text-sm focus:outline-none focus:border-blue-500/50"
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
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="space-y-2">
              <div className="text-white/50 text-xs uppercase tracking-wider">认知栈</div>
              <div className="flex gap-2">
                {selectedMBTI.cognitive_stack.map((func: string) => (
                  <span key={func} className="px-2 py-0.5 bg-blue-500/20 text-blue-300 rounded text-xs font-mono">
                    {func}
                  </span>
                ))}
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-white/50 text-xs uppercase tracking-wider">描述</div>
              <p className="text-white/80">{selectedMBTI.description}</p>
            </div>
            <div className="space-y-1">
              <div className="text-white/50 text-xs uppercase tracking-wider">交互风格</div>
              <p className="text-white/80">{selectedMBTI.interaction_style}</p>
            </div>
            <div className="space-y-1">
              <div className="text-white/50 text-xs uppercase tracking-wider">优势</div>
              <div className="flex flex-wrap gap-1">
                {selectedMBTI.strengths.map((s: string) => (
                  <span key={s} className="px-1.5 py-0.5 bg-green-500/10 text-green-300 rounded text-xs">
                    {s}
                  </span>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>

      <div>
        <label className="block text-white/70 text-sm font-medium mb-2">
          System Prompt (人设/灵魂)
        </label>
        <textarea
          value={config.system_prompt || ""}
          onChange={(e) => onChange({ ...config, system_prompt: e.target.value })}
          placeholder="You are a helpful AI assistant..."
          rows={12}
          className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50 font-mono text-sm leading-relaxed"
        />
        <p className="mt-1 text-white/30 text-xs">
          定义 Agent 的行为、语气和核心指令。选择 MBTI 类型会自动填充建议的 Prompt。
        </p>
      </div>

      <div className="flex items-center justify-between p-4 bg-white/5 rounded-lg border border-white/10">
        <div>
          <h4 className="text-white font-medium">Compact Context</h4>
          <p className="text-white/50 text-sm">压缩历史上下文以节省 Token</p>
        </div>
        <button
          onClick={() => onChange({ ...config, compact_context: !config.compact_context })}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none ${
            config.compact_context ? "bg-blue-600" : "bg-white/10"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              config.compact_context ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>
    </div>
  );
};
