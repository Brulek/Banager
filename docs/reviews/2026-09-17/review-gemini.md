这份设计文档展示了作者在技术选型和架构分层上的良好直觉（如 Core 与 Tauri 解耦、TOML 适配器驱动），但从**工程落地、系统底层集成、产品定位**三个维度来看，它存在多处会导致项目在第一周就无法推进、或发布后遭遇灾难性差评的致命缺陷。

以下是严格、具体的架构评审意见。

---

## 1. 致命问题（会导致项目失败或必须推倒重来的）

### 1.1 「普通人定位」与「未签名全平台分发」存在不可调和的逻辑自杀
- **涉及章节**：0. 已定需求、1. 定位、10. 构建与发布
- **依据**：
  - 现代操作系统对未签名 GUI 应用的拦截极其严厉：
    - macOS Sequoia 对未签名/未公证应用默认移除“右键打开”快捷通道，用户必须在“系统设置 - 隐私与安全”深处手动授权，或在终端敲 `xattr -dr com.apple.quarantine`。
    - Windows SmartScreen 会弹出亮蓝色大弹窗拦截（“Windows 已保护你的电脑”），仅有不起眼的“更多信息”。
  - **真正的“普通人”看到这种弹窗，第一反应是“病毒/木马/损坏”，100% 放弃使用。** 让他们打开 Terminal 输命令来使用一个“为了避免用 Terminal 而生的 GUI 软件”，属于自相矛盾的逻辑死循环。只有硬核极客愿意绕过 Gatekeeper，但硬核极客一打开发现界面是“白话模式、高级功能折叠、不能看详细依赖树”，会立即卸载。
- **建议改法**：
  1. 放弃“普通人”的伪定位，将 v1 目标受众精准修正为：**“需要跨机器管理多套环境的初中级开发者与极客”**。
  2. 若坚持面向广大小白，**在拿到 Apple Developer Program ($99/年) 和 Windows 代码签名证书（或通过 MS Store 分发）前，绝不能对外宣发**。在首发期，至少利用 GitHub Releases 结合 Homebrew Cask、Winget 社区仓库提交清单（借助社区信任背书安装）。

---

### 1.2 Arch Linux 下 `pacman -Qu` 前置 `-Sy` 是毁灭用户系统的“部分升级”操作
- **涉及章节**：4.3 各来源落地方案（pacman 行）
- **依据**：
  - 文档在 4.3 表格中写道：`pacman -Qu（需先 -Sy）`。这是 Arch Linux 官方 Wiki 明确警告、社区人人喊打的禁忌操作——**Partial Upgrade（部分升级）**。
  - 当你在没有提权或者提权执行 `pacman -Sy` 更新了本地同步数据库，然后只查了更新却没有立即运行 `pacman -Su` 进行系统级完整升级，本地的软件包数据库就会与当前系统已安装的共享库（如 `glibc`, `openssl`, `icu`）版本脱节。此时如果用户用系统包管理器安装了任何新软件，或者其他后台工具被触发，系统核心动态链接库将被静默破坏，导致桌面环境崩溃、Terminal 无法打开、甚至系统无法引导启动。
  - Arch 社区著名工具 `checkupdates`（`pacman-contrib` 内置）就是为了解决此问题而生：它通过在 `/tmp` 下建立只读的临时隔离数据库来拉取更新，绝不触碰系统级 `/var/lib/pacman/`。
- **建议改法**：
  - 严禁在主数据库上执行 `pacman -Sy`。
  - 检查更新策略改为：
    1. 若系统存在 `checkupdates` 命令，直接调用 `checkupdates`（返回状态码 0 为有更新，2 为无更新）。
    2. 若无，使用命令：`fakeroot pacman -Sy --dbpath /tmp/canager-checkup-db && pacman -Qu --dbpath /tmp/canager-checkup-db`。

---

