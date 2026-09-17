# Canager 设计文档（spec v0.2，2026-09-17，作者已通过）

> v0.1 经 Gemini / Grok / Codex / Claude 六视角评审后重写。改动依据见同目录 `REVIEW-合并报告.md`。本版待作者审阅通过后进入实施计划。

## 0. 已定决定

| 项 | 决定 | 备注 |
|---|---|---|
| 作者 | 非专业程序员，只有 Apple Silicon Mac，主要靠 AI agent 写代码，首个开源项目 | 已注册 Apple Developer |
| 目标 | GitHub star；**面向不写代码的小白** | 作者坚持，评审意见已知悉 |
| 平台 | **v1 只发 macOS**（universal），Windows / Linux 视进度后续做；架构与代码保持三平台可移植 | 2026-09-17 改 |
| 签名 | **从第一个构建起 Developer ID 签名 + 公证**；Tauri updater 自签密钥同时启用 | 2026-09-17 改 |
| v1 来源 | Homebrew（formulae + casks）、npm 全局、pipx、uv tool、pip（只读）、cargo、ollama 模型、已知独立安装器、来源不明扫描（只读） | Windows/Linux 来源进 roadmap |
| 功能 | 已装管理（列出 / 查更新 / 更新 / 卸载）+ 精选商店 + 搜索（brew、npm 有原生搜索；ollama 为精选清单 + 按名拉取；pipx / uv / pip / cargo 无搜索） | |
| 分发 | GitHub Releases（.dmg + updater 清单）+ 自建 brew tap；知名度达标后提交官方 homebrew-cask | |
| 语言 | 英文默认、内置简体中文，跟随系统 | |
| 后台 | 菜单栏常驻可选，首次进入"更新"页时询问；macOS 上所有后台检查都不需要密码 | |
| 名字 | Canager | 保留 |

## 1. 定位

> **Canager：把 Mac 上用命令行装的东西管起来。** 你跟着教程装了 Homebrew、Ollama、Claude Code、一堆 npm 和 pip 工具，然后就忘了它们。Canager 让你看见它们、知道它们是什么、一键更新、放心删除；什么都没装的 Mac 也能从这里一键装好第一批工具。

面向小白意味着四条产品硬规则：

1. **双击就能开**：签名 + 公证，没有任何"无法验证开发者"弹窗。
2. **空机器有正门**：没装任何来源时，首页是精选商店而不是空列表。
3. **每一步说人话**：名字旁边有一句白话说明；依赖库默认折叠；删除前告诉你会影响什么。
4. **绝不问密码做后台的事**：后台检查只读；需要密码的操作只在用户主动点击时发生，并且先看到将执行的命令。

与竞品的诚实差异：

| | UniGetUI（macOS 版） | Applite | mxcl PMM | Canager |
|---|---|---|---|---|
| 定位 | Windows-first 的跨平台移植，功能密集 | 只管 brew cask | 开发者的清单与占用盘点 | 小白的工具管家 |
| ollama 模型 | 无 | 无 | 有（盘点） | 有（列出 / 查更新 / 拉取 / 删除） |
| AI CLI 与脚本安装器 | 当普通 npm 包 | 无 | 部分（mise/pkgx 等） | 有（多来源识别） |
| 空机器引导 | 无 | 无 | 无 | 有 |
| 白话说明 / 删除影响 | 无 | 无 | 无 | 有 |

README 不再用"10 MB vs 50 MB"做首屏；体积只在下载页按事实写，并注明依赖系统 WebView。

## 2. 技术选型

- **Tauri 2 ≥ 2.11.1**（CVE-2026-42184 修复版起），系统 WebKit。最低 macOS **13.3**（Tailwind v4 需要 Safari 16.4）。
- 前端：React 19 + TypeScript + Vite + Tailwind v4 + Radix Primitives + TanStack Query + `@tanstack/react-virtual`（长列表）+ i18next。
- Rust：tokio（使用 Tauri 提供的运行时，core 不自建）、serde / serde_json / toml、regex、rusqlite（bundled）、reqwest（rustls，仅 crates.io / PyPI / ollama registry 三处）、tracing、`fix-path-env-rs`。
- Tauri 插件：tray、notification、autostart、single-instance、updater、log、opener。
- 包管理 pnpm；CI GitHub Actions + tauri-action。

