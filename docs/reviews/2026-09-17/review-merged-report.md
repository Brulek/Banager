# Banager 设计草案 v0.1 · 四家 AI 评审合并报告

日期：2026-09-17　评审对象：`design-draft.md`（草案 v0.1）

## 0. 评审是怎么做的，结论有多可信

| 评审方 | 方式 | 产出 |
|---|---|---|
| Gemini（agy，effort high） | 通读 + 知识核查 | 4 致命 / 4 重大 / 4 次要 / 25 行事实表 / 6 缺失 |
| Grok（联网实时核查） | 通读 + 打开真实 issue/文档 | 5 致命 / 6 重大 / 11 次要 / 60 行事实表 / 12 缺失，附来源链接 |
| Codex（medium 推理） | 通读 + 查官方文档与上游源码 | 7 致命 / 10 重大 / 6 次要 / 80 行事实表 / 10 缺失 |
| Claude 六视角（三平台、适配器、产品、交付、Rust/Tauri、开源运营） | 各自通读 + 本机跑命令 + 联网 | 59 条；"安全"视角因额度中断未产出 |

**原计划的"每条意见三名裁判对抗裁定"没有完成**：裁判阶段撞上 Claude 5 小时会话额度，工作流返回的"56 条被驳回"是空投票造成的，一律作废。替代方案：

1. 以"几家独立提出同一问题"作为可信度依据（下文标注）；
2. 30 条有争议或影响设计的事实，由我在本机跑命令、打开官方文档和 GitHub 原帖逐条核实（第 4 节）。

Grok 引用的 10 个 GitHub issue/PR 编号我全部打开核对，9 个真实且内容相符（UniGetUI discussion #4943 未找到；CVE-2026-42184 真实但官方评级 medium，不是它说的 CVSS 8.8）。

## 1. 一句话总评

四家的总评惊人一致：**草案把 15 套包管理器的"事务、身份、权限"差异误当成了"输出格式"差异，把面向普通人的产品建在一条普通人走不通的分发通道上；架构分层（内核不依赖 Tauri、fixtures 测试）是对的，但 §4–§6 要重写，§0 里至少两条约束要重新考虑。**

## 2. 四家一致（≥3 家独立提出）的问题

### 2.1 未签名首发 × "面向普通人" = 互相取消（4/4，致命）

- macOS 15 起，未公证 app 的"右键→打开"绕过已失效，必须去 系统设置→隐私与安全性→"仍要打开"（Apple 官方 102445，我已核对）；Apple Silicon 上完全未签名的二进制会直接报"已损坏"。Windows 上 SmartScreen 是拦截级。
- UniGetUI 自己也是 2026.1.9 才把 macOS 签名补上（release notes 原文："All macOS build artifacts are now properly code signed"，已核对）。
- "有 star 再签"是倒因果：普通人打不开 → 没 star → 永远不签。
- **建议**：Apple Developer（99 美元/年）+ 公证提前到首发前；Windows 首发用 SignPath（开源免费，门槛是"已公开发布"而非星数）或至少 winget 社区仓库分发（winget 不要求签名）。若坚持不签，把定位改成"给会用终端的人"，删掉所有"普通人"文案。

### 2.2 `pacman -Sy` 后 `-Qu` 制造 Arch 部分升级（4/4，致命）

草案 §4.3 写"`pacman -Qu`（需先 `-Sy`）"。`-Sy` 刷新同步库不升级系统，之后任何安装（含 Banager 商店装新包）都是 Arch 官方明令不支持的 partial upgrade；配合 §8 后台定时检查，等于定时把用户 DB 拉到未来。**改法**：查更新只用 `checkupdates`（pacman-contrib，独立临时 DB，退出码 0 有更新 / 2 无更新 / 1 错误），装新包用 `-Syu {id}` 整体事务并在预览里写明"会升级整个系统"，禁止 `-Sy`。

### 2.3 "适配器是数据不是代码"站不住（4/4，致命）

