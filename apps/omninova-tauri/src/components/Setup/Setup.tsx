import { useMemo, useState } from "react";
import {
  DEFAULT_PROVIDERS,
  DEFAULT_ROBOT_CONFIG,
  type AppConfig,
} from "../../types/config";
import { ProviderConfigForm } from "./ProviderConfigForm";
import { RobotConfigForm } from "./RobotConfigForm";

const initialConfig: AppConfig = {
  api_key: "",
  api_url: "",
  default_provider: "",
  default_model: "",
  workspace_dir: "",
  omninoval_gateway_url: "http://localhost:18789",
  omninoval_config_dir: "~/.omninoval",
  robot: DEFAULT_ROBOT_CONFIG,
  providers: DEFAULT_PROVIDERS,
};

export function Setup() {
  const [config, setConfig] = useState<AppConfig>(initialConfig);

  const jsonPreview = useMemo(
    () => JSON.stringify(config, null, 2),
    [config]
  );

  return (
    <div className="setup-page">
      <header className="setup-header">
        <div>
          <div className="setup-title">OmniNova 启动配置</div>
          <div className="setup-subtitle">
            参照 novalclaw-main 与 omninoval 的关键配置点
          </div>
        </div>
      </header>

      <section className="setup-section">
        <h2>基础信息</h2>
        <div className="setup-grid">
          <label>
            Workspace 目录
            <input
              value={config.workspace_dir}
              onChange={(event) =>
                setConfig({ ...config, workspace_dir: event.target.value })
              }
              placeholder="/path/to/workspace"
            />
          </label>
          <label>
            默认模型服务
            <input
              value={config.default_provider ?? ""}
              onChange={(event) =>
                setConfig({ ...config, default_provider: event.target.value })
              }
              placeholder="openai / anthropic / ollama"
            />
          </label>
          <label>
            默认模型
            <input
              value={config.default_model ?? ""}
              onChange={(event) =>
                setConfig({ ...config, default_model: event.target.value })
              }
              placeholder="gpt-4o"
            />
          </label>
          <label>
            API 地址
            <input
              value={config.api_url ?? ""}
              onChange={(event) =>
                setConfig({ ...config, api_url: event.target.value })
              }
              placeholder="https://api.openai.com/v1"
            />
          </label>
          <label>
            API Key
            <input
              value={config.api_key ?? ""}
              onChange={(event) =>
                setConfig({ ...config, api_key: event.target.value })
              }
              placeholder="sk-..."
            />
          </label>
        </div>
      </section>

      <section className="setup-section">
        <h2>omninoval 连接</h2>
        <div className="setup-grid">
          <label>
            Gateway 地址
            <input
              value={config.omninoval_gateway_url ?? ""}
              onChange={(event) =>
                setConfig({
                  ...config,
                  omninoval_gateway_url: event.target.value,
                })
              }
              placeholder="http://localhost:18789"
            />
          </label>
          <label>
            配置目录
            <input
              value={config.omninoval_config_dir ?? ""}
              onChange={(event) =>
                setConfig({
                  ...config,
                  omninoval_config_dir: event.target.value,
                })
              }
              placeholder="~/.omninoval"
            />
          </label>
        </div>
      </section>

      <RobotConfigForm
        value={config.robot ?? DEFAULT_ROBOT_CONFIG}
        onChange={(robot) => setConfig({ ...config, robot })}
      />

      <ProviderConfigForm
        value={config.providers}
        onChange={(providers) => setConfig({ ...config, providers })}
      />

      <section className="setup-section">
        <h2>配置预览</h2>
        <textarea readOnly value={jsonPreview} className="setup-preview" />
      </section>
    </div>
  );
}