## 3. 架构

```
canager/
├── crates/canager-core/        纯 Rust 库，不依赖 Tauri
│   ├── model.rs                ManagerInstance / InstalledArtifact / UpdateCandidate / Operation / …
│   ├── adapters/               每个来源一个 Rust 模块，实现 Adapter trait
│   │   ├── brew.rs npm.rs pipx.rs uv.rs pip.rs cargo.rs ollama.rs
│   │   ├── standalone/         独立安装器：每个工具一个描述（元数据 TOML + Rust 配方）
│   │   └── unknown.rs          来源不明扫描（只读）
│   ├── runner/                 子进程执行、PATH 注水、超时、进程组、流式输出
│   ├── ops/                    操作队列、资源锁、状态机、结果核对
│   ├── catalog/                精选清单加载与匹配
│   ├── store/                  SQLite：实例与制品缓存、操作历史、设置、忽略列表
│   └── events.rs               EventSink trait（core 向外推事件的唯一出口）
├── src-tauri/                  Tauri 壳：IPC 命令、Channel 日志、菜单栏、通知、定时器、自启动、updater
├── src/                        React 前端
├── adapters/meta/*.toml        适配器元数据（id、名称、平台、主页、i18n、fixture 采集版本），编译时 include_str! 进 core
├── adapters/fixtures/<id>/     真机录制的命令输出 + 期望解析结果
├── catalog/                    精选清单 JSON + i18n
└── .github/workflows/          ci.yml、release.yml、canary.yml（Windows/Linux 每周编译健康检查）
```

三条边界约定（评审补的）：

1. **事件出口**：core 通过 `EventSink` trait 推送 `OperationEvent`（状态变化、日志块、进度）；Tauri 壳实现它并转成 **Channel**（不是全局 emit），按 50 ms 时间片合并日志块。
2. **运行时归属**：core 只写 async 函数，不创建 tokio Runtime；由 Tauri 的 `async_runtime` 驱动；SQLite 调用经 `spawn_blocking`，`Connection` 由单个专用线程持有（`Send + !Sync` 约束）。
3. **路径注入**：core 接收 `Paths { data_dir, log_dir, cache_dir }`；壳用 Tauri path API 填 `~/Library/Application Support/Canager/`、`~/Library/Logs/Canager/`。

数据流：界面读缓存立即渲染 → core 后台刷新（带代际号，单事务写入）→ Channel 通知 → 界面按代际号接受最新快照；刷新失败保留旧数据并标"可能过期"。

## 4. 适配器规范

### 4.1 原则（v0.1 的"适配器是数据不是代码"作废）

适配器是**有类型的 Rust 实现**；TOML 只承载元数据。每个适配器实现：

```rust
trait Adapter {
    fn meta(&self) -> &AdapterMeta;                       // 来自 adapters/meta/<id>.toml
    fn capabilities(&self) -> Capabilities;              // search / per_item_upgrade / upgrade_all / uninstall / background_check / cancel_safe
    async fn detect(&self, env: &Env) -> Vec<ManagerInstance>;
    async fn inventory(&self, inst: &ManagerInstance) -> Result<Vec<InstalledArtifact>>;
    async fn check_updates(&self, inst: &ManagerInstance) -> Result<Vec<UpdateCandidate>>;
    async fn search(&self, inst: &ManagerInstance, q: &Query) -> Result<Vec<SearchHit>>;
    fn plan(&self, inst: &ManagerInstance, op: &OpRequest) -> Result<Plan>;   // 命令预览、警告、取消策略、是否需要密码、锁
    async fn execute(&self, plan: &Plan, sink: &dyn EventSink, cancel: CancelToken) -> Result<Outcome>;
    async fn reconcile(&self, inst: &ManagerInstance, target: &ArtifactKey) -> Result<Reconciled>; // 执行后核对实际状态
}
```