- 我本机核实：`brew info --json=v2` 里 formula 与 cask 是两套 schema（cask 主键 `token`、`installed` 是字符串、没有 `installed_on_request`）；`npm ls -g --json` 顶层是 `dependencies` 对象不是数组；`brew search --desc` 输出带 `==> Formulae` 分节头，草案的单行正则会把它当包名。
- Codex 指出草案示例 `[install]   command = [...]` 单行写法**不是合法 TOML**（我用 tomllib 验证：`Expected newline or end of document after a statement`）。
- winget 无机器可读输出（winget-cli#2032 已关闭但结论是"用 PowerShell 模块/COM"，CLI 至今没有 `--output json`，2026-01 仍有人要求重开）；UniGetUI 2024 年 PR #2035 已改用 COM API。
- 查更新这一列，pipx、cargo、ollama、独立安装器、winget 五类都要 native 策略，"声明式"只剩 list/install。
- **改法**（Codex 版本最完整）：适配器 = Rust 行为接口（detect / inventory / candidates / plan / execute / reconcile）+ TOML 只承载元数据与简单命令映射；声明能力矩阵（能否搜索、逐包升级、原生取消、后台检查）；冻结接口前先打通 brew cask、winget、apt、flatpak 四条最难的纵向样板。CONTRIBUTING 删掉"30 行 TOML 加一个包管理器"。

### 2.4 三平台提权方案都不闭环（4/4，致命）

- macOS："brew 不用 root"不等于"macOS 不需要提权"：cask 的 `.pkg` 安装脚本会要 sudo（Codex，Homebrew Cask Cookbook）。
- Linux：`pkexec` 需要会话里有 polkit 认证代理（Hyprland/Sway/i3 常没有）；它会清空环境变量（草案 TOML 里的 `env` 对提权命令完全无效）、每次调用都要密码；用户取消退出码 126、未授权 127（pkexec 手册，已核对）。后台定时检查若触发它，就是"托盘程序突然弹密码框"。
- Windows：`Start-Process -Verb RunAs` 不能与输出重定向同用、提权进程杀不掉（中完整性进程拿不到高完整性进程的 TERMINATE 句柄）、Job Object 同理；而 winget 本身会自行弹 UAC，再包一层 RunAs 反而毁掉流式输出。
- **改法**：权限策略放到"具体操作 × 安装实例"上而非 adapter 顶层；Linux 自带 `.policy` 文件、检测认证代理、后台检查绝不触发提权；Windows 首版就做"每次操作起一个、结束即退的最小提权 helper + 命名管道"（不能推到 v1.x），或首版只做不需 UAC 的源（scoop、用户级 winget、npm）。

### 2.5 "取消 = 杀进程树"是假取消（Codex、Grok、Claude×2）

对 apt/dnf/pacman 在 unpack/configure 阶段 SIGKILL 会留下 "dpkg was interrupted"；snap 事务在 snapd 里，杀 CLI 没用；提权进程根本杀不掉；tokio `Child::kill` 只杀直接子进程，brew/npm/cargo 的孙进程会成孤儿（需 `process_group(0)` + 组 kill）。**改法**：状态机加 `cancel_requested / cancelling / detached / needs_reconciliation`；每个操作声明取消策略；取消后重新查询管理器状态，不伪造成功/失败。

### 2.6 后台定时检查在 apt/pacman 上不可行（Gemini、Grok、Claude×3）

`apt-get update` 需 root；unattended-upgrades/packagekitd 会占 dpkg 锁。**改法**：后台只读本地缓存（`/var/lib/apt/lists/` 时效标注），只在用户主动点"刷新"时提权；写明锁冲突的退避与提示。

### 2.7 竞品叙事过期，差异化会被逐条打脸（Grok、Codex、Claude×2）

UniGetUI 2026.3.0 已三平台、已覆盖草案 15 个来源里的 12 个、macOS 已签名、有公司维护；Windows 用户正在骂它的 Avalonia UI（discussion #4770 "The new design"，已核对）。同类还有 topgrade（4.5k 星，CLI 一键更新一切，含 ollama）、bauh（1.4k，Linux）、Applite（7k，macOS cask）。诚实的差异只剩：ollama 模型、独立安装器/游离二进制、克制的 UI。"10 MB vs 50 MB"在 Linux AppImage 上不成立（AppImage 打包 WebKitGTK 约 70 MB），Tauri 的"轻"依赖系统 WebView，README 不写清会被 HN 第一句反驳。

### 2.8 范围与目标用户（Gemini、Grok、Claude 产品+交付视角）

