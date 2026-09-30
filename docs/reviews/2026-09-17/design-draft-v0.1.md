# Canager 设计文档（草案 v0.1，2026-09-17，待多方评审）

> 状态：草案。本文用于评审，尚未开始任何编码。评审目标：找出会让项目失败、返工或拿不到 star 的问题。

## 0. 已定需求（评审时视为约束，除非你认为它会让项目失败）

| 项 | 决定 | 备注 |
|---|---|---|
| 作者 | 非专业程序员，电子设备爱好者，主要靠 AI agent 写代码；只有 Mac 电脑 | 首个开源项目 |
| 目标 | GitHub star；面向"普通人"的包管理器管家 | |
| 平台 | macOS + Windows + Linux **同日首发** | 作者已被劝告风险并坚持 |
| 来源覆盖 | brew / winget / scoop / choco / apt / dnf / pacman / snap / flatpak / npm / pip / pipx / uv / cargo / ollama + 已知独立安装器 + 来源不明扫描 | 全覆盖，作者坚持 |
| 功能 | 已装管理（列出/查更新/更新/卸载）+ 精选商店 + 自由搜索 | |
| 分发 | GitHub Releases；首发不签名，README 写绕过步骤；有 star 后补签名 | |
| 语言 | 英文默认、内置简体中文，跟随系统 | |
| 普通人友好 | 依赖库自动标灰 + 精选清单手写中英白话说明 | 不接 AI 解释 |
| 后台 | 托盘常驻可选，首次启动询问 | |
| 名字 | Canager | GitHub / npm / brew / PyPI 均未被占 |
| 竞品 | UniGetUI（2.6 万星，C#/Avalonia，2026 起三平台，被 Devolutions 收购）；mxcl/package-manager-manager（macOS，Swift，2 个月，41 星） | |

## 1. 定位

> **Canager：一个 10 MB、长得像原生应用、普通人也看得懂的包管理器管家。** 顺带做 UniGetUI 不做的 ollama 模型、AI CLI、脚本安装的游离工具。

三个刀刃：**更轻**（Tauri，安装包 ≈10 MB，UniGetUI macOS 版 50 MB）、**更好看**（原生质感、留白、系统字体、深色模式，不是 Fluent 密集表格）、**更懂普通人**（依赖标灰、白话说明、"能不能删"、精选商店）。

差异化清单（UniGetUI 代码与 issue 中均无）：ollama 模型管理；claude / codex / gemini(agy) / grok 等 AI CLI 的识别与更新；`curl | sh` 装的游离工具（uv、rustup、bun、deno、nvm、mise…）；来源不明可执行文件扫描；简单/高级两档界面。

## 2. 技术选型

- **Tauri 2**（Rust 内核 + 系统 WebView）。选它因为"更轻"是刀刃，Electron 150 MB 比竞品还重。
- 前端：React 19 + TypeScript + Vite + Tailwind CSS v4 + Radix Primitives（无样式，自定视觉）+ TanStack Query（后端数据）+ Zustand（界面状态）+ i18next。
- Rust：tokio、serde / serde_json / toml、regex、serde_json_path（JSONPath）、rusqlite（bundled）、reqwest（仅少数需要联网查版本的适配器）、tracing。
- Tauri 插件：tray（内置）、notification、autostart、single-instance、os、log、store（或直接 SQLite）、updater（后期，需签名）。
- 包管理：pnpm；Rust 工具链 stable；CI 用 GitHub Actions + tauri-action。

## 3. 架构

```
canager/
├── crates/
│   ├── banager-core/        纯 Rust 库，不依赖 Tauri
│   │   ├── adapter/          TOML 适配器加载、命令模板、解析器（json/lines/table/native）
│   │   ├── runner/           子进程执行（tokio::process）、流式输出、超时、取消、提权策略
│   │   ├── scanner/          独立安装器识别、来源不明扫描
│   │   ├── ops/              操作队列（按来源串行、跨来源并行）、历史
│   │   ├── catalog/          精选商店清单加载、与已装匹配
│   │   ├── store/            SQLite：包缓存、历史、设置、忽略列表
│   │   └── model.rs          Source / Package / Operation / CatalogEntry / UnknownBinary
│   └── canager-cli/          （v1.1）薄命令行壳：canager list / outdated / doctor
├── src-tauri/                Tauri 壳：IPC 命令、托盘、通知、定时器、自启动
├── src/                      React 前端
├── adapters/                 每个包管理器一个 TOML + fixtures/（真实输出样本 + 期望结果）
├── catalog/                  精选商店 JSON（entries.json + i18n/en.json、zh-CN.json）
├── docs/
└── .github/workflows/        ci.yml（三平台测试 + 构建）、release.yml（打 tag 出包）
```