### 1.3 macOS GUI 应用继承的空泛 PATH 导致所有探测与子进程全面瘫痪
- **涉及章节**：4.1 Homebrew 适配器、4.4 独立安装器、4.5 来源不明扫描
- **依据**：
  - 在 macOS 下，通过 Finder、Spotlight 或 Dock 启动的 GUI 桌面应用（即 Tauri 包装后的 App bundle），**其父进程是 `launchd`，不会加载用户的交互式 shell 配置文件**（`~/.zshrc`, `~/.bash_profile`, `~/.zprofile`）。
  - 这意味着 Tauri 后端在运行时，其 `std::env::var("PATH")` 仅包含极简的默认路径：`/usr/bin:/bin:/usr/sbin:/sbin`。
  - 结果是：
    - Apple Silicon 机器上的 Homebrew 在 `/opt/homebrew/bin`，GUI 读取不到；
    - `cargo`、`uv`、`bun`、`nvm`、`fnm`、`pipx` 安装的工具在 `~/.cargo/bin`、`~/.local/bin`，全军覆没；
    - 执行 `version = ["brew", "--version"]` 会直接抛出 `No such file or directory` (ENOENT)。
- **建议改法**：
  - 架构必须在 `banager-core` 初始化最顶层加入**环境变量注水（Env Hydration）机制**。
  - 在 Unix 平台启动时，探测用户的 `$SHELL`，先在后台非交互执行一次 `$SHELL -l -c 'printenv PATH'`（或使用 Rust 的 `shell-words` / 类似 `fix-path-env` 机制），将解析出的真实用户 PATH 更新注入到当前 Tauri 进程的全局上下文中，否则后续所有适配器一律失效。

---

### 1.4 Windows 下利用 `Start-Process -Verb RunAs` 结合临时文件回读极其脆弱且无法交互
- **涉及章节**：6. 操作执行与安全（提权）
- **依据**：
  - 在 Windows 下，以非提权身份运行的 Tauri 进程若使用 `Start-Process powershell -Verb RunAs -ArgumentList "... > temp.log"`：
    1. **权限与文件锁**：提权（Administrator）进程写入的文件，低权限应用读取时可能产生共享冲突；
    2. **杀进程失效**：Tauri 无法将 RunAs 提权子进程加入当前低权限进程的 Windows Job Object，文档所称的“取消：杀进程树”在提权命令下完全失效，子进程脱管；
    3. **输出流丢失**：提权安装器弹出交互式窗口、网络中断卡死或崩溃时，由于没有真正的 IPC 管道，前端日志窗口一片死寂，用户无法判断进度；
    4. **杀毒软件报毒**：高频创建临时的 PowerShell RunAs 脚本重定向执行，会立即触发 Windows Defender 和各类杀毒软件的启发式报警（Heuristic Detection / Behavior Blocker），首发就会被冠以“木马程序”上报。
- **建议改法**：
  - 放弃自己拼装 PowerShell RunAs 临时脚本。
  - v1 阶段，Windows 提权命令统一直接调用带 UAC manifest 触发的标准外部提权执行器（例如借助 `gsudo` 的集成模式，或自编一个极简的带 `requireAdministrator` manifest 的 helper exe），通过标准命名管道（Named Pipe）实现带鉴权的跨进程 stdout/stderr 双向流动与取消控制。

---

## 2. 重大问题（明显影响质量、进度或 GitHub Star 的）

### 2.1 Winget CLI 国际化与控制台宽度导致字符截断与正则报废
- **涉及章节**：4.3 各来源落地方案（winget 行）、14. 已知风险
- **依据**：
  - UniGetUI 的作者在 issue #412、#884 中详述过为何放弃解析 `winget` CLI：
    1. 当 `winget` 重定向到标准管道（非交互式 Console）时，其内部输出缓冲区默认被限制在 80 字符列宽。超出宽度的软件名、ID 会被截断为 `...`，解析出来的 Package ID 缺失，拿去卸载直接报 `0x8A150014`（找不到包）。
    2. **多语言表头灾难**：在中文 Windows 下，`winget list` 表头是 `名称 | ID | 版本 | 可用 | 源`；在德文下是 `Name | ID | Version | Verfügbar | Quelle`。通用正则完全失效。
    3. 文档提出的备选 `Get-WinGetPackage | ConvertTo-Json` 依赖未默认安装的 `Microsoft.WinGet.Client` 模块，且由于 Windows 默认执行策略（Restricted ExecutionPolicy），非专业用户的 PowerShell 默认禁止运行脚本，直接报错。
