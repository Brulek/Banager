我会先联网核实文档里的关键事实（Tauri、UniGetUI、winget/uv/ollama、pkexec），再对照 4.3/4.4 的命令做逐条核查，最后给出可落地的评审。接下来会并行核对命令、竞品口碑和 Linux 提权现状，避免只凭文档字面下结论。先说结论：**这份草案把「产品愿景」写成了「可执行架构」，但核心路径上有几处是错的，而且错在会被真实系统打脸的地方。** 三平台同日 + 15 个管理器 + 未签名 + 面向普通人 + 作者只有 Mac、靠 AI 写代码，这组约束叠在一起，不是「难」，是 **v0.1 会以半残状态上线，然后在 Windows/Linux 的第一周 issue 里被钉死**。

UniGetUI 用了多年、26.1k star、被公司收购之后，2026 年才把 Avalonia 三平台做出来，而且 Windows 用户还在骂 UI（[#4943](https://github.com/Devolutions/UniGetUI/discussions/4943)、[#4770](https://github.com/Devolutions/UniGetUI/discussions/4770)）。你要用更小的团队、更脆的技术栈，在第一天覆盖它刚补上的三平台，再外加它不做的 ollama/游离二进制——这不是差异化，是把竞品的五年坑压缩进一周。

---

## 1. 致命问题

### 1.1 未签名 +「普通人」是互相取消的产品定义
**章节：** §0 分发、§1 定位、§10、§14

**依据：**
- 你自己写「面向普通人」，又写「首发不签名，README 写绕过」。普通人不会读 README，也不会跑 `xattr`。
- macOS 15+ 上，未签名/未公证应用经常直接显示 **「已损坏，无法打开」**，右键打开也不再够用，要去系统设置里放行。2026 年的实测记录：[Handling macOS Gatekeeper as an Unsigned Indie Dev](https://dev.to/hiyoyok/handling-macos-gatekeeper-as-an-unsigned-indie-dev-the-xattr-struggle-1028)、[ytyng.com 的 Tauri 公证指南](https://www.ytyng.com/en/blog/tauri-github-actions-macos-sign-notarize) 写明 Sequoia 后右键绕过基本失效。
- Windows 上 SmartScreen 对未签名安装包的默认动作是 **拦截**，不是温和提示。Tauri 官方文档把签名写成 Store 上架和「下载后能启动」的前提：[Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)。
- 「有 star 再补签名」是倒因果：普通人打不开 → 没有 star → 一直不签。UniGetUI 都是 2026.1.9 才把 macOS 公证补上，补完才停止「未验证开发者」弹窗（[Devolutions release notes 2026.1.9](https://devolutions.net/unigetui/release-notes/)）。

**建议改法：** 二选一，写进 §0：
1. **v0.1 只做 macOS，Apple Developer（$99）+ notarize 再发**；Windows/Linux 标 `experimental` 或不提供安装包；或
2. 定位改成 **「给愿意关 Gatekeeper 的开发者」**，删掉所有「普通人」「10MB vs 50MB 就能赢 UniGetUI」的句子。

坚持「普通人 + 三平台同日 + 未签名」= 产品在安装这一步就死。这不是风险表里的一项，是发布通道不存在。

---

### 1.2 「适配器是数据不是代码」是假架构，会被 winget/scoop/cargo/uv 当场拆穿
**章节：** §3 关键决定 2、§4.1–4.3

**依据：**
- 你自己在 §4.3 写了：UniGetUI **已经放弃解析 winget CLI，改用 COM API**。这是 2024 年 PR [#2035](https://github.com/Devolutions/UniGetUI/pull/2035) 落地的，维护者 2025-01 还确认「Currently, UniGetUI uses the WinGet COM APIs」（[Discussion #1725](https://github.com/marticliment/UniGetUI/discussions/1725)）。你计划用表格解析 + PowerShell 模块兜底——这是 UniGetUI 走过、扔掉的路。
- 截至 2026-09，winget CLI **仍然没有** `list --output json`。开了四年的 issue 还开着：[microsoft/winget-cli#2032](https://github.com/microsoft/winget-cli/issues/2032)。v1.29 只修了「重定向 stdout 时不再截断列」（[winget-cli v1.29.240 notes](https://newreleases.io/project/github/microsoft/winget-cli/release/v1.29.240)），不是 JSON。
- `Microsoft.WinGet.Client` **不是系统自带**：要 `Install-Module`。`Get-WinGetPackage | ConvertTo-Json` 默认深度 2，会把嵌套属性切成 `{...}`。COM API 是 in-process，PowerShell 模块是另一套版本漂移。文档把这条写成「优先 JSON」——它既不是默认可用，也不是 JSON-first。
- Homebrew 的 TOML 示例在本机 Homebrew 7.0.3 上已经对不上（见事实核查表）：cask 的主键是 `token` 不是 `name`，`installed` 是字符串不是数组，**没有** `installed_on_request`。`brew search --desc` 会打出 `==> Formulae` / `==> Casks` 分组头，你的 `^(?P<id>\S+): (?P<description>.*)$` 会把分组头当包名或直接跳过。
- cargo / uv / scoop status / ollama list / snap list 全部是「看起来像表」的人类输出。文档把它们塞进同一个 `table`/`lines` 解析器，等于把产品核心建成「正则农场」。

**建议改法：** 删掉「加一个包管理器 = 几十行 TOML」。改成两层：
- **稳定 JSON/API 源**：brew json v2、npm `--json`、pip `--format=json`、pipx `--json`、flatpak `--columns`、ollama `/api/tags`、dnf5 `--json`。这些才配 TOML。
- **一等公民原生适配器（Rust）**：`winget`（COM，不走 CLI）、`scoop`（直接读 `apps/` 目录，别解析 `scoop status`）、`cargo`、`uv`、`pacman`。Windows 不做 COM 就不要做 Windows。

TOML 可以当配置，不能当解析器。

---

### 1.3 `pacman -Sy` 再 `-Qu` 会制造 Arch 部分升级；文档把「查更新」写成了毁系统的前置步骤
**章节：** §4.3 pacman、§6 危险操作硬规则、§14

**依据：**
- 文档：查更新用 `pacman -Qu`（需先 `-Sy`），升级只允许 `-Syu`。
- Arch 社区的铁律是：**不要 `pacman -Sy` 而不立刻 `-Su`**。刷新同步数据库但不升级，之后任何安装都是部分升级。EndeavourOS 版主写得很清楚：`pacman -Sy` 后装包就会把系统置于部分升级状态；查更新应使用 **`checkupdates`（pacman-contrib）**，它不碰本地数据库（[Avoiding partial upgrades](https://forum.endeavouros.com/t/avoiding-partial-upgrades/6300)）。
- 你把「硬禁单包升级」写进了风险表，却把更常见的脚枪（`-Sy` + 稍后用户点「安装商店里的一个包」）设计成默认查更新路径。后台每 N 小时跑一次的话，等于定时把 Arch 用户的 pacman DB 拉到未来。

**建议改法：**
- 查更新：**只**调用 `checkupdates` / `checkupdates --nocolor`，**禁止** `pacman -Sy`。
- 升级：**只**提供「打开终端并预填 `sudo pacman -Syu`」或 pkexec 一次跑完 `-Syu`；不要在 GUI 里拆成 refresh 和 upgrade 两步。
- 安装新包：必须检测「DB 是否比本机包新」，是则拒绝安装、要求先全量升级。不写这一条就不要宣称支持 pacman。

---

### 1.4 三平台同日首发，在 Tauri 2 的 Linux/Windows 现状下会交出三个不同的残次品
**章节：** §0 平台、§2、§8、§10、§13 阶段 0

**依据（2025–2026 仍在发生）：**
- **Linux 托盘：** Tauri 2.8 + GNOME Wayland，`.deb`/dev 模式托盘图标消失，只有 AppImage+X11 正常（[tauri#14234](https://github.com/tauri-apps/tauri/issues/14234)，2025-10，到 2026 仍 open）。你的 v1 卖点之一是托盘常驻。
- **AppImage：** 默认 bundler 在 Mesa 25+（Ubuntu 26.04）上 WebKitWebProcess 直接 abort，窗口永不出现（[tauri#15665](https://github.com/tauri-apps/tauri/issues/15665)，2026-07）。linuxdeploy 的 GTK 插件仍会 `export GDK_BACKEND=x11`（[DEV 文 2026-04](https://dev.to/fengzeng/fixing-wayland-crashes-in-tauri-appimage-linuxdeploy-gtk-issue-1381)）。你计划用 `ubuntu-22.04` 打 AppImage 发给「所有 Linux」——这正是 #15665 的复现条件。
- **WebKitGTK：** NVIDIA 空白窗/闪烁是官方文档级问题（[Linux Graphics Issues，2026-06](https://v2.tauri.app/develop/debug/linux-graphics/)）；Fedora 43 + Sway 空白窗（[tauri#15050](https://github.com/tauri-apps/tauri/issues/15050)）；Linux 字重整体偏粗 100（[tauri#14286](https://github.com/tauri-apps/tauri/issues/14286)）。「原生质感、系统字体、留白」在 Linux 上会先变成「字更粗、窗空白」。
- **Windows 托盘：** Start Menu 启动后托盘菜单跑到任务栏后面（[tray-icon#303](https://github.com/tauri-apps/tray-icon/issues/303)，2026-04，Tauri 2.10.3）。
- **Windows 安全：** CVE-2026-42184（CVSS 8.8）是 Tauri `is_local_url()` 在 Windows 把远程 URL 当成本地 origin，2.10.3 / 2.11.1 才修。你这种「WebView 里一键跑提权包管理命令」的应用，用错版本就是 RCE 形状。

作者只有 Mac。阶段 0「CI 出空壳包」只能证明 **链接器没炸**，证明不了 Wayland 托盘、NVIDIA WebKit、SmartScreen、UAC 提权回读。UniGetUI 的 Linux/macOS 口碑到 2026-09 仍然是「能用的移植，不是产品」（README 自己写 Windows-first；Linux sudo 直到 2026.1.x 还不全）。

**建议改法：**
- 阶段 0–7 **只发 macOS signed dmg**。Linux/Windows 进 `nightly`，不出现在 README 主按钮。
- 若坚持 Linux 包：v1 **只发 .deb/.rpm，不发 AppImage**；文档写死 WebKitGTK 依赖和 NVIDIA 环境变量。
- Windows v1 若做：WebView2 Evergreen 检测 + 托盘不作为默认关闭行为。
- CI 矩阵把 `ubuntu-22.04` 改成 **build 用 22.04、# Canager 设计文档评审（草案 v0.1）

结论先说：**按现在这份草案做，项目大概率会烂尾或首发即口碑崩盘。** 不是“写得不够细”，而是产品定位、竞品事实、适配器模型和发布策略四件事同时错了。作者约束（非专业、只有 Mac、AI 写代码、三平台同日、15 个包管理器）**会直接导致失败**，不是“有风险但可坚持”。

---

## 1. 致命问题

### 1.1 三平台同日 × 15 个包管理器 × 只有 Mac = 失败配方

- **涉及章节**：§0、§10、§13、§14
- **依据**：
  - UniGetUI 在 Windows 上做了数年、26.1k star、2026-03 被 Devolutions 收购后，才把 Avalonia 三平台做出来。macOS/Linux 到 2026.3.0 仍被官方写成 *Windows-first, cross-platform builds also available*；Linux 提权、RPM、resize、glibc 基线都是 2026 上半年才修。来源：[Devolutions/UniGetUI README](https://github.com/Devolutions/UniGetUI)、[release notes](https://devolutions.net/unigetui/release-notes/)、[v2026.1.7](https://devolutions.net/unigetui/release-notes/)（才加 APT/DNF/Pacman/Snap）。
  - Homebrew 作者 Max Howell 的 [Package Manager Manager](https://mxcl.dev/package-manager-manager/) 选择 **只做 macOS 原生 GUI**，Linux 用 SSH 远程看，不装 Linux GUI。这是懂包管理器的人给出的范围答案。
  - 作者没有 Windows/Linux 真机，却要把 winget 表格/COM、choco 管理员、scoop 的 PowerShell 函数、pacman 部分升级、dnf5、WebKitGTK 托盘全部首发。CI 冒烟装一个 `hello` **测不到** 列截断、中文 winget、UAC、polkit 无 agent、NVIDIA+WebKitGTK 白屏。
- **建议改法**：v0.1 **只发 macOS**。包管理器砍到 `brew（formulae+casks）+ npm 全局 + pipx/uv tool + 独立安装器（claude/uv/rustup/bun/deno）`。Windows/Linux 标 `experimental` 或等有真实测试者再发。坚持三平台同日等于用空壳安装包换 GitHub 上的一星差评。

### 1.2 面向「普通人」却首发不签名，产品自杀

- **涉及章节**：§0 分发、§1、§10
- **依据**：
  - 2026 年 macOS 未公证应用经常直接报 **“已损坏 / 无法打开”**，不是“未识别开发者”。Sequoia 起右键打开绕过已不可靠，用户要去系统设置里放行。见 [ytyng.com Tauri 签名指南（2026-07）](https://www.ytyng.com/en/blog/tauri-github-actions-macos-sign-notarize)、[DEV: unsigned indie Gatekeeper 2026](https://dev.to/hiyoyok/handling-macos-gatekeeper-as-an-unsigned-indie-dev-the-xattr-struggle-1028)。
  - README 写 `xattr -dr com.apple.quarantine` 是给开发者看的。你的目标用户过不了第一道门，就不会 star，会在 HN/少数派评论区写“打不开”。
  - Windows SmartScreen 对未签名安装包是 **拦截级**，不是提示级；Tauri 官方文档写明签名是为了避免 SmartScreen。见 [Tauri 2 Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)。
  - UniGetUI 自己也是到 **2026.1.9** 才把 macOS 签名补上，专门修 Gatekeeper。你要比它更“普通人友好”，却连这一步都推迟。
- **建议改法**：要么申请 Apple Developer（$99）+ 公证，Windows 至少 OV 证书；要么把口号改成 **“给会用终端的人用的包管理器仪表盘”**，并在下载按钮上方用红字写清 Gatekeeper/SmartScreen。不要同时说“普通人”和“不签名”。

### 1.3 竞品分析过期：UniGetUI 2026 已经做了你当作差异化的大半件事

- **涉及章节**：§0 竞品、§1 差异化清单
- **依据**（截至 2026-09-17）：
  - UniGetUI **已发** macOS arm64/x64 `.dmg`（约 49–53 MB）和 Linux `.deb/.rpm/.tar.gz`（约 28–30 MB）。
  - 已支持：WinGet、Scoop、Chocolatey、**Homebrew、APT、DNF、Pacman、Flatpak、Snap、pip、npm、Bun、Cargo**。见 [GitHub README](https://github.com/Devolutions/UniGetUI) 与 2026.1.7/2026.1.8 发行说明。
  - 文档写“UniGetUI 代码与 issue 中均无 ollama / AI CLI / curl|sh / 来源不明”。这只对 **ollama 模型和来源不明扫描** 大致成立；Homebrew/APT/npm/pip/cargo 已不是空缺。
  - 更接近的竞品你没写：[topgrade-rs/topgrade](https://github.com/topgrade-rs/topgrade) **4.5k star**，三平台 CLI，已加 Ollama pull、mise、Antigravity CLI；[vinifmor/bauh](https://github.com/vinifmor/bauh) 1.4k，Linux 多格式 GUI；mxcl PMM 覆盖 brew/npm/pnpm/bun/uv/pipx/cargo/rustup/mise + 远程 Linux。
- **建议改法**：重写 §1。诚实差异只剩：**(a) ollama 模型清单与更新检测；(b) 独立安装器/来源不明；(c) 更克制的 UI**。不要再用“三平台管家”去打 26k star、有公司养的产品。

### 1.4 「适配器是数据不是代码」撑不住真实包管理器

- **涉及章节**：§3 决定 2、§4.1–4.3、§12 CONTRIBUTING
- **依据**：你自己的表格已经承认 winget 表格、uv 文本、cargo 文本、ollama 非公开 registry、dnf5 输出变化、apt CLI 不稳定。这些不是“再加一个 `parser = json`”能过的。
  - Homebrew **一条** `brew info --installed --json=v2` 里 formulae 与 casks **schema 不同**：formula 用 `name` + `installed[0].version` + `installed_on_request`；cask 用 `token` + `installed`（字符串）+ **没有** `installed_on_request`。我在本机 Homebrew 7.0.3 上核对过。TOML 里 `items = "$.formulae[*]"` 会丢掉全部 cask。
  - `npm ls -g --json` 是 `{ dependencies: { "pkg": { "version": "..." } } }`，不是数组。JSONPath `$.[*]` 对不上。
  - UniGetUI 在 2024 就放弃解析 winget CLI，改 COM API（[PR #2035](https://github.com/Devolutions/UniGetUI/pull/2035)、[Discussion #1725](https://github.com/marticliment/UniGetUI/discussions/1725)）。你计划用表格 + PowerShell 模块兜底——PowerShell 模块还要用户另装，CI/普通人机器上经常没有。
- **建议改法**：把适配器分成两档：
  1. **声明式 TOML**：仅限真正稳定的 JSON（brew formulae、pip list --format=json、pipx --json、choco -r、flatpak --columns）。
  2. **Rust 原生适配器**：winget（COM 或 WinGet.Client）、brew cask、npm、scoop、dnf5、pacman、ollama、cargo、uv。CONTRIBUTING 删掉“30 行 TOML 加一个包管理器”，改成“先交 fixtures + 原生解析，再考虑 TOML”。

### 1.5 Linux 主分发形态选错：默认 AppImage 在 2026 会白屏

- **涉及章节**：§2、§10、§14 WebKitGTK
- **依据**：
  - [tauri#15665（2026-07）](https://github.com/tauri-apps/tauri/issues/15665)：默认 bundler 的 AppImage 在 Mesa 25+ / Ubuntu 26.04 上 WebKitWebProcess abort，窗口永不出现。原因是 linuxdeploy 打进 `libwayland*`/`libglib`/`gstreamer`。
  - linuxdeploy-plugin-gtk 仍硬编码 `GDK_BACKEND=x11`，Wayland 上崩。见 [DEV 2026-04](https://dev.to/fengzeng/fixing-wayland-crashes-in-tauri-appimage-linuxdeploy-gtk-issue-1381)、[tauri#15902](https://github.com/tauri-apps/tauri/issues/15902)。
  - 托盘：[tauri#14234](https://github.com/tauri-apps/tauri/issues/14234)（2025-10，Tauri 2.8）GNOME Wayland 上 **dev/.deb 托盘图标消失，只有 AppImage 有**——和 AppImage 图形 bug 正好互斥。
  - NVIDIA + WebKitGTK 白屏是官方文档级问题：[Tauri Linux Graphics Issues](https://v2.tauri.app/develop/debug/linux-graphics/)（页面标注 2026-06）。
- **建议改法**：Linux **只发 .deb + .rpm**，AppImage 标实验或干脆不做。README 写清依赖 `webkit2gtk-4.1`，给 NVIDIA 用户写 `WEBKIT_DISABLE_DMABUF_RENDERER=1`。托盘做成可选，Wayland 上失败就静默降级，不要作为首发卖点。

---

## 2. 重大问题

### 2.1 「10 MB、长得像原生」打不过真实观感，还会被 UniGetUI 用户教育过的标准打脸

- **涉及章节**：§1、§2、§7
- **依据**：UniGetUI 2026.2.2 丢掉 WinUI 后，用户用 “clusterfuck / 十年最丑 UI” 形容 Avalonia 跨平台皮，见 [Discussion #4770](https://github.com/Devolutions/UniGetUI/discussions/4770)、[#4943](https://github.com/Devolutions/UniGetUI/discussions/4943)。市场已经证明：**功能用户要原生控件，不要跨平台皮。** Tauri 在 Windows 是 WebView2，Linux 是 WebKitGTK（[字体粗细偏移 100](https://github.com/tauri-apps/tauri/issues/14286)）。Tailwind + Radix 做“macOS 侧栏”在 Win/Linux 上就是一个网站。
- **体积**：UniGetUI NativeAOT Windows 安装包约 **29 MB**（ComputerBase 2026.3.0），macOS dmg **49–53 MB**。Tauri 安装包可以到 10 MB，是因为 **WebView 在系统里**。Windows 没装 WebView2 Evergreen 会先下载；Linux 没 webkit2gtk 直接起不来。对比表必须写“需系统 WebView”，否则 HN 第一句就是 fake。
- **建议改法**：定位改成 “small installer, webview UI”，不要写“原生质感”。截图分三平台，Linux 用 Fedora+NVIDIA 和 Ubuntu GNOME Wayland 各一张。

### 2.2 提权方案在 Linux 上不可用，在 Windows 上首发就是残的

- **涉及章节**：§6
- **依据**：
  - `pkexec <cmd>` **不会**无条件弹出图形密码框。没有 polkit agent 的 WM（Hyprland/Sway/i3，Arch 用户重灾区）会失败或掉到 TTY。见 Brodie Robertson 的常见复现、[Arch Wiki / polkit](https://wiki.archlinux.org/title/Polkit)。
  - pkexec 默认清环境；对 **CLI**（apt-get）通常够用，但你必须：① 安装自己的 `.policy`（`org.canager.pkexec.policy`，标注允许的二进制绝对路径）；② 用 `--disable-internal-agent` 避免卡在无 TTY 的文本 agent；③ 处理退出码 126（用户点取消）。
  - Flatpak 系统级是 **polkit 自己弹**，再包一层 pkexec 会双重提权。
  - Windows `Start-Process -Verb RunAs` + 临时文件：UAC 取消、编码（chcp 65001 vs OEM）、杀进程、并发三个操作抢同一个 temp 文件，首发就会有“点了更新没输出”。UniGetUI 为此做了常驻 elevator；你把正确方案放到 v1.x，等于 Windows 核心路径（choco、部分 winget）首发不可用。
- **建议改法**：Linux 用 **polkit action + pkexec 绝对路径**，不要 `pkexec apt-get ...` 这种任意命令。Windows v1 要么不做需要 UAC 的源（只做 scoop/user-scope winget/npm），要么第一天做 helper。不要假装 auto 探测可写性就能覆盖 choco。

### 2.3 CI「用真实管理器装 hello」会浪费时间且给假信心

- **涉及章节**：§10、§11
- **依据**：GitHub `windows-latest` 自带 winget 和 Chocolatey，**不自带 scoop**。`macos-latest` 现为 ARM，universal dmg 要 `--target universal-apple-darwin`，不是“跑一下 tauri-action”就有。`ubuntu-22.04` 对 Tauri 是对的（官方 workflow 仍钉 22.04），但 runner 上 `sudo apt-get install hello` 测不到 polkit 弹窗、测不到 dnf/pacman/snap/flatpak。
- 在 CI 里跑 `winget install` 会卡源协议、卡 UAC、偶发 Store 源。UniGetUI 自己都不靠这种方式保证质量。
- **建议改法**：CI 强制 **fixtures 解析测试**（这点你做对了）。集成测试用 **录制的 subprocess mock**（你已经有 CommandRunner trait）。真实安装放到手工清单，按平台招募 3 个测试者，不要写进 PR CI。

### 2.4 独立安装器模型会马上撞上「同一工具三条安装路径」

- **涉及章节**：§4.4、§4.5
- **依据**：Claude Code 官方现在推荐 **native installer**（`~/.local/bin/claude` → `~/.local/share/claude/versions/<ver>`），npm 全局包 **2026-01 起 deprecated**。`claude update` 只对 native 正确；brew cask / winget / npm 各走各的。双重安装会导致 `claude update` 下 0 字节二进制，见 [anthropics/claude-code#37393](https://github.com/anthropics/claude-code/issues/37393)。
- 你的卸载列表 `~/.local/bin/claude` + `~/.local/share/claude` 会误删 versions 目录，但漏掉 `~/.claude` 配置，也处理不了 brew/npm 装的副本。
- uv、pnpm、bun 都可以同时出现在 brew、独立脚本、npm 里。来源不明扫描若按 PATH 归类，会把 brew 的 shim 标成“不明”或把不明标成 brew。
- **建议改法**：每个 standalone 条目必须写 `detect.priority`（brew > native > npm）、冲突时 UI 显示“检测到 2 个 claude”，升级命令按来源分支。卸载调用官方方法，禁止硬删目录。Docker Desktop「仅识别」请直接删掉——识别了不能管，普通人会点卸载然后骂你。

### 2.5 安全模型对「普通人点卸载」不够硬

- **涉及章节**：§4.5、§5 CatalogEntry.safe_to_remove、§6
- **依据**：apt `showmanual` 里有大量用户随系统装上的包（`ubuntu-desktop`、内核元包）。pacman `-Qe` 同样。你禁止 pacman 单包升级是对的，但 **单包卸载** 一样能拆掉桌面。brew 卸载 dependency 默认会拒，你若将来加 `--ignore-dependencies` 就是事故。
- `id_pattern` 拒绝 `-` 开头很好；但 `{query}` 进 `brew search` / `apt-cache search` 仍可能是奇怪字符串。参数数组能防注入，防不了 `brew install` 一个恶意 formula 名。
- 来源不明默认不执行 `--version` 是对的；扫描 `$HOME` 下 PATH 会扫到 `~/Library/Application Support/...` 里的辅助二进制，列表会脏到不可用。
- **建议改法**：系统源（apt/dnf/pacman/brew 非 leaf）默认 **禁止卸载**，高级模式二次输入包名。未知二进制 v1 只读。搜索 query 另做 `query_pattern`（禁止空格以外的 shell 元字符，尽管你不走 shell）。

### 2.6 阶段划分把最难的东西（winget、Linux 系统包管理器）放到「其余适配器」一次做完

- **涉及章节**：§13 阶段 3
- **依据**：阶段 1 是 brew/npm/pip——这是你有 Mac 能测的。阶段 3 一次塞 12 个适配器，还写“AI 批量生成”。AI 生成的 winget 表格解析会在中文 Windows、截断 ID、商店应用上立刻碎。这是 2022–2024 UniGetUI issue 区的尸体堆。
- **建议改法**：阶段 3 拆成 3a winget COM（有 Windows 测试者再合并）、3b 其余 JSON 源、3c Linux 系统 PM 各发行版一个维护者。没有测试者就不要合并。

---

## 3. 次要问题

| 标题 | 章节 | 依据 | 改法 |
|---|---|---|---|
| 名字 Canager 像拼写错误 | §0 | 英语用户会看成 Manager 打错；Google 无语义；和 “canager”（不存在的词）撞。UniGetUI/Topgrade/bauh 都可发音。 | 改名再占 GitHub。候选要能念、能搜、不暗示 cane/cancer。 |
| 前端栈过重 | §2 | React19+Tailwind v4+Radix+Query+Zustand+i18next，对“已装列表+日志”过杀。AI 生成的 Tailwind 在 WebKitGTK 上更容易出滚动/焦点 bug。 | v1 用更薄的视图层；或接受这是开发速度税，但不要承诺 10 MB 且流畅。 |
| `macos-latest` ≠ universal | §10 | 当前 macos-latest 是 ARM。universal 必须双 target。 | workflow 写明 `universal-apple-darwin`，Intel 单独测一次。 |
| 无崩溃上报 | 全文缺失 | 作者无 Win/Linux 机器，issue 会是 “doesn't work”。 | 首发就加 opt-in 诊断（适配器 id、OS、脱敏日志），不要等 star。 |
| 精选商店手写中英 | §1、§5、§9 | 15 个源 × 分类 × 两语言，一人维护会烂。 | v1 只做 30 个条目，或先不做发现页。 |
| 简单/高级两档 | §7 | UniGetUI 刚因双 UI 被骂然后砍掉 Classic。 | 一个 UI，默认藏版本号，设置里开“显示技术细节”。不要两套布局。 |
| i18n 只有 en/zh-CN | §9 | GitHub star 来自全球；winget 表格解析却依赖系统语言。 | UI 先英+中；**解析 fixtures 必须覆盖 en-US 和 zh-CN winget 输出**。 |
| AUR / mas / Windows Store 缺失 | §4.3 | Arch 用户没有 AUR 会直接去 Octopi；Mac 普通人更多用 App Store（`mas`）。 | 写进非目标，避免 issue 轰炸。 |
| `serde_json_path` | §2 | 能用，但 brew/npm 的结构用手写 serde 更稳。 | JSONPath 只给真正规则的输出。 |
| 更新器后期才做 | §2 updater | 未签名就不能用 Tauri updater。 | 与签名绑定；未签名期用“打开 Releases 页”。 |
| 仓库运营期望 | §12 | 首发同时打 HN、r/linux、少数派、小红书，带未签名三平台，是在主动收集差评。 | 先在 r/macapps 或 v2ex 发 **macOS beta**。 |

---

## 4. 事实核查表

格式：`来源 | 文档说法 | 核查结论 | 正确做法`

### 4.3 各来源

| 来源 | 文档说法 | 核查结论 | 正确做法 |
|---|---|---|---|
| Homebrew 列出 | `brew info --installed --json=v2` | **属实**（本机 Homebrew 7.0.3 验证，顶层键 `formulae`/`casks`） | 必须 **两套字段映射**。cask 用 `$.casks[*].token`、`$.casks[*].installed`（字符串），不要 `installed[0].version` |
| Homebrew 依赖标记 | `installed_on_request` | **部分属实** | 仅 formulae。cask 无此字段；可用 `brew leaves --installed-on-request` / `brew list --installed-on-request` 辅助 |
| Homebrew 更新 | `brew outdated --json=v2`，字段 `installed_versions[0]` / `current_version` | **属实**（本机样本：`name`, `installed_versions`, `current_version`, `pinned`） | cask 默认 **不算** `auto_updates`/`version:latest`，普通人会觉得“Chrome 为什么不更新”。要则加 `--greedy` 并在 UI 标明 |
| Homebrew 刷新 | `brew update` TTL 6h | **属实**（outdated 用本地 cache） | 保持；失败时标明“数据可能过期” |
| Homebrew 搜索 | `brew search --desc`，正则 `^(?P<id>\S+): (?P<description>.*)$` | **命令属实，解析有误** | 输出含 `==> Formulae` / `==> Casks` 头。本机：`gdown: Google Drive ...`。必须跳过 `==>` 行，分段解析 |
| Homebrew 装/卸/升 | install/uninstall/upgrade | **属实** | `brew upgrade` **默认不加 cask**。upgrade_all 应 `brew upgrade --formula` 与 `--cask` 分开或明确勾选 |
| Homebrew 提权 | 不允许 root | **属实** | 检测 `id -u == 0` 直接拒绝 |
| npm 列出 | `npm ls -g --depth=0 --json` | **属实，结构写错** | 对象：`{ dependencies: { name: { version } } }`，不是 items 数组。npm 还会把警告打到 stderr（本机 npm 12 在 FORCE_COLOR 下甚至 warning） |
| npm 更新 | `npm outdated -g --json`，退出码 1 | **属实** | [npm/cli#1109](https://github.com/npm/cli/issues/1109)、[rfc#473](https://github.com/npm/rfcs/issues/473)。JSON 是 **按包名索引的对象**。`exit_ok=[0,1]` 必须把真正的网络错误（stderr + 非 JSON）分开 |
| npm 升级单包 | `npm install -g x@latest` | **属实**（比 `npm update -g` 正确） | 全局包被当成 caret range，`update -g` 经常不登 latest |
| npm 搜索 | `npm search --json` | **属实** | 数组，字段 `name/version/description/publisher`（本机验证）。注意 `--searchlimit` 默认 20 |
| npm 提权 | Linux 系统 node 需 sudo（auto） | **属实** | 探测 `npm root -g` 可写性；Mac Homebrew node 通常不需要 |
| pip 列出 | `pip list --format=json` | **属实** | 应用 `python3 -m pip`，不要假设 `pip` 在 PATH。多 Python（pyenv/官方/Homebrew）会列错环境 |
| pip 更新 | `pip list --outdated --format=json` | **属实** | 很慢；PEP 668 下仍可 list |
| pip 搜索 | 无，PyPI 关闭搜索 | **属实** | [pip search 文档](https://pip.pypa.io/en/stable/cli/pip_search/)：PyPI 不再支持 XML-RPC search |
| pip 依赖 | `pip list --not-required` | **属实** | 可与 `--format=json` 组合 |
| pip 提权 | 无 | **有误** | 系统 Python 只读；用户站点不需要。不要写“无” |
| pipx 列出 | `pipx list --json` | **属实**（新版本测试里也有 `--output json`） | 按 `pipx list --help` 探测 |
| pipx 更新 | 「无原生命令 → native:pypi_latest」 | **有误** | 官方文档已有 **`pipx list --outdated`**（[pipx manage apps](https://pipx.pypa.io/latest/how-to/manage-installed-apps.html)，2026-09）。不要自己打 PyPI |
| uv 列出 | `uv tool list` 文本 | **属实** | 同时有 `--show-paths` |
| uv 更新 | `uv tool list --outdated`「待核实是否存在」 | **属实（已存在）** | [uv CLI 文档](https://docs.astral.sh/uv/reference/cli/)：`--outdated` List outdated tools。2024 的 #9309 已关闭 |
| cargo 列出 | `cargo install --list` 文本 | **属实** | 格式 `crate vX.Y.Z:\n    binary`。没有 JSON |
| cargo 更新 | `native:crates_io_latest` + `install --force` | **属实但危险** | crates.io API 要 User-Agent。升级是重新编译；应提示 cargo-binstall。不要对 git 安装的 crate 查 crates.io |
| ollama 列出 | `GET localhost:11434/api/tags` 否则 `ollama list` | **属实** | `/api/tags` 含 `digest`/`size`/`modified_at`。[官方 api.md](https://raw.githubusercontent.com/ollama/ollama/main/docs/api.md)。`ollama list` 是空格对齐表（本机：`NAME ID SIZE MODIFIED`） |
| ollama 更新 | 比对本地 digest 与 registry.ollama.ai v2 manifest | **有误 / 会误报** | Ollama 自己用 digest 对比出过 false stale，因为写盘时重序列化 manifest：[ollama@e7ccc12](https://github.com/ollama/ollama/commit/e7ccc129ea45cd9383d91f0c233f324a95ad0572)。官方更新方式是 **`ollama pull`**（已存在则只在有新版本时下载）。没有稳定的“只检查不下载”公开 API。可退化为显示 `modified_at` + 按钮“重新拉取” |
| ollama 装/卸 | pull / rm | **属实** | 大模型 pull 需流式进度（API 是 NDJSON），不是“一行一行的 CLI 日志”那么简单 |
| winget 列出 | `winget list --disable-interactivity --accept-source-agreements` 表格 | **属实（命令），解析仍脆** | 截至 2026-09 **仍无** `winget list --output json`（[winget-cli#2032](https://github.com/microsoft/winget-cli/issues/2032) 仍开着）。v1.29 只改善重定向时不截断列宽，不是 JSON |
| winget 备选 | `Get-WinGetPackage \| ConvertTo-Json` | **属实，有前提** | 模块 [Microsoft.WinGet.Client](https://www.powershellgallery.com/packages/Microsoft.WinGet.Client) **需另装**。`Get-WinGetPackage` 返回对象，有 `IsUpdateAvailable`。UniGetUI 最终走 **COM** 不是这个模块 |
| winget 更新/装卸 | upgrade/install/uninstall `--id -e` | **属实** | 机器级安装要管理员；`--scope user` 与 `machine` 要分开。许多包无更新源（ARP） |
| winget 搜索 | `winget search` 表格 / `Find-WinGetPackage` | **属实** | 同表格问题 |
| scoop 列出 | `scoop export` JSON | **属实** | 结构是 `{ apps, buckets [, config] }`，不是包数组。apps 来自 `scoop list` 对象 |
| scoop 更新 | `scoop status` 表格 | **部分属实** | status 还打印 scoop/bucket 过期、失败安装、缺依赖，不只是 outdated。无官方 JSON；维护者建议 PowerShell 对象 + `ConvertTo-Json`（[Discussion #6537](https://github.com/ScoopInstaller/Scoop/discussions/6537)） |
| scoop 调用 | `powershell -NoProfile -Command` | **有误风险** | scoop 常是 profile 函数。`-NoProfile` 会找不到命令。应调 `scoop.cmd` shim（`~\scoop\shims\scoop.cmd`） |
| scoop 升级命令 | `scoop update` | **属实** | 不是 `upgrade` |
| choco 列出 | `choco list --limit-output` → `name\|version` | **属实** | `-r/--limit-output` |
| choco 更新 | `choco outdated --limit-output` | **属实** | Enhanced 退出码：**2 = 发现过时包**（[choco outdated 文档](https://docs.chocolatey.org/en-us/choco/commands/outdated/)）。`exit_ok` 不能只写 `[0,1]` |
| choco 提权 | **required，全程管理员** | **过度** | 默认装到 `C:\ProgramData\chocolatey` 的 **变更** 需要管理员；`list`/`outdated` 非管理员能跑，但会卡 20s 提示（[choco#2062](https://github.com/chocolatey/choco/issues/2062)）。应 `--fail-on-unfound` + 禁提示，list 不提权 |
| apt 列出 | `dpkg-query -W -f='${Package}\t${Version}\t${binary:Summary}\n'` + `apt-mark showmanual` | **属实** | Summary 可能空。用 `${binary:Package}` 更稳（多 arch） |
| apt 更新 | `apt-get -s upgrade` 或 `apt list --upgradable`，前置 `apt-get update` | **部分属实** | `apt-get -s upgrade` 是人类可读段落，不是包表。`apt list --upgradable` 好解析但 apt 自己警告 **CLI 不稳定**。优先 `apt-get -o Debug::NoLocking=true -s dist-upgrade` 再自己解析，或 python-apt（那就不是 TOML 了） |
| apt 装 | `apt-get install/remove/--only-upgrade -y` | **属实** | 正确是 `apt-get install --only-upgrade -y {id}`。不要用 `apt` |
| apt 搜索 | `apt-cache search` | **属实** | 输出 `name - description` |
| apt 提权 | required | **部分属实** | query/list 不需要；`update`/install/remove 需要 |
| dnf 列出 | `rpm -qa --queryformat` | **属实** | 给完整 format 字符串，如 `%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\n` |
| dnf 更新 | `dnf check-update` 退出码 100 | **dnf4 属实；dnf5 命令变了** | dnf5 文档是 **`dnf5 check-upgrade`**，同样 100，且有 **`--json`**。[dnf5 check-upgrade](https://dnf5.readthedocs.io/en/latest/commands/check-upgrade.8.html)。Fedora 41+ 的 `dnf` 就是 dnf5 |
| dnf 依赖 | `dnf repoquery --userinstalled` | **部分属实** | dnf5 把 group/unknown reason 也算进去，比 dnf4 宽（[dnf5#1684](https://github.com/rpm-software-management/dnf5/issues/1684)、F41 升级后“全部显示为手动安装”） |
| pacman 列出 | `-Q`、`-Qi` | **属实** | `-Qi` 每包一次很慢；用 `-Qe`/`-Qm`/`-Q --info` 批量或解析 `/var/lib/pacman/local` |
| pacman 更新检测 | `-Qu` 需先 `-Sy` | **做法有误（会制造部分升级）** | Arch 官方：不要 `pacman -Sy` 而不 `-u`。用 **`checkupdates`**（`pacman-contrib`）读临时 db。见 [Arch Wiki Partial upgrades](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported) |
| pacman 升级 | 只允许 `-Syu` | **属实，应坚持** | 安装新包用 `-Syu {id}`（先升级系统再装），不要 `-Sy {id}` |
| snap 列出 | `snap list` 表格 | **属实** | 列 Name Version Rev Tracking Publisher Notes |
| snap 更新 | `snap refresh --list` | **属实** | 无更新时可能打印 “All snaps up to date.” |
| snap 提权 | required | **属实** | 多数操作要 sudo；用户级 snap 少见 |
| flatpak 列出 | `flatpak list --app --columns=application,name,version,size,origin` | **属实** | 实际是 tab 分隔。[flatpak-list(1)](https://www.man7.org/linux/man-pages/man1/flatpak-list.1.html) |
| flatpak 更新 | `flatpak remote-ls --updates --columns=application,version` | **命令属实，version 列不可靠** | `--updates` 存在。version 可能空，直到 appstream 缓存填满（[flatpak#6115](https://github.com/flatpak/flatpak/issues/6115)）。更好：`flatpak update --no-deploy --no-deps` dry 或解析 `flatpak remote-ls --updates --columns=application,branch,version` |
| flatpak 提权 | 系统级 polkit 自弹 | **属实** | `--user` 安装不要 pkexec |

### 4.4 独立安装器（以文档示例 claude-code 为主）

| 来源 | 文档说法 | 核查结论 | 正确做法 |
|---|---|---|---|
| detect paths | `~/.local/bin/claude`、`%USERPROFILE%\.local\bin\claude.exe` | **macOS/Linux native 属实；Windows 路径不确定** | 本机确认为 symlink → `~/.local/share/claude/versions/<ver>`。Windows native 常用 `%USERPROFILE%\.local\bin\claude.exe` 或 WinGet 路径，必须 `where.exe claude` 多路径 |
| version | `claude --version` + `\d+\.\d+\.\d+` | **属实** | 实际输出 `2.1.273 (Claude Code)`，正则能抠出版本 |
| latest | `native:npm_registry` `@anthropic-ai/claude-code` | **部分有误** | native 通道与 npm dist-tag 可能不一致（latest vs stable）。应按安装来源查 |
| upgrade | `claude update` | **仅 native 属实** | brew：`brew upgrade --cask claude-code`；npm：`npm i -g @anthropic-ai/claude-code@latest`；winget：`winget upgrade Anthropic.ClaudeCode`。[官方 Getting Started](https://code.claude.com/docs/en/getting-started) |
| uninstall | 删除 `~/.local/bin/claude` 与 `~/.local/share/claude` | **不完整** | 跟官方卸载走；配置在 `~/.claude`。硬删 versions 目录可能留下死 symlink |
| 首批清单其余项 | agy/grok/uv/rustup/bun/deno/nvm/fnm/volta/pnpm/mise/pyenv/ollama/Docker | **未给出可验证命令** | 每条都要像 claude 一样写 detect/upgrade/uninstall。否则 §4.4 只是愿望清单。例如 bun：`bun upgrade`；deno：`deno upgrade`；rustup：`rustup self update` + `rustup update`；nvm **没有 Windows 官方版** |

### 专项核查（你点名的 7 项）

| 来源 | 文档说法 / 隐含假设 | 核查结论 | 正确做法 |
|---|---|---|---|
| Tauri 2 Win/Linux | 选 Tauri 因为轻，风险只写了“WebKitGTK 渲染差异” | **严重低估** | Linux：NVIDIA/DMABUF 白屏、Wayland 托盘、AppImage 在 Mesa 25 上无窗口、字体 weight。Windows：托盘菜单 z-order（[tray-icon#303](https://github.com/tauri-apps/tray-icon/issues/303)）、SmartScreen、WebView2 运行时。CVE-2026-42184（`is_local_url` 在 Windows 误判，CVSS 8.8）要求锁 Tauri ≥ 2.10.3 |
| UniGetUI 2026 | “2026 起三平台”，差异化仍按 Windows 竞品写 | **过时** | 2026.3.0 已三平台；Windows 用户在骂 Avalonia UI；macOS/Linux 是可用但不成熟的移植。口碑：功能仍被 MakeUseOf 等推荐，UI 在 GitHub discussion 剧烈差评 |
| 同类 GUI star | 只列 UniGetUI 与 mxcl PMM | **不完整** | Topgrade 4.5k（CLI 全能更新，含 ollama）；bauh 1.4k；Octopi ~870；pacseek 660。能拿到 star 的是 **深耕一个平台/一个发行版** 或 **一个命令更新一切**，不是 15×3 的空壳 |
| winget JSON | 备选 PowerShell JSON；未声称 CLI `--output json` | **CLI JSON 仍无；模块/COM 有** | 不要解析表格当主路径。Rust 调 COM（`windows` crate）或捆绑探测 WinGet.Client。模块缺失时明确报错，而不是 silently 解析乱码表 |
| `uv tool list --outdated` | 待核实 | **存在** | 直接写进适配器，parser=lines |
| ollama 官方检查更新 | native registry digest | **无稳定官方 check API** | UI：上次 pull 时间 + “Pull 以检查更新”。不要自己实现 registry 客户端当卖点 |
| pkexec GUI 提权 | “弹系统密码框，stdout 可流式读取” | **仅在有 agent 的会话里、对 CLI 大致属实** | 交付 polkit policy；检测 `ps` 里是否有 `polkit-gnome-authentication-agent`/`polkit-kde`/`lxqt-policykit`；没有则提示用户，不要卡死。替代：`sudo -A` + `SUDO_ASKPASS`（更糟）、systemd-run + polkit、或只支持 `--user` 范围的操作 |

---

## 5. 缺失项（必须补进文档）

1. **非目标清单**：AUR、Mac App Store (`mas`)、Windows Store 不可更新包、conda/mamba、nix、SDKMAN、JetBrains Toolbox、公司托管 winget 源。不写就会被 issue 淹没。
2. **多副本 / 多安装前缀**：brew Intel vs ARM、npm prefix、pyenv 的 pip、flatpak `--user` vs `--system`、pipx `--global`。
3. **锁与并发**：apt/dpkg 锁、pacman 锁、brew 已有进程。队列不能只管 Canager 内部。
4. **取消的真实语义**：`apt-get` 杀到一半会留下半安装；必须写“取消 = 杀进程，系统可能需要 `dpkg --configure -a`”。
5. **磁盘与时间预算**：`brew update`、`apt-get update`、`cargo install`、`ollama pull` 70GB。UI 要有 size 估计，ollama 不能和 `npm update` 同一个进度条语义。
6. **签名与公证的具体计划**：Apple ID、公证、Windows 证书预算。写“有 star 再签”等于“永远不签”。
7. **崩溃/诊断 telemetry（opt-in）**：否则 Windows 用户发 “error” 你无法复现。
8. **适配器 schema 正式语法**：多 list 块、object-map JSON、`exit_ok`、timeout、`stdin`（choco 确认）、编码（winget UTF-16/控制台代码页）。
9. **Windows 打包**：用户是否必须 WebView2？arm64 Windows 做不做？（UniGetUI 做了。）
10. **Linux 依赖**：webkit2gtk-4.1、libayatana-appindicator 托盘、polkit。Fedora vs Ubuntu 包名不同。
11. **许可证与源输出**：把 winget/apt 原始日志塞进 GitHub issue 可能含用户名/路径；脱敏规则要写死。
12. **品牌与用户**：在「普通人」和「AI CLI + cargo + ollama」里选一个。现在两个都要，两个都做不成。

---

## 6. 一句话总评 + 最该先改的 3 件事

**总评：这是一份把 UniGetUI 的全平台野心、Topgrade 的覆盖面、mxcl PMM 的独立安装器，叠在 Tauri 空壳上、准备用 AI 一周写完的愿望清单；2026 年的事实是这些事已经分别有人做了，且更强的团队仍做不满。**

**先改这三件：**

1. **砍范围**：v0.1 只做 macOS + brew/npm/pipx/uv/独立安装器。三平台同日从约束里删除，否则评审后面的架构都是空谈。
2. **重写竞争与发布**：按 UniGetUI 2026.3.0 的真实能力改差异化；签名/公证作为首发门槛，或放弃“普通人”叙事。
3. **拆掉“TOML 万能适配器”神话**：winget/brew-cask/npm/ollama/dnf5/pacman 按原生适配器设计；把 §4.3 里已核实的错误命令、退出码、digest 方案改掉，再谈写代码。