数据流：界面 → Tauri IPC → core（读缓存立即返回 → 后台刷新 → 事件推送更新）→ 界面。操作：界面发起 → 入队 → runner 起进程 → 逐行事件推送到界面日志面板 → 完成后刷新该来源缓存。

三个关键决定：
1. **内核不依赖 Tauri**：可用录制的真实输出做单元测试；日后可出 CLI。
2. **适配器是数据不是代码**：加一个包管理器 = 一份几十行 TOML + 一组 fixtures。解析不了的特例由内核里**带名字的原生解析器/策略**兜底（`parser = "native:winget_table"`、`outdated.strategy = "native:ollama_registry"`）。
3. **三平台从第一天由 CI 构建**：仓库第一周即产出 .dmg / .msi+.exe / .deb+.rpm+.AppImage。

## 4. 适配器规范（核心）

### 4.1 清单格式（示例：Homebrew）

```toml
id = "homebrew"
name = "Homebrew"
kind = "system"                 # system | language | model | standalone
platforms = ["macos", "linux"]
homepage = "https://brew.sh"
id_pattern = "^[A-Za-z0-9@._+/-]+$"   # 包名白名单，代入命令前校验，拒绝以 - 开头
elevation = "never"             # never | required | auto
env = { HOMEBREW_NO_AUTO_UPDATE = "1", HOMEBREW_NO_ENV_HINTS = "1", NO_COLOR = "1" }

[detect]
paths = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew", "/home/linuxbrew/.linuxbrew/bin/brew"]
version = ["brew", "--version"]

[list]
command = ["brew", "info", "--installed", "--json=v2"]
parser = "json"
items = "$.formulae[*]"
[list.fields]
id = "$.name"
version = "$.installed[0].version"
description = "$.desc"
homepage = "$.homepage"
explicit = "$.installed[0].installed_on_request"   # false ⇒ 依赖库，标灰
# casks 作为第二个 list 块：items = "$.casks[*]"，install 命令带 --cask

[outdated]
refresh = ["brew", "update"]          # 可选的前置刷新，带 TTL（默认 6 小时）
command = ["brew", "outdated", "--json=v2"]
parser = "json"
items = "$.formulae[*]"
[outdated.fields]
id = "$.name"
version = "$.installed_versions[0]"
latest = "$.current_version"

[install]   command = ["brew", "install", "{id}"]
[uninstall] command = ["brew", "uninstall", "{id}"]
[upgrade]   command = ["brew", "upgrade", "{id}"]
[upgrade_all] command = ["brew", "upgrade"]

[search]
command = ["brew", "search", "--desc", "{query}"]
parser = "lines"
pattern = '^(?P<id>\S+): (?P<description>.*)$'

[info]
command = ["brew", "info", "--json=v2", "{id}"]
parser = "json"
```

### 4.2 通用语义

- 命令一律是**参数数组**，绝不经 shell 拼接；`{id}` `{query}` 只替换单个参数，且先过 `id_pattern`。
- `parser`：`json`（JSONPath 取 items 与字段）、`lines`（每行一个正则，命名分组）、`table`（表头对齐列切分，用于 winget / ollama / snap）、`tsv`、`native:<name>`（内核内置 Rust 函数）。
- `exit_ok = [0, 1]`：允许的退出码（`npm outdated` 有更新时退出码 1，`dnf check-update` 为 100）。
- `elevation`：`never` / `required` / `auto`（探测目标目录可写性）。提权方式见 §6。
- `refresh` 有 TTL，避免每次都 `brew update` / `apt update`。
- 每个适配器目录必须带 `fixtures/`：真实命令输出 + 期望解析结果，CI 跑解析器测试。

### 4.3 各来源的落地方案（评审重点：命令与格式是否属实）