- **建议改法**：
  - 不要尝试用通用的 `lines` 或 `table` 解析器硬吃 winget。
  - Windows 端适配器必须强制指定环境变量：`$env:WT_SESSION = 1` 或调用 Windows API 将虚拟终端（VT100）列宽设为 999 避免折行。
  - 必须通过设置当前进程的环境变量强制使用英文输出：`chcp 65001` 并注入 `DOTNET_CLI_UI_LANGUAGE=en-US`、`LANG=en_US.UTF-8`。
  - 获取更新时，优先读取本地 WinGet SQLite 数据库（路径通常在 `%LOCALAPPDATA%\Packages\Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\LocalState` 中的索引库，只读读取速度极快，彻底绕过 CLI 解析）。

---

### 2.2 把编译型管理器（Cargo）与环境冲突管理器（Pip）与普通桌面应用混排
- **涉及章节**：4.3 各来源落地方案（pip, cargo）、1. 定位
- **依据**：
  - **Cargo**：`cargo install` 是本地源码下载并在本地编译。没有预装 Rust 工具链、C 编译器（MSVC/clang）或缺少系统库（如 `openssl-dev`）时会直接爆出成百上千行的编译错误。一个 `cargo upgrade` 可能会让 CPU 满载 20 分钟并耗尽十几 GB 内存。普通用户如果点了“全部更新”，会认为电脑“卡死”。
  - **Pip**：现代 Linux（Debian 12+, Ubuntu 24.04+, Fedora 40+）与现代 macOS 全面激活 PEP 668，系统级 pip install 默认被拒（`externally-managed-environment`）。若强加参数，会直接覆写系统 apt/brew 的 Python 绑定库，引发系统桌面崩溃。
- **建议改法**：
  - **直接从首发支持列表移除系统级 `pip`**，仅保留 `uv tool` 与 `pipx`。
  - 将 `cargo` 标记为“开发者专有”扩展插件，且升级命令优先使用 `cargo-binstall`（下载预编译二进制），缺失 binstall 时必须弹窗警示“即将进行本地全量源码编译，耗时较长”。

---

### 2.3 后台无感检查更新与 Linux 系统级包管理器的特权与锁冲突
- **涉及章节**：4.3、6、8. 后台与托盘
- **依据**：
  - 文档设计：后台每隔 N 小时自动静默检查更新，发现后托盘通知。
  - 但在 Linux 上，`apt-get update`、`pacman -Sy`、`dnf check-update` 需要网络刷新。其中：
    - `apt-get update` 必须 root 权限。后台定时器触发时，**系统会每隔几小时无故弹出一个 polkit 密码输入框**，这是流氓软件的行为特征。
    - Ubuntu 默认启用了 `unattended-upgrades` 和 `packagekitd`。当它们在后台跑时，apt 会锁死 `/var/lib/dpkg/lock-frontend`。Canager 后台检查更新会频繁撞锁报错，向日志抛出海量垃圾。
- **建议改法**：
  - Linux 下**严禁在后台定时器中执行需要 root 的 `update` 命令**。
  - 后台检查更新必须退化为只读缓存探测：例如 apt 只读读取 `/var/lib/apt/lists/` 缓存比对可升级包（模拟 `apt list --upgradable`，不前置 `update`）；
  - 只有在用户主动点击“刷新更新”按钮时，才弹窗请求提权并前置更新索引。