通用约定：

- 命令一律参数数组 + 探测到的**绝对路径**，绝不经 shell；包名、搜索词、版本各有独立校验规则；拒绝 `-` 开头。
- 启动时用 `fix-path-env-rs` 恢复登录 shell 的 PATH（本机验证：Finder 启动的进程 PATH 为空默认值）。
- 每个命令有超时（探测 30 s、查更新 120 s、安装/升级 30 min）；stdout/stderr 独立并发读取；处理 `\r` 进度行；输出按 UTF-8 解码、失败时 lossy。
- 子进程用 `process_group(0)` 启动，取消时向进程组发信号；取消后一律 `reconcile`。
- 每个适配器目录带 fixtures：**只收真机录制**（作者 Mac、CI macOS runner），文件名含来源版本；禁止 AI 生成 fixture。
- 元数据 TOML 记录 `schema_version`、已验证的来源版本范围；检测到超出范围时界面标"未验证版本"。

### 4.2 各来源落地（macOS v1，命令均已核实或标注待验证）

**Homebrew**（`brew.rs`）
- 实例：`/opt/homebrew/bin/brew`、`/usr/local/bin/brew` 各为一个实例（Intel 迁移用户可能两者并存）。拒绝以 root 运行。
- 环境：`HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ENV_HINTS=1 HOMEBREW_NO_INSTALL_CLEANUP=1 NO_COLOR=1`。
- 列出：`brew info --installed --json=v2`。formulae：`name`、版本取 `installed` 数组中 `linked_keg` 对应项（数组按时间升序，`[0]` 是最老版本）、`installed_on_request` / `installed_as_dependency` → 安装原因、`desc`、`homepage`。casks：`token`、`installed`（字符串）、`auto_updates`、`desc`、`name[0]`；cask 无安装原因字段，一律视为用户安装。
- 查更新：先按 TTL（6 小时）`brew update`，再 `brew outdated --json=v2`（顶层 `formulae` / `casks`）。设置项"包含自更新的应用"对应 `--greedy`，默认关，界面解释"Chrome 这类应用自己更新，勾选后也会列出"。
- 安装：`brew install --formula {id}` / `brew install --cask {token}`。卸载：`brew uninstall --formula|--cask {id}`，**永不**加 `--ignore-dependencies`；卸载前用 `brew uses --installed {id}` 列出会受影响的包，有则默认阻止。升级：`brew upgrade --formula|--cask {id}`；"全部更新"= 对界面上勾选的每一项分别执行，不跑裸 `brew upgrade`。
- 搜索：`brew search {query}` + `brew search --desc {query}` 两次；按 `==> Formulae` / `==> Casks` 分节解析，条目格式 `name: desc`。
- 需要管理员密码的 cask（含 `pkg` 或 `sudo` 的安装）：Homebrew 官方支持 `SUDO_ASKPASS`（manpage："If set, pass the -A option when calling sudo"，Homebrew 7.0.3 已核对），指向应用内置的原生密码对话框 helper；**阶段 1 spike** 只需验证 helper 在 GUI 无 TTY 下的行为，不成立则此类 cask 标"需在终端安装"并提供一键打开终端。
- 缓存：`brew info --installed` 在包多时耗时数秒，只在刷新时跑；界面先读 SQLite。

**npm 全局**（`npm.rs`）
- 实例：每个 `npm` 可执行文件（brew node、nvm、fnm、volta）一个实例，`npm prefix -g` 为实例键；prefix 不可写则只读。
- 环境：`NO_COLOR=1 npm_config_update_notifier=false npm_config_fund=false`。
- 列出：`npm ls -g --depth=0 --json`（退出码 0 或 1 都接受，只解析 stdout；顶层 `dependencies` 对象）。描述懒加载：`npm view {id} description --json`。
- 查更新：`npm outdated -g --json`（有更新退出码 1；对象按包名索引，含 current / wanted / latest）；stderr 不解析。
- 装 / 卸 / 升：`npm install -g {id}` / `npm uninstall -g {id}` / `npm install -g {id}@latest`（跨大版本时预览里标出）。
- 搜索：`npm search --json --searchlimit 20 {query}`。