| 来源 | 列出已装 | 查更新 | 装/卸/升 | 搜索 | 依赖标记 | 提权 | 已知难点 |
|---|---|---|---|---|---|---|---|
| Homebrew | `brew info --installed --json=v2` | `brew outdated --json=v2` | brew install/uninstall/upgrade | `brew search --desc` | installed_on_request | 不允许 root | casks 与 formulae 两套字段 |
| npm 全局 | `npm ls -g --depth=0 --json` | `npm outdated -g --json`（退出码 1） | npm install -g / uninstall -g / install -g x@latest | `npm search --json` | 全部视为显式 | Linux 系统 node 需 sudo（auto） | 输出可能夹杂 warn 行 |
| pip | `pip list --format=json` | `pip list --outdated --format=json` | pip install / uninstall -y / install -U | **无**（PyPI 已关闭搜索） | `pip list --not-required` | 无 | PEP 668 外部管理环境会拒装；应引导用 pipx/uv |
| pipx | `pipx list --json` | 无原生命令 → `native:pypi_latest`（查 PyPI JSON API） | pipx install / uninstall / upgrade | 无 | 全显式 | 无 | |
| uv tool | `uv tool list`（文本） | `uv tool list --outdated`（待核实是否存在） | uv tool install / uninstall / upgrade | 无 | 全显式 | 无 | 输出格式无稳定承诺 |
| cargo | `cargo install --list`（文本） | `native:crates_io_latest` | cargo install / uninstall / install --force | 无（v1） | 全显式 | 无 | 编译安装很慢，需明确提示 |
| ollama | 优先本地 API `GET localhost:11434/api/tags`（JSON 含 digest/size）；无服务时 `ollama list` 表格 | `native:ollama_registry`：比对本地 manifest digest 与 registry.ollama.ai v2 manifest | ollama pull / rm / pull | 精选清单 + 任意名字直接 pull | 全显式 | 无 | registry API 非公开承诺 |
| winget | `winget list --disable-interactivity --accept-source-agreements`（文本表格，列宽截断、Unicode 对齐问题）；**备选**：PowerShell 模块 `Microsoft.WinGet.Client` 的 `Get-WinGetPackage \| ConvertTo-Json` | `winget upgrade`（表格）/ `Get-WinGetPackage \| ? IsUpdateAvailable` | winget install/uninstall/upgrade --id x -e | `winget search`（表格）/ `Find-WinGetPackage` | 无 | 机器级安装需 UAC（auto） | UniGetUI 已放弃解析 CLI 改用 COM API；这是 Windows 最大风险 |
| scoop | `scoop export`（JSON） | `scoop status`（表格） | scoop install/uninstall/update | `scoop search` | 无 | 无 | 需经 `powershell -NoProfile -Command` 调用 |
| chocolatey | `choco list --limit-output`（name\|version） | `choco outdated --limit-output` | choco install/uninstall/upgrade -y | `choco search --limit-output` | 无 | **required** | 全程管理员 |
| apt | `dpkg-query -W -f='${Package}\t${Version}\t${binary:Summary}\n'` + `apt-mark showmanual` | `apt-get -s upgrade` 或 `apt list --upgradable`，前置 `apt-get update`（sudo） | apt-get install/remove/--only-upgrade -y | `apt-cache search` | showmanual | required | apt 明言 CLI 不稳定，用 apt-get/apt-cache/dpkg-query |
| dnf | `rpm -qa --queryformat` | `dnf check-update`（退出码 100） | dnf install/remove/upgrade -y | `dnf search` | `dnf repoquery --userinstalled` | required | dnf5（Fedora 41+）输出变化 |
| pacman | `pacman -Q`、`pacman -Qi` | `pacman -Qu`（需先 `-Sy`） | **只允许 `pacman -Syu` 全量升级**，禁止单包升级（部分升级会毁系统） | `pacman -Ss` | `pacman -Qe / -Qd` | required | 单包升级必须禁用 |
| snap | `snap list`（表格） | `snap refresh --list` | snap install/remove/refresh | `snap find` | 无 | required | |
| flatpak | `flatpak list --app --columns=application,name,version,size,origin`（tab 分隔） | `flatpak remote-ls --updates --columns=application,version` | flatpak install/uninstall/update -y | `flatpak search --columns=…` | 无 | 系统级由 polkit 自弹窗 | |

### 4.4 独立安装器（kind = "standalone"）

同一 TOML 格式，但没有"列出"，只有"识别"：

```toml
id = "claude-code"
kind = "standalone"
name = "Claude Code"
platforms = ["macos", "linux", "windows"]
[detect]
paths = ["~/.local/bin/claude", "%USERPROFILE%\\.local\\bin\\claude.exe"]
version = ["claude", "--version"]
version_pattern = '(?P<version>\d+\.\d+\.\d+)'
[latest]  strategy = "native:npm_registry"  package = "@anthropic-ai/claude-code"
[upgrade] command = ["claude", "update"]
[uninstall] remove = ["~/.local/bin/claude", "~/.local/share/claude"]
[i18n]
description.en = "Anthropic's AI coding agent for the terminal"
description.zh-CN = "Anthropic 的终端 AI 编程助手"
```