---

### 2.4 单日发布 15 个包管理器导致质量雪崩，无法维持维护承诺
- **涉及章节**：0. 约束、13. 实施阶段
- **依据**：
  - 作者仅有 Mac，主要靠 AI 写代码，是首个开源项目。15 个包管理器的底层行为差异巨大（包含参数、退出码、流式日志拦截、中断处理、提权弹窗、源锁定）。
  - 一个包含 15 个异构后端的系统，其状态空间是爆炸的。首发发布后，GitHub Issues 会在 48 小时内涌入各种发行版和 Windows 环境的边界报错（如 GBK 乱码、sudo 挂死、Snap 守护进程断联）。没有真实 Windows/Linux 调试经验的作者会瞬间被 Issue 淹没，陷入“AI 生成代码修 A 却崩了 B”的死循环，最终导致烂尾放弃。
- **建议改法**：
  - 必须执行分期发布，将架构与首发解耦：
    - **v0.1 首发最小胜利集（Tier 1）**：
      - macOS: `Homebrew` + `ollama` + 独立安装器
      - Windows: `Winget` + `Scoop`
      - Linux: `Flatpak` + `Snap`（这两个无需 root 即可查更新且支持全发行版）
    - **v0.2（Tier 2，通用语言包管理器）**：`npm`, `uv tool`, `pipx`
    - **v0.3（Tier 3，高危原生系统包管理器）**：`apt`, `dnf`, `pacman`, `choco`, `cargo`。

---

## 3. 次要问题

### 3.1 独立安装器的卸载采用硬编码路径删除极其危险
- **涉及章节**：4.4 独立安装器
- **依据**：文档中 `[uninstall] remove = ["~/.local/bin/claude", "~/.local/share/claude"]`。如果用户是用 npm 全局安装的 claude，直接删除该文件会破坏 npm 的 link 引用；更严重的是，直接在 Rust 侧执行硬编码目录删除，若路径拼接或符号链接处理有 Bug，存在误删上层目录的数据安全隐患。
- **建议改法**：独立安装器首发版本**只做“识别与检测版本”，不提供“卸载”操作**，或者强制调用软件官方的反安装脚本。

### 3.2 缺乏网络超时与子进程僵尸防范
- **涉及章节**：4.2、6. 操作执行
- **依据**：很多包管理器（如 `brew search`、`npm search`、`flatpak search`）在网络差或代理配置不当时会无限期挂起（Hang）。
- **建议改法**：Runner 的 `tokio::process` 执行必须强制包装 `tokio::time::timeout`（例如探测与查更新默认 30 秒超时，安装/升级默认 15 分钟超时）。超时后自动终止进程树并标记为 `TimedOut` 错误。

### 3.3 精选商店（Catalog）静态维护成本极高
- **涉及章节**：5. 数据模型（CatalogEntry）
- **依据**：每个软件要手写中英 summary、why、多平台包 ID 映射（例如一个 VLC，在 Mac 是 cask `vlc`，Winget 是 `VideoLAN.VLC`，Ubuntu 是 snap `vlc`）。单人维护静态 JSON 会迅速过时、链接失效。
- **建议改法**：精选商店条目不要硬编码在安装包内，应改为托管在 GitHub Repository 的 JSON 资产，应用启动时定时拉取增量更新，便于社区直接提 PR 扩充，无需发版。

### 3.4 SQLite 数据库对于只读信息属于过度设计
- **涉及章节**：3. 架构、5. 数据模型
- **依据**：包列表（packages）本质是临时探测缓存，生命周期极短；将其全量存入 SQLite 每次启动做 ORM 映射反而带来复杂的版本迁移（Migration）成本。
- **建议改法**：保留 SQLite 存储“用户设置、历史日志、忽略列表”；已装软件包列表建议纯内存持有，并在磁盘落一个极简的 JSON 缓存即可。

---

