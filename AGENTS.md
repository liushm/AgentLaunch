# AgentLaunch

轻量级跨平台（Windows / macOS）AI Agent 代理启动器，无需开启全局 TUN 劫持所有流量，可为每个 Agent 精准指定独立代理。

## 特性

- **精准按进程代理**：
  - Electron 应用（Antigravity、Codex Desktop、Claude Desktop、Cursor、VS Code 等）：自动注入 `--proxy-server`、`--proxy-bypass-list`（分号分隔）与全套大小写代理环境变量（`HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY`/`NODE_USE_ENV_PROXY=1`）。
  - CLI Agent（Codex CLI、Claude Code 等）：自动注入环境变量并在终端启动。
- **动态路径发现**：
  - 不写死版本号路径，启动时动态解析最新版（例如 Windows 下 `OpenAI.Codex` 等 Appx/MSIX 应用每次更新都会变更目录，通过注册表毫秒级自动追踪最新版本）。
  - macOS 支持自动扫描 `/Applications` & `~/Applications` 下的 `.app` Bundle `Info.plist` 与 Homebrew/PATH。
- **单实例检测与智能重启**：检测到 Electron 应用已在运行时，自动切换为“重启”并终止旧实例以应用新代理。
- **一键创建桌面快捷方式**：保留原程序图标，双击静默无头注入代理启动，零黑框闪烁。
- **架构解耦**：所有核心能力封装在 `agentlaunch-core`，共享给 CLI（`al`）与桌面端（Slint / egui）。

## 目录结构

```text
AgentLaunch/
├── assets/                   # 应用图标 (icon.png / icon.ico)
└── crates/
    ├── agentlaunch-core/     # 跨平台核心引擎（配置、代理、动态发现、启动、快捷方式）
    ├── agentlaunch-cli/      # 命令行工具 `al`
    ├── agentlaunch-slint/    # Slint 桌面端（推荐）
    └── agentlaunch-gui/      # egui 桌面端（备选）
```

## 快速开始

```powershell
# 运行 Slint 桌面端
cargo run -p agentlaunch-slint

# 命令行工具
cargo run -p agentlaunch-cli -- scan
cargo run -p agentlaunch-cli -- proxies
cargo run -p agentlaunch-cli -- run codex --dry-run
cargo run -p agentlaunch-cli -- shortcut codex

# 构建 Release 正式版
cargo build --release -p agentlaunch-slint -p agentlaunch-cli
```

配置文件默认位于：
- Windows: `%APPDATA%\AgentLaunch\config.toml`
- macOS: `~/Library/Application Support/AgentLaunch/config.toml`
