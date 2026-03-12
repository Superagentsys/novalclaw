import React from "react";
import type { AgentPersonaConfig } from "../../types/config";

interface Props {
  config: AgentPersonaConfig;
  onChange: (config: AgentPersonaConfig) => void;
}

export const PersonaConfigForm: React.FC<Props> = ({ config, onChange }) => {
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

      <div>
        <label className="block text-white/70 text-sm font-medium mb-2">
          System Prompt (人设/灵魂)
        </label>
        <textarea
          value={config.system_prompt || ""}
          onChange={(e) => onChange({ ...config, system_prompt: e.target.value })}
          placeholder="You are a helpful AI assistant..."
          rows={8}
          className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50 font-mono text-sm"
        />
        <p className="mt-1 text-white/30 text-xs">
          定义 Agent 的行为、语气和核心指令。
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