## 4. 事实核查表（第 4.3 与 4.4 节命令逐条核验）

| 来源 | 文档说法 | 核查结论 | 正确做法 / 技术事实分析 |
|---|---|---|---|
| **Homebrew (List)** | `brew info --installed --json=v2` | **基本属实但有性能隐患** | 当安装包较多（200+）时，`brew info` 生成 JSON 极为耗时（5~10 秒）。快速列出已装应用应优先用 `brew list --versions`，仅在需要详情时按需查 `info`。另外，Cask 的项不在 `formulae[*]`，必须同时解析 `casks[*]` 且其结构无 `installed_on_request` 字段。 |
| **Homebrew (Outdated)** | `brew outdated --json=v2` | **有误 (漏关键参数)** | 默认不包含自动更新机制的 Cask。需显式加上 `--greedy`（或配置化）才能查到大多数 Cask 的更新；且输出顶层是 `{ "formulae": [...], "casks": [...] }`，并非单列表。 |
| **Homebrew (Search)** | `brew search --desc {query}` (lines 正则匹配) | **有误** | `brew search --desc` 的输出格式极不稳定，包含 `==> Formulae` 和 `==> Casks` 分节头，且当搜索无结果时退出码为 1。非英文字符常导致解析崩溃。不可作为稳定的 key-value 解析。 |
| **npm (List)** | `npm ls -g --depth=0 --json` | **有误 (退出码处理有误)** | 当存在无效 peer 依赖或 extraneous 包时，**`npm ls` 退出码为 1** 但 stdout 仍包含完整正确的 JSON！若按文档的 `exit_ok = [0]` 会被系统判定为执行失败。 |
| **npm (Outdated)** | `npm outdated -g --json` (退出码 1) | **属实** | 确实有更新时返回 1，无更新时返回 0。但必须注意有时 stderr 会混杂 `npm notice` 提示，必须仅截取 stdout 的 JSON 块。 |
| **pip (List)** | `pip list --format=json` | **属实** | 格式正确，输出扁平 `[{"name": "...", "version": "..."}]`。 |
| **pip (Outdated)** | `pip list --outdated --format=json` | **属实** | 格式正确，输出包含 `latest_version`。 |
| **pip (依赖标记)** | `pip list --not-required` | **有误** | `--not-required` 无法与 `--format=json` 在旧版本 pip 中良好共存；且 pip 没有原生记录“哪个是显式安装”的元数据文件，只能列出“当前无其他已装包依赖它”，与 `installed_on_request` 语义不同。 |
| **pipx (List)** | `pipx list --json` | **属实但结构深** | 结构并不是文档通用的 flat 数组，嵌套在 `venvs.<pkg>.metadata.main_package.package_version`，需专属 JSONPath。 |
| **uv tool (List)** | `uv tool list` (文本) | **属实** | 输出是纯文本，形如 `package v1.0.0 \n- binary`，尚无官方稳定 `--json`。 |
| **uv tool (Outdated)** | `uv tool list --outdated` (待核实) | **严重有误 (参数不存在)** | **uv 根本没有 `--outdated` 这个参数！** 执行会直接报 `error: unexpected argument '--outdated'`。检测更新需逐个对比 PyPI API 或直接依赖 `uv tool upgrade --all`。 |
| **cargo (List)** | `cargo install --list` | **属实** | 格式为多行文本，第一行为包名与版本，后续为二进制名称。必须写自定义原生解析器。 |
| **cargo (Upgrade)** | `cargo install / upgrade` | **严重有误 (体验毁灭)** | 没有现成的单包 upgrade 命令，且该命令会拉取源码本地全核编译。缺乏 C 依赖时经常编译失败。必须禁止普通人盲目执行。 |
| **ollama (Tags API)** | `GET localhost:11434/api/tags` | **属实** | 官方稳定 API，返回 JSON 包含 `models[].name`, `models[].digest`, `models[].size`。 |
| **ollama (Outdated)** | 比对本地 digest 与 registry.ollama.ai v2 manifest | **基本属实但有复杂度陷阱** | 需注意 Ollama 官方使用的是 Docker Registry v2 协议，拉取 manifest 需要先向 `registry.ollama.ai/v2/token` 发起认证交换获取临时匿名 Bearer Token，否则直接请求 manifest 会报 401 Unauthorized。 |
| **winget (List)** | `winget list` 表格解析 | **有误 (工程上不可行)** | CLI 重定向默认 80 宽截断、Unicode 对齐错误、不同 Windows 语言下表头翻译不同（中文、日文、德文各异）。直接用 table 规则必挂。且没有更新时可能退出码非 0。 |
| **winget (PS 模块)** | `Get-WinGetPackage \| ConvertTo-Json` | **有误 (依赖不成立)** | `Microsoft.WinGet.Client` 模块并非 Windows 自带，绝大多数普通电脑上没有安装，且会被系统的 PowerShell 脚本执行策略拦截。 |
| **scoop (List)** | `scoop export` (JSON) | **有误** | `scoop export` 导出的 JSON 仅包含 `Name` 和 `Source`，**根本不包含当前安装的版本号（version）**！要获取版本号，必须解析 `scoop list` 的纯文本输出。 |
| **chocolatey (List)** | `choco list --limit-output` | **严重有误 (重大变更)** | **在 Chocolatey v2.0+ 中，`choco list` 默认搜索远程仓库！** 必须写成 `choco list --local-only --limit-output`，否则会直接拉取远程数万个条目，导致超时或流量暴涨。 |
| **apt (List)** | `dpkg-query -W` + `apt-mark showmanual` | **属实但基数巨大** | 逻辑可行，但输出的包通常有 1500~3000 个（含系统底层动态库 libc、systemd 等），全量加载会严重拖慢前端渲染，必须做虚拟滚动或二次过滤。 |
| **apt (Outdated)** | `apt list --upgradable` 前置 `apt-get update` | **有误 (提权冲突)** | `apt-get update` 必须 root，后台轮询无法自动执行。且经常与系统的 `unattended-upgrades` 产生 `/var/lib/dpkg/lock-frontend` 锁冲突。 |
| **dnf (Outdated)** | `dnf check-update` (退出码 100) | **属实** | 退出码 100 确实代表有可用更新，0 代表全最新，1 代表报错。dnf5 支持 `--json`，dnf4 需文本解析。 |
| **pacman (Outdated)** | `pacman -Qu` (需先 `-Sy`) | **致命错误** | 严禁直接 `-Sy`（造成破坏系统的部分升级）。正确做法是使用 `checkupdates` 或利用 `fakeroot` 指定临时隔离 dbpath。 |
| **snap (List/Outdated)** | `snap list` / `snap refresh --list` | **属实** | 输出标准对齐表格。但 snapd 自身自带强制自动刷新，GUI 查更新经常显示为空。 |
| **flatpak (List/Outdated)**| `flatpak list --columns=...` / `remote-ls --updates` | **属实** | Flatpak 的 `--columns` 支持标准 TSV 输出，是非常规范且适合机器解析的格式。 |
| **独立安装器 (4.4)** | `claude-code` 探测与更新 | **有误** | 很多用户使用 npm 全局安装（`npm i -g @anthropic-ai/claude-code`），若硬编码路径检测会造成来源重叠；且 GUI 环境下默认 PATH 读取不到 `~/.local/bin`。 |

