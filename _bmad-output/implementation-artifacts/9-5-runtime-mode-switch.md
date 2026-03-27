# Story 9.5: 运行模式管理

**Story ID:** 9.5
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 9 - 系统监控与管理

---

## 用户故事

**As a** 用户,
**I want** 在不同运行模式间切换,
**So that** 我可以根据需要选择合适的使用方式.

---

## 验收标准

### 功能验收标准

1. **Given** 桌面应用已安装, **When** 我切换运行模式, **Then** 可以选择桌面模式（完整界面）
2. **Given** 桌面应用已安装, **When** 我切换运行模式, **Then** 可以选择后台服务模式（最小化到系统托盘）
3. **Given** 应用在后台模式, **When** 查看系统托盘, **Then** 系统托盘图标显示应用状态
4. **Given** 应用在后台模式, **When** 点击托盘图标, **Then** 托盘菜单提供快速操作（新建对话、退出等）
5. **Given** 应用运行, **When** 我配置开机自启动, **Then** 系统启动时自动运行应用
6. **Given** 后台模式时, **When** 有 API 请求或渠道消息, **Then** 应用仍可正常响应

### 非功能验收标准

- 托盘图标响应时间 < 200ms
- 模式切换无闪烁或卡顿
- 跨平台支持（macOS、Windows、Linux）

---

## 技术需求

### 后端实现 (Rust + Tauri 2.x)

#### 1. 系统托盘配置

**Tauri 2.x 托盘 API 变更:**
- Tauri 2.x 使用 `tauri::tray::TrayIconBuilder` 而非 1.x 的 `SystemTray`
- 需要在 `tauri.conf.json` 中配置托盘图标

**位置:** `apps/omninova-tauri/src-tauri/src/lib.rs`

```rust
use tauri::{
    tray::{TrayIcon, TrayIconBuilder},
    menu::{Menu, MenuItem},
    Manager, WindowEvent,
};

/// 运行模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// 桌面模式（显示窗口）
    Desktop,
    /// 后台服务模式（最小化到托盘）
    Background,
}

/// 运行模式管理器
pub struct RunModeManager {
    mode: parking_lot::RwLock<RunMode>,
    auto_start: parking_lot::RwLock<bool>,
}

impl RunModeManager {
    pub fn new() -> Self {
        Self {
            mode: parking_lot::RwLock::new(RunMode::Desktop),
            auto_start: parking_lot::RwLock::new(false),
        }
    }

    pub fn get_mode(&self) -> RunMode {
        self.mode.read().clone()
    }

    pub fn set_mode(&self, mode: RunMode) {
        *self.mode.write() = mode;
    }

    pub fn is_auto_start(&self) -> bool {
        *self.auto_start.read()
    }

    pub fn set_auto_start(&self, enabled: bool) {
        *self.auto_start.write() = enabled;
    }
}
```

#### 2. Tauri Commands

```rust
/// 获取当前运行模式
#[tauri::command]
fn get_run_mode(manager: tauri::State<'_, RunModeManager>) -> RunMode {
    manager.get_mode()
}

/// 设置运行模式
#[tauri::command]
fn set_run_mode(
    mode: RunMode,
    app: tauri::AppHandle,
    manager: tauri::State<'_, RunModeManager>,
) -> Result<(), String> {
    manager.set_mode(mode.clone());

    match mode {
        RunMode::Desktop => {
            // 显示窗口
            if let Some(window) = app.get_webview_window("main") {
                window.show().map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
            }
        }
        RunMode::Background => {
            // 隐藏窗口到托盘
            if let Some(window) = app.get_webview_window("main") {
                window.hide().map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

/// 获取开机自启动状态
#[tauri::command]
fn get_auto_start(manager: tauri::State<'_, RunModeManager>) -> bool {
    manager.is_auto_start()
}

/// 设置开机自启动
#[tauri::command]
async fn set_auto_start(
    enabled: bool,
    app: tauri::AppHandle,
    manager: tauri::State<'_, RunModeManager>,
) -> Result<(), String> {
    manager.set_auto_start(enabled);

    // 使用 tauri-plugin-autostart 或手动实现
    #[cfg(target_os = "macos")]
    {
        // macOS: 使用 LaunchAgent
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: 使用注册表
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: 使用 Desktop Entry
    }

    Ok(())
}
```

#### 3. 托盘菜单

```rust
fn setup_tray(app: &tauri::AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    // 创建菜单项
    let new_chat = MenuItem::new(app, "新建对话", true, None::<&str>)?;
    let show_window = MenuItem::new(app, "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::new(app, "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&new_chat, &show_window, &quit])?;

    // 创建托盘图标
    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "新建对话" => {
                    // 打开新对话
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        let _ = window.emit("new-chat", ());
                    }
                }
                "显示窗口" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "退出" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // 单击托盘图标显示窗口
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(tray)
}
```

#### 4. 窗口关闭行为

