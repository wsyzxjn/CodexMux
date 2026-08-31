# ModelMux

ModelMux 是一个面向 macOS 的本地模型路由器，让 Codex Desktop 和 Codex CLI
可以在同一个模型选择器中使用：

- 当前 ChatGPT 账号可用的官方 Codex 模型；
- [CLIProxyAPI（CPA）](https://github.com/router-for-me/CLIProxyAPI) 提供的外部模型。

官方模型名称保持不变，CPA 模型统一加上 `cpa/` 前缀。例如，官方的
`gpt-5.6` 和 CPA 的 `cpa/gpt-5.6` 可以同时存在。ModelMux 按完整模型名称精确
路由，不认识的名称会直接拒绝。

ModelMux 只处理模型目录合并、请求分流、凭据隔离和切换模型时的对话衔接。
外部服务的协议转换、模型别名和供应商凭据均由 CPA 管理。

## 使用前准备

需要准备：

- macOS；
- 已登录 ChatGPT 的 Codex Desktop 或 Codex CLI；
- 已安装 Rust 工具链；
- 已安装并配置 CPA。

CPA 可以运行在本机或远端。本文默认使用本机地址
`http://127.0.0.1:8317`；远端 CPA 必须使用 HTTPS。ModelMux 自身提供给
Codex 的监听地址始终只能是本机回环地址。

## 快速开始

### 1. 编译并放置可执行文件

```bash
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/modelmux "$HOME/.local/bin/modelmux"
```

确保 `~/.local/bin` 在 `PATH` 中。当前终端可以执行：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

如需永久生效，将这一行加入 `~/.zshrc`，再打开一个新终端。

后续示例都假设可以直接运行 `modelmux`。

### 2. 初始化 ModelMux

```bash
modelmux init
```

默认数据目录为：

```text
~/Library/Application Support/ModelMux/
```

其中主要文件是：

| 文件 | 用途 |
| --- | --- |
| `config.toml` | ModelMux 和 CPA 的本地监听地址 |
| `credentials.json` | ModelMux 与 CPA 的本地访问令牌，权限必须为 `0600` |
| `model-catalog.json` | 最近一次成功获取的完整模型目录 |
| `logs/` | 后台服务的标准输出和错误日志 |

如需使用其他数据目录，应在运行所有 `modelmux` 命令前设置
`MODELMUX_HOME`。

### 3. 让 ModelMux 和 CPA 使用同一个 CPA 令牌

`modelmux init` 会在 `credentials.json` 中生成两个不同的随机令牌：

```json
{
  "proxy_token": "generated-proxy-token",
  "cpa_token": "generated-cpa-token"
}
```

- `proxy_token`：Codex 访问 ModelMux 时使用；
- `cpa_token`：ModelMux 访问 CPA 时使用。

把 `cpa_token` 的值加入 CPA 顶层的 `api-keys`：

```yaml
host: 127.0.0.1
port: 8317
api-keys:
  - generated-cpa-token
```

也可以反过来，把 CPA 现有的一个 `api-keys` 值写入 ModelMux 的
`cpa_token`。两边必须完全一致。`proxy_token` 与 `cpa_token` 必须不同。

手动修改凭据文件后，重新确认权限：

```bash
chmod 600 "$HOME/Library/Application Support/ModelMux/credentials.json"
```

供应商 API Key、OAuth 凭据和模型别名只配置在 CPA 中，不要写入
ModelMux。

ModelMux 默认连接本机 CPA：

```toml
listen = "127.0.0.1:48682"

[cpa]
base_url = "http://127.0.0.1:8317/v1"
```

如果 CPA 在远端，将 `cpa.base_url` 改成其 HTTPS API 根地址：

```toml
listen = "127.0.0.1:48682"

[cpa]
base_url = "https://cpa.example.com/v1"
```

远端地址不允许使用明文 HTTP，也不要把 token 写入 URL；认证信息只存放在
`credentials.json` 的 `cpa_token` 中。

### 4. 把 `proxy_token` 提供给 Codex

Codex 配置会从环境变量 `MODELMUX_PROXY_TOKEN` 读取 `proxy_token`。该变量
必须进入 **Codex 进程的环境**，不是只提供给 ModelMux 服务。

#### Codex CLI

在启动 Codex CLI 的同一个终端中执行：

```bash
export MODELMUX_PROXY_TOKEN='把 credentials.json 中的 proxy_token 填在这里'
codex
```

如需让新终端自动设置，可将 `export` 命令加入 `~/.zshrc`。令牌属于本地
访问凭据，不要提交到 Git 仓库或写入项目的公开脚本。

#### Codex Desktop

让从 macOS 图形界面启动的应用取得变量：

```bash
launchctl setenv MODELMUX_PROXY_TOKEN '把 credentials.json 中的 proxy_token 填在这里'
```

设置后需要**完全退出 Codex Desktop，再重新打开**。仅关闭窗口通常不会
重启应用进程。注销 macOS 登录会话后如果变量消失，需要再次执行上述命令。

同时使用 Codex CLI 和 Desktop 时，两种方式都应配置。

### 5. 启动 CPA 和 ModelMux

先确认 CPA 正在运行，然后选择以下一种方式启动 ModelMux。不要同时运行
前台服务和后台服务，否则两者会争用 `127.0.0.1:48682`。

#### 推荐：安装为 macOS 后台服务

```bash
modelmux install
```

该命令会立即注册并启动当前用户的 LaunchAgent，并在登录后自动运行。后台
代理启动成功后会自动备份并接管 Codex 配置；执行 `modelmux uninstall` 时会
停止代理、注销 LaunchAgent 并恢复原配置。

安装时会记录当前 `modelmux` 可执行文件和 Codex 配置文件的绝对路径，因此应
先把二进制文件放到稳定位置，并确定 `CODEX_CONFIG` 或 `CODEX_HOME`，再执行
`install`。

#### 调试：在前台运行

```bash
modelmux serve
```

`serve` 会在成功绑定本地端口后自动备份并接管 Codex 配置。按 `Ctrl-C` 或
收到正常终止信号时，代理会停止接收新请求，最多等待现有请求 30 秒，再恢复
原配置。终端被强制关闭、进程崩溃或系统断电时可能来不及恢复；下一次启动会
根据持久化的操作阶段完成接管或恢复。需要调试日志或首次排查配置时可使用
这种方式。

启动或停止 ModelMux 后都应完全退出并重新打开 Codex，使其重新读取配置。

### 6. 检查运行状态

在已正确设置 `MODELMUX_PROXY_TOKEN` 的终端中运行：

```bash
modelmux doctor
```

正常情况下应看到：

```text
proxy: reachable
CPA: reachable
settings: ok
credentials: ok
```

首次启动且 Codex 尚未请求模型目录时，出现
`catalog snapshot: not fetched yet` 是正常现象。`doctor` 会报告代理和 CPA
是否可访问，但这两项不可访问时只打印状态；请根据输出修复后再继续。

### 7. 重新启动 Codex

`serve` 或 LaunchAgent 启动后，ModelMux 已经自动在 Codex 的 `config.toml`
中加入托管 provider 配置，并保留原文件备份。默认配置文件是
`~/.codex/config.toml`。

ModelMux 不会写入静态 `model_catalog_json`。Codex 会从 ModelMux 的
`/v1/models` 动态获取模型目录。

完全退出并重新启动 Codex。模型选择器中应同时看到：

- 原名称的官方模型，例如 `gpt-5.6`；
- 带 ` · CPA` 显示后缀的 CPA 模型，例如模型名称为 `cpa/gpt-5.6`。

如果 CPA 中新增、删除或修改模型，重新打开模型选择器或重启 Codex，使其
再次请求模型目录即可，无需手动编辑 ModelMux 配置。

## 日常命令

| 命令 | 作用 |
| --- | --- |
| `modelmux status` | 显示数据路径、Codex 配置路径和启用状态 |
| `modelmux doctor` | 检查配置、凭据、环境变量、服务连通性和目录快照 |
| `modelmux serve` | 在前台运行代理，启动时接管、退出时恢复 Codex 配置 |
| `modelmux serve --no-codex-config` | 在前台运行代理但不管理 Codex 配置 |
| `modelmux install` | 启用 Codex 托管配置，然后注册并启动 LaunchAgent |
| `modelmux uninstall` | 停止并注销 LaunchAgent，同时恢复 Codex 配置 |

`status` 显示的是配置管理状态，不代表后台进程一定可访问；检查服务连通性应
使用 `doctor`。

LaunchAgent 中的代理以 `serve --no-codex-config` 运行：macOS 的 TCC 可能
拒绝后台进程访问 Codex 配置所在的宗卷，并且 `open()` 会无限阻塞而不是立刻
报错。Codex 配置的启用与恢复由 `modelmux install` / `modelmux uninstall`
在终端中完成。

### 管理本地 CPA

ModelMux 可以直接下载并托管一个本地 CLIProxyAPI（CPA），不再要求用户自行
安装：

| 命令 | 作用 |
| --- | --- |
| `modelmux cpa install` | 下载固定版本的 CLIProxyAPI 发布包（校验 sha256 后解压）、写入托管配置并启动 |
| `modelmux cpa start` / `modelmux cpa stop` | 启动或停止本地 CPA 服务 |
| `modelmux cpa status` | 显示已安装版本和运行状态 |
| `modelmux cpa provider-import <file>` | 从 TOML 文件导入 `[[openai-compatibility]]` / `[[codex-api-key]]` 提供商并重启服务 |
| `modelmux cpa uninstall` | 移除 CPA LaunchAgent（保留二进制、配置和登录凭据） |

安装位置和数据：

- 二进制：`$MODELMUX_HOME/cpa/cli-proxy-api`
- 配置：`$MODELMUX_HOME/cpa/config.yaml`（ModelMux 托管；手动改过的配置不会被覆盖）
- 日志：`$MODELMUX_HOME/logs/cpa-*.log`
- 登录凭据：`~/.cli-proxy-api`（与 CPA 自身约定一致）

本地 CPA 只监听 `127.0.0.1`，对 Codex 侧的接入方式与远端 CPA 完全一致：
ModelMux 按 `cpa/` 前缀路由到 `config.toml` 中配置的 CPA 地址。

### 菜单栏应用（ModelMuxBar）

仓库内的 `menubar/` 是一个独立的 Swift 菜单栏应用，提供：

- 原生折叠菜单中的实时 token 速度；
- ModelMux 与本地 CPA 的运行状态显示；
- 重启 / 停止 ModelMux；
- 启动 / 停止 CPA；
- 打开日志目录。

生成过程中，Token 速度子菜单从公开的 Responses SSE delta 估算输出 token，并以
`≈ tok/s` 显示；收到 `response.completed` 后，如果上游返回
`usage.output_tokens`，会用该值校准最终平均速度。估算与校准统计只保存在
ModelMux 进程内存中，不记录 prompt、响应文本或 provider 私有字段。

点击菜单栏图标打开原生菜单，再展开“Token 速度”即可查看。统计数据来自
需要 `x-modelmux-token` 的本地 `/telemetry` 接口。

构建并运行：

```bash
cd menubar
swift build -c release
.build/release/ModelMuxBar
```

菜单栏应用不提供模型切换——模型选择由 Codex 客户端完成。

### 查看后台日志

默认数据目录下执行：

```bash
tail -f "$HOME/Library/Application Support/ModelMux/logs/stderr.log"
```

标准输出位于同目录的 `stdout.log`。如果设置了 `MODELMUX_HOME`，日志位于
`$MODELMUX_HOME/logs/`。

### 更新 ModelMux

在仓库中重新编译并替换稳定路径下的二进制文件，然后重新安装 LaunchAgent
以重启服务：

```bash
cargo build --release
install -m 755 target/release/modelmux "$HOME/.local/bin/modelmux"
modelmux install
modelmux doctor
```

更新二进制文件本身不会让已经运行的进程自动加载新版本，必须重新执行
`modelmux install`。

## 停用和卸载

停止后台服务、注销 LaunchAgent 并恢复 Codex 配置：

```bash
modelmux uninstall
```

如果是前台运行，先按 `Ctrl-C`，ModelMux 会在退出前恢复 Codex 配置。

如果为 Codex Desktop 设置过环境变量，再清除它：

```bash
launchctl unsetenv MODELMUX_PROXY_TOKEN
```

同时从 `~/.zshrc` 删除为 Codex CLI 添加的 `MODELMUX_PROXY_TOKEN`，并打开
新终端。

上述操作会保留 ModelMux 数据、目录快照和日志。确认不再需要后可以删除：

```bash
rm -rf "$HOME/Library/Application Support/ModelMux"
```

应先退出前台代理或执行 `uninstall`，再删除数据目录，否则会丢失恢复 Codex
原配置所需的状态和备份。

## 故障排查

### `MODELMUX_PROXY_TOKEN is missing or does not match credentials.json`

当前终端中的 `MODELMUX_PROXY_TOKEN` 没有设置，或与 `credentials.json` 的
`proxy_token` 不一致。重新设置变量后，在同一个终端运行 `modelmux doctor`
或启动 Codex CLI。Codex Desktop 需要使用 `launchctl setenv` 并完全退出后
重开。

### `proxy: not running`

ModelMux 未启动，或监听地址已被其他进程占用。使用以下一种方式启动：

```bash
modelmux install
# 或者用于前台调试：
modelmux serve
```

如果刚执行过 `install`，查看 `logs/stderr.log`。

### `CPA: not reachable`

确认：

1. 本机 CPA 已启动，或远端 CPA 的 HTTPS 地址可以访问；
2. `config.toml` 中的 `cpa.base_url` 与 CPA API 根地址一致并包含 `/v1`；
3. ModelMux 的 `cpa_token` 存在于 CPA 顶层 `api-keys`；
4. 网络、防火墙和 TLS 证书允许当前 Mac 访问远端 CPA；
5. 修改 CPA 配置后已经重启 CPA。

### 模型列表为空或没有 CPA 模型

先运行 `modelmux doctor`，确认 ModelMux 和 CPA 均可访问。然后完全重启
Codex，使其重新请求模型目录。

ModelMux 只有在官方目录和 CPA 目录都成功返回有效数据后才会保存新的完整
快照。如果本次刷新失败，它会继续提供上一次成功的快照；首次刷新之前没有
可用快照，请检查后台日志。

CPA 目录中满足以下任一条件的模型不会显示：

- `visibility = "hide"`；
- `supported_in_api = false`。

### `Codex config already defines ...; refusing to overwrite it`

ModelMux 会保留已有的 `model_providers` 服务商，并追加自己的
`model_providers.modelmux`。只有现有配置已经占用 ModelMux 自己的 provider 名称，
或定义了 ModelMux 需要临时托管的顶层 `model_provider`、`model_catalog_json` 时才会
拒绝接管。

先备份 `~/.codex/config.toml`，核对报错指出的具体冲突项后再处理。不要删除其他
`model_providers` 服务商配置；它们不会阻止 ModelMux 启动。

### `Codex config changed inside the managed ModelMux block`

ModelMux 接管后，其托管标记之间的内容被手动修改。为避免误删用户配置，
自动接管和恢复会停止操作。恢复托管块原状后重试，或根据
`state/codex-config.json` 与 `backups/` 中的记录手动核对。

### 修改了 `credentials.json` 后服务无法启动

确认文件是普通文件、JSON 格式正确、两个令牌非空且彼此不同，并恢复严格
权限：

```bash
chmod 600 "$HOME/Library/Application Support/ModelMux/credentials.json"
modelmux install
```

### 使用自定义 Codex 配置路径

ModelMux 按以下优先级寻找 Codex 配置：

1. `CODEX_CONFIG` 指定的完整文件路径；
2. `$CODEX_HOME/config.toml`；
3. `~/.codex/config.toml`。

运行 `serve`、`install`、`uninstall`、`status` 和 `doctor` 时必须使用一致的环境
变量，否则 ModelMux 会拒绝操作由另一路径记录的托管状态。相对形式的
`CODEX_CONFIG` 会在执行命令时转换为绝对路径。Codex 配置不能是符号链接，
以免原子替换破坏由 dotfiles 工具维护的链接；请直接指定链接目标文件。

## 模型目录与请求路由

Codex 请求：

```http
GET http://127.0.0.1:48682/v1/models?client_version=<Codex version>
```

ModelMux 会并行请求官方 ChatGPT Codex 目录和 CPA 原生 Codex 目录，再合并
结果：

- 官方模型 slug 保持不变；
- CPA 模型 slug 从 `<model>` 改为 `cpa/<model>`；
- CPA 模型 `display_name` 追加 ` · CPA`；
- CPA 返回的其他模型字段原样保留，包括未来新增的未知字段。

合并后的完整目录会原子写入 `model-catalog.json`，同时作为精确路由表。CPA
请求转发前只把顶层模型名从 `cpa/<model>` 还原为 `<model>`；其余原生
Responses 请求保持不变。

## 在同一对话中切换模型

响应 ID 只属于生成它的具体后端和模型，不能直接交给另一个路由。切换官方
模型、切换 CPA 模型，或在官方与 CPA 之间切换时，ModelMux 会移除
`previous_response_id`，改为回放本地记录的公开对话历史。

回放内容仅包括允许的文本消息、工具调用与结果，以及公开压缩内容。推理、
签名、加密内容、供应商私有状态和未知项目不会跨路由传递；图片也不会在切换
时回放。历史保存在内存中，重启 ModelMux 后会清空。历史链不完整、已淘汰或
无法确认安全时，请求会被明确拒绝，而不是把响应 ID 发给错误的供应商。

## 安全边界

- ModelMux 提供给 Codex 的监听地址必须是本机回环地址；
- 本机 CPA 可以使用回环 HTTP，远端 CPA 必须使用 HTTPS；
- ModelMux 的所有接口都要求 `x-modelmux-token`；
- 官方请求只使用 Codex 传入的 ChatGPT OAuth，并固定发送到官方 Codex
  endpoint；
- CPA 请求只转发 `Accept` 和 `User-Agent`，再注入独立的 CPA 令牌；其他
  Codex 请求头不会进入 CPA；
- CPA 令牌不会发送给官方，ChatGPT OAuth 也不会发送给 CPA；
- 凭据不会写入模型目录、Codex 配置或 API 响应；
- 代理启动时会保存原 Codex 配置，正常退出时只恢复原文件或移除未被修改的
  托管内容。

## 开发检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Attribution

CPA（[`router-for-me/CLIProxyAPI`](https://github.com/router-for-me/CLIProxyAPI)）
负责外部供应商协议转换并提供原生 Codex 模型目录。ModelMux 不包含 CPA
源码。