三家都建议分级首发（如 v0.1 只发 macOS 或 macOS 正式 + Win/Linux 标 experimental），并指出"普通人"画像自相矛盾：真正的普通人机器上没有 brew/npm/pip/cargo/ollama，打开概览是"已装 0"；真实用户是"被 AI agent 塞满电脑的爱好者"（就是作者本人）。**这条与 §0 的约束冲突，需要你拍板**（见第 6 节）。

### 2.9 macOS GUI 进程的 PATH 是空的（Gemini、Codex、Claude）

我本机核实：`launchctl getenv PATH` 为空，Finder/Dock 启动的 app 只有 `/usr/bin:/bin:/usr/sbin:/sbin`，`brew`、`~/.local/bin/claude` 全找不到；终端里 `tauri dev` 一切正常、打包后双击全部失效。**改法**：启动时用 `fix-path-env-rs`（tauri-apps 官方 crate）或 `$SHELL -lc 'printenv PATH'` 注水；适配器改用探测到的绝对路径执行。

### 2.10 同一工具多条安装路径（Grok、Codex、Gemini）

claude 可同时来自 native installer（本机核实：`~/.local/bin/claude` 是软链到 `~/.local/share/claude/versions/2.1.273`）、brew cask、npm、winget；双重安装曾导致 `claude update` 下到 0 字节（claude-code#37393，已核对）。uv/bun/pnpm 同理。**改法**：数据模型引入"安装实例"（管理器类型 + 可执行绝对路径 + prefix + 作用域），检测/列出/执行绑定同一实例；冲突时 UI 显示"检测到 2 个 claude"，卸载只走官方方法、禁止硬删目录。

### 2.11 "显式安装 ≠ 可删除"（Codex、Grok、Claude）

`pip list --not-required` 是"没人依赖它"不是"用户主动装的"；apt `showmanual` 含 `ubuntu-desktop` 这类元包；brew 的 `installed_on_request` 只是安装原因。**改法**：`explicit: bool` 改成安装原因枚举（含 unknown）；删掉目录级 `safe_to_remove: yes`；卸载前展示管理器实际会删什么、影响哪些依赖。

## 3. 单家提出、但我核实属实或高度可信的重要问题

**Codex（工程最深）**
- 更新候选不是"公共注册表最新版"：PyPI/crates.io 最大版本可能不兼容当前解释器/工具链；Git/私有源包不能查公网。需要 `UpdateCandidate`（来源、渠道、约束、可执行性）。
- "全部更新"会绕过应用内忽略列表：`brew upgrade` 升级的是全部，不是界面勾选的 8 个；要区分"更新选中项"与"来源原生全量升级"。
- 退出码不是产品事实：需要 `verifying` 阶段核对目标实例版本；结果分 成功/无变更/部分成功/需重启/待确认/失败。
- 按来源串行防不住跨来源冲突（brew 升 node 时 npm 在装包；winget/choco/scoop 装 MSI 都经 Windows Installer 互斥）。用资源锁而非来源做并发单位。
- Tailwind v4 基线是 Safari 16.4 / Chrome 111，必须写死最低 macOS / WebView2 / WebKitGTK 版本。
- 缺：适配器 schema_version 与管理器最低验证版本；持久化状态机与崩溃恢复；首发验收标准。

**Claude 六视角（本机实测 + 联网）**
- `ubuntu-22.04` runner 自 2026-09-17（今天）进入弃用期，2027-04-17 下线（actions/runner-images 公告，已核对）；换 24.04 会把 glibc 基线抬到 2.39。
- 作者只有 Apple Silicon Mac，VM 里只能跑 arm64 客户机，而发布物是 x64：首发前不会有任何一台真机跑过 Windows/Linux 产物。
- GNOME 默认不显示 appindicator 托盘：Fedora/Debian 用户勾了"关闭到托盘"后窗口直接消失。
- brew JSON `installed[0]` 按安装时间升序，多版本共存时取到的是最老版本；`brew search --desc` 只匹配描述不匹配名字（搜 ripgrep 搜不到 ripgrep）。
- 逐行 `emit` 事件推日志会压垮 WebView（cargo/brew 编译每秒上千行），应改 Tauri Channel + 时间片合并，并处理 `\r` 进度行。
- `rusqlite::Connection` 是 `Send + !Sync`，不能直接塞进 Tauri State 并发用；刷新写入需单事务 + 代际标记，否则界面闪"0 个包"或旧数据覆盖新状态。
- "内核不依赖 Tauri"缺三样约定：事件出口（不能引 AppHandle）、运行时归属（不能双 tokio）、路径注入。
- 私有仓库跑三平台 CI，macOS 分钟 10 倍计费，一周十几次 push 就耗尽 GitHub Free 的 2000 分钟。
- `winget list` 也显示非 winget 安装的软件，会与 choco/独立安装器重复。