```rust
// 修改窗口关闭行为：关闭时最小化到托盘而非退出
fn setup_window_close_behavior(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 阻止默认关闭行为
                api.prevent_close();
                // 隐藏窗口到托盘
                let _ = app_handle.get_webview_window("main").map(|w| w.hide());
            }
        });
    }
}
```

### 前端实现 (React + TypeScript)

#### 1. 类型定义

**位置:** `apps/omninova-tauri/src/types/runtime.ts`

```typescript
/**
 * 运行模式
 */
export type RunMode = 'desktop' | 'background';

/**
 * 运行模式配置
 */
export interface RunModeConfig {
  /** 当前运行模式 */
  mode: RunMode;
  /** 是否开机自启动 */
  autoStart: boolean;
}
```

#### 2. 状态管理

**位置:** `apps/omninova-tauri/src/stores/runtimeStore.ts`

```typescript
import { create } from 'zustand';
import type { RunMode, RunModeConfig } from '@/types/runtime';

interface RuntimeState {
  mode: RunMode;
  autoStart: boolean;
  setMode: (mode: RunMode) => Promise<void>;
  setAutoStart: (enabled: boolean) => Promise<void>;
  loadConfig: () => Promise<void>;
}

export const useRuntimeStore = create<RuntimeState>((set, get) => ({
  mode: 'desktop',
  autoStart: false,

  setMode: async (mode) => {
    // 调用 Tauri command
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_run_mode', { mode });
    set({ mode });
  },

  setAutoStart: async (enabled) => {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_auto_start', { enabled });
    set({ autoStart: enabled });
  },

  loadConfig: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const mode = await invoke<RunMode>('get_run_mode');
    const autoStart = await invoke<boolean>('get_auto_start');
    set({ mode, autoStart });
  },
}));
```

#### 3. 组件结构

**位置:** `apps/omninova-tauri/src/components/runtime/`

```
runtime/
├── RunModeSwitch.tsx       # 运行模式切换组件
├── AutoStartToggle.tsx     # 开机自启动开关
├── TrayIcon.tsx            # 托盘图标状态显示（可选，用于设置页）
└── index.ts
```

#### 4. 设置页面集成

在设置页面添加"运行模式"设置项：

```tsx
// RunModeSwitch.tsx
import { useRuntimeStore } from '@/stores/runtimeStore';
import type { RunMode } from '@/types/runtime';

export function RunModeSwitch() {
  const { mode, setMode } = useRuntimeStore();

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-medium">运行模式</h3>
      <div className="flex gap-4">
        <button
          onClick={() => setMode('desktop')}
          className={cn(
            "px-4 py-2 rounded-lg border",
            mode === 'desktop' ? "border-primary bg-primary/10" : "border-border"
          )}
        >
          桌面模式
        </button>
        <button
          onClick={() => setMode('background')}
          className={cn(
            "px-4 py-2 rounded-lg border",
            mode === 'background' ? "border-primary bg-primary/10" : "border-border"
          )}
        >
          后台模式
        </button>
      </div>
    </div>
  );
}
```

---

## 架构合规要求

### 命名约定

| 层级 | 规则 | 示例 |
|------|------|------|
| Rust 结构体 | PascalCase | `RunMode`, `RunModeManager` |
| Rust 函数 | snake_case | `get_run_mode()`, `set_run_mode()` |
| TypeScript 类型 | PascalCase | `RunMode`, `RunModeConfig` |
| TypeScript 函数 | camelCase | `setMode()`, `loadConfig()` |
| Tauri Commands | snake_case | `get_run_mode`, `set_run_mode` |

### 文件组织

```
apps/omninova-tauri/src-tauri/src/
├── lib.rs                   # 主入口，注册 commands 和 tray
└── runtime/                 # 运行模式模块（可选独立模块）
    ├── mod.rs
    └── manager.rs

apps/omninova-tauri/src/
├── types/runtime.ts         # 运行模式类型
├── stores/runtimeStore.ts   # 运行模式状态
└── components/runtime/      # 运行模式组件
```

### 平台差异处理