首批清单：claude、agy（Gemini/Antigravity）、grok、uv、rustup、bun、deno、nvm、fnm、volta、pnpm（独立安装版）、mise、pyenv、ollama 本体、Docker Desktop（仅识别）。

### 4.5 来源不明扫描

扫描目录（可配置）：macOS/Linux `~/.local/bin ~/bin /usr/local/bin ~/.cargo/bin ~/go/bin ~/.bun/bin ~/.deno/bin` + PATH 中位于 `$HOME` 下的目录；Windows `%LOCALAPPDATA%\Programs %APPDATA%\npm %USERPROFILE%\.local\bin` + PATH 中位于用户目录下的项。规则：只看可执行文件；真实路径（解析符号链接）落在已知管理器前缀（Cellar、node_modules、site-packages、~/.cargo…）的归属该管理器；其余列为"来源不明"，展示路径、大小、修改时间、`--version` 输出（可选，默认不执行未知程序）。**不提供删除操作**（v1）。

## 5. 数据模型

- `Source { id, name, kind, platforms, present, version, icon }`
- `Package { source_id, id, name, version, latest?, description?, homepage?, explicit: bool, size?, installed_at?, path?, catalog_id? }`
- `CatalogEntry { id, category, name{en,zh}, summary{en,zh}, why{en,zh}（"你需要它吗"）, homepage, install: { macos: {source, id}, windows: {...}, linux: {...} }, safe_to_remove: yes|careful|no }`
- `Operation { id, kind: install|uninstall|upgrade|upgrade_all|refresh, source_id, package_id?, command_preview, status: queued|running|succeeded|failed|cancelled, started_at, finished_at, exit_code?, log_path }`
- `UnknownBinary { path, size, mtime, guessed_source? }`
- `Settings { language, simple_mode, background_check, check_interval_hours, autostart, close_to_tray, enabled_sources[], ignored_updates[] }`

SQLite 表：sources、packages（缓存，带 fetched_at）、operations、operation_logs（或落盘文件）、settings、ignored_updates、catalog_cache。

## 6. 操作执行与安全

- **命令预览**：任何操作执行前展示将运行的确切命令；卸载与"全部更新"需确认。高级模式可复制命令。
- **队列**：同一来源串行（brew/apt 不允许并发），不同来源并行，最多 3 路。
- **流式日志**：逐行推送到界面；操作历史保留最近 500 条，日志落盘 `~/.canager/logs/`（Windows `%APPDATA%\Canager\logs`）。
- **取消**：杀进程树（Unix 进程组 kill；Windows Job Object）。
- **提权**（永不接触密码）：
  - macOS：v1 不需要（brew/npm/pip/ollama 均为用户级）。
  - Linux：`pkexec <cmd>`（polkit 弹系统密码框，标准输出可正常流式读取）。
  - Windows：`Start-Process -Verb RunAs` 触发 UAC，输出经临时文件回读（无法直接捕获提权进程 stdout）；v1.x 改为常驻提权 helper + 命名管道（UniGetUI 方案）。
- **注入防护**：参数数组 + `id_pattern` 白名单 + 拒绝 `-` 开头。
- **危险操作硬规则**：pacman 禁单包升级；`brew` 不以 root 运行；系统级 Python 包（PEP 668）不强行 `--break-system-packages`，改为提示用 pipx/uv。
- **错误分类**：管理器缺失 / 网络 / 权限 / 解析失败 / 命令失败。解析失败弹出原始输出 + 一键生成 GitHub issue（含适配器 id、命令、脱敏输出）。

## 7. 界面

侧栏：**概览**（已装 N、可更新 M、来源不明 K、占用空间、上次检查时间）｜**更新**｜**已安装**（按来源分组，依赖默认折叠隐藏）｜**发现**（精选商店分类 + 跨来源搜索）｜**来源不明**｜**历史**｜**设置**。右侧详情面板；底部操作条显示进行中的操作与日志抽屉。

- **简单模式（默认）**：只显示名字、白话说明、状态徽章（可更新 / 正常 / 依赖）、一个主按钮。
- **高级模式**：显示版本号、路径、来源、命令预览、参数。
- 视觉：系统字体、跟随系统深浅色、8pt 网格、少边框多留白、macOS 风格侧栏（Windows/Linux 同一套但去掉 mac 特有的交通灯留白）。
- 首次启动：欢迎页 → 选择语言 → 询问是否开启后台检查 → 扫描动画 → 概览。

## 8. 后台与托盘

Tauri 托盘图标；设置里的开关控制"关闭窗口最小化到托盘"、"开机自启"、"每 N 小时检查更新"。检查到更新时托盘角标 + 系统通知（点击打开更新页）。定时器在 Rust 侧（tokio），电脑休眠唤醒后补跑。单实例。