**Grok（联网核实，引用均已打开核对）**
- Tauri Linux 现状：#15665 默认 AppImage 在 Mesa 25+/Ubuntu 26.04 上 WebKitWebProcess abort（open）；#14234 GNOME Wayland 下 .deb 托盘图标消失、只有 AppImage 正常（open，与上一条互斥）；#14286 WebKitGTK 字重偏粗 100（open）；tray-icon#303 Windows 托盘菜单跑到任务栏后面（open）。**建议 Linux 首发只出 .deb/.rpm，AppImage 标实验**。
- CVE-2026-42184：Tauri `is_local_url` 在 Windows 误判，2.11.1 修复（真实，评级 medium）；锁定 Tauri ≥ 2.11.1。
- scoop 常是 profile 函数，`powershell -NoProfile` 会找不到；应调 `scoop.cmd` shim。
- dnf5（Fedora 41+）命令是 `dnf check-upgrade`，支持 `--json`（已核对）。
- 名字 Canager 像 Manager 打错，不可发音、不可搜索（Claude 开源视角同样提出）。

**Gemini**
- 非英文 Windows 控制台是 OEM 代码页（GBK），`String::from_utf8` 会直接崩，需要 `encoding_rs`。
- CLI 子进程不继承系统 GUI 代理设置，国内用户查更新会大面积超时，需要代理配置/注入。
- apt/rpm 一次返回 2000+ 包，前端必须虚拟滚动。
- 需要包管理器锁（dpkg lock、db.lck、brew.lock）的退避与提示。

## 4. 事实核查表（我亲自核实，2026-09-17）