| 平台 | 托盘实现 | 自启动实现 |
|------|----------|------------|
| macOS | NSStatusItem (原生) | LaunchAgent plist |
| Windows | System Tray (原生) | 注册表 HKCU\Run |
| Linux | libayatana-appindicator | ~/.config/autostart/*.desktop |

---

## 测试要求

### Rust 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_mode_default() {
        let manager = RunModeManager::new();
        assert_eq!(manager.get_mode(), RunMode::Desktop);
    }

    #[test]
    fn test_run_mode_switch() {
        let manager = RunModeManager::new();
        manager.set_mode(RunMode::Background);
        assert_eq!(manager.get_mode(), RunMode::Background);
    }

    #[test]
    fn test_auto_start_default() {
        let manager = RunModeManager::new();
        assert!(!manager.is_auto_start());
    }
}
```

### 前端测试

- 运行模式切换交互测试
- 状态持久化测试
- 设置页面渲染测试

---

## 依赖关系

### 前置依赖

- ✅ Tauri 2.x 框架 (已使用 2.2.0)
- ✅ Shadcn/UI 组件库
- ✅ Zustand 状态管理

### 后置依赖

- 无直接依赖项

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 跨平台托盘差异 | 高 | 使用 Tauri 抽象层，针对平台测试 |
| 自启动权限问题 | 中 | 提供友好的权限引导 |
| Linux 托盘支持不完整 | 中 | 检测环境，降级处理 |

---

## 实施建议

### 推荐实施顺序

1. **托盘基础设施**
   - 配置 tauri.conf.json 托盘图标
   - 实现 setup_tray 函数
   - 添加托盘菜单

2. **运行模式管理**
   - 实现 RunModeManager
   - 添加 Tauri Commands
   - 修改窗口关闭行为

3. **开机自启动**
   - 实现跨平台自启动逻辑
   - 添加 Tauri Commands

4. **前端集成**
   - 类型定义和状态管理
   - 设置页面组件
   - 初始化加载配置

### 参考现有实现

- Story 9-1, 9-2, 9-3, 9-4 的 Tauri Commands 模式
- `observability/monitor.rs` - 后端服务模式参考
- `components/notifications/` - UI 组件模式参考

### Tauri 2.x 重要变更

**注意:** Tauri 2.x 与 1.x 的托盘 API 有显著差异：

```rust
// Tauri 1.x (不兼容)
use tauri::SystemTray;
let tray = SystemTray::new();

// Tauri 2.x (正确)
use tauri::tray::TrayIconBuilder;
let tray = TrayIconBuilder::new().build(app)?;
```

**配置文件变更:**

```json
// tauri.conf.json
{
  "productName": "OmniNova",
  "identifier": "com.omninova.app",
  "plugins": {
    // Tauri 2.x 不需要在此配置托盘
  }
}
```

---

## 完成标准

- [x] 后端 RunModeManager 实现
- [x] Tauri Commands (get_run_mode, set_run_mode, get_auto_start, set_auto_start)
- [x] 系统托盘图标和菜单
- [x] 窗口关闭时最小化到托盘
- [x] 开机自启动功能（跨平台）
- [x] 前端类型定义和状态管理
- [x] 设置页面运行模式组件
- [x] 后端单元测试覆盖核心逻辑
- [x] 更新 sprint-status.yaml 状态为 done

---

## Dev Agent Record

### Implementation Notes

**后端实现 (Rust + Tauri 2.x):**
- 创建 `RunMode` 枚举（Desktop, Background）
- 创建 `RunModeManager` 结构体，使用 `parking_lot::RwLock` 实现线程安全
- 实现 4 个 Tauri Commands: `get_run_mode`, `set_run_mode`, `get_auto_start`, `set_auto_start`
- 实现系统托盘（TrayIconBuilder），包含"显示窗口"和"退出"菜单
- 实现窗口关闭行为：关闭时隐藏到托盘而非退出
- 实现跨平台开机自启动：
  - macOS: LaunchAgent plist
  - Windows: 注册表 HKCU\Run
  - Linux: Desktop Entry

**前端实现 (React + TypeScript):**
- 创建 `types/runtime.ts` 定义 RunMode 类型
- 创建 `stores/runtimeStore.ts` 使用 Zustand 管理状态
- 创建 `components/runtime/RunModeSwitch.tsx` 运行模式切换组件
- 创建 `components/runtime/AutoStartToggle.tsx` 开机自启动开关

**技术决策:**
- 使用 `parking_lot::RwLock` 替代 `std::sync::RwLock`，性能更好
- 托盘菜单使用 Tauri 2.x 的 `menu` API
- 自启动功能使用平台原生机制

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/runtime.ts`
- `apps/omninova-tauri/src/stores/runtimeStore.ts`
- `apps/omninova-tauri/src/components/runtime/RunModeSwitch.tsx`
- `apps/omninova-tauri/src/components/runtime/AutoStartToggle.tsx`
- `apps/omninova-tauri/src/components/runtime/index.ts`

**修改文件:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加 RunMode, RunModeManager, Commands, Tray
- `apps/omninova-tauri/src-tauri/Cargo.toml` - 添加 parking_lot, dirs, winreg 依赖

### Change Log

- 2026-03-27: Story 创建，状态为 ready-for-dev
- 2026-03-27: 完成后端 RunModeManager 和 Tauri Commands
- 2026-03-27: 完成系统托盘和窗口关闭行为
- 2026-03-27: 完成跨平台开机自启动
- 2026-03-27: 完成前端类型、状态管理和组件
- 2026-03-27: 添加后端单元测试
- 2026-03-27: Story 状态更新为 review

---

## Review Findings

**Code Review: 2026-03-27**

- [x] [Review][Patch] Unused variable `get` in `runtimeStore.ts:51` — Fixed: Removed unused `get` from destructuring.

**Known Limitations:**

- The `--hidden` flag passed to autostart commands is not currently handled by the application. This is a placeholder for future enhancement. When autostart is enabled, the app will start with the window visible by default.