**pipx**（`pipx.rs`）
- 列出：`pipx list --json`（`venvs.<name>.metadata.main_package.package_version`）。
- 查更新：`pipx list --outdated`（pipx ≥ 1.16，启动时按 `pipx --version` 判断；低版本回退为对比 PyPI JSON API）。
- 装 / 卸 / 升：`pipx install {id}` / `pipx uninstall {id}` / `pipx upgrade {id}`；全部升级 `pipx upgrade-all`。无搜索。

**uv tool**（`uv.rs`）
- 列出：`uv tool list --show-paths`（文本：`name vX.Y.Z (path)` + `- bin` 行）。
- 查更新：`uv tool list --outdated`（已核实存在）。
- 装 / 卸 / 升：`uv tool install/uninstall/upgrade {id}`；`uv tool upgrade --all`。无搜索。

**pip（只读 + 引导）**（`pip.rs`）
- 实例：每个解释器一个（`/usr/bin/python3`、brew `python3.x`、pyenv、官方安装器），用 `{python} -m pip`。
- 列出：`-m pip list --format=json`；安装原因用 `--not-required` 标"没有其它包依赖它"（明确不等于"用户装的"）。查更新：`-m pip list --outdated --format=json`。
- **不提供装 / 卸 / 升**；条目旁提示"Python 工具请用 pipx 或 uv 安装"。

**cargo**（`cargo.rs`）
- 列出：读 `~/.cargo/.crates2.json`（含名称、版本、来源、二进制、features；机器可读），`cargo install --list` 仅作校验。
- 查更新：仅对来源为 crates.io 的包，`GET https://crates.io/api/v1/crates/{name}`（须带 User-Agent）取 `max_stable_version`；git / path 来源标"不可检查"。
- 升级：`cargo install {id}`（若装了 cargo-binstall 优先 `cargo binstall -y {id}`），执行前确认"将在本机编译，可能需要几分钟并占满 CPU"。卸载：`cargo uninstall {id}`。无搜索。

**ollama 模型**（`ollama.rs`）
- 实例：`OLLAMA_HOST`（默认 `http://127.0.0.1:11434`）。服务未运行时**不调用** `ollama list`（它同样依赖服务并会拉起 Ollama.app），界面显示"Ollama 未运行"和"启动"按钮（`open -a Ollama`）。
- 列出：`GET /api/tags`（`models[].name/digest/size/modified_at/details`）。
- 查更新：读本地 manifest `~/.ollama/models/manifests/{host}/{ns}/{name}/{tag}`，与 `GET https://registry.ollama.ai/v2/{ns}/{name}/manifests/{tag}` 比对 **layers 的 digest 集合**（本机验证匿名 GET 返回 200；不比整文件以免重序列化误报）。失败时退化为显示"上次拉取时间 + 重新拉取"。
- 拉取：`POST /api/pull`（NDJSON 流，按字节显示进度）；删除：`DELETE /api/delete`。搜索 = 精选清单 + 任意名称拉取；预览里显示预计大小与剩余磁盘。

**独立安装器**（`standalone/`）
每个工具一份元数据 TOML + Rust 配方（detect / version / latest / upgrade / uninstall）。首批：claude、agy（Antigravity CLI，与 Gemini CLI 分开）、grok、uv 本体、rustup、bun、deno、mise、pnpm 独立版、ollama 本体。规则：
- 同一工具可能同时来自 brew / npm / 官方脚本，每个来源都建实例，界面显示"检测到 2 个 claude"，升级按各自来源走（如 claude：native → `claude update`；brew → `brew upgrade --cask claude-code`；npm → `npm install -g @anthropic-ai/claude-code@latest`）。
- 版本用探测到的绝对路径执行，正则限定到版本行（本机 `claude --version` = `2.1.273 (Claude Code)`）。
- 卸载只走官方方法；没有官方卸载命令的（如 claude native），展示官方文档中的删除清单并让用户确认逐项删除，仅删归属明确的路径，保留配置目录另作选项。
- nvm 是 shell 函数，不按可执行文件识别；Docker Desktop 不在清单。

