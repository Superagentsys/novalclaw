import React, { useState } from "react";
import type { SkillsConfig } from "../../types/config";
import { invokeTauri } from "../../utils/tauri";

interface Props {
  config: SkillsConfig;
  onChange: (config: SkillsConfig) => void;
}

export const SkillsConfigForm: React.FC<Props> = ({ config, onChange }) => {
  const [importPath, setImportPath] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);

  const handleImport = async () => {
    if (!importPath) return;
    setIsImporting(true);
    setImportStatus(null);
    try {
      const result = await invokeTauri<string>("import_skills", { sourceDir: importPath });
      setImportStatus(`✅ ${result}`);
    } catch (e) {
      setImportStatus(`❌ 导入失败: ${String(e)}`);
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <div className="space-y-8">
      {/* Enable Toggle */}
      <div className="flex items-center justify-between p-4 bg-white/5 rounded-lg border border-white/10">
        <div>
          <h4 className="text-white font-medium">启用 Open Skills</h4>
          <p className="text-white/50 text-sm">允许 Agent 加载并使用外部技能（SKILL.md 格式）</p>
        </div>
        <button
          onClick={() => onChange({ ...config, open_skills_enabled: !config.open_skills_enabled })}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none ${
            config.open_skills_enabled ? "bg-blue-600" : "bg-white/10"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              config.open_skills_enabled ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      {config.open_skills_enabled && (
        <div className="space-y-6 animate-in fade-in slide-in-from-top-2 duration-300">
          
          {/* Config Section */}
          <div className="space-y-4 p-4 bg-white/5 rounded-lg border border-white/10">
            <h4 className="text-white font-medium text-sm uppercase tracking-wider opacity-70">基础配置</h4>
            
            <div>
              <label className="block text-white/70 text-sm font-medium mb-2">
                Skills 目录路径
              </label>
              <input
                type="text"
                value={config.open_skills_dir || ""}
                onChange={(e) => onChange({ ...config, open_skills_dir: e.target.value })}
                placeholder="~/.omninova/skills"
                className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50"
              />
              <p className="mt-1 text-white/30 text-xs">
                Agent 将从该目录及其子目录中扫描包含 SKILL.md 的文件夹
              </p>
            </div>

            <div>
              <label className="block text-white/70 text-sm font-medium mb-2">
                提示词注入模式
              </label>
              <select
                value={config.prompt_injection_mode || "full"}
                onChange={(e) => onChange({ ...config, prompt_injection_mode: e.target.value })}
                className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white focus:outline-none focus:border-blue-500/50"
              >
                <option value="full">全量注入 (推荐)</option>
                <option value="summary">仅注入摘要</option>
                <option value="disabled">不注入</option>
              </select>
            </div>
          </div>

          {/* Import Section */}
          <div className="space-y-4 p-4 bg-white/5 rounded-lg border border-white/10">
            <h4 className="text-white font-medium text-sm uppercase tracking-wider opacity-70">从 OpenClaw 导入</h4>
            <p className="text-white/50 text-sm">将 OpenClaw 格式的 skills 目录导入到当前工作区</p>
            
            <div className="flex gap-2">
              <input 
                type="text" 
                value={importPath}
                onChange={(e) => setImportPath(e.target.value)}
                placeholder="/path/to/openclaw/skills"
                className="flex-1 bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50"
              />
              <button
                onClick={handleImport}
                disabled={isImporting || !importPath}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:bg-white/10 disabled:text-white/30 text-white rounded-md transition-colors whitespace-nowrap cursor-pointer"
              >
                {isImporting ? "导入中..." : "开始导入"}
              </button>
            </div>
            
            {importStatus && (
              <div className={`mt-2 p-3 rounded text-sm ${importStatus.includes("❌") ? "bg-red-500/20 text-red-200" : "bg-green-500/20 text-green-200"}`}>
                {importStatus}
              </div>
            )}
          </div>

        </div>
      )}
    </div>
  );
};
