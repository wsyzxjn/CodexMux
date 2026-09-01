# CodexMux

CodexMux 是面向 macOS 的本地模型路由器，让 Codex Desktop 和 Codex CLI 可以在同一个模型选择器中同时使用：

- **官方模型**：当前 ChatGPT 账号可用的官方 Codex 模型（名称不变，如 `gpt-5.6`）；
- **CPA 外部模型**：通过 [CLIProxyAPI (CPA)](https://github.com/router-for-me/CLIProxyAPI) 接入的外部模型（统一加 `cpa/` 前缀，如 `cpa/gpt-5.6`、`cpa/glm-5.3-uni`）；
- **直接端点（Direct Endpoints）**：显式直连第三方原生 Responses API 端点（无需安装 CPA 亦可使用）。

CodexMux 负责动态模型目录合并、精确请求路由、凭据隔离与模型切换时的公开对话历史安全衔接。

---

## 菜单栏应用安装与快速开始

推荐使用菜单栏应用（`CodexMux.app`）作为主要控制器：**下载后直接打开即可启动代理并接管 Codex 配置**。App 已内置 CLI，不需要再安装命令行工具或运行初始化命令。

### 1. 下载与安装

1. 从 [Releases](../../releases) 页面下载 `CodexMux-<version>-macos-arm64.zip`。
2. 解压并将 `CodexMux.app` 移至 `/Applications/` 目录后直接打开。
3. 首次打开时，CodexMux 会自动创建安全凭据、准备 Codex 配置和后台服务。若 macOS 提示无法验证开发者，请右键 App 选择“打开”；当前发布包使用临时签名，尚未进行 Apple 公证。

### 2. 直接路由：打开即可体验

首次启动不安装 CPA 也可以使用官方模型和 Direct Endpoint。点击菜单栏的“直接端点”，填入原生 Responses API 的地址、令牌和模型列表，CodexMux 会把对应的 `cpa/<slug>` 模型直接路由到该端点。令牌只保存在本机权限为 `0600` 的私有配置中。

### 3. 启动与日常使用

1. 打开 `/Applications/CodexMux.app`。
2. 菜单栏将出现 CodexMux 图标，支持以下图形化操作：
   - **下载并启用 CPA**：点击一次即可自动下载、校验、安装、配置并启动 CPA，同时打开 CPA Web 管理页面；
   - **打开 CPA Web 管理**：自动携带管理密钥一键登录，方便配置外部模型别名与 API Key；
   - **直接端点（Direct Endpoints）**：在弹窗中快速添加/移除原生 Responses 端点与模型映射；
   - **审批模型覆盖**：为 `codex-auto-review` 指定走特定的 CPA 模型；
   - **CPA 配置方案切换**：在多个预设 CPA 端点间自由切换；
   - **查看日志与状态**：一键打开日志目录或查看服务运行状态。
3. CodexMux 会在启动时为 Codex Desktop 准备代理令牌；若 Desktop 正在运行会自动重启它。随后打开模型选择器即可看到官方模型与 Direct/CPA 模型。

---

## 常用命令与状态排查

### 常用命令速查

| 命令 | 说明 |
| --- | --- |
| `codexmux doctor` | 检查配置、凭据、Token 环境变量及服务连通性 |
| `codexmux status` | 查看数据路径与 Codex 配置接管状态 |
| `codexmux cpa install` | 下载、校验并安装托管的本地 CLIProxyAPI |
| `codexmux cpa start` / `stop` | 启动或停止本地 CPA 服务 |
| `codexmux cpa model-list` | 列出当前 CPA 端点提供的所有可用模型 |
| `codexmux install` | （无菜单栏时）注册并启动后台 LaunchAgent 并接管配置 |
| `codexmux uninstall` | 停止后台 LaunchAgent 并还原 Codex 配置 |

### 常见问题排查

- **`CODEXMUX_PROXY_TOKEN is missing or does not match`**：退出并重新打开 CodexMux；App 会重新准备 GUI 会话令牌并重启正在运行的 Codex Desktop。仅手动运行 CLI 时才需要自行导出该环境变量。
- **`CPA: not reachable`**：在菜单栏中确认 CPA 已启动；若使用远端 CPA，确认 `cpa.base_url` 为 HTTPS 地址且远端已允许对应 `cpa_token`。
- **模型选择器没有显示 CPA 模型**：运行 `codexmux doctor` 确认连通性；完全重启 Codex 以触发重新拉取模型目录。
- **停用与卸载**：退出 `CodexMux.app`（或执行 `codexmux uninstall`）即可自动还原 Codex 配置。如需彻底删除数据，清理 `~/Library/Application Support/CodexMux` 目录即可。

---

## 开发者专栏：手动构建与源码运行

本栏面向需要对 CodexMux 进行二次开发或从源码构建的开发者。

### 1. 环境要求

- macOS (Apple Silicon / Intel)
- Rust 2024 (stable)
- Swift 命令行工具 / Xcode

### 2. 编译 CLI 与菜单栏应用

```bash
# 1. 编译 CLI（打包脚本会将它嵌入 App）
cargo build --release

# 2. 打包并安装菜单栏应用
menubar/package-app.sh release "" target/release/codexmux
ditto menubar/.build/release/CodexMux.app /Applications/CodexMux.app
```

### 3. 前台调试运行

可在前台直接运行代理服务（按 `Ctrl-C` 优雅退出并自动恢复 Codex 配置）：

```bash
codexmux serve
# 或在前台运行但不接管 Codex 配置
codexmux serve --no-codex-config
```

### 4. 代码质量与测试

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

### 5. 核心架构与安全原则

- **本地回环与凭据隔离**：CodexMux 仅监听 `127.0.0.1` 并校验代理令牌；官方 ChatGPT OAuth 仅转发至官方端点，CPA 令牌与直连令牌绝不跨路由泄露。
- **对话历史安全回放**：跨不同提供商或模型切换时，移除 `previous_response_id`，仅在内存中回放公开安全消息与工具调用历史，拒绝传递私有状态或加密字段。
- **安全配置接管**：通过托管标记块管理配置，启动时自动备份，退出时安全恢复，遇到冲突主动拒绝覆盖以保护用户原有配置。