## 9. 国际化

前端 i18next，`locales/en.json`、`locales/zh-CN.json`；Rust 侧极少量字符串（托盘菜单、通知）用同一套 JSON 由前端注入。精选清单与独立安装器 TOML 自带 en / zh-CN 字段。语言跟随系统，可手动改。

## 10. 构建、CI、发布

- `ci.yml`（PR 与 main）：三平台矩阵 `macos-latest`、`windows-latest`、`ubuntu-22.04`：cargo fmt/clippy/test、pnpm lint/test、tauri build（不上传）。
- **集成冒烟**：各 runner 自带包管理器（macOS runner 有 brew，Windows runner 有 choco/winget/scoop 可装，Ubuntu 有 apt）→ 用真实管理器装一个极小的包（如 `hello`）跑一遍 list/outdated/install/uninstall。
- `release.yml`（tag `v*`）：tauri-action 出 macOS universal .dmg、Windows x64 .msi + .exe(NSIS)、Linux x64 .deb/.rpm/.AppImage；草稿 Release + 自动 changelog。
- 不签名期：README 写清 macOS `xattr -dr com.apple.quarantine`/右键打开、Windows SmartScreen "仍要运行"。
- 体积目标：.dmg ≤ 15 MB、.msi ≤ 10 MB、.deb ≤ 10 MB。
- 作者只有 Mac：Windows/Linux 的本地验证靠 UTM/Parallels 虚拟机 + CI runner 冒烟 + 社区测试者。

## 11. 测试策略

1. 适配器解析器：fixtures 驱动（真实输出 → 期望 JSON），每个适配器必须有；CI 强制。
2. 内核：`CommandRunner` trait + mock，测队列/取消/超时/错误分类。
3. 集成冒烟：见 §10。
4. 前端：vitest + Testing Library 覆盖关键组件；e2e（WebDriver）后期。
5. 发布前手工清单：三平台各跑一遍"首次启动 → 概览 → 更新一个包 → 卸载一个包 → 商店装一个 → 来源不明列表"。

## 12. 仓库与开源运营

- MIT；英文 README（首屏 GIF、"10 MB vs 50 MB"对比、三平台下载按钮、与 UniGetUI 的差异表）+ README.zh-CN。
- CONTRIBUTING：`docs/adapters.md`"30 行 TOML 加一个包管理器"教程；PR 模板要求 fixtures。
- Issue 模板：解析失败（应用内一键生成）、新适配器请求、新精选条目。
- 发布渠道：GitHub Releases → 随后 brew tap cask、winget manifest、AUR、Flathub（后期）。
- 首发宣传：Hacker News（Show HN）、r/macapps、r/linux、r/Windows、V2EX、少数派、即刻/小红书。

## 13. 实施阶段（供制定计划）

0. 仓库骨架 + 三平台 CI 出空壳安装包（第 1 周即验证"同日首发"可行）
1. core：适配器引擎 + brew/npm/pip 三个适配器 + fixtures 测试
2. 界面骨架：已安装 / 更新 / 操作队列与日志
3. 其余适配器：winget、scoop、choco、apt、dnf、pacman、snap、flatpak、pipx、uv、cargo、ollama
4. 独立安装器清单 + 来源不明扫描
5. 发现页：精选清单 + 跨来源搜索
6. 托盘 / 后台检查 / 通知 / 自启动 / 首次启动向导
7. i18n 校对、README、截图 GIF、v0.1.0 发布

## 14. 已知风险

| 风险 | 影响 | 对策 |
|---|---|---|
| winget 文本解析脆弱 | Windows 核心体验 | 优先 PowerShell 模块 JSON；表格解析兜底；fixtures 覆盖多语言系统 |
| 作者无 Windows/Linux 机器 | 三平台同发质量 | VM + CI 冒烟 + 早期招募测试者 |
| WebKitGTK 渲染差异 | Linux 观感 | 避免高级 CSS；CI 截图比对（后期） |
| 未签名 | 普通人首启失败 | README 图文教程；尽早补签名 |
| ollama registry API 非公开 | 模型更新检测失效 | 失效时退化为"手动重新拉取" |
| Arch 部分升级 | 毁用户系统 | 硬禁单包升级 |
| PEP 668 | pip 装不上 | 引导 pipx/uv |
| 范围过大（15 个适配器 + 3 平台） | 拖延、烂尾 | 阶段 0 先证明流水线；适配器按 fixtures 驱动并行由 AI 批量生成 |