| # | 争议事实 | 谁说什么 | 核实结论 | 依据 |
|---|---|---|---|---|
| 1 | `brew info --json=v2` cask 结构 | Grok：cask 用 `token`、`installed` 是字符串、无 `installed_on_request` | **属实** | 本机 jq：cask keys 含 token/installed(string)/auto_updates，无 installed_on_request；formula 有 installed_on_request |
| 2 | `brew upgrade` 不带参数是否含 cask | Grok：默认不含 | **Grok 错** | 本机 `brew upgrade --help`："upgrade all outdated formulae and casks" |
| 3 | `brew search --desc` 输出格式 | Grok/Gemini：有 `==> Formulae` 分节头 | **属实** | 本机输出首行即 `==> Formulae` |
| 4 | `npm ls -g --depth=0 --json` 结构 | Grok/Claude：顶层 `dependencies` 对象 | **属实** | 本机 jq keys = ["dependencies","name"] |
| 5 | `pip list --not-required --format=json` 能否并用 | Gemini：旧版不能 | **Gemini 错**（现代 pip） | 本机 pip 26.2.1 正常输出 JSON |
| 6 | 本机 pip 是否 PEP 668 外部管理 | 多家 | **属实** | `pip3 install --dry-run` → externally-managed-environment |
| 7 | ollama registry 匿名取 manifest 是否 401 | Gemini：需先换 token | **Gemini 错** | 走代理 curl → HTTP 200，manifest 与本机 `~/.ollama/models/manifests/.../qwen3.5/35b` 逐字节一致（config digest 相同） |
| 8 | ollama digest 比对会误报 | Grok：官方曾因 manifest 重序列化误判 | **部分属实** | 本机样本一致；实现时按 layers digest 比对而非整文件 |
| 9 | macOS GUI 进程 PATH | Gemini/Codex/Claude：只有系统默认路径 | **属实** | `launchctl getenv PATH` 为空；登录 shell PATH 才含 homebrew |
| 10 | claude 安装布局 | Grok：软链到 versions 目录 | **属实** | `~/.local/bin/claude -> ~/.local/share/claude/versions/2.1.273`；`claude --version` = `2.1.273 (Claude Code)` |
| 11 | Claude Code npm 安装是否已 deprecated | Grok：2026-01 起 deprecated | **未核实** | 官方 setup 页写 "Native Install (Recommended)"，npm 仍列在安装/卸载方法中 |
| 12 | `uv tool list --outdated` 是否存在 | Gemini：不存在；Grok/Codex：存在 | **存在，Gemini 错** | docs.astral.sh CLI 参考："--outdated: List outdated tools" |
| 13 | `pipx list --outdated` 是否存在 | Grok/Codex：pipx 1.16.0（2026-07-15）加入 | **高度可信，未直接核实** | 抓取 changelog 页失败；实现时按本机版本探测 |
| 14 | `scoop export` JSON 是否含版本 | Gemini：不含；Grok/Codex：含 | **含，Gemini 错** | scoop-export.ps1 的 apps 来自 scoop-list.ps1，后者用 `Select-CurrentVersion` 输出 Version |
| 15 | Chocolatey 2.x `choco list` 本地还是远程 | Gemini：远程；Grok/Codex：本地 | **本地（两家 + 2.0 变更记录），Gemini 错** | 文档页抓取只命中导航，未直接核实原文 |
| 16 | `choco outdated` 增强退出码 | Grok/Codex：2 = 有过时包 | **属实** | 文档有 Exit Codes 节，enhanced exit codes 需开启 |
| 17 | dnf5 检查更新命令 | Grok/Codex：`check-upgrade`，有 `--json` | **属实** | dnf5.readthedocs check-upgrade 页含 `--json` |
| 18 | winget CLI 有无 `--output json` | Grok：无，#2032 仍 open | **无 JSON 属实；issue 状态 Grok 错** | #2032 于 2024-03-06 以 completed 关闭（指向 PS 模块/COM），2026-01 仍有人要求重开加 CLI 开关 |
| 19 | `Microsoft.WinGet.Client` 是否预装 | 多家：需 Install-Module | **属实** | PowerShell Gallery 页：`Install-Module -Name Microsoft.WinGet.Client` |
| 20 | UniGetUI 是否改用 COM API | Grok：PR #2035 | **属实** | PR #2035 "Winget COM API compatibility"，2024-04 合并 |
| 21 | macOS 15 右键打开是否仍可绕过 Gatekeeper | 多家：已失效 | **属实** | Apple 102445：需 系统设置→隐私与安全性→"Open Anyway" |
| 22 | UniGetUI 2026.1.9 补签名 | Grok | **属实** | release notes："All macOS build artifacts are now properly code signed" |
| 23 | UniGetUI discussion #4770/#4943 是 UI 差评 | Grok | **#4770 属实（"The new design"）；#4943 未找到** | GitHub GraphQL |
| 24 | Tauri #15665 / #14234 / #14286 / tray-icon#303 | Grok | **全部真实、均 open、内容相符** | gh api |
| 25 | CVE-2026-42184 | Grok：CVSS 8.8 | **真实但评级 medium，2.11.1 修复** | Tauri 安全公告 |
| 26 | `ubuntu-22.04` runner 弃用 | Claude | **属实，2026-09-17 起** | actions/runner-images 公告 |
| 27 | `macos-latest` 架构 / `windows-latest` 版本 | Claude/Grok | **macos-latest = macOS 26 arm64；windows-latest = Server 2025；ubuntu-latest = 24.04** | runner-images README |
| 28 | pkexec 退出码与认证代理 | Grok/Codex/Claude | **属实**：未授权 127、取消/无法认证 126、依赖会话认证代理 | pkexec 手册 |
| 29 | `checkupdates` 退出码 0/2/1 | 三家一致 | **未直接核实（源码抓取失败），三家一致** | — |
| 30 | 草案 TOML 单行 `[table] key = ...` 是否合法 | Codex：非法 | **属实** | tomllib：Expected newline… |
| 31 | fix-path-env-rs 是否存在 | 多家推荐 | **存在** | tauri-apps/fix-path-env-rs，82 星 |
| 32 | Windows Defender 会在用户看到按钮前删除未签名文件 | Claude 开源视角 | **未核实** | — |

## 5. 不需要你拍板、直接改进文档的技术修正清单