**来源不明扫描**（`unknown.rs`，只读）
扫描 `~/.local/bin ~/bin /usr/local/bin ~/.cargo/bin ~/go/bin ~/.bun/bin ~/.deno/bin` 与 PATH 中位于 `$HOME` 下的目录（深度 1，最多 2000 个文件，10 s 预算）；解析符号链接后落在已知实例前缀内的归属该实例；其余列为"来源不明"：路径、大小、修改时间。不执行、不删除。

### 4.3 Roadmap 来源（不在 v1，但接口已预留）

Windows：winget（走 COM API，不解析 CLI 表格；PowerShell 模块不预装）、scoop（调 `scoop.cmd` + `ConvertTo-Json`）、choco（查询不提权、写操作经提权 helper、退出码 2 = 有过时包）。Linux：apt（`dpkg-query` + `apt-mark showmanual`；后台只读缓存）、dnf5（`check-upgrade --json`）、pacman（`checkupdates`，禁止 `-Sy`，安装用 `-Syu {id}`）、flatpak（`--columns` tab 输出）、snap。提权：Linux 自带 polkit `.policy` + 检测认证代理；Windows 最小提权 helper + 命名管道。评审报告第 2 节有完整依据。

## 5. 数据模型

- `ManagerInstance { id, adapter_id, exe_path, prefix, scope: User|System, version, healthy: bool }`
- `InstalledArtifact { instance_id, key: ArtifactKey, name, version, kind: Formula|Cask|Package|Tool|Model|Binary, reason: Requested|Dependency|Unknown, description?, homepage?, size?, installed_at?, path?, catalog_id? }`
- `UpdateCandidate { artifact: ArtifactKey, current, target, channel: Native|Registry|Digest, checkable: bool, warnings: [] }`
- `SearchHit { adapter_id, key, name, description?, kind }`
- `Plan { op, instance_id, argv_preview, needs_password: bool, locks: [ResourceLock], cancel_policy: SafeKill|KillThenReconcile|NoCancel, warnings: [], estimated: { download?, duration? } }`
- `Operation { id, plan, status, started_at, finished_at, outcome?: Succeeded|NoChange|PartialSuccess|NeedsAttention|Failed|Unconfirmed, log_path }`
  - 状态机：`Queued → Running → (CancelRequested → Cancelling) → Verifying → Done`
- `CatalogEntry { id, category, name{en,zh}, summary{en,zh}, why{en,zh}, homepage, install: [{ platform, adapter_id, key, note? }], prerequisites: [catalog_id] }`（无 `safe_to_remove` 字段）
- `Settings { language, show_technical_details, background_check, check_interval_hours, launch_at_login, close_to_menubar, greedy_casks, enabled_adapters[], ignored_updates[] }`
- SQLite 表：instances、artifacts（带 generation）、update_candidates、operations、settings、ignored_updates、catalog_cache。

## 6. 操作执行与安全

- **预览**：执行前展示 argv、来源、影响（卸载列出反向依赖；升级列出版本跳变；cargo 编译提示；ollama 大小），需要密码的操作单独标注。
- **队列与锁**：并发单位是资源锁而非来源：`brew:/opt/homebrew`、`npm:<prefix>`、`cargo:~/.cargo`、`ollama:<host>`、`pipx`、`uv`；同锁串行、异锁并行（最多 3）。后台刷新也走队列。检测到外部锁（`brew.lock`）时等待并提示"另一个 brew 正在运行"。
- **取消**：按 `cancel_policy`；取消后进入 `Verifying`，重新查询实例状态；无法确认时显示"结果未确认"，绝不伪造成功或失败。
- **超时**：见 §4.1；超时视同取消并核对。
- **密码**：应用本身不提权；仅 brew cask 需要 sudo 的场景通过 `SUDO_ASKPASS` helper 弹原生对话框（Homebrew 官方支持，helper 行为待 spike），密码不落盘、不进日志。
- **注入与信任边界**：IPC 只接受已知操作 + 对象 ID，Rust 侧重新解析校验；没有通用 exec；精选清单只含 `adapter_id + key`，不含命令；适配器全部编译进应用；外部描述文本按纯文本渲染；CSP 禁止远程脚本与导航。
- **日志与隐私**：日志按操作落盘、按 20 MB 轮转；"报告问题"先生成本地可预览的报告，自动把用户名替换为 `~`、去除疑似 token（`[A-Za-z0-9_-]{32,}`）与环境变量，用户确认后才打开 GitHub issue 页面。
- **错误分类**：来源缺失 / 网络 / 权限 / 外部锁 / 解析失败 / 命令失败 / 超时。解析失败保留原始输出供报告。