---

## 5. 缺失项（文档未写但架构落地必须包含的）

1. **GUI 进程的环境变量捕获机制**：
   - 缺少启动阶段从用户的 Login Shell 中恢复完整 `PATH`、`LANG`、`SSL_CERT_FILE` 的前置钩子定义（见 1.3）。
2. **包管理器进程锁与独占冲突处理（Process Lock Handling）**：
   - Linux apt 的 `dpkg lock`、dnf 的 `transaction.lock`、pacman 的 `/var/lib/pacman/db.lck`、Homebrew 的 `brew.lock`。
   - 当检测到锁被占用时，Runner 必须有“退避重试”或“捕获并提示用户哪一个外部进程正在占用安装器”的语义，否则只会抛出一长串无解的退出码错误。
3. **针对大体量列表的前端虚拟化渲染（Virtualization）规范**：
   - 执行 `dpkg-query` 或 `rpm -qa` 后，一次性返回 2000+ 个包。如果直接塞进 React 19 的普通 DOM 树，会直接导致 DOM 节点过多卡死。第 7 节未提到 `@tanstack/react-virtual` 等虚拟列表方案。
4. **统一的多语言编码转换层（Encoding Normalization）**：
   - 在旧版或非英文 Windows 下，PowerShell / CMD / 传统 CLI 软件标准输出采用 OEM 代码页（如 GBK / CP936、Shift-JIS）。Rust 的 `tokio::process` 若直接按 `String::from_utf8` 处理，会抛出 `Utf8Error` 导致解析器直接崩溃。必须在 Core 中集成 `encoding_rs` 自动探测并转码为 UTF-8。