1. §4.1 TOML 示例改为合法语法；formula/cask 分两套字段映射；`brew search` 改为 `brew search {query}` + `--desc` 两次并跳过 `==>` 行；版本取 `installed[-1]` 或按 `linked_keg`。
2. §4.3：pacman 改 `checkupdates`；dnf5 改 `check-upgrade --json`；scoop 改调 `scoop.cmd` + `ConvertTo-Json`；`npm ls` 允许退出码 1 且只解析 stdout；pip 改为"每个解释器一个实例"且用 `python -m pip`；`apt-get install --only-upgrade -y {id}` 写全；choco 查询不提权、写操作提权、退出码含 2；uv/pipx 用原生 `--outdated`；ollama 不做 `ollama list` 兜底（它也依赖服务且会拉起 Ollama.app）。
3. §4.4：每个独立安装器写 detect/version/latest/upgrade/uninstall 合同 + 多来源优先级；agy 与 Gemini CLI 分开；nvm 是 shell 函数不能按可执行文件识别；Docker Desktop 删掉。
4. §5：加 `ManagerInstance`、`InstalledArtifact`、`UpdateCandidate`；`explicit` 改枚举；删 `safe_to_remove`。
5. §6：重写提权（按操作 × 实例）、取消（状态机 + 策略）、结果核对（verifying）、资源锁；加 PATH 注水、编码转换、代理注入、锁退避、超时（探测 30 s / 安装 15 min）。
6. §7：一套 UI + "显示技术细节"开关（UniGetUI 刚因双 UI 被骂）；首启不问语言、不先要后台权限；apt/rpm 列表虚拟滚动；已装列表默认只显示叶子包。
7. §8：后台检查只读缓存、永不提权、永不拉起其它程序；GNOME 无托盘时降级。
8. §10：Tauri ≥ 2.11.1；`macos-latest` 是 arm64，universal 需显式双 target；`ubuntu-22.04` 弃用后的 glibc 基线策略；Linux 首发只出 .deb/.rpm；写最低 OS/WebView 版本；README 体积数字按平台分别写并注明"依赖系统 WebView"；updater 签名密钥与代码签名证书是两回事，可以先做。
9. §11：fixtures 必须来自真机（招募测试者或 CI runner 录制），禁止 AI 生成；CI 冒烟只测解析，不测真实安装；补首发验收清单。
10. §12：补 LICENSE、SECURITY.md、校验和、"提权时到底执行什么"说明、AI 参与声明；发布渠道写清 winget 首日可提交、官方 homebrew-cask 要求 ≥ 75 星（以官方当前门槛为准）、Flathub 最后。

## 6. 需要你拍板的 5 个决定

| # | 决定 | 四家意见 | 与你已定约束的关系 |
|---|---|---|---|
| 1 | **签名与公证提前到首发前**（至少 macOS Developer ID 99 美元/年） | 4/4 认为不签就没有普通人用户 | 推翻 §0"先不签" |
| 2 | **三平台同日首发改为：macOS 正式版 + Windows/Linux 标 experimental / nightly** | 3/4 明确建议（Codex 保留同日但要求发布日期服从验收矩阵） | 推翻或弱化 §0"同日首发" |
| 3 | **15 个来源全覆盖改为"全部能列出、能查更新；写操作分级"**：首发写操作只开 brew、npm、pipx/uv、ollama、scoop、winget 用户级、flatpak 用户级；apt/dnf/pacman/choco/cargo 首发只读 | Gemini/Grok/Claude 建议砍，Codex 建议保留但补验收矩阵 | 弱化 §0"全覆盖" |
| 4 | **目标用户从"普通人"改为"被 AI 工具塞满电脑的爱好者/开发者"**，文案与渠道随之改 | 4/4 指出"普通人"画像不成立 | 修改 §0/§1 定位 |
| 5 | **名字**：Canager 像 Manager 打错、不可发音、搜索被淹没 | Grok、Claude 提出 | §0 名字 |

其余问题（第 2、3、5 节）不改变你的约束，我会直接改进设计文档。

## 附：原始评审文件

- `gemini.out`（24 KB）、`grok.out`（42 KB）、`codex.out`（46 KB）
- Claude 六视角 59 条：工作流 journal `wf_6e80bcf2-960/journal.jsonl`