## 7. 界面

一套界面 + 设置里的"显示技术细节"开关（默认关；开后显示版本号、路径、argv）。

侧栏：**概览**｜**更新**｜**已安装**｜**发现**（精选商店 + 搜索）｜**来源不明**｜**历史**｜**设置**。右侧详情面板；底部操作条 + 日志抽屉。

- **空机器首启**：检测不到任何实例时，首页直接是"发现"，顶部一句话"你的 Mac 上还没有命令行工具，从这里开始"，精选分类：**基础（Homebrew）**、**AI 助手（Claude Code、Codex、Gemini/Antigravity、Grok）**、**本地模型（Ollama + 推荐模型）**、**常用工具**。Homebrew 本身的安装需要 Xcode 命令行工具与管理员密码：v1 提供"打开终端并粘贴官方命令"的一键按钮，并轮询检测完成（阶段 5 spike 若能在应用内跑通则升级为一键）。
- **已安装**：按来源分组；依赖库默认折叠成"N 个被其它软件带来的组件"；每行 = 图标 + 名字 + 一句说明 + 状态徽章 + 主按钮。说明来源优先级：精选清单手写 → 来源自带描述（brew desc、npm/PyPI description）→ 无。
- **卸载对话框**：显示"删除后这些会受影响：…"（brew uses / 依赖关系），有影响时默认禁用确认。
- **首启流程**：欢迎 → 扫描（语言跟随系统，不问）→ 概览；后台检查的询问放在第一次看到"更新"页时。
- 视觉：系统字体、跟随深浅色、8 pt 网格、macOS 侧栏风格；长列表虚拟滚动。

## 8. 后台与菜单栏

菜单栏图标（Tauri tray）显示可更新数；设置控制"关闭窗口保留在菜单栏"、"登录时启动"、"每 N 小时检查"。后台检查只执行不需要密码的只读命令（`brew update` + outdated、npm/pipx/uv outdated、crates.io、ollama registry），失败退避（1 h → 6 h），休眠唤醒后补跑，离线跳过。有更新时系统通知，点击打开"更新"页。单实例。

## 9. 国际化

`locales/en.json`、`locales/zh-CN.json`；前端 i18next；Rust 侧（菜单栏、通知、updater 提示）用 `rust-i18n` 直接读同一份 JSON（构建时 include），不依赖前端曾经启动。精选清单与独立安装器元数据自带 en / zh-CN。

## 10. 构建、CI、发布

- **ci.yml**（PR / main，macOS runner `macos-latest`，arm64）：cargo fmt / clippy / test、pnpm lint / test、`tauri build --target universal-apple-darwin`（不签名）。集成冒烟：runner 自带 brew、node、python，录制/校验 fixtures 并跑一次 `brew install hello → inventory → uninstall`。
- **canary.yml**（每周）：Windows / Linux 编译健康检查，不阻塞，不发布。
- **release.yml**（tag `v*`）：universal .dmg + `.app.tar.gz` + updater `latest.json`；Developer ID 签名、`notarytool` 公证、装订；updater 用 minisign 密钥（私钥存 GitHub Secrets，备份到钥匙串）。Intel 产物在 `macos-*-intel` runner 上冒烟启动。
- 分发：GitHub Releases；`brew tap brulek/tap` 提供 cask；官方 homebrew-cask 达知名度门槛后提交。
- 私有仓库阶段 CI 分钟数：macOS 10 倍计费，PR 只跑测试，构建仅 main 与 tag。
- 体积、冷启动、空闲内存：阶段 0 实测后写进 README，不预设数字。

