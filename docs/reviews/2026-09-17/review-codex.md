**结论：不建议按这份草案直接开工。** Tauri 2 可以承担这个产品，但当前设计把包管理器之间的事务、身份、权限和更新语义差异，误当成了输出格式差异。照此实现，最可能的结果是：界面完成得很快，随后在 Windows 提权、多环境识别、取消操作和系统包升级上大幅重写。

以下保留“三平台同日首发、15 个来源全覆盖”的约束。核查依据为本次查阅的官方文档及上游源码；**没有执行真实安装、卸载或三平台实机验证**。在线文档中的新能力也不等于用户现有版本支持，适配器必须声明最低版本。

## 1. 致命问题

### F1. “几十行 TOML 加一个管理器”不能作为架构成立的前提

**涉及章节：§3、§4.1—4.4、§12、§13。**

**依据：** JSONPath、正则和表格解析只解决“如何读取输出”，解决不了以下行为：

- apt：安装状态过滤、多架构身份、事务模拟、配置文件冲突。
- winget：已安装软件与仓库条目的关联、安装器类型、作用域、重启结果。
- pip/pipx/uv：解释器、环境、安装约束、私有索引、Git 来源。
- Flatpak：用户/系统安装、remote、架构、branch、commit。
- Ollama：服务端状态、流式 API、自建模型、共享 blob。
- 独立安装器：安装归属、自更新渠道、服务停启、文件删除边界。

这些不是少量解析特例。不断增加 `native:<name>`，最终会得到“一套没有类型系统的 TOML 编程语言，加大量隐式 Rust 策略”。

规范本身也尚未成立：

- `[install] command = [...]`、`[latest] strategy = ... package = ...` **不是合法 TOML**。本次使用 `tomllib` 验证，均报 `Expected newline or end of document after a statement`。
- 同级 `[list]` 不能靠“第二个 list 块”重复声明；需要数组表或命名集合。
- 没有定义字段类型、缺失值、多个结果、对象键作为包名、集合连接、解析部分失败等语义。

**建议改法：**

1. 把“适配器是数据不是代码”改成：**适配器是有类型的 Rust 行为实现，TOML 承载元数据和简单命令映射。**
2. Rust 接口至少分为 `detect / inventory / candidates / plan / execute / reconcile`；不是每个来源都必须使用同一种执行方式。
3. 声明能力：是否支持搜索、事务预览、逐包升级、原生取消、后台检查、交互安装。
4. 在冻结接口前完成四个纵向样板：**brew cask、winget 机器级安装、apt 配置冲突、Flatpak 双安装范围**。
5. 15 个来源仍全部交付，但不要让后面 11 个依赖一个只被 brew/npm/pip 验证过的抽象。

---

### F2. 数据模型没有“安装实例”，会更新或卸载错误对象

**涉及章节：§4.1、§4.3—4.5、§5、§6。**

**依据：** `Package { source_id, id, version }` 无法唯一标识操作对象。

例如：

- 同一机器可以有两个 Homebrew 前缀。
- 同一个 npm 包可能存在于系统 Node、nvm Node、Volta 管理的环境中。
- 同一个 Python 包可以存在于多个解释器或 venv。
- Flatpak 同一个 application ID 可以同时存在于用户级和系统级，并有不同 branch。
- `winget list` 能识别并非 winget 安装的软件，因此可能与 Chocolatey、独立安装器重复。
- 发现 `~/.local/bin/claude`，却执行 PATH 上的 `claude update`，可能更新另一份安装。

最后一种错误不需要恶意输入即可发生。

**建议改法：**

拆出以下身份：

```text
ManagerInstance
  = 管理器类型 + 可执行文件绝对路径 + root/prefix + 用户/作用域 + 环境

InstalledArtifact
  = 实例 ID + 原生包键 + 架构/branch/安装范围等限定信息

OwnershipEvidence
  = 归属依据 + 可信度 + 可用操作
```

检测、列出和执行必须绑定同一个实例；UI 折叠重复展示可以，但不能合并操作身份。归属不确定时保留为“不确定”，禁止自动选一个管理器执行卸载。

---

### F3. 三平台提权方案没有闭环，macOS 的前提直接错误

**涉及章节：§4.2—4.3、§6、§10。**

**依据：**

