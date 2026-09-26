# CodexMux

CodexMux 是面向 macOS 的本地模型路由器，让 Codex Desktop 和 Codex CLI 可以在同一个模型选择器中同时使用：

- **官方模型**：当前 ChatGPT 账号可用的官方 Codex 模型（名称不变，如 `gpt-5.6`）；
- **CPA 外部模型**：通过 [CLIProxyAPI (CPA)](https://github.com/router-for-me/CLIProxyAPI) 接入的外部模型（统一加 `cpa/` 前缀，如 `cpa/gpt-5.6`、`cpa/glm-5.3-uni`）。

CodexMux 负责动态合并两边的模型目录、按模型精确路由、隔离凭据，并在切换模型时安全衔接公开的对话历史。

---

## 菜单栏应用安装与快速开始

推荐使用菜单栏应用（`CodexMux.app`）作为主要控制器：**下载后直接打开即可启动代理并接管 Codex 配置**。App 已内置 CLI，不需要再安装命令行工具或运行初始化命令。

### 1. 下载与安装

1. 从 [Releases](../../releases) 页面下载 `CodexMux-<version>-macos-arm64.zip`。
2. 解压并将 `CodexMux.app` 移至 `/Applications/` 目录后直接打开。
3. 首次打开时，CodexMux 会自动创建安全凭据、准备 Codex 配置和后台服务。若 macOS 提示无法验证开发者，请右键 App 选择“打开”；当前发布包使用临时签名，尚未进行 Apple 公证。

### 2. 接入外部模型

首次启动时即使没有安装 CPA，官方模型也能照常使用。需要外部模型时，在菜单栏的 **CPA** 子菜单里选择“安装”：CodexMux 会下载并校验最新稳定版 CLIProxyAPI，启动后自动打开它的 Web 管理页，在那里添加服务商凭据即可。完全重启 Codex 后，模型选择器里就会出现 `cpa/` 开头的外部模型。

### 3. 启动与日常使用

1. 打开 `/Applications/CodexMux.app`。
2. 菜单栏将出现 CodexMux 图标，支持以下图形化操作：
   - **CPA**：统一管理安装/启停、随 CodexMux 启动（默认开启）、更新/回滚、配置方案切换、Web 管理入口与管理密钥。“随 CodexMux 启动”指 Codex 连接代理时自动拉起本机 CPA，登录系统时不会单独启动它；
   - **高级功能 ▸ 为所有模型声明 Ultra**：开启后，凡是声明了推理档位的模型都会在目录中多出 Codex 侧的 `ultra` 预设，Codex 会把它映射到模型支持的实际档位；没有推理档位的模型保持上游原样。该调整只作用于下发给 Codex 的目录，不写入磁盘快照。
   - **高级功能 ▸ 统一压缩兼容哈希**（默认开启）：Codex 在两轮的 `comp_hash` 不一致时，会在采样前用**上一个模型**强制压缩一次；官方 5.6 家族与其余官方/CPA 模型的取值本就不同，因此会话中途换模型会把压缩请求送回你正要离开的模型，遇到限额时对话直接卡死。开启后 CodexMux 只在下发给 Codex 的目录里统一该值（取官方默认模型当前的取值），磁盘快照仍保留上游原值；官方真的更换压缩格式时所有模型会一起变更，Codex 仍会正常重压缩一次。
   - **审批模型**：为 `codex-auto-review` 指定走特定的 CPA 模型；
   - **图像生成**：选择 Codex 内置图像工具走哪条路 —— 默认走官方端点，也可以固定到某个 CPA 图像模型（如 `grok-imagine-image`）。菜单里的候选**只对 CPA 路线有意义**：官方端点不接受模型选择（只认 `prompt`，`model` 传什么都忽略），所以候选列表描述的是 CPA 能分发的型号。CPA 的 `/v1/models` 不上报图像模型，候选是向 CPA 探测得到的；探测不到时仍可用 CLI 指定任意 slug，选错的 slug 由上游报错，CodexMux 不会自动改道；
   - **关于与 App 更新**：查看 App/内置 CLI 版本及源码仓库；可检查 GitHub 最新稳定版，下载后校验 `SHA256SUMS`、代码签名、Bundle 标识和版本，替换成功并重新启动后删除旧版本备份，失败时自动还原；
   - **查看日志与状态**：一键打开日志目录。代理日志按天写入 `codexmux.<日期>.log`，保留最近 7 天。
3. CodexMux 会在启动时为 Codex Desktop 准备代理令牌；若 Desktop 正在运行会自动重启它。随后打开模型选择器即可看到官方模型与 CPA 模型。

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
| `codexmux cpa profile-save` / `profile-switch <name>` | 保存 CPA 端点方案；切换后 CodexMux 改用该端点和令牌，并重启代理使其生效 |
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

合并后，Codex 模型列表中的 slug 会是 `cpa/example-provider-gpt-6-astra`。同样
的模型从 Lxns 接入时就使用另一个 alias，例如 `lxns-gpt-6-astra`，最终显示为
`cpa/lxns-gpt-6-astra`。`codexmux cpa provider-import` 会拒绝空 alias 和跨
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
- **停用与卸载**：通过菜单退出 `CodexMux.app`（或执行 `codexmux uninstall`）会先还原 Codex 配置，再停止后台服务；还原失败时会提示原因，不会留下指向已停止代理的配置。如需彻底删除数据，清理 `~/Library/Application Support/CodexMux` 目录即可。
- **`cpa-profiles.toml` 手动编辑出错**：官方模型不受影响；依赖该文件的审批模型、图像模型和共享搜索会返回 `local_config` 错误，错误信息给出出错的行列号，修正文件后立即生效。

## 共享 Responses Web Search

自定义模型自身不支持搜索时，可以让 CodexMux 先用一个支持 `web_search` 的
Responses 后端完成搜索，再把结果作为上下文交给当前模型生成答案：

```toml
[web_search]
enabled = true
backend_model = "gpt-5.6-sol" # 合并后目录中的精确 slug
```

所有自定义模型默认都会声明支持搜索。官方模型和经 `--verify` 真实验证过的
模型自己就能原生搜索，CodexMux 会原样放行 `web_search` 工具，Codex 界面里
能看到真实的搜索过程。其余模型在配置 `backend_model` 后走共享后端：当
`/v1/responses` 请求声明 `web_search` 工具、且本轮输入以新的用户文本消息
结尾时，CodexMux 只把最新的那条用户消息发给后端完成搜索（单次调用上限
120 秒），Codex 自动附加的 AGENTS.md 指令和环境信息不会发出去。返回的结果
和来源作为“不可信的参考资料”插在这条用户消息之前，然后移除 `web_search`
工具并继续按用户选择的模型转发；这条注入路径在界面上没有搜索过程可显示。
只回传工具结果的中间轮与 `/responses/compact` 压缩请求不会触发搜索，只做
工具移除；注入的搜索上下文也不会进入跨模型回放的历史记录。后端不可用时
这一轮照常回答，模型会被告知本次搜索没有成功。

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

- macOS Apple Silicon (arm64)
- Rust 2024（版本由 `rust-toolchain.toml` 固定，rustup 会自动安装）
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

### 4. 代码质量与测试

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
swift test --package-path menubar
```

服务端集成测试会绑定 `127.0.0.1` 的临时端口，需要在允许本机监听的环境里运行。

### 5. 核心架构与安全原则

- **本地回环与凭据隔离**：CodexMux 仅监听 `127.0.0.1` 并校验代理令牌；官方 ChatGPT OAuth 仅转发至官方端点，CPA 令牌只发给 CPA，两者绝不跨路由。
- **Codex Alpha Search 透传**：`/v1/alpha/search` 原样转发给 CPA，由 CPA 完成搜索模型与凭据选择；CodexMux 不翻译该搜索协议。
- **图像生成透传**：Codex 内置 `image_gen` 工具请求的 `/v1/images/generations` 与 `/v1/images/edits` 按字节原样转发。默认走官方端点、只携带官方 OAuth；在菜单里指定图像模型后改走 CPA，并只改写 JSON body 里的 `model` 字段。这两个请求里的 `gpt-image-*` 不在任何目录中（官方与 CPA 目录都不上报图像模型），因此不参与按 slug 路由。注意两侧对 `model` 的语义不同：官方端点完全忽略它（缺失或乱填也照样出图，且不回显 model），CPA 则按它分发并拒绝无法服务的型号 —— 这正是只在固定模型时才改写该字段的原因。
- **对话历史安全回放**：跨不同提供商或模型切换时，移除 `previous_response_id`，仅在内存中回放公开安全消息与工具调用历史（每个工具调用都保留对应的输出），拒绝传递私有状态或加密字段。
- **安全配置接管**：在 Codex 配置顶部写入一个带标记的托管块。Codex 自己写进块内的设置会被当作用户内容保留；修复或还原前会比对解析后的配置，只有结果等于“用户原配置去掉 CodexMux 两个键”时才写入，否则拒绝并提示原因。
