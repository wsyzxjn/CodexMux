# CodexMux

CodexMux 是面向 macOS 的本地模型路由器，让 Codex Desktop 和 Codex CLI 可以在同一个模型选择器中同时使用：

- **官方模型**：当前 ChatGPT 账号可用的官方 Codex 模型（名称不变，如 `gpt-5.6`）；
- **CPA 外部模型**：通过 [CLIProxyAPI (CPA)](https://github.com/router-for-me/CLIProxyAPI) 接入的外部模型（统一加 `cpa/` 前缀，如 `cpa/gpt-5.6`、`cpa/glm-5.3-uni`）；
- **直连端点（Direct Endpoints）**：显式直连第三方原生 Responses API 端点（无需安装 CPA 亦可使用）。

CodexMux 负责动态模型目录合并、精确请求路由、凭据隔离与模型切换时的公开对话历史安全衔接。

---

## 菜单栏应用安装与快速开始

推荐使用菜单栏应用（`CodexMux.app`）作为主要控制器：**下载后直接打开即可启动代理并接管 Codex 配置**。App 已内置 CLI，不需要再安装命令行工具或运行初始化命令。

### 1. 下载与安装

1. 从 [Releases](../../releases) 页面下载 `CodexMux-<version>-macos-arm64.zip`。
2. 解压并将 `CodexMux.app` 移至 `/Applications/` 目录后直接打开。
3. 首次打开时，CodexMux 会自动创建安全凭据、准备 Codex 配置和后台服务。若 macOS 提示无法验证开发者，请右键 App 选择“打开”；当前发布包使用临时签名，尚未进行 Apple 公证。

### 2. 直接路由：打开即可体验

首次启动不安装 CPA 也可以使用官方模型和 Direct Endpoint。点击菜单栏的“直连端点”，填入原生 Responses API 的地址、令牌和模型列表，CodexMux 会把对应的 `cpa/<slug>` 模型直接路由到该端点。令牌只保存在本机权限为 `0600` 的私有配置中。非官方 slug 的目录字段默认从官方模型模板复制，上下文窗口等元信息可在 `cpa-profiles.toml` 里按模型覆盖。

### 3. 启动与日常使用

1. 打开 `/Applications/CodexMux.app`。
2. 菜单栏将出现 CodexMux 图标，支持以下图形化操作：
   - **CPA**：统一管理安装/启停、随 CodexMux 启动（默认开启）、更新/回滚、配置方案切换、Web 管理入口与管理密钥；
   - **直连端点（Direct Endpoints）**：在弹窗中快速添加/移除原生 Responses 端点与模型映射；
   - **高级功能 ▸ 为所有模型声明 Ultra**：开启后在模型目录中为每个模型显示 Codex 侧的 `ultra` 预设；Codex 会把该预设映射到模型支持的实际档位。
   - **高级功能 ▸ 统一压缩兼容哈希**（默认开启）：Codex 在两轮的 `comp_hash` 不一致时，会在采样前用**上一个模型**强制压缩一次；官方 5.6 家族与其余官方/CPA 模型的取值本就不同，因此会话中途换模型会把压缩请求送回你正要离开的模型，遇到限额时对话直接卡死。开启后 CodexMux 只在下发给 Codex 的目录里统一该值（取官方默认模型当前的取值），磁盘快照仍保留上游原值；官方真的更换压缩格式时所有模型会一起变更，Codex 仍会正常重压缩一次。
   - **审批模型**：为 `codex-auto-review` 指定走特定的 CPA 模型；
   - **图像生成**：选择 Codex 内置图像工具走哪条路 —— 默认走官方端点，也可以固定到某个 CPA 图像模型（如 `grok-imagine-image`）。菜单里的候选**只对 CPA 路线有意义**：官方端点不接受模型选择（只认 `prompt`，`model` 传什么都忽略），所以候选列表描述的是 CPA 能分发的型号。CPA 的 `/v1/models` 不上报图像模型，候选是向 CPA 探测得到的；探测不到时仍可用 CLI 指定任意 slug，选错的 slug 由上游报错，CodexMux 不会自动改道；
   - **关于与 App 更新**：查看 App/内置 CLI 版本及源码仓库；可检查 GitHub 最新稳定版，下载后校验 `SHA256SUMS`、Bundle 标识、版本和代码签名，再备份当前 App、替换并重新启动；
   - **查看日志与状态**：一键打开日志目录或查看服务运行状态。
3. CodexMux 会在启动时为 Codex Desktop 准备代理令牌；若 Desktop 正在运行会自动重启它。随后打开模型选择器即可看到官方模型与 Direct/CPA 模型。

---

## 常用命令与状态排查

### 常用命令速查

| 命令 | 说明 |
| --- | --- |
| `codexmux doctor` | 检查配置、凭据、Token 环境变量及服务连通性 |
| `codexmux status` | 查看数据路径与 Codex 配置接管状态 |
| `codexmux cpa install` | 下载、校验并安装最新稳定版的本地 CLIProxyAPI |
| `codexmux cpa update-check` | 查询最新稳定版并比较当前版本，不执行安装 |
| `codexmux cpa update [--version <tag>] [--dry-run]` | 校验并更新本地 CPA；`--version` 固定目标版本，`--dry-run` 只检查 |
| `codexmux cpa rollback` | 恢复更新前的本地 CPA 二进制和版本记录 |
| `codexmux cpa start` / `stop` | 启动或停止本地 CPA 服务 |
| `codexmux cpa model-list` | 列出当前 CPA 端点提供的所有可用模型 |
| `codexmux cpa search-get` / `search-set` | 查询或设置共享 Web 搜索后端（模型、`off` 或 `default`） |
| `codexmux cpa search-detect [--model <slug>] [--verify]` | 本地识别并缓存搜索能力；`--verify` 需配 `--model`，只验证一个后端 |
| `codexmux cpa search-capabilities` | 打印缓存的搜索能力检测结果 |
| `codexmux cpa image-get` / `image-set <slug>` | 查询或设置图像生成走的模型（空字符串恢复官方路由） |
| `codexmux cpa image-model-list` | 列出 CPA 报告的可用图像模型（官方路线不适用） |
| `codexmux catalog ultra-get` / `ultra-set <bool>` | 查询或设置是否为所有模型声明 `ultra` 预设 |
| `codexmux catalog comp-hash-get` / `comp-hash-set <bool>` | 查询或设置是否为所有模型统一压缩兼容哈希（默认开启） |
| `codexmux install` | （无菜单栏时）注册并启动后台 LaunchAgent 并接管配置 |
| `codexmux uninstall` | 停止后台 LaunchAgent 并还原 Codex 配置 |

### 多服务商模型命名

CodexMux 使用 CPA 返回的模型 alias 作为唯一路由身份。同一个上游模型如果由
多个服务商提供，CPA 里必须给不同服务商配置不同 alias；否则 CodexMux 无法
可靠地区分请求应该发往哪一家。

推荐使用 `服务商-模型` 的稳定 ID，并用 `display-name` 写人类可读名称：

```toml
[[openai-compatibility]]
name = "example-provider"
base-url = "https://provider.example/v1"

[[openai-compatibility.models]]
name = "gpt-6-astra"
alias = "example-provider-gpt-6-astra"
display-name = "GPT-6 Astra · Example"
```

合并后，Codex 模型列表中的 slug 会是 `cpa/codeapi-gpt-6-astra`。同样的模型
从 Lxns 接入时就使用另一个 alias，例如 `lxns-gpt-6-astra`，最终显示为
`cpa/lxns-gpt-6-astra`。`codexmux cpa provider import` 会拒绝空 alias 和跨
provider 重复 alias，避免两个服务商占据同一个路由身份。

如需修正某些 CPA/OAuth 模型下发给 Codex 的上下文元数据，可用精确的合并后
slug 做本地覆盖；覆盖只影响模型目录展示，不改变 CPA 路由：

```toml
[model-overrides."cpa/gpt-6-astra"]
context_window = 1000000
max_context_window = 1000000
```

### 常见问题排查

- **`CODEXMUX_PROXY_TOKEN is missing or does not match`**：退出并重新打开 CodexMux；App 会重新准备 GUI 会话令牌并重启正在运行的 Codex Desktop。仅手动运行 CLI 时才需要自行导出该环境变量。
- **`CPA: not reachable`**：在菜单栏中确认 CPA 已启动；若使用远端 CPA，确认 `cpa.base_url` 为 HTTPS 地址且远端已允许对应 `cpa_token`。
- **更新后服务不可用**：先运行 `codexmux cpa rollback` 恢复上一版本；菜单栏的“CPA 更新”也提供同一操作。
- **模型选择器没有显示 CPA 模型**：运行 `codexmux doctor` 确认连通性；完全重启 Codex 以触发重新拉取模型目录。
- **停用与卸载**：退出 `CodexMux.app`（或执行 `codexmux uninstall`）即可自动还原 Codex 配置。如需彻底删除数据，清理 `~/Library/Application Support/CodexMux` 目录即可。

## 共享 Responses Web Search

自定义模型自身不支持搜索时，可以让 CodexMux 先用一个支持 `web_search` 的
Responses 后端完成搜索，再把结果作为上下文交给当前模型生成答案：

```toml
[web_search]
enabled = true
backend_model = "gpt-5.6-sol" # 合并后目录中的精确 slug
```

所有自定义模型默认都会声明支持搜索。配置 `backend_model` 后，当
`/v1/responses` 请求声明 `web_search` 工具、且本轮输入以新的用户文本消息
结尾时，CodexMux 会先把这些用户文本发给该后端完成搜索（单次调用上限
120 秒），把返回结果和来源注入 `input`，然后移除 `web_search` 工具并继续
按用户选择的模型转发。只回传工具结果的中间轮与 `/responses/compact` 压缩
请求不会触发搜索，只做工具移除；注入的搜索上下文也不会进入跨模型回放的
历史记录。后端不可用时搜索请求会失败，但不会隐藏 Codex 的搜索入口。

菜单栏的“共享 Web 搜索”子菜单可以随时切换到“默认（config.toml）”、
“关闭共享搜索”或任意目录模型，选择会写入 `cpa-profiles.toml` 并在下一次
请求生效，不需要重启代理。“关闭共享搜索”只关闭共享执行，目录仍保留搜索能力。
子菜单还提供“重新检测搜索后端…”和“仅显示已验证”；检测结果会缓存在
`search-capabilities.json`。默认检测只做本地提供商能力识别，不会批量访问
上游；需要确认真实搜索时用 `--model <slug> --verify` 单独验证。探测因网络
或上游故障失败时不会覆盖已缓存的能力结论。
点击“重新检测”时会显示一个悬浮进度窗口，完成后自动关闭并刷新菜单。

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

请求体上限默认为 128 MiB，可在 `~/Library/Application Support/CodexMux/config.toml` 中调整后重启服务：

```toml
[server]
max_request_mib = 128
```

非官方 Direct 模型没有上游目录可抄，会复用一份官方模型模板（因此上下文窗口可能不对）。可在 `~/Library/Application Support/CodexMux/cpa-profiles.toml` 里按本地 slug 覆盖：

```toml
[direct-route.model_metadata."deepseek-v4-flash-vision-exp"]
context_window = 1000000
max_context_window = 1000000
```

或在添加直连时带上 `--context-window` / `--max-context-window`。覆盖后需刷新 Codex 模型列表。

### 4. 代码质量与测试

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

### 5. 核心架构与安全原则

- **本地回环与凭据隔离**：CodexMux 仅监听 `127.0.0.1` 并校验代理令牌；官方 ChatGPT OAuth 仅转发至官方端点，CPA 令牌与直连令牌绝不跨路由泄露。
- **Codex Alpha Search 透传**：`/v1/alpha/search` 原样转发给 CPA，由 CPA 完成搜索模型与凭据选择；CodexMux 不翻译该搜索协议。
- **图像生成透传**：Codex 内置 `image_gen` 工具请求的 `/v1/images/generations` 与 `/v1/images/edits` 按字节原样转发。默认走官方端点、只携带官方 OAuth；在菜单里指定图像模型后改走 CPA/直连，并只改写 JSON body 里的 `model` 字段。这两个请求里的 `gpt-image-*` 不在任何目录中（官方与 CPA 目录都不上报图像模型），因此不参与按 slug 路由。注意两侧对 `model` 的语义不同：官方端点完全忽略它（缺失或乱填也照样出图，且不回显 model），CPA 则按它分发并拒绝无法服务的型号 —— 这正是只在固定模型时才改写该字段的原因。
- **对话历史安全回放**：跨不同提供商或模型切换时，移除 `previous_response_id`，仅在内存中回放公开安全消息与工具调用历史，拒绝传递私有状态或加密字段。
- **安全配置接管**：通过托管标记块管理配置，启动时自动备份，退出时安全恢复，遇到冲突主动拒绝覆盖以保护用户原有配置。