- **brew 不应整体作为 root 运行，不等于 brew 的所有安装流程都不需要管理员权限。** Cask 的 `.pkg` 和安装脚本可以要求 `sudo`。[Homebrew Cask 官方说明](https://docs.brew.sh/Cask-Cookbook)
- npm/pip 是否可写取决于安装实例和目标位置，不能由操作系统推断。
- `pkexec` 需要认证代理；并非所有 Linux 桌面会话都有可用弹窗。它还会清理环境，取消认证和授权失败有不同返回码。[pkexec 手册](https://polkit.pages.freedesktop.org/polkit/pkexec.1.html)
- `Start-Process -Verb RunAs` 只解决启动提权进程，未解决结构化结果、可靠退出码、取消、临时文件完整性和重启状态。`-ArgumentList` 还会重新组合参数，不能继承 Rust 参数数组的安全保证。[PowerShell 官方文档](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/start-process?view=powershell-7.5)
- “目录可写”不能推出安装器是否需要安装服务、驱动或写注册表。

**建议改法：**

- 权限策略放到**具体操作和安装实例**上，不放在整个 adapter 顶层。
- Linux：验证 `pkexec`、会话代理、环境传递和取消认证；后台任务不得突然弹提权框。
- Windows：首版就实现**每次操作启动、结束即退出的最小提权 broker**。可以延后常驻 helper，不能延后可靠通信协议。
- broker 只接受受限的操作请求，不接受任意 shell 或任意可执行路径；校验调用者、参数、日志目录和结果文件权限。
- macOS：brew 保持普通用户运行；需要交互授权的 cask 必须有经过验证的交互路径，例如受控终端接管，并在完成后重新核对状态。

“永不接触密码”是正确目标，但不是实现方案。

---

### F4. “取消＝杀进程树”会造成假取消，甚至留下损坏状态

**涉及章节：§3 runner、§5 Operation、§6、§11。**

**依据：**

- 杀掉包管理器可能发生在解包、替换文件、执行维护脚本期间；这不提供事务回滚。
- snap 的实际事务在 snapd 中；安装器可能转交系统服务；CLI 消失不等于工作停止。
- 普通用户未必能杀死提权后的进程。
- Windows 经 UAC 启动的进程不能假定自动加入原进程的 Job Object。
- Unix 子进程可以创建新会话，单个进程组不等于完整后代集合。
- Tokio 的任务取消、`Child` 被丢弃和进程终止是不同事件；还必须回收子进程。[Tokio 进程文档](https://docs.rs/tokio/latest/tokio/process/index.html)

当前状态机只有 `cancelled`，无法表达“用户请求停止，但事务仍在进行”。

**建议改法：**

- 区分 `cancel_requested / cancelling / detached / interrupted / needs_reconciliation`。
- 每个操作声明取消策略：安全终止、调用原生取消接口、仅停止等待、提交阶段不可取消。
- 超时和取消不能自动触发重复安装。
- 崩溃重启后重新查询管理器状态；无法确认结果时显示“结果未确认”，不能伪造失败或成功。
- Job Object 与 Unix 进程组作为进程清理机制使用，不能作为事务取消保证。

---

### F5. Arch 的后台查更新设计正在制造自己声称禁止的风险

**涉及章节：§4.3 pacman、§6、§8、§14。**

**依据：** `pacman -Qu` 查询现有同步数据库本身没有问题；危险的是为了查更新先执行 `pacman -Sy`。它刷新系统仓库数据库却不升级系统，此后用户在 Banager 或终端安装某个包，就可能形成不受支持的部分升级。

只禁止 Banager 的“单包升级”按钮，防不住这个场景。Arch 官方推荐使用独立检查数据库的 `checkupdates`。[Arch 维护指南](https://wiki.archlinux.org/title/System_maintenance)、[checkupdates 手册](https://man.archlinux.org/man/checkupdates.8)

**建议改法：**

- 后台查更新使用 `pacman-contrib` 的 `checkupdates`；缺失时明确说明，不偷偷运行 `-Sy`。
- 处理 `checkupdates` 的 0／2／1：有更新／无更新／失败。
- 安装新包也必须考虑数据库与系统一致性，必要时作为 `-Syu <package>` 的完整事务呈现。
- 预览和确认必须写清它可能升级整个系统，不能伪装成只更新一个应用。

---

### F6. 参数数组没有覆盖真正的信任边界

**涉及章节：§4.2、§4.4—4.5、§6。**

**依据：**

参数数组能防一类 shell 注入，但不能防：

- `powershell -Command` 再次解释拼接进去的字符串。
- 包管理器对参数值的二次解释，如路径、URL、仓库标识、APT 包名末尾的操作符。
- 工作目录中的 `.npmrc`、Cargo 配置或 Python 配置改变实际下载源。
- PATH 中同名程序替换被检测到的程序。
- 一个 TOML 适配器直接指定任意程序，或 `remove` 删除任意路径。
- 包描述或日志中的 HTML 进入 WebView 后调用高权限 IPC。

因此“数据适配器”实际上是一个**可以执行代码和删除文件的信任对象**。

**建议改法：**

- 首版仅加载随应用发布、经过审查的适配器；远程精选清单不得下发命令和删除规则。
- IPC 只接收已知操作与对象 ID，由 Rust 重新解析并验证；不要提供通用 `exec(command, args)`。
- 独立规定包 ID、搜索词、版本约束、路径的验证规则；不能共用 `id_pattern`。
- 固定程序绝对路径、工作目录和明确的环境策略。
- 删除规则必须绑定安装归属证据，明确“删除链接”与“删除目标目录”，拒绝越界和符号链接逃逸。
- 外部描述与日志按纯文本显示；补写 CSP、远程导航和链接协议限制。

---

### F7. “显式安装”“依赖”“能不能删”被混成一个危险结论

**涉及章节：§1、§4.1—4.3、§5、§7。**

**依据：**

- `pip list --not-required` 表示没有其他已安装分发包依赖它，不表示用户显式安装了它。
- Homebrew 的安装原因不说明用户脚本是否仍在使用该程序。
- apt 的 manual 包可能是系统元包或基础组件。
- `safe_to_remove` 放在精选目录里，无法获知本机依赖、服务、脚本和用户数据。

一个被目录标记为“可以删除”的 Python、Node、数据库或模型，完全可能正在被用户业务使用。

**建议改法：**

- `explicit: bool` 改为安装原因枚举，并允许 `unknown`。
- 删除目录级 `safe_to_remove: yes` 保证；目录只说明用途和典型影响。
- 卸载前展示管理器实际计划删除的对象，以及是否影响依赖、服务和数据。
- 把“没有发现管理器记录的反向依赖”与“可以安全删除”明确分开。

## 2. 重大问题

### M1. 更新候选不是“公共注册表上的最新版本”

**涉及章节：§4.3—4.4、§5。**

**依据：** PyPI/crates.io 的最大版本可能不兼容当前解释器或工具链；包可能来自 Git、路径、私有源，或者锁定稳定渠道。Ollama 的 digest 变化也不等于新版本号。把这些统一成 `latest?` 会产生无法执行或违反用户约束的“更新”。

**建议改法：** 使用 `UpdateCandidate`，记录来源、渠道、目标版本或 digest、约束、可执行性及检查错误。优先使用管理器自身的解析结果；公共 API 查询只适用于已确认归属的公共源包。

### M2. 按来源串行不能防止跨来源冲突

**涉及章节：§3 ops、§6。**

**依据：** brew 升级 Node 时，npm 可以正在安装全局包；rustup 更新工具链时，cargo 可以正在编译；winget 和 choco 可能操作同一应用。用户终端和系统自动更新也完全不受 Banager 队列控制。

**建议改法：** 用资源锁描述冲突，例如安装前缀、Python 环境、系统包数据库、Windows 安装器及具体应用。先保守串行化写操作，再逐项开放确定独立的并行路径。外部锁冲突应进入等待或可重试状态，不能删除管理器锁文件。

### M3. “全部更新”没有明确边界，会绕过忽略列表

**涉及章节：§4.1、§5、§6、§7。**

**依据：** 界面可能展示用户选择的 8 个更新，最后执行 `brew upgrade` 却更新全部符合条件的软件。应用自己的 `ignored_updates[]` 不会自动传给管理器。Arch 全系统升级更无法简单跳过界面忽略项。

**建议改法：** 在设计中区分“更新选中项”和“执行来源原生全量升级”；预览中显示真实影响范围。明确遵守 pin/hold/channel 的规则；不能为了实现忽略列表制造 Arch 部分升级。

### M4. 安装后的验证缺失，退出码被当成了产品事实

**涉及章节：§3 数据流、§4.2、§5—6。**

**依据：** 命令可能成功但没有改变任何内容，也可能要求重启、部分成功、转交安装器或留下待完成状态。`exit_ok` 只能描述进程结果，不能证明应用已经安装或更新。

**建议改法：** 执行结束进入 `verifying`，核对目标实例的安装状态和版本。结果至少区分成功、无变更、部分成功、需重启、待确认、失败。Windows HRESULT 保留原始 32 位值和十六进制表示。

### M5. 现有 CI 矩阵验证的是编译能力，不是 15 个适配器

**涉及章节：§10—11、§13—14。**

**依据：**

- Ubuntu runner 不等于 Fedora、Arch 或带桌面认证代理的 Linux。
- 容器里跑 apt/dnf/pacman 可以验证部分命令，不能验证 polkit、snapd、桌面通知和真实系统升级。
- GitHub Windows runner 的身份和预装环境不能替代干净标准用户的 UAC 路径。
- 从最新版安装 `hello` 再运行 outdated，通常只测到“没有更新”，根本没验证升级。
- 空壳打包成功不能证明后续 MSI、WebView2、提权和升级安装可行。

**建议改法：**

保留三平台同日首发，但建立发布阻断矩阵：

- 每个来源：空库存、非空库存、有更新、无更新、安装、卸载、升级、失败。
- 每个平台：普通用户、需要授权、拒绝授权、外部锁、断网、崩溃重启。
- 用自建微型测试包仓库提供 v1→v2 和故障版本，避免依赖公共包“恰好有更新”。
- Fedora、Arch 使用独立环境；snap/polkit/UAC 使用有桌面的 VM 或测试机器。
- 社区测试者必须在首发前进入名单并交付记录，不能作为尚未落实的风险对策。

### M6. 未签名首发与“普通人友好”存在直接商业目标冲突

**涉及章节：§0、§1、§10、§12。**

**依据：** 一个要求管理员权限、负责卸载软件的新项目，却首先要求用户绕过系统保护，会削弱最需要建立的信任。README 绕过教程不等于用户愿意执行。它不会必然导致技术失败，但会直接威胁普通用户转化和传播目标。

**建议改法：** 我建议修改这条约束：至少把 macOS Developer ID 签名与公证提前到公开推广前。若仍坚持不签名，应诚实定位为技术预览，提供校验和、构建来源和明确的支持边界；不要把“等有 star 再建立分发信任”作为计划。

另外，**Tauri updater 的更新签名与操作系统代码签名是两件事**，前者不能因为没购买后者就推迟。[Tauri updater 文档](https://v2.tauri.app/plugin/updater/)

### M7. 体积目标可以争取，但现在不能成为产品事实

**涉及章节：§1、§2、§10、§12。**

**依据：**

- Tauri 使用系统 WebView，不等于用户完整安装成本只有应用包大小。
- Windows 是否携带 WebView2 离线运行时，会显著改变体积。
- universal macOS 包包含两种架构。
- `.deb` 依赖系统库与 AppImage 携带依赖不能混为一谈。
- “10 MB 对 50 MB”没有固定版本、架构和包格式，无法形成可复现比较。[Tauri Windows 安装选项](https://v2.tauri.app/distribute/windows-installer/)

**建议改法：** 将三个体积上限先列为预算，在第一个包含实际 Rust 依赖和完整前端的 release 构建后测量。README 分别报告下载大小、安装大小、运行时依赖、冷启动和空闲内存；只比较同口径发布物。

### M8. 最低系统版本缺失，会让“原生质感”变成白屏或错版

**涉及章节：§2、§7、§10、§14。**

**依据：** Tailwind v4 依赖现代浏览器功能，官方兼容基线包含 Safari 16.4、Chrome 111、Firefox 128。使用系统 WebView 就必须承担系统浏览器版本差异，“避免高级 CSS”无法绕过框架本身的要求。[Tailwind 兼容说明](https://tailwindcss.com/docs/compatibility)

**建议改法：** 固定最低 macOS、Windows、WebView2、Linux 发行版及 WebKitGTK 版本；在最低版本运行构建产物。若需要支持更老环境，先调整 CSS 栈，不能等视觉验收时返工。

### M9. 自由搜索与精选商店没有可落地的安装决策

**涉及章节：§1、§5、§7。**

**依据：** Linux 只有一个 `{source,id}` 映射无法同时服务 apt/dnf/pacman 用户；同一软件来自不同来源时，版本、权限、更新渠道和安装范围可能完全不同。搜索结果也不能按名字去重。

**建议改法：** `CatalogEntry.install` 改为带适用条件的候选列表，标注发行版、架构、已配置源和安装范围；优先匹配现有安装。安装按钮必须绑定明确候选，来源与发布者在简单模式也应可见。

### M10. 差异化陈述过度，容易被一个反例击穿

**涉及章节：§0—1、§12。**

**依据：** 已支持 npm/brew 的竞品天然能管理部分 AI CLI 的包，因此“竞品不做 AI CLI”不能直接成立。当前 UniGetUI 官方仓库也确实已有跨平台版本和大量来源支持；不能用过时的技术印象作对比。[UniGetUI 官方仓库](https://github.com/Devolutions/UniGetUI)

**建议改法：** 将竞争点改成可演示任务，例如“识别同一 CLI 的多份安装并正确选择更新方式”“模型与工具安装统一查看”。差异表固定竞品版本并附证据；删除“代码与 issue 中均无”这种难以证明的绝对陈述。

## 3. 次要问题

### N1. 一开始就承诺通用表格解析器，收益很小

**涉及章节：§4.2—4.3。**

**依据：** 终端显示宽度不是字符串字节长度；截断过的 ID 无法靠 Unicode 对齐算法恢复。

**建议改法：** 表格解析必须针对来源和版本编写；身份字段截断时拒绝生成可执行对象。不要把“解析出几列”当成“获取了完整身份”。

### N2. 逐行日志没有背压与容量控制

**涉及章节：§3、§6。**

**依据：** 进度输出可能只有 `\r`，构建日志可能极大，stderr 不持续读取还可能阻塞进程。500 条操作记录也不能限制磁盘占用。

**建议改法：** stdout/stderr 独立持续读取，支持非换行片段；UI 批量推送、有界缓存、日志按字节轮转。日志写盘失败不能阻塞包管理事务。

### N3. 来源不明扫描的归属启发式太自信

**涉及章节：§4.5。**

**依据：** `~/.cargo/bin` 可以包含 rustup shim、手工文件及其他安装器产物；目录前缀不是所有权证据。Windows 扫描还缺少递归深度、reparse point 和可执行类型定义。

**建议改法：** 前缀匹配只生成候选归属，再与管理器库存或安装记录核对；设置扫描深度、文件数、时间预算，并处理断链和不可读路径。

### N4. 后台本地化不能依赖前端曾经启动

**涉及章节：§8—9。**

**依据：** 自启动到托盘时可能没有打开前端，Rust 侧拿不到注入的翻译。

**建议改法：** 同一份翻译资源在构建时供两端读取；Rust 自己读取持久化语言设置，前端负责通知语言变更。

### N5. 缓存刷新可能把新状态覆盖成旧状态

**涉及章节：§3、§5、§8。**

**依据：** 后台 list 在卸载前启动、卸载后才完成，可能把已删除包重新写回缓存。

**建议改法：** 为来源实例维护刷新代次；写操作后使旧快照失效，只接受最新代次。解析失败保留旧数据并标记陈旧，不能写成空库存。

### N6. “一键生成 issue”没有明确隐私边界

**涉及章节：§6、§12。**

**依据：** 命令、日志和配置可能包含用户名、私有仓库 URL、访问令牌或公司软件信息。

**建议改法：** 先生成本地可预览报告，默认删去环境变量、凭据和原始路径；用户明确提交后才上传，支持只复制报告。

## 4. 事实核查表

“属实”表示命令或能力有依据，**不表示草案中的集成策略完整**。未列出的退出码不能默认视为 0/1 二选一；版本相关能力应在检测阶段验证。

### 4.3：语言工具与 Homebrew、Ollama

| 来源 | 文档说法 | 核查结论（属实/有误/不确定） | 正确做法 |
|---|---|---|---|
| [Homebrew](https://docs.brew.sh/Manpage) | `brew info --installed --json=v2` 列出已装 | 属实 | 同时处理 formulae 与 casks；保留类型、完整名称和安装实例，不能只取统一的 `name`。 |
| Homebrew | `brew outdated --json=v2` 查更新 | 属实 | 明确 pinned、`auto_updates`、`version :latest` cask 的策略；默认结果不等于所有软件的全部更新。 |
| Homebrew | `brew install/uninstall/upgrade` | 属实 | 补包名并绑定实例；需要消歧时显式指定 `--formula`/`--cask`。 |
| Homebrew | `brew search --desc` | 属实 | 必须提供查询词；输出含分组等非结果行，当前单行正则不是完整协议。 |
| Homebrew | `installed_on_request=false` 即依赖库 | 有误 | 这是安装原因，不是软件类别或删除安全性；且不能照搬至 cask。 |
| [Homebrew Cask](https://docs.brew.sh/Cask-Cookbook) | “不允许 root”，因此 macOS 不需要提权 | 有误 | 普通 brew 操作不应整体提权；cask 的 pkg/脚本仍可能要求管理员授权。 |
| [npm ls](https://docs.npmjs.com/cli/v11/commands/npm-ls) | `npm ls -g --depth=0 --json` | 属实 | 读取该 npm 实例的全局顶层包；检查 JSON 内问题与退出状态，不能只看是否有包数组。 |
| [npm outdated](https://docs.npmjs.com/cli/v11/commands/npm-outdated/) | `npm outdated -g --json`，有更新返回 1 | 属实 | 1 也不能无条件吞掉：验证输出是否为正常结果；保留 current/wanted/latest 差异。[上游退出码讨论](https://github.com/npm/cli/issues/3844) |
| npm | `npm install -g`、`uninstall -g`、`install -g x@latest` | 属实 | `@latest` 表示 latest dist-tag，可能跨大版本或偏离原渠道，不能默认为所有包的升级策略。 |
| [npm search](https://docs.npmjs.com/cli/v11/commands/npm-search) | `npm search --json` | 属实 | 加查询词；尊重配置 registry，补搜索限流和取消。 |
| npm | 全部视为显式 | 属实 | 仅可作为“展示全局顶层项”的产品策略，不是完整安装历史证据。 |
| npm | Linux 系统 Node 需 sudo，warn 会夹杂输出 | 有误 | 权限由 prefix 决定，macOS 也可能遇到；通常应分开读取 stderr 与 JSON stdout，不能先合流再删 warn。 |
| [pip list](https://pip.pypa.io/en/stable/cli/pip_list/) | `pip list --format=json` | 属实 | 用已识别解释器的 `python -m pip`，结果只覆盖对应环境。 |
| pip | `pip list --outdated --format=json` | 属实 | 返回安装版本及候选版本；不应套用 npm 的“有更新返回 1”。 |
| pip | `pip install`、`uninstall -y`、`install -U` | 属实 | 绑定解释器和安装范围；升级可能影响依赖，`-U` 不提供环境级无损保证。 |
| [pip search](https://pip.pypa.io/en/stable/cli/pip_search/) | PyPI 已关闭搜索 | 有误 | 停用的是 PyPI 的 XML-RPC 搜索接口；网站搜索仍存在。不要把网页抓取当稳定 API。 |
| pip | `pip list --not-required` 识别显式安装 | 有误 | 它识别没有其他已安装包依赖的包，不记录用户安装意图。 |
| pip | 提权“无”；PEP 668 会拒绝安装 | 有误 | PEP 668 限制属实，但是否需权限取决于目标环境；不可由 pip 类型统一写死“无”。 |
| [pipx](https://pipx.pypa.io/stable/reference/cli.html) | `pipx list --json` | 属实 | JSON 有版本/格式差异，保留主包、注入包、环境和原始安装规格。 |
| [pipx 更新日志](https://pipx.pypa.io/latest/changelog.html) | 没有原生查更新命令 | 有误 | 官方记录 `pipx 1.16.0` 于 2026-07-15 加入 `list --outdated`；按本机版本探测后使用，旧版才走明确的兼容路径。 |
| pipx | `pipx install/uninstall/upgrade`，全显式、无需提权 | 有误 | 命令存在；主工具可按显式展示，注入包不能混同。用户模式通常不需提权，`--global` 等范围另行处理。 |
| [uv CLI](https://docs.astral.sh/uv/reference/cli/) | `uv tool list` 为文本 | 属实 | 针对工具条目、附属可执行文件和可选路径输出分别解析。 |
| uv | `uv tool list --outdated` 待核实 | 属实 | 当前官方参考及 CLI 定义均存在；补支持版本或 help 能力探测，不能假定旧版本支持。 |
| uv | `uv tool install/uninstall/upgrade` | 属实 | 保留原安装约束、extras、Python 和索引；工具与工具环境中的依赖分开展示。 |
| uv | 无搜索、全显式、无需提权 | 属实 | 可作为首版用户级工具范围；不是对 uv 所有子功能的描述，定制目录仍可能不可写。 |
| [Cargo](https://doc.rust-lang.org/cargo/commands/cargo-install.html) | `cargo install --list` | 属实 | 文本包含包与其二进制，可能带 Git 来源；按 Cargo 安装根目录区分实例。 |
| Cargo | crates.io latest 即可查更新 | 有误 | 仅适用于确认来自 crates.io 的包；Git、路径、替代 registry、版本约束和 Rust 版本要求必须保留。 |
| Cargo | `cargo install/uninstall/install --force` | 有误 | 子命令存在，但 `--force` 不是保留原配置的通用升级算法；必须带包名并重建来源、features、profile、root 等。 |
| Cargo | 无搜索（v1） | 属实 | 作为产品取舍成立；Cargo 自身有 `cargo search`，不能写成管理器不支持搜索。 |
| Cargo | 全显式、无需提权 | 属实 | 限于普通用户安装根目录；自定义 `--root` 仍需权限检查。 |
| [Ollama API](https://docs.ollama.com/api/tags) | `GET localhost:11434/api/tags` 返回 digest/size | 属实 | 使用完整 HTTP 地址，尊重配置端点；记录管理的是哪一个服务实例。 |
| [Ollama CLI 源码](https://raw.githubusercontent.com/ollama/ollama/main/cmd/cmd.go) | 无服务时回退 `ollama list` | 有误 | `ollama list` 本身通过客户端调用服务。CLI 可能尝试启动服务，但不是无需服务的离线库存读取。 |
| Ollama | 本地 manifest digest 与 registry v2 manifest 比较 | 不确定 | 对可确认来自相同远端的模型可实验；自建、复制、私有模型及 tag 映射不能统一推断，必须记录来源并允许“不可检查”。 |
| Ollama | `ollama pull/rm/pull` 实现装/卸/升 | 属实 | pull 是同步指定模型引用，不是 SemVer 升级；CLI 权限与服务端磁盘写入权限分开处理。 |
| Ollama | 精选清单＋任意名字 pull 作为搜索 | 有误 | 这是按引用安装，不是搜索；UI 应明确区分，并提前处理不存在的模型、认证及磁盘不足。 |

### 4.3：Windows 来源

| 来源 | 文档说法 | 核查结论（属实/有误/不确定） | 正确做法 |
|---|---|---|---|
| [winget list](https://learn.microsoft.com/en-us/windows/package-manager/winget/list) | `winget list --disable-interactivity --accept-source-agreements` | 属实 | 文本不是稳定库存 API；缺失或截断的 ID 不得用于后续操作。注意它也显示非 winget 安装的软件。 |
| [WinGet PowerShell](https://github.com/microsoft/winget-cli/blob/master/src/PowerShell/Help/Microsoft.WinGet.Client/Update-WinGetPackage.md) | `Get-WinGetPackage`、按 `IsUpdateAvailable` 过滤 | 属实 | 先检测模块可用性及版本；固定选取字段、JSON 深度、数组形状和输出编码，设置终止式错误处理。 |
| winget | `winget upgrade` 查更新 | 属实 | 无包参数时列候选；不能假定其结果包含 pinned、未知版本等所有项目。 |
| [winget install](https://learn.microsoft.com/en-us/windows/package-manager/winget/install) | `install/uninstall/upgrade --id x -e` | 属实 | `-e` 是精确匹配；仍需绑定 source、scope 等，并分别定义协议接受与安装器交互策略。 |
| winget | `winget search`／`Find-WinGetPackage` | 属实 | 查询仓库候选；保留来源，不按显示名称合并。 |
| winget | 机器级安装需 UAC（auto） | 有误 | 常见但不能仅由“机器级”或目录可写性决定；具体安装器、策略及现有权限决定授权路径。 |
| [WinGet 返回码](https://github.com/microsoft/winget-cli/blob/master/doc/windows/package-manager/winget/returnCodes.md?plain=1) | 未给退出码合同 | 不确定 | 按返回码表映射无候选、无适用升级、失败、重启等；保留原始 HRESULT，不简化为非零失败。 |
| UniGetUI | 已放弃 CLI 解析，改用 COM API | 不确定 | 本次未做该实现的完整版本化源码审计；在文档中附具体版本和代码位置，不能靠这句话证明 Banager 的接口选择。 |
| [Scoop export 源码](https://raw.githubusercontent.com/ScoopInstaller/Scoop/master/libexec/scoop-export.ps1) | `scoop export` 为 JSON | 属实 | 输出包含 apps、buckets；可作库存来源，但 schema 必须绑定版本并保留安装范围。 |
| [Scoop status 源码](https://raw.githubusercontent.com/ScoopInstaller/Scoop/master/libexec/scoop-status.ps1) | `scoop status` 是更新表格 | 有误 | 它还报告失败、废弃、移除、缺依赖及 bucket 状态，不能将每条记录都当可更新。 |
| Scoop | `scoop install/uninstall/update` | 属实 | 带应用参数；裸 `scoop update` 与更新指定应用语义不同，通配全量更新也需单独处理。 |
| Scoop | `scoop search` | 属实 | 结果受已配置 bucket 和索引状态影响，不等于全网软件搜索。 |
| Scoop | 必须 `powershell -NoProfile -Command` | 有误 | 需要合适的 PowerShell 执行环境，但不必动态拼接 `-Command`；可用固定脚本入口及独立参数。 |
| Scoop | 无需提权 | 有误 | 普通用户安装通常如此；global 安装和特殊操作需要另外处理。 |
| [Chocolatey list](https://docs.chocolatey.org/en-us/choco/commands/list/) | `choco list --limit-output` 为本地 `name\|version` | 属实 | 这是 Chocolatey 2.x 语义；旧版本 list 行为不同，必须声明版本下限。 |
| [Chocolatey outdated](https://docs.chocolatey.org/en-us/choco/commands/outdated/) | `choco outdated --limit-output` | 属实 | 解析安装版本、候选版本及 pinned 等字段；增强退出码启用时，2 表示发现过期包。 |
| [Chocolatey upgrade](https://docs.chocolatey.org/en-us/choco/commands/upgrade/) | `install/uninstall/upgrade -y` | 属实 | `-y` 确认提示；需处理包安装器返回码，3010/1641 等不是普通失败。 |
| Chocolatey | `choco search --limit-output` | 属实 | 查询配置的软件源；补来源身份、认证和限流处理。 |
| [Chocolatey 安装说明](https://docs.chocolatey.org/en-us/choco/setup/) | 全程管理员 | 有误 | 查询操作不应全程提权，且存在非管理员安装模式；通常需要提权的是标准系统范围写操作。 |

### 4.3：Linux 系统来源

| 来源 | 文档说法 | 核查结论（属实/有误/不确定） | 正确做法 |
|---|---|---|---|
| [dpkg-query](https://manpages.debian.org/bookworm/dpkg/dpkg-query.1.en.html) | `-W -f='${Package}\t${Version}\t${binary:Summary}\n'` 列出已装 | 有误 | 字段有效，但数据库记录不全是当前已安装；加入安装状态并过滤，使用能保留多架构身份的字段。 |
| dpkg-query | 上述引号直接用于参数数组 | 有误 | 单引号是 shell 写法，Rust argv 中不得保留外层引号；格式表达式作为单个原始参数传入。 |
| apt | `apt-mark showmanual` | 属实 | 表示 manual 标记，不表示普通应用或可安全删除；与库存按规范身份关联。 |
| [apt-get](https://manpages.debian.org/bookworm/apt/apt-get.8.en.html) | `apt-get -s upgrade` 或 `apt list --upgradable` | 有误 | 两者不等价：前者是 upgrade 事务模拟，可能保留某些包；后者是候选列表，且 apt 面向交互。需分别定义“有新版本”和“当前事务可升级”。 |
| apt | 前置 `apt-get update` 需要 sudo | 属实 | 标准系统数据库需要权限；普通后台检查应读取缓存并标记时效，刷新失败不能更新成功时间。 |
| apt | `apt-get install/remove/--only-upgrade -y` | 有误 | `--only-upgrade` 不是子命令；应为 `apt-get install --only-upgrade -y <pkg>`。 |
| apt | `-y` 足以无人值守 | 有误 | 不保证解决 debconf、dpkg 配置冲突和维护脚本交互；必须制定明确的交互策略。 |
| apt | `apt-cache search` | 属实 | 是文本搜索；补查询参数，按包身份解析，不能把描述作为 ID。 |
| apt | required 提权 | 有误 | 系统写操作需权限；dpkg-query、apt-cache、showmanual 和通常的模拟不应统一提权。apt-get 通常以 100 表示错误，不能与 DNF 的 100 混用。 |
| DNF/RPM | `rpm -qa --queryformat` | 有误 | 缺少必需的格式字符串；明确 name、epoch、version、release、arch 和分隔符。 |
| [DNF](https://dnf.readthedocs.io/en/stable/command_ref.html) | `dnf check-update`，有更新退出 100 | 属实 | DNF4 中 0 为无更新、100 为有更新、1 为错误；按操作定义。 |
| [DNF5](https://dnf5.readthedocs.io/en/latest/commands/check-upgrade.8.html) | dnf5 只是输出变化 | 有误 | 应单独声明能力和命令变体；当前文档提供 `check-upgrade --json`，但需确认安装版本支持。 |
| DNF | `dnf install/remove/upgrade -y`、`dnf search` | 属实 | 读写分开；预览依赖变化，不能由查询成功推断升级一定能解算成功。 |
| [DNF5 repoquery](https://dnf5.readthedocs.io/en/latest/commands/repoquery.8.html) | `repoquery --userinstalled` 等于用户显式安装 | 有误 | 该集合还可能包含 group/module 安装和原因未知的包；需要时读取具体 reason。 |
| DNF | 全部 required 提权 | 有误 | 系统写操作需要；RPM 查询、搜索和检查更新通常可非 root 执行。 |
| [pacman](https://man.archlinux.org/man/pacman.8.en.html) | `-Q`、`-Qi` 列库存 | 属实 | 前者适合基本列表，后者是本地化多行详情；不应逐包启动 `-Qi` 造成大量子进程。 |
| pacman | `-Qu` 前置 `-Sy` | 有误 | 后台检查改用独立数据库的 `checkupdates`，禁止为检查刷新系统同步数据库。 |
| pacman | 只允许 `-Syu`，禁止单包升级 | 属实 | 作为保守产品策略成立；“任何单包操作必毁系统”不成立，真正问题是受支持的系统一致性。安装路径也必须受该策略约束。 |
| pacman | `-Ss` 搜索，`-Qe/-Qd` 标记原因 | 属实 | 原因不是安全删除判断；查询一般不需提权，写事务才需。 |
| pacman | 装/卸方案已覆盖 | 有误 | 该行只写了升级；还缺安装事务和 `-R` 等卸载选项的明确边界，尤其不能默认递归删除依赖。 |
| [Snap](https://snapcraft.io/docs/how-to-guides/manage-snaps/manage-updates/) | `snap list`、`snap refresh --list` | 属实 | 是文本输出；保留 revision、tracking、notes，不能仅比较展示 version。 |
| [Snap 入门](https://snapcraft.io/docs/tutorials/get-started/) | `snap install/remove/refresh`、`snap find` | 属实 | 安装还可能需要明确 channel、classic 等选项；删除数据语义必须单独说明。 |
| Snap | required 提权、无依赖 | 有误 | 列出和搜索不需统一提权；写操作经 snapd 授权。存在 base/content 等关联，不能把所有 snap 都当用户应用。 |
| [Flatpak](https://docs.flatpak.org/en/latest/flatpak-command-reference.html) | `list --app --columns=application,name,version,size,origin` | 属实 | 字段有效，但不足以定位对象；加入 ref、installation 等身份。`--app` 也会隐藏 runtime。 |
| [Flatpak 输出实现](https://raw.githubusercontent.com/flatpak/flatpak/main/app/flatpak-table-printer.c) | 输出为 tab 分隔 | 属实 | 非 fancy 输出使用 tab；必须固定捕获方式，不应假设终端格式与管道格式完全相同。 |
| Flatpak | `remote-ls --updates --columns=application,version` | 属实 | 可列更新候选；必须保留 branch/arch/origin/installation，版本文字相同也可能有 commit 更新。 |
| [Flatpak issue](https://github.com/flatpak/flatpak/issues/3748) | 上述候选等同实际 update 事务 | 有误 | 上游存在差异报告；实际执行前重新核对，不能只依赖 remote-ls 结果。 |
| Flatpak | `install/uninstall/update -y` | 属实 | 绑定 `--user`、`--system` 或具体 installation；安装明确 remote，目标使用完整引用。 |
| Flatpak | `search --columns=…` | 不确定 | `--columns` 存在，但省略号不是可执行规范；补每个字段和查询参数。 |
| Flatpak | 系统级由 polkit 自弹窗 | 有误 | 取决于桌面认证代理与授权策略；可能无需弹窗，也可能不能认证，需显式处理。 |

### 4.4：独立安装器

该节除 Claude 外只有名称，**没有提供可逐项核对的命令、退出码和权限合同**。这本身是待补的实现工作，不能算已经设计完成。

| 来源 | 文档说法 | 核查结论（属实/有误/不确定） | 正确做法 |
|---|---|---|---|
| [Claude Code](https://code.claude.com/docs/en/setup) | `~/.local/bin/claude`、Windows 对应 `.exe` | 属实 | 属于默认原生安装路径；不覆盖包管理器安装、自定义位置和重复安装。 |
| Claude Code | `["claude","--version"]` | 属实 | 改为执行检测到的绝对路径，不能重新走 PATH。 |
| Claude Code | 正则 `\d+\.\d+\.\d+` | 有误 | 只能抽取三段数字，会丢失其他版本标识，也可能误匹配其他输出；限定输出行并保留原始版本。 |
| Claude Code | 用 npm registry 最新版本判断原生安装更新 | 有误 | 按原生安装渠道和版本策略判断；stable/latest、版本限制和不同分发渠道不能混用。 |
| Claude Code | `claude update` | 属实 | 按实际安装方式、渠道及禁用更新策略处理，并核对最终版本。 |
| Claude Code | 删除 `~/.local/bin/claude` 和 `~/.local/share/claude` | 属实 | 对确认的 Unix 原生默认安装成立；Windows 补 `.exe` 路径。只能删除确认拥有的文件，保留用户配置作为独立选项。 |
| Claude Code | TOML 支持三平台但共用 Unix 删除列表 | 有误 | 删除规则按平台和安装方式分别声明；明确 `~`、`%USERPROFILE%` 由程序展开，Rust argv 不自动展开。 |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli)、[Antigravity](https://antigravity.google/docs/cli/using/) | `agy（Gemini/Antigravity）` | 有误 | Gemini CLI 与 Antigravity CLI 是不同产品和安装对象；分别定义 ID、路径、渠道与更新方式。 |
| grok | 名字足以定义适配器 | 不确定 | 指定官方/第三方项目、仓库、包 ID、平台和签名证据，不能通过命令名推断身份。 |
| uv 本体 | 属于独立安装器 | 不确定 | 可来自独立安装、brew、pip 等；先确认归属，不能对所有 uv 调用自更新。 |
| rustup | 属于独立安装器 | 不确定 | 区分 rustup 本体与工具链；补 self-update/self-uninstall 的平台、渠道和影响范围。 |
| bun | 属于独立安装器 | 不确定 | 存在多种安装方式；补归属判断、升级命令和卸载文件清单。 |
| deno | 属于独立安装器 | 不确定 | 同上，不能用路径或名称直接推导所有权。 |
| nvm | 可套相同独立可执行文件识别 | 有误 | 常见 nvm 是 shell 函数；nvm-windows 是不同实现，不能共用一个可执行文件适配器。 |
| fnm | 首批支持 | 不确定 | 未写更新 fnm 本体还是其管理的 Node，也未写安装归属合同。 |
| volta | 首批支持 | 不确定 | 未区分管理器本体、工具链和 shim；补对应范围。 |
| pnpm 独立版 | 首批支持 | 不确定 | 需区分 standalone、npm、Corepack、brew 等方式，否则容易产生第二份安装。 |
| mise | 首批支持 | 不确定 | 管理器本体与其管理的运行时是两类对象，不能只给一个版本字段。 |
| pyenv | 三平台独立安装器 | 不确定 | Unix pyenv 与 pyenv-win 分别定义；补 Git、brew 等安装方式。 |
| Ollama 本体 | 首批支持 | 不确定 | macOS 应用、Windows 安装器、Linux 服务的升级卸载不是同一文件删除方案；模型数据默认保留。 |
| Docker Desktop | 仅识别 | 属实 | 作为能力范围成立；只检测到 `docker` CLI 不足以证明 Docker Desktop 已安装。 |

## 5. 缺失项

### D1. 适配器版本与兼容合同

**涉及章节：§4、§11—12。**

必须写出 `schema_version`、管理器最低/最高已验证版本、能力探测、fixture 采集版本，以及未知版本如何处理。否则上游改变输出后，应用只能继续错误解析或全盘报错。

### D2. 明确的执行计划与交互协议

**涉及章节：§5—6。**

必须写出一次操作的目标实例、影响对象、下载量、授权需求、交互模式、取消策略和完成核对。`command_preview` 不能替代这些信息。

尤其要规定遇到 EULA、配置文件冲突、sudo、安装器 GUI 时：如何提示、谁接管、何时超时、如何确认结束。

### D3. 持久化状态机与崩溃恢复

**涉及章节：§3、§5—6。**

必须规定：

- 数据库何时写入“操作已开始”。
- 应用退出时谁继续持有进程与结果。
- 重启后如何核对进行中的操作。
- 哪些操作可以重试，哪些绝不能自动重放。
- migration 失败、数据库不可写、日志盘满时的行为。

### D4. 环境发现与管理器缺失时的产品路径

**涉及章节：§4 detect、§7 首启。**

必须说明 GUI 的 PATH 与终端不同如何处理，如何发现多个 prefix/解释器，如何避免执行用户 shell 初始化脚本产生副作用。

没有安装管理器的普通用户打开商店后，到底看到“不可用”、安装管理器引导还是替代来源，也必须写清。

### D5. 后台检查合同

**涉及章节：§4 refresh、§8。**

必须规定离线、代理、私有源认证、计费网络、休眠恢复、失败退避及检查合并。缓存刷新 TTL 按实例和成功时间计算。后台检查不应安装管理器、接受新协议或触发提权。

### D6. 真实的平台支持矩阵

**涉及章节：§10—11。**

必须列 OS 版本、架构、WebView、桌面环境和安装格式。`tauri-action` 确实能支撑三平台构建，但 universal 需要安装两个 Rust target 并显式选择目标；Linux 还需相应系统依赖。[tauri-action 官方示例](https://github.com/tauri-apps/tauri-action)

“Linux 支持”不能只等同于“Ubuntu 构建出的程序能启动”。

### D7. 发布安全与升级维护

**涉及章节：§10、§12。**

补充锁文件、Rust/Node/pnpm 版本、Action 固定版本或提交、Release 权限、校验和、依赖许可证、漏洞报告渠道、签名密钥备份及撤销流程。

带提权 helper 的包管理 GUI，不应把安全维护全部推迟到有用户之后。

### D8. 可执行的首发验收标准

**涉及章节：§11、§13—14。**

不是“跑一遍”，而是逐条可判定：

- 不会对另一个安装实例执行操作。
- 不会把检查失败展示为“全部最新”。
- 拒绝授权后不会继续排队执行写操作。
- UI 取消后不会谎称服务端事务已停止。
- 全部更新不会绕过忽略、pin 或安装范围。
- 非空升级路径在 15 个来源各有证据。
- 安装包在干净系统上安装、启动、升级自身、卸载均有记录。

### D9. 三平台 UI 自动化路线

**涉及章节：§11。**

“WebDriver 后期”太含糊。当前 Tauri 文档已区分内嵌 WebDriver 服务与外部 `tauri-driver` 路线；直接使用后者不能假定覆盖 macOS。选定路线，并确保测试插件不进入正式发布构建。[Tauri WebDriver 文档](https://v2.tauri.app/develop/tests/webdriver/)

### D10. 面向普通人的可信信息边界

**涉及章节：§1、§5、§7。**

必须规定哪些信息是目录编辑判断，哪些来自管理器，哪些只是推测。占用空间也应区分安装体积、缓存、共享依赖和可回收空间，不能把各项 `size` 相加后承诺卸载可释放同样大小。

## 6. 一句话总评与最该先改的 3 件事

**一句话总评：这份草案足以指导 AI 快速生成一个漂亮演示，却不足以指导它安全管理真实电脑；最大的返工源是把十五套安装事务误抽象成十五份命令模板。**

最该先改的三件事：

1. **重写 §4—5：** 引入安装实例、归属证据、能力和更新候选；确立 Rust 行为接口与 TOML 元数据的边界。
2. **重写 §6：** 完成提权、交互、事务结果、取消与崩溃恢复闭环；立即删除 Arch 后台 `-Sy` 和“macOS 无需提权”的假设。
3. **重排 §13：** 在通用引擎和完整 UI 之前，打通 brew cask、winget、apt、Flatpak 四条高风险路径，并建立覆盖全部 15 个来源的发布阻断矩阵。三平台仍可同日发布，但发布日期必须服从这些验收结果。