## 11. 测试

1. 适配器解析：fixtures（真机录制，带来源版本）→ 期望 JSON，CI 强制；每个适配器 PR 必须带 fixtures。
2. core：`CommandRunner` trait + mock，覆盖队列、锁、取消、超时、状态机、代际写入。
3. 集成冒烟：见 §10。
4. 前端：vitest + Testing Library 覆盖列表、详情、卸载对话框、空机器首页。
5. 发布前手工清单（作者 Mac + 一台干净 macOS 虚拟机 arm64）：首次启动（空机器 / 满机器）→ 更新一个 formula、一个 cask、一个 npm 包、一个 ollama 模型 → 卸载各一个 → 商店装一个 → 取消一个进行中的操作 → updater 升级自身。
6. 首发验收标准（可判定）：不会对错误实例执行操作；检查失败不显示"全部最新"；取消后不谎称已停止；"全部更新"不绕过忽略列表；每个来源都有非空升级路径的证据。

## 12. 仓库与开源运营

- MIT；README（英文，附 zh-CN）：一句话 + 首屏 GIF（空机器 → 装好 Claude Code 与一个模型 → 更新页）+ 下载按钮 + 支持来源表 + 与 UniGetUI / Applite / PMM 的诚实对比。
- 信任文件：LICENSE、SECURITY.md、`docs/what-we-run.md`（每个操作到底执行什么命令）、Release 校验和、"本项目大量使用 AI 辅助开发"声明。
- CONTRIBUTING：`docs/adapters.md`——"用 Rust 加一个来源"教程（trait、fixtures、元数据）；PR 模板要求 fixtures；issue 模板：解析失败（应用内生成）、新来源、新精选条目。
- 首发节奏：先 r/macapps、V2EX、少数派、小红书发 macOS beta，收一轮反馈修完再考虑 Show HN。

## 13. 实施阶段

0. 仓库骨架 + 签名公证流水线：第 1 周产出一个能双击打开的空壳 .dmg（验证证书、公证、updater 链路）。
1. core 基础 + Homebrew 适配器（formulae + casks）+ fixtures + PATH 注水 + askpass spike。
2. 界面骨架：已安装 / 更新 / 操作队列与日志 / 设置。
3. npm、pipx、uv、pip（只读）、cargo、ollama。
4. 独立安装器 + 来源不明扫描。
5. 发现页（精选清单 ≥ 30 条 + 搜索）+ 空机器首启 + Homebrew 安装引导 spike。
6. 菜单栏 / 后台检查 / 通知 / 登录启动 / updater 接入。
7. i18n 校对、README、截图、信任文件、v0.1.0 发布。
8. （roadmap）Windows：winget COM + scoop；Linux：apt/dnf/flatpak + polkit policy。

## 14. 已知风险

| 风险 | 影响 | 对策 |
|---|---|---|
| cask 需要 sudo 的安装在 GUI 下失败 | 部分 cask 装不了 | `SUDO_ASKPASS` 已确认受 Homebrew 支持；阶段 1 spike 验证 helper；不成立则引导终端 |
| Homebrew 本身无法在应用内安装（需 CLT + 密码） | 空机器正门打折 | 一键打开终端粘贴官方命令 + 轮询检测 |
| ollama registry 非公开承诺 | 模型更新检测失效 | 退化为"上次拉取 + 重新拉取" |
| 精选清单是内容工程 | 30 条 × 三段 × 两语言 | 首发 30 条，社区 PR 扩充 |
| 作者不能测 Intel Mac | universal 包 x64 半边未验证 | CI intel runner 冒烟；README 征集 Intel 测试者 |
| 系统 WebKit 版本差异 | 老 macOS 样式错版 | 最低 macOS 13.3；CI 用最低版本 runner 截图（后期） |
| 范围膨胀到 Windows/Linux | 拖延 | roadmap 明确"macOS v0.1 发布后再评估" |
