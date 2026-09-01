# CodexMux

CodexMux 是一个面向 macOS 的本地模型路由器，让 Codex Desktop 和 Codex CLI
可以在同一个模型选择器中使用：

- 当前 ChatGPT 账号可用的官方 Codex 模型；
- [CLIProxyAPI（CPA）](https://github.com/router-for-me/CLIProxyAPI) 提供的外部模型。

官方模型名称保持不变，CPA 模型统一加上 `cpa/` 前缀。例如，官方的
`gpt-5.6` 和 CPA 的 `cpa/gpt-5.6` 可以同时存在。CodexMux 按完整模型名称精确
路由，不认识的名称会直接拒绝。

CodexMux 只处理模型目录合并、请求分流、凭据隔离和切换模型时的对话衔接。
外部服务的协议转换、模型别名和供应商凭据通常由 CPA 管理；用户也可以把模型
显式直连到原生 Responses 上游。直连路由声明自己的模型列表，无论 CPA 是否
安装或运行都可用；它是按模型配置的明确覆盖，不是自动故障转移。CPA 不可达
时，CodexMux 会用官方目录加上直连模型提供降级目录（仅内存，不落盘），因此
不装 CPA 也能通过直连端点使用第三方模型。

## 使用前准备

需要准备：

- macOS；
- 已登录 ChatGPT 的 Codex Desktop 或 Codex CLI；
- 已安装 Rust 工具链。

本地 CPA 由 CodexMux 托管，不必事先单独安装 CLIProxyAPI。也可以接入已经
在运行的本机或远端 CPA。本文默认使用本机地址 `http://127.0.0.1:8317`；
远端 CPA 必须使用 HTTPS。CodexMux 提供给 Codex 的监听地址始终只能是本机
回环地址。

## 快速开始

### 1. 编译并放置可执行文件

```bash
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/codexmux "$HOME/.local/bin/codexmux"
```

确保 `~/.local/bin` 在 `PATH` 中。当前终端可以执行：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

如需永久生效，将这一行加入 `~/.zshrc`，再打开一个新终端。

后续示例都假设可以直接运行 `codexmux`。

### 2. 初始化 CodexMux

```bash
codexmux init
```

默认数据目录为：

```text
~/Library/Application Support/CodexMux/
```

其中主要文件是：

| 文件 | 用途 |
| --- | --- |
| `config.toml` | CodexMux 和 CPA 的本地监听地址 |
| `credentials.json` | CodexMux 与 CPA 的本地访问令牌，权限必须为 `0600` |
| `cpa-profiles.toml` | CPA 端点配置、直连路由和审批模型覆盖，权限 `0600` |
| `model-catalog.json` | 最近一次成功获取的完整模型目录 |
| `model-catalog.retention.json` | CPA 模型首次缺失时间，用于目录移除宽限期 |
| `cpa/` | 托管的 CLIProxyAPI 二进制和 `config.yaml` |
| `logs/` | 后台服务的标准输出和错误日志 |

如需使用其他数据目录，应在运行所有 `codexmux` 命令前设置
`CODEXMUX_HOME`。

### 3. 让 CodexMux 和 CPA 使用同一个 CPA 令牌

`codexmux init` 会在 `credentials.json` 中生成三个互不相同的随机凭据：

```json
{
  "proxy_token": "generated-proxy-token",
  "cpa_token": "generated-cpa-token",
  "cpa_management_key": "generated-management-key"
}
```

- `proxy_token`：Codex 访问 CodexMux 时使用；
- `cpa_token`：CodexMux 访问 CPA 时使用；
- `cpa_management_key`：登录 CPA Web 管理页时使用。

缺少 `cpa_management_key` 的凭据文件会在首次读取时原子补上，权限保持
`0600`。管理密钥不会写入模型目录、日志或 Codex 配置。菜单栏“打开 CPA Web
管理”在本机 CPA 上会带上当前端点和管理密钥并自动登录，随后从地址栏清除查询
参数。也可以用“复制 CPA 管理密钥”手动复制。远端 CPA 的管理密钥由远端管理员
决定；使用远端地址时应把 `cpa_management_key` 改成远端已有的密钥，CodexMux
不会改写远端配置。

使用 `codexmux cpa install` 托管本地 CPA 时，会自动把 `cpa_token` 写入 CPA
的 `api-keys`。若 CPA 由你自己运行，把 `cpa_token` 的值加入其顶层 `api-keys`：

```yaml
host: 127.0.0.1
port: 8317
api-keys:
  - generated-cpa-token
```

也可以反过来，把 CPA 现有的一个 `api-keys` 值写入 CodexMux 的
`cpa_token`。两边必须完全一致。`proxy_token` 与 `cpa_token` 必须不同。

手动修改凭据文件后，重新确认权限：

```bash
chmod 600 "$HOME/Library/Application Support/CodexMux/credentials.json"
```

供应商 API Key、OAuth 凭据和模型别名只配置在 CPA 中。显式直连的上游令牌
写在 `cpa-profiles.toml`，不要写进 `config.toml` 或模型目录。

CodexMux 默认连接本机 CPA：

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

Codex 配置会从环境变量 `CODEXMUX_PROXY_TOKEN` 读取 `proxy_token`。该变量
必须进入 **Codex 进程的环境**，不是只提供给 CodexMux 服务。

#### Codex CLI

在启动 Codex CLI 的同一个终端中执行：

```bash
export CODEXMUX_PROXY_TOKEN='把 credentials.json 中的 proxy_token 填在这里'
codex
```

如需让新终端自动设置，可将 `export` 命令加入 `~/.zshrc`。令牌属于本地
访问凭据，不要提交到 Git 仓库或写入项目的公开脚本。

#### Codex Desktop

让从 macOS 图形界面启动的应用取得变量：

```bash
launchctl setenv CODEXMUX_PROXY_TOKEN '把 credentials.json 中的 proxy_token 填在这里'
```

设置后需要**完全退出 Codex Desktop，再重新打开**。仅关闭窗口通常不会
重启应用进程。注销 macOS 登录会话后如果变量消失，需要再次执行上述命令。

同时使用 Codex CLI 和 Desktop 时，两种方式都应配置。

### 5. 启动 CPA 和 CodexMux

不要同时运行前台代理和后台代理，否则会争用 `127.0.0.1:48682`。

#### 推荐：菜单栏应用

菜单栏应用是控制器：打开它会启动代理和本地 CPA，并从当前图形界面上下文
备份并接管 Codex 配置；退出时停止整栈并还原 Codex 配置。

```bash
codexmux cpa install
cd menubar
./package-app.sh
ditto .build/release/CodexMux.app /Applications/CodexMux.app
open /Applications/CodexMux.app
```

打开菜单栏或执行 `codexmux install` 时，会记录当前 `codexmux` 可执行文件和
Codex 配置的绝对路径，因此应先把 CLI 放到 `~/.local/bin/codexmux`。`~/.codex`
若是指向真实 Codex 数据目录的符号链接，默认路径即可；否则先设置
`CODEX_CONFIG` 或 `CODEX_HOME`。

也可以在终端执行同样的接管：

```bash
codexmux install
```

该命令会启用 Codex 托管配置，再注册并启动当前用户的 LaunchAgent。登录后
代理会自动运行，但 LaunchAgent 里的进程不再改写 Codex 配置（见下方 TCC
说明）。`codexmux uninstall` 会停止代理、注销 LaunchAgent 并恢复原配置。

#### 调试：在前台运行

```bash
codexmux serve
```

`serve` 会在成功绑定本地端口后备份并接管 Codex 配置。按 `Ctrl-C` 或收到
正常终止信号时，代理会停止接收新请求，最多等待现有请求 30 秒，再恢复原
配置。终端被强制关闭、进程崩溃或系统断电时可能来不及恢复；下一次启动会
根据持久化的操作阶段完成接管或恢复。

启动或停止 CodexMux 后都应完全退出并重新打开 Codex，使其重新读取配置。

### 6. 检查运行状态

在已正确设置 `CODEXMUX_PROXY_TOKEN` 的终端中运行：

```bash
codexmux doctor
```

正常情况下应看到：

```text
proxy: reachable
CPA: reachable
settings: ok
credentials: ok
catalog snapshot: available
managed config: enabled
```

首次启动且 Codex 尚未请求模型目录时，出现
`catalog snapshot: not fetched yet` 是正常现象。`doctor` 会报告代理和 CPA
是否可访问，但这两项不可访问时只打印状态；请根据输出修复后再继续。

### 7. 重新启动 Codex

打开菜单栏、执行 `codexmux install` 或前台 `serve` 后，CodexMux 已经在
Codex 的 `config.toml` 中加入托管 provider 配置，并保留原文件备份。默认
配置文件是 `~/.codex/config.toml`。

CodexMux 不会写入静态 `model_catalog_json`。Codex 会从 CodexMux 的
`/v1/models` 动态获取模型目录。

完全退出并重新启动 Codex。模型选择器中应同时看到：

- 原名称的官方模型，例如 `gpt-5.6`；
- 带 ` · CPA` 显示后缀的 CPA 模型，例如模型名称为 `cpa/gpt-5.6`。

如果 CPA 中新增、删除或修改模型，重新打开模型选择器或新开对话，使其再次
请求模型目录即可，无需手动编辑 CodexMux 配置。Codex 的模型列表是事件驱动
的，不会在空闲时轮询。CPA 因冷却等原因暂时从上游目录消失的模型，会按上次
成功快照继续列在选择器中；请求在上游恢复前仍可能失败。
连续缺失满 24 小时后，模型才会从下一份完整快照中移除；期间重新出现会重置
缺失计时。

## 日常命令

| 命令 | 作用 |
| --- | --- |
| `codexmux status` | 显示数据路径、Codex 配置路径和启用状态 |
| `codexmux doctor` | 检查配置、凭据、环境变量、服务连通性和目录快照 |
| `codexmux serve` | 在前台运行代理，启动时接管、退出时恢复 Codex 配置 |
| `codexmux serve --no-codex-config` | 在前台运行代理但不管理 Codex 配置 |
| `codexmux install` | 启用 Codex 托管配置，然后注册并启动 LaunchAgent |
| `codexmux uninstall` | 停止并注销 LaunchAgent，同时恢复 Codex 配置 |

`status` 显示的是配置管理状态，不代表后台进程一定可访问；检查服务连通性应
使用 `doctor`。

LaunchAgent 中的代理以 `serve --no-codex-config` 运行：macOS 的 TCC 可能
拒绝后台进程访问 Codex 配置所在的卷，并且 `open()` 会无限阻塞而不是立刻
报错。Codex 配置的启用与恢复由菜单栏应用，或终端里的 `codexmux install` /
`codexmux uninstall` 完成。

### 管理本地 CPA

CodexMux 会下载并托管一个固定版本的本地 CLIProxyAPI（当前为 7.2.146）：

| 命令 | 作用 |
| --- | --- |
| `codexmux cpa install` | 下载固定版本的 CLIProxyAPI 发布包（校验 sha256 后解压）、写入托管配置并启动 |
| `codexmux cpa start` / `codexmux cpa stop` | 启动或停止本地 CPA 服务，并记录为启动偏好 |
| `codexmux cpa sync-start` | 按保存的启动偏好对齐 CPA 运行状态（菜单栏启动时调用） |
| `codexmux cpa autostart-set <bool>` | 设置 CodexMux 启动时是否一并启动 CPA |
| `codexmux cpa status` | 显示已安装版本、运行状态和启动偏好 |
| `codexmux cpa model-list` | 列出当前 CPA 端点提供的模型 slug |
| `codexmux cpa management-url` | 输出 CPA Web 管理页地址；`--connect` 会为本地 CPA 附带当前端点和管理密钥，供 Web UI 自动登录 |
| `codexmux cpa management-key` | 显式输出 CPA Web 管理密钥（请勿记录或分享） |
| `codexmux cpa provider-import <file>` | 从 TOML 文件导入 `[[openai-compatibility]]` / `[[codex-api-key]]` 提供商并重启服务 |
| `codexmux cpa profile-list` | 列出已保存的 CPA 端点配置与当前配置 |
| `codexmux cpa profile-save <name> --base-url <url>` | 从 `CODEXMUX_CPA_PROFILE_TOKEN` 读取令牌并保存或更新 CPA 端点配置 |
| `codexmux cpa profile-switch <name>` | 验证端点后切换；启动失败会恢复原配置 |
| `codexmux cpa profile-remove <name>` | 删除已保存的 CPA 端点配置 |
| `codexmux cpa uninstall` | 移除 CPA LaunchAgent（保留二进制、配置和登录凭据） |

保存端点前先把令牌放入当前 shell 的环境，避免凭据进入命令历史：

```bash
export CODEXMUX_CPA_PROFILE_TOKEN='CPA 端点令牌'
codexmux cpa profile-save remote \
  --base-url https://cpa.example.com/v1
```

安装位置和数据：

- 二进制：`$CODEXMUX_HOME/cpa/cli-proxy-api`
- 配置：`$CODEXMUX_HOME/cpa/config.yaml`（CodexMux 托管；手动改过的配置不会被覆盖）
- 日志：`$CODEXMUX_HOME/logs/cpa-*.log`
- 登录凭据：`~/.cli-proxy-api`（与 CPA 自身约定一致）

本地 CPA 只监听 `127.0.0.1`，对 Codex 侧的接入方式与远端 CPA 完全一致：
CodexMux 按 `cpa/` 前缀路由到 `config.toml` 中配置的 CPA 地址。

### 显式直连与审批模型覆盖

如果某个上游原生支持 Responses API，可以把模型 slug 显式直连过去，绕过 CPA
的协议执行层。直连路由声明自己的模型列表，模型不必存在于 CPA 目录中，因此
不装 CPA 也可以用（例如接入第三方 Responses 端点）。令牌从环境变量读取，
避免出现在命令行参数和 shell history：

```bash
export CODEXMUX_DIRECT_TOKEN='直连上游令牌'
codexmux cpa direct-add gpt-5.6-sol,gpt-5.6-terra \
  --base-url https://responses.example.com/v1
# 本地 slug 与原生 Responses model id 不同时，显式声明映射：
codexmux cpa direct-add glm-5.3-uni \
  --upstream-model zai-org/GLM-5.3 \
  --base-url https://responses.example.com/v1
codexmux cpa direct-list
```

直连配置保存在权限为 `0600` 的 `cpa-profiles.toml` 中。已存在于目录快照的
`cpa/<slug>` 按 CPA（或其直连覆盖）路由；快照没有的声明模型会在解析时回退到
声明的直连路由；两者都没有的未知模型仍然拒绝。CPA 不可达时，`/v1/models`
返回官方目录加直连模型的降级视图（仅内存，不落盘）。远端直连地址必须使用
HTTPS，本机回环地址可以使用 HTTP。直连令牌必须与 CodexMux proxy token、CPA
token 和 CPA 管理密钥全部不同。删除部分模型或整个直连路由：

```bash
codexmux cpa direct-remove gpt-5.6-sol --base-url https://responses.example.com/v1
codexmux cpa direct-remove --base-url https://responses.example.com/v1   # 整条路由
codexmux cpa direct-clear
```

菜单栏“直接端点”子菜单提供添加与移除；令牌通过进程环境传给 CLI，不会出现在
参数或日志里。

隐藏模型 `codex-auto-review` 默认始终走官方路由，官方请求失败时不会自动切换到
CPA。若确实需要覆盖，可以显式指定一个当前 CPA 目录中的模型：

```bash
codexmux cpa review-set glm-5.3-uni
codexmux cpa review-get
codexmux cpa review-set ''
```

最后一条命令清除覆盖并恢复官方路由。菜单栏应用的“审批模型”子菜单提供相同
操作。

### 菜单栏应用

仓库内的 `menubar/` 会打成 `/Applications/CodexMux.app`。它是整栈的控制器，
不是附加开关：

- 打开应用会启动代理，并按保存的启动偏好对齐本地 CPA（未安装 CPA 时状态栏
  会显示“CPA：未安装”，并提供“安装 CPA…”入口）；
- 退出应用会停止代理与 CPA，并还原 Codex 配置；启动偏好保持不变，下次打开
  按该偏好决定是否启动 CPA；
- 显示 CodexMux 与本地 CPA 的运行状态；
- 重启 / 停止 CodexMux，启动 / 停止 CPA，安装 CPA；
- “随 CodexMux 启动 CPA”开关（即启动偏好）；
- 管理直接端点（添加 / 移除），不依赖 CPA 是否安装；
- 切换已保存的 CPA 端点配置；
- 显式选择 `codex-auto-review` 使用的 CPA 模型；
- 打开日志目录；
- 打开 CPA Web 管理页（本机 CPA 会自动带上当前连接信息并登录）；
- 将 CPA Web 管理密钥复制到剪贴板；
- 中文 / 英文 / 跟随系统语言。

构建并安装：

```bash
cd menubar
./package-app.sh
ditto .build/release/CodexMux.app /Applications/CodexMux.app
open /Applications/CodexMux.app
```

`package-app.sh` 会生成带应用图标并经过本地临时签名的 `.app`；第二个可选参数可
指定 Swift 架构，例如 `./package-app.sh release arm64`。应用图标的可编辑矢量源
位于 `menubar/Resources/CodexMux.svg`。

菜单栏应用不替代 Codex 的普通模型选择；它只提供 CPA 端点切换和隐藏审批模型的
显式路由覆盖。

## 持续集成与发版

`.github/workflows/ci.yml` 会在 push、Pull Request 和手动触发时，于 macOS 上执行
Rust 格式检查、Clippy、全量测试、Release 构建，并实际打包和验证菜单栏应用。

推送 `v*` 标签会触发 `.github/workflows/release.yml`。标签版本必须同时匹配
`Cargo.toml` 与菜单栏 App 的 `CFBundleShortVersionString`；验证通过后会创建 GitHub
Release，并上传：

- `codexmux-<version>-macos-arm64.tar.gz`：Apple Silicon CLI、README 与 LICENSE；
- `CodexMux-<version>-macos-arm64.zip`：临时签名的菜单栏 App；
- `SHA256SUMS`：上述两个归档的 SHA-256。

例如发布 `0.1.0` 时，在版本文件已经同步并提交后推送 `v0.1.0` 标签即可。工作流
只对标签引用自动发版，不会从普通分支构建覆盖已有 Release。

### 查看后台日志

默认数据目录下执行：

```bash
tail -f "$HOME/Library/Application Support/CodexMux/logs/stderr.log"
```

标准输出位于同目录的 `stdout.log`。如果设置了 `CODEXMUX_HOME`，日志位于
`$CODEXMUX_HOME/logs/`。

### 更新 CodexMux

在仓库中重新编译并替换稳定路径下的二进制文件，然后重新安装 LaunchAgent
以重启服务：

```bash
cargo build --release
install -m 755 target/release/codexmux "$HOME/.local/bin/codexmux"
./menubar/package-app.sh
ditto menubar/.build/release/CodexMux.app /Applications/CodexMux.app
codexmux install
codexmux doctor
```

更新二进制文件本身不会让已经运行的进程自动加载新版本。CLI 需要重新
`codexmux install`；菜单栏需要替换 `.app` 后再打开一次。

## 停用和卸载

停止后台服务、注销 LaunchAgent 并恢复 Codex 配置：

```bash
codexmux uninstall
```

如果是前台运行，先按 `Ctrl-C`，CodexMux 会在退出前恢复 Codex 配置。

如果为 Codex Desktop 设置过环境变量，再清除它：

```bash
launchctl unsetenv CODEXMUX_PROXY_TOKEN
```

同时从 `~/.zshrc` 删除为 Codex CLI 添加的 `CODEXMUX_PROXY_TOKEN`，并打开
新终端。

上述操作会保留 CodexMux 数据、目录快照和日志。确认不再需要后可以删除：

```bash
rm -rf "$HOME/Library/Application Support/CodexMux"
```

应先退出前台代理或执行 `uninstall`，再删除数据目录，否则会丢失恢复 Codex
原配置所需的状态和备份。

## 故障排查

### `CODEXMUX_PROXY_TOKEN is missing or does not match credentials.json`

当前终端中的 `CODEXMUX_PROXY_TOKEN` 没有设置，或与 `credentials.json` 的
`proxy_token` 不一致。重新设置变量后，在同一个终端运行 `codexmux doctor`
或启动 Codex CLI。Codex Desktop 需要使用 `launchctl setenv` 并完全退出后
重开。

### `proxy: not running`

CodexMux 未启动，或监听地址已被其他进程占用。使用以下一种方式启动：

```bash
codexmux install
# 或者用于前台调试：
codexmux serve
```

如果刚执行过 `install`，查看 `logs/stderr.log`。

### `CPA: not reachable`

确认：

1. 本机 CPA 已启动，或远端 CPA 的 HTTPS 地址可以访问；
2. `config.toml` 中的 `cpa.base_url` 与 CPA API 根地址一致并包含 `/v1`；
3. CodexMux 的 `cpa_token` 存在于 CPA 顶层 `api-keys`；
4. 网络、防火墙和 TLS 证书允许当前 Mac 访问远端 CPA；
5. 修改 CPA 配置后已经重启 CPA。

### 模型列表为空或没有 CPA 模型

先运行 `codexmux doctor`，确认 CodexMux 和 CPA 均可访问。然后完全重启
Codex，使其重新请求模型目录。

CodexMux 只有在官方目录和 CPA 目录都成功返回有效数据后才会保存新的完整
快照。如果本次刷新失败，它会继续提供上一次成功的快照；首次刷新之前没有
可用快照，请检查后台日志。

除 Codex 内部使用的隐藏 `codex-auto-review` 条目外，CPA 目录中满足以下任一条件的
模型不会显示：

- `visibility = "hide"`；
- `supported_in_api = false`。

### `Codex config already defines ...; refusing to overwrite it`

CodexMux 会保留已有的 `model_providers` 服务商，并追加自己的
`model_providers.codexmux`。只有现有配置已经占用 CodexMux 自己的 provider 名称，
或定义了 CodexMux 需要临时托管的顶层 `model_provider`、`model_catalog_json` 时才会
拒绝接管。

先备份 `~/.codex/config.toml`，核对报错指出的具体冲突项后再处理。不要删除其他
`model_providers` 服务商配置；它们不会阻止 CodexMux 启动。

### `Codex config changed inside the managed CodexMux block`

CodexMux 接管后，其托管标记之间的内容被手动修改。为避免误删用户配置，
自动接管和恢复会停止操作。恢复托管块原状后重试，或根据
`state/codex-config.json` 与 `backups/` 中的记录手动核对。

### 修改了 `credentials.json` 后服务无法启动

确认文件是普通文件、JSON 格式正确，`proxy_token`、`cpa_token` 和
`cpa_management_key` 均非空且互不相同，并恢复严格权限：

```bash
chmod 600 "$HOME/Library/Application Support/CodexMux/credentials.json"
codexmux install
```

### 使用自定义 Codex 配置路径

CodexMux 按以下优先级寻找 Codex 配置：

1. `CODEX_CONFIG` 指定的完整文件路径；
2. `$CODEX_HOME/config.toml`；
3. `~/.codex/config.toml`。

`~/.codex` 作为目录符号链接（例如指向另一块磁盘上的真实 Codex 数据目录）
是支持的：CodexMux 会解析到真实文件后再比较托管状态。配置文件本身不能是
符号链接，以免原子替换破坏由 dotfiles 工具维护的链接；请直接指定链接目标
文件。相对形式的 `CODEX_CONFIG` 会在执行命令时转换为绝对路径。

## 模型目录与请求路由

Codex 请求：

```http
GET http://127.0.0.1:48682/v1/models?client_version=<Codex version>
```

CodexMux 会并行请求官方 ChatGPT Codex 目录和 CPA 原生 Codex 目录，再合并
结果：

- 官方模型 slug 保持不变；
- CPA 模型 slug 从 `<model>` 改为 `cpa/<model>`；
- CPA 模型 `display_name` 追加 ` · CPA`；
- CPA 返回的其他模型字段原样保留，包括未来新增的未知字段。

合并后的完整目录会原子写入 `model-catalog.json`，同时作为精确路由表。若
新的 CPA 目录暂时少了某个上次见过的模型（例如上游冷却），该条目仍会留在
快照里供选择器显示。首次缺失时间原子写入 `model-catalog.retention.json`；连续
缺失满 24 小时才移除，重新出现则清除计时。CPA 请求转发前只把顶层模型名从
`cpa/<model>` 还原为 `<model>`；显式直连映射可进一步换成声明的原生 model id，
其余原生 Responses 请求保持不变。

## 在同一对话中切换模型

响应 ID 只属于生成它的具体后端和模型，不能直接交给另一个路由。切换官方
模型、切换 CPA 模型，或在官方与 CPA 之间切换时，CodexMux 会移除
`previous_response_id`，改为回放本地记录的公开对话历史。

回放内容仅包括允许的文本消息、工具调用与结果，以及公开压缩内容。推理、
签名、加密内容、供应商私有状态和未知项目不会跨路由传递；图片也不会在切换
时回放。历史保存在内存中，重启 CodexMux 后会清空。历史链不完整、已淘汰或
无法确认安全时，请求会被明确拒绝，而不是把响应 ID 发给错误的供应商。

## 安全边界

- CodexMux 提供给 Codex 的监听地址必须是本机回环地址；
- 本机 CPA 可以使用回环 HTTP，远端 CPA 必须使用 HTTPS；
- CodexMux 的所有接口都要求 `x-codexmux-token`；
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
负责外部供应商协议转换并提供原生 Codex 模型目录。CodexMux 不包含 CPA
源码。