5. **代理与网络环境配置（Proxy Support）**：
   - 中国大陆等地区的开发者普遍使用 Clash / v2ray 等本地代理端口（如 `http://127.0.0.1:7890`）。CLI 进程不会自动继承系统 GUI 代理设置。适配器若不提供继承系统代理或注入 `HTTP_PROXY` / `HTTPS_PROXY` 的配置，查更新和下载将大面积网络超时。
6. **卸载级联依赖安全检查（Dependency Reverse-Tree Guard）**：
   - 既然文档声明“面向普通人”，当普通人在界面上点击卸载某个库（例如 `openssl` 或某个底层 python 库）时，文档没有提供反向依赖检查机制。若直接运行 `brew uninstall openssl` 或 `pacman -R openssl`，轻则被拦截报错，重则直接连带卸载掉几十个正常使用的上位软件。

---

## 6. 一句话总评与优先改进建议

### 一句话总评
> **这是一份典型的“极客用理想模型推导、AI 生成胶水层、却幻想卖给小白用户”的过度设计草案；它低估了跨平台操作系统底层的脏逻辑与破坏性，若按当前范围首发必遭技术反噬。**

### 最该先改的 3 件事

1. **砍掉 60% 的来源，推行「Tier 分级首发」策略**：
   - 立即废除 15 个包管理器单日全上的激进约束。
   - **v0.1 首发聚焦于：macOS (Homebrew) + Windows (Winget, Scoop) + Linux (Flatpak)**。
   - 彻底从首发版本中剔除系统级 `pip`、`cargo` 与破坏性极高的原生 `pacman/apt` 提权操作。验证成功且测试用例稳固后，再在 v0.2 引入其他管理源。

2. **重塑目标受众定位，剔除“非技术普通人”的虚妄假设**：
   - 将定位调整为**“面向开发者与数码极客的轻量级统一包管理看板”**。
   - 删掉华而不实的手写白话说明（“你需要它吗”），保留清晰的依赖分组、命令预览、跨源检索、版本对比。
   - 在未自费购买开发者签名证书之前，坦诚面向 GitHub 技术圈分发，优先上线 Homebrew Cask、Winget 社区仓库提交，避免因系统安全弹窗而丧失初始口碑。

3. **彻底重构 Windows (Winget) 与 Arch (Pacman) 的底层执行核心**：
   - 修正 `pacman` 的查更新逻辑，禁用危险的 `-Sy`，改用 `checkupdates` 隔离沙盒；
   - 针对 `winget`，放弃不可靠的纯正则剪裁，强制固定控制台 UTF-8 编码与宽字符模式，或者在 Windows 端引入本地 SQLite 索引直接读取机制，保证最核心的数据解析绝对稳固。
