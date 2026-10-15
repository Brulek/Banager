# Banager 阶段 4 设计规格：独立安装器 + 来源不明扫描（2026-09-24）

> 调研原始记录（逐工具事实、代码地图、三份候选设计与评审）未入库，因为含作者本机细节：`~/dev/Banager/.superpowers/phase4/`。下文 `xxx.md §n` 形式的引用都指那里的文件。

三份候选设计（`design-safety.md` / `design-reuse.md` / `design-journey.md`）经三位评审独立打分，
两位判 safety 胜、一位判 journey 胜。本稿以 **safety 为骨架**，删掉三位评审在任一设计里点名的
每一处致命缺陷，把经得起核对的好想法嫁接进来，并把三份设计互相矛盾的地方逐条定案。
本稿每一条关于现有代码的断言都由本稿作者在 `~/dev/Banager-polish` 上核过：初稿核对于
`17d8ef7`（2026-09-24 15:07）；评审处理（§十三）后**重新钉到当前 HEAD `8ba6f52`**（分支
`feat/pre-release-polish`；`17d8ef7` 之后的七个提交里 `99a9d6f` 改了 `NoCancel` 的语义、`201f760`
改了 `OperationBar` 的按钮规则，本稿据此改写）。行号一律取自 `8ba6f52`，不是从任何一份设计里抄来的。
工具事实来自 `.superpowers/phase4/{claude,agy,grok,rustup,uv,bun,
deno,mise,pnpm,ollama,unknown-scan}.md`；凡是那些文件标 UNVERIFIED 的事实，本稿要么不依赖，
要么在用到的地方当场标出并给出它不成立时的退路。

结构沿用 `docs/superpowers/2026-09-22-instance-level-channel-spec.md`：先订正、再决定、然后 Rust、
TypeScript、每个新字段点名生产读取方、可独立合并的步骤、明确不在射程的清单（各附正确形状）、
只有作者能拍板的问题，最后是评审处理记录（§十三：48 条逐条核实与处理）。

产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
没有证据绝不说成功；fixture 只收真机录制；Banager 不跑 shell、不把下载管进 `sh`；
界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

---

## 零、代码现状订正与三份设计的取舍

### 0.1 三份设计写作时的前提，与 `8ba6f52` 的真相

| 设计里的说法 | `8ba6f52` 的真相 | 后果 |
|---|---|---|
| `ArtifactKind::Binary` 从未在生产代码构造（架构图 §1.5；reuse D2、journey §1.4 照抄） | cargo 适配器为每个 `cargo install` 的 crate 构造它：`adapters/cargo.rs:64`（inventory）与 `:259`（check_updates） | 不是空槽位。本稿仍用 `Binary` 表示独立安装的工具——它就是一个二进制；`ArtifactKey` 是 `(instance_id, kind, name)`（`model.rs:184-189`），实例 id 不同即不撞。**任何代码不得用 `kind == Binary` 来表示「独立安装」。** |
| `cancel_policy` 零生产读取方，`NoCancel` 要等一个并行的 ops 改动（三份设计都写成前置依赖） | **已完整落地**（`17d8ef7` 立、`99a9d6f` 改定）：`CancelPolicy` 只剩 `KillThenReconcile \| NoCancel`（`model.rs:356-370`，`SafeKill` 在 Rust 与 TS 里都已删除，只残留在两份旧计划文档里）；`OpSummary.cancel_policy`（`ops/mod.rs:205`，`:295` 抄自 plan）；`OperationManager::cancel` 对 `NoCancel` 的 op **只在 Running 时拒绝，Queued 时照常接受**（`ops/mod.rs:331-332`——什么都还没启动，取消它等于让它永不启动；初稿写的「任何状态都拒绝」是 `17d8ef7` 的旧语义）；IPC 报 `{"kind":"no_cancel"}`（`src-tauri/src/ipc.rs:341`）；`OperationBar.tsx:38` `cancellable = policy !== "NoCancel" \|\| status === "Queued"`；策略矩阵测试在 `tests/ops_cancel_test.rs:736-866` | 阶段 4 **没有**前置依赖。rustup 是第一个生产 `NoCancel` 的适配器，Running-only 语义与它相容（Queued 时没有 spawn 任何东西，`noCancelHint` 说的「一旦开始」也对）。「No adapter produces `NoCancel` yet」一句在**四处**：`model.rs:366-368`、`src/lib/types.ts:157-158`、`OperationBar.tsx:37`、`tests/ops_cancel_test.rs:740-741`，步骤 E 全部改为指向 rustup。 |
| `/usr/bin/trash` 是 macOS 14 起自带（journey §5.2） | 本机 `man trash` HISTORY：**First appeared in macOS 15.0**（reuse 的说法对） | spec 最低支持 13.3，13.3–14.x 没有 `trash`。本稿不依赖它（§6.2）。 |
| `mv -n` 遇到同名目标会报错 | 本机实测（评审后补测，§十三 #19/#23）：`mv -n a dest/`（dest 已有 a）**退出 0、源文件原地不动、无任何输出**；**同一次调用里两个源同名也一样**——`mv -n ~/.local/bin/claude ~/.local/share/claude dest/` 先把链接移成 `dest/claude`，第二个源（程序目录）被静默跳过，退出 0；`mv` **单个**源到不存在的目录不是报错，而是把源改名成那个名字、退出 0（≥ 2 个源才报「is not a directory」退出 1）；`mv` 一个符号链接只移链接、目标原样 | claude 的两条路径基名都叫 `claude`，**一条 `mv` 命令根本表达不了这次卸载**（初稿 §6.2 的示例会静默留下 626 MB 的程序目录并报成功）。路径清单卸载改为每条路径一次系统「移到废纸篓」调用（§6.2）；不再有 `mv`、不再有时间戳子目录。 |
| `/bin/mv … ~/.Trash/…` 从 Banager 里跑得通（三份设计与本稿初稿） | 本稿的 `mv` 实验全部在一个已有「完全磁盘访问权限」（FDA）的终端里做（本会话 `ls ~/Library/Mail` 可读），**没有在 Finder 启动的、无 FDA 的 app 上下文里试过**。`~/.Trash` 自 macOS 10.15 起受 TCC 保护（kTCCServiceSystemPolicyAllFiles）：无 FDA 的进程连 `ls ~/.Trash` 都是 Operation not permitted，子进程 `/bin/mv` 继承 app 的 TCC 身份，而 `access(W_OK)` 不问 TCC——本会话里 **UNVERIFIED**（有 FDA 的进程复现不了），但与 Catalina 起「终端要 FDA 才能进 ~/.Trash」的已知事实一致 | 「进废纸篓」不能建在一次 rename 上。改用系统 API `NSFileManager trashItemAtURL:`（访达与 macOS 15 的 `trash(1)` 用的就是它，不需要 FDA，访达「放回原处」可用），步骤 C 前必须在 Finder 启动的无 FDA 构建上核实（§6.2）。 |
| agy 的 `.old` 备份文件常驻（safety §0 观察到 184 MB） | 14:36 `agy` 自更新为 1.2.10 并留下 `agy.<ts>.old`；15:11 目录 mtime 更新，15:17 核实 `.old` **已不在** | 备份是**瞬态**的（约半小时）。卸载清单与扫描归属仍要认得它（碰上那半小时不能漏），但不能把它当稳定事实写进 fixture。 |
| agy `--version` = `1.2.9` | 本机现为 `1.2.10`（调研到设计之间它自己更新了） | fixture 录 `1.2.10`；「徽章可能过时」这条危害是现场证明的，不是推测。 |

其余被三份设计引用、且本稿继续依赖的断言，全部核实无误，行号见各节。

### 0.2 谁贡献了什么、删掉了什么

| 来源 | 采纳 | 删除（评审点名的缺陷） |
|---|---|---|
| safety | 单一 `StandaloneAdapter` 类型；`execute()` 前对每条路径**再核一遍**（`Fault::PathChanged`）；rustup 卸载同时持有 cargo 实例的锁；移入废纸篓而不是永久删除（全部支持版本都可逆；机制改为系统调用，§6.2）；`EditsShellConfig` 披露 rustup 改 shell 配置；agy `.old` glob；HTTPS 主机 allowlist 在 `send()` 里失败关闭；附录 B「拒绝清单与执行点」 | 单一 `standalone` 适配器 id + `standalone:<tool>` 实例（评审 1 指出：绕过 `AdapterMeta::unverified_version`，四个工具的录制混在一个 fixture 目录）；自更新工具默认藏在「包含自更新的应用」开关后面（评审 3 指出：Banager 的差异化就是 AI CLI，默认看不见「它落后了」等于自砍第一问）；配方写成 TOML（spec §4.1「TOML 只承载元数据」） |
| reuse | 每工具注册一次、id `standalone-<tool>`（`uv`/`ollama` 不撞、`unverified_version` 零改动、每工具一个 fixture 目录、检测并发免费）；「先共享排除、再各自指纹」的路线归属；版本读取带 `DISABLE_AUTOUPDATER=1` / `AGY_CLI_DISABLE_AUTO_UPDATE=true`；Rust `static` 配方；`what-we-run.md` 单独一步先合 | `trash -s` / `rm -rf` 按 OS 分支（13.3–14.x 永久删除，评审 2 判为更差）；rustup 卸载不持 cargo 锁（评审 1 指出与 `.crates2.json` 竞态）；`execute()` 无 TOCTOU 复核 |
| journey | 四个区分来源的 PATH 遮蔽通知（`NotOnPath` / `ShadowedByHomebrew` / `ShadowedByNpm` / `ShadowedByOther`）；`LeavesUnmanaged { names }` 点名会失管的 cargo 二进制；「屏幕先行」的双语文案；`.app` 归属；本机现场核实的纪律 | 裸 id（第二批 `uv`、`ollama` 会在 `Session::build` 的断言处 panic）；rustup 工具链行（三个新 trait 方法，超出 spec §13 阶段 4）；agy `.old` 未处理；无 TOCTOU 复核；未披露 rustup 改 rc |

三份设计**独立收敛**的部分原样保留，那是最强的信号：无官方卸载命令的卸载仍是一个普通 `Plan` 走
`execute()`，不加 `OpKind`、`execute()` 合同不变；`run_plan` 与 `run_operation` 的前后核对直接给出
`UnchangedAfterUpgrade` / `Unconfirmed`；来源不明扫描是独立的只读路径，不是 `Adapter`；首批 = 本机
四个真实存在的独立安装（claude、agy、grok、rustup）。**一处收敛被评审推翻**（§十三 #19、#20）：
「一个程序 + 一个 argv」装不下 claude 的两条同名路径，也进不了受 TCC 保护的 `~/.Trash`，所以 `Plan`
的命令部分改成一个两臂枚举 `PlanAction`（§6.2、Q16）——这是本稿唯一动 `Plan` 形状的地方。

---

## 一、决定（一屏）

| # | 决定 | 为什么现有机制装不下 / 为什么不选另一种 |
|---|---|---|
| D1 | **一个 Rust 类型 `StandaloneAdapter`，由 `&'static Recipe` 驱动，每个工具注册一次**，adapter id `standalone-claude` / `standalone-agy` / `standalone-grok` / `standalone-rustup`；实例 id = `instance_id(id, None)` = 裸 adapter id（单实例，同 pipx/uv，`model.rs:25-34`）。 | 标签（`sources.ts:18`）、`verified_versions`（`adapters/mod.rs:106-111`）、fixture 目录（`tests/fixtures_layout_test.rs:30-33`）今天都按 adapter id 键。单一 `standalone` 适配器要在前端从实例 id 里切字符串取名、自己重写版本核验、把四个工具的录制混进一个目录。裸 `uv`/`ollama` 与已注册 id 相撞（`session/mod.rs:320-323` 是 `assert!`）。逻辑只有一份；重复的是四行数据。 |
| D2 | 实例**就是**那份原生安装：`exe_path` = 安装器写死的启动器（`~/.local/bin/claude` 等），`prefix` = 工具自己的根；唯一制品 = 工具本身，`ArtifactKind::Binary`。 | `ManagerInstance` 已有这两个字段；已安装页按实例分组、按行给按钮，不需要新的分组概念。 |
| D3 | 本适配器**只认原生/脚本路线**；同一工具的 Homebrew / npm 副本留给 brew/npm 适配器（它们今天已经列出）。探测 = 检查安装器的固定路径，**不用** `resolve_exe`；共享排除（`/Cellar/`、`/Caskroom/`、`/node_modules/`、`/corepack/`）先跑，各工具指纹后跑。 | 双重列出在构造上不可能，不靠跨适配器查表。PATH 只回答「你敲名字时跑哪份」（D7）。 |
| D4 | 最新版本只用 **VERIFIED** 端点；只有 remote > local（点分整数比较）才建候选；查不到是现有的 `checkable: false` 行。加编译期 `ALLOWED_HTTPS_HOSTS`，在 `RealHttpClient::send` 里失败关闭。 | `UpdateCandidate` + `uncheckable_candidate`（`adapters/mod.rs:269-284`）已能表达「有新版」与「查不到」；主机从 4 个涨到 7 个（第二批 11 个），注释不是检查。 |
| D5 | **自更新的工具照常给徽章**，不藏在 `include_self_updating` 后面；行描述用 `auto_updates` 说「它平时会自己更新」（该字段第一个非 Pinned 读取方）。首批只有 claude、agy 算自更新；grok 的 `auto_update = true` 只被证实为「启动时检查」，是否静默安装 UNVERIFIED，核实前按不自更新对待（§4.4）。没有可运行更新命令的（agy）候选带 `UpdateBlocked::SelfUpdatesOnly`：无按钮、Rust 闸门同样拒绝。版本读取带停用自更新的环境变量。 | brew 的 `--greedy` 存在是因为 `brew outdated` 查不到 cask 的活版本；这里 Banager 读的就是启动器**此刻**的版本，比较是精确的。徽章说的是「这份比最新版旧」，是真话；点了之后工具已自己更新 → `UnchangedAfterUpgrade`，也是真话。本机 claude 的自更新被环境变量关掉，徽章正是有用的那种信号。 |
| D6 | 升级 = 工具自己的文档命令，走 `run_plan` 不变；rustup `self update` 为 `NoCancel`（原子性 UNVERIFIED），且与 `self uninstall` 一样同时持 cargo 实例的锁——它替换的是 13 个代理都 exec 的那个二进制（§2.4）；**永不** `rustup update`。 | `ops/mod.rs:676-716` 的前后核对免费给出诚实结果；`NoCancel` 已被 ops 层遵守——Running 时拒绝、Queued 时接受（§0.1）。 |
| D7 | 「敲名字跑哪份」= 四个无载荷 `InstanceNote` 变体，在 `detect()` 里按 `resolve_exe` 的解析路径分类。「装了 2 份」的合并计数推迟，形状记 §十一。 | `InstanceNote` 是现成的实例级通道，两页都渲染（`sourceNoticesFor`，`sources.ts:123`）；四句各自可行动的话对小白比一句带路径的话有用。 |
| D8 | 无官方卸载命令的卸载 = 一个 `Plan`，其 `action` 是 `PlanAction::TrashPaths { paths }`：`execute()` 对每条路径**依次**调一次 macOS 的 `NSFileManager trashItemAtURL:`（访达用的那个调用：同卷 rename、瞬间完成、访达「放回原处」可用、不需要完全磁盘访问权限），程序目录在前、**启动器最后**。五条包含性检查在 `plan()` 跑一遍、`execute()` 再跑一遍，且每移一条前核对 `(st_dev, st_ino)`，有差异 → `Fault::PathChanged`。设置、登录、历史默认保留并逐条列出。 | 一条 `mv` 命令装不下两条基名相同的路径（claude 的两条都叫 `claude`），无 FDA 的进程进不了 `~/.Trash`（都在 §0.1）；`/usr/bin/trash` 只在 15.0+ 存在。`Plan` 因此多一个两臂枚举（Q16），`CommandPreview` 多一支「Banager 自己移、不运行命令」，其余执行合同（`execute()` 签名、`run_operation` 的前后核对、锁、取消令牌）不变。 |
| D9 | rustup 卸载 = 官方 `rustup self uninstall -y`，`NoCancel`，四条固定警告（工具链、Cargo 缓存与记录、失管的 cargo 二进制、改 shell 配置）+ 一条条件警告（rustup 不管的 rc 文件里残留的 `.cargo/env` 行，本机 `~/.zshrc:17` 就是），`Plan.locks` 同时持有 `standalone-rustup` 与 `cargo:<cargo_home>`。 | 它会删 `~/.cargo/.crates2.json`——cargo 适配器 inventory 读的正是这个文件（`cargo.rs:187`），刷新在实例锁下读（`refresh.rs:275-277`）；`run_operation` 一次性取齐 `plan.locks`（`ops/mod.rs:437-450`）。 |
| D10 | 来源不明扫描 = `crates/banager-core/src/scan/` 里一个纯函数 + `Session::scan_unknown` + 一个 IPC 命令 + 一个新页面；**不是** `Adapter`，不进 `Snapshot`，不进 refresh。 | 归属需要所有其它适配器的实例，`inventory(&self, inst)` 看不到（`adapters/mod.rs:428-431`）；注册成 Adapter 会撞 fixture 集合相等测试、ops 注册与闸门；进 `Snapshot` 会进 `same_content`（`session/mod.rs:122`）或被它忽略。 |
| D11 | 首批 claude、agy、grok、rustup；其余六个是第二批，每个都等一份真机（CI runner）录制才注册。 | fixture 只收真机录制，且 `fixtures_layout_test.rs` 要求注册 id 与目录集合**完全相等**。 |
| D12 | `docs/what-we-run.md` 先为六个阶段 3 来源补齐，作为独立一步先合；本阶段每步各加自己那一节。 | 它是 spec §12 的信任文件，今天标题就是「Phase 0–1: Homebrew only」（78 行，只写 brew）。 |

---

## 二、模型：适配器、实例、制品

### 2.1 注册与 id

`Session::new`（`session/mod.rs:261-271`）的 `vec![...]` 后接 `standalone::all(runner.clone(), http.clone())`
返回的四个 `Arc<dyn Adapter>`；`test_new_registers_all_seven_adapters`（`session/mod.rs:500`）改为十一。
`Session::build` 的两条断言（无 `:`、不重复，`session/mod.rs:314-323`）对 `standalone-<tool>` 都成立。

**id 形状为最终形状**：`standalone-<tool>`，无限定符。理由：
(a) 裸名 `bun`/`deno`/`pnpm`/`mise` 留给将来「由该工具管理的包」适配器（`uv` 今天就是 uv 装的工具、`ollama` 是模型），`standalone-uv`、`standalone-ollama` 永远不撞；
(b) 实例 id 持久化在 `Settings.ignored_updates`（`settings.rs:16`，`model.rs:21-24` 的告诫），一旦定下不再改；
(c) 十个安装器每个只有一个固定启动器路径（或一个 `--dir`），第二份原生安装不是它们会产生的状态，所以单实例是真实基数；`dedupe_instance_ids`（`refresh.rs:222` 前后）是配方出错时的兜底。

刷新按 adapter id 字母序扇出（`refresh.rs:155`），已安装页因此依次显示 brew、cargo、npm、ollama、pip、pipx、standalone-agy、standalone-claude、standalone-grok、standalone-rustup、uv。不需要排序代码。

`AdapterMeta`（`adapters/mod.rs:83-91`）七个字段照填，`kind = "standalone"`（今天没有代码按 `kind` 分支，架构图 §1.2 已核；填它只为 what-we-run.md 可机读）。每工具一份 `adapters/meta/standalone-<tool>.toml`，`verified_versions` 是每工具的，`unverified_version`（`:106-111`）不改一字。

### 2.2 实例字段（`ManagerInstance`，`model.rs:123-146`）

| 字段 | 值 | 生产读取方 |
|---|---|---|
| `id` | `"standalone-claude"` 等 | 全工作区；`Settings.ignored_updates` |
| `adapter_id` | 同 `id` | `ADAPTER_LABEL_KEYS`（`sources.ts:18`，读取点 `InstalledPage.tsx:166`、`UninstallDialog.tsx:50-51`、`UpdatesPage.tsx:518` `sourceLabelFor`）；`ops/mod.rs` 按它找适配器 |
| `exe_path` | **启动器本身**（`~/.local/bin/claude` 这个符号链接，不是它的目标；agy/rustup 是普通文件），**不是** `resolve_exe` 找到的那份 | `plan()` 的 `program`（同 `uv.rs:170,268` 的做法）；`reconcile()` 的存在性判断（`symlink_metadata` + 指纹，§3.6）；`CommandPreview`；`sourceNoticesFor` 新分支取 `file_name()` 作 `{{command}}`；扫描归属规则 0 与 1（§8.3） |
| `prefix` | 工具的根：claude `~/.local/share/claude`，agy `~/.gemini/antigravity-cli`，grok `~/.grok`，rustup **`$CARGO_HOME`**（启动器与 13 个代理住在它的 `bin/` 下；Banager 不读 `RUSTUP_HOME` 下的任何东西，`rustup self uninstall` 自己知道它在哪，所以 `HostEnv` 不加 `rustup_home`，§3.2） | 扫描归属规则 3——只对 `prefix` 是「自己拥有的根」的适配器生效（standalone-claude/agy/grok、brew 的 Cellar/Caskroom/opt；**不含** rustup，它的一切都靠规则 1，§8.3）；`SymlinkIntoRoot` 路线的指纹（`canonicalize(launcher)` 必须落在其下）。今天前端不渲染 `prefix`（grep 只命中 `sources.ts` 注释） |
| `scope` | `User`（四条路线全在 `$HOME` 下） | — |
| `version` | `--version` 解析结果；失败 `None` | `unverified_version`；`refresh.rs` |
| `unverified_version` | `meta.unverified_version(&version)` | `InstalledPage.tsx:279-281` |
| `read_only_reason` | `None`（四个都可写；「没有安全卸载方法」是每制品的，§6.1） | 闸门 `plans.rs:172-177`；`canWrite()`（`sources.ts:56`） |
| `status.unavailable` | `--version` 不应答 → `Some(NotResponding)`（uv 的规则，`uv.rs:154`）；永不 `NotRunning`（没有东西可启动）、永不 `RefusesAsRoot`。**启动器是悬空链接但链接文本指向工具根**（上次卸载中途停下）→ **不是** unavailable：`version: None` + `InstanceNote::LauncherOnly`（§3.3 第 2 步），闸门放行，卸载把剩下的链接移走 | 闸门（`plans.rs:172-177`）；`sourceNoticesFor` |
| `status.notes` | §七 的四个 PATH 变体之一，以及 `LauncherOnly`，在 `detect()` 里填；`merge_instance_notes` 是 `extend`（`refresh.rs:598`），不会被 `check_updates` 的 notes 覆盖 | `sourceNoticesFor` 的 notes 循环（`sources.ts:198-225`），`:222` 的 `const unhandled: never = note` 让漏写分支**编译失败** |

### 2.3 制品字段（`InstalledArtifact`，`model.rs:192-210`），每实例恰好一个

| 字段 | 值 | 生产读取方 |
|---|---|---|
| `key` | `{ instance_id, kind: Binary, name: recipe.id }`（`name` 是工具 id，不是显示名：`OpRequest.name` 经 `validate_package_name`（`adapters/mod.rs:348-365`）后在 `plan()` 里必须等于 `recipe.id`，否则 `InvalidName`） | `reconcile_from` 按 `(kind, name)` 匹配（`:385-399`）；`OpRequest` 回传 |
| `display_name` | `meta.name`（TOML 的 `name`：`"Claude Code"`、`"Antigravity CLI"`、`"Grok Build"`、`"rustup"`；不在 `Recipe` 里再抄一遍，§十三 #45） | `ArtifactRow` 标题；`UninstallDialog` 标题 |
| `version` | 同实例 `version`；`None` 时 `""`（brew 的回退，`ops/mod.rs:81-84` 附近；`LauncherOnly` 时就是 `""`） | `installed.nameWithVersion`（`InstalledPage.tsx:316`）；前后核对 |
| `reason` | `Requested`（用户亲手跑了安装器；`Dependency` 会被折叠进「N 个组件」，`InstalledPage.tsx:196-197`） | 依赖折叠 |
| `description` | **`None`**。一句说明要双语，而 `description` 是不知道界面语言的裸字符串；前端按 adapter id 查 `STANDALONE_SUMMARY_KEYS`（§9.2） | `InstalledPage.tsx:344` 的回退分支改为 `artifact.description ?? summaryFor(adapter_id) ?? t("installed.noDescription")` |
| `homepage` | `meta.homepage`（TOML 已有，同 `AdapterMeta` 的七个字段） | 今天无前端读取方（现状，与七个适配器一致，不新增） |
| `path` | `Detected.real`：真实二进制（`LauncherOnly` 时 `None`） | 扫描归属规则 2（§8.3）。**不是**「第一个填它的适配器」：uv 今天就填（`uv.rs:65`，tool 的 venv 目录），规则 2 因此是「以它开头」而不是相等，uv 的 shim（`~/.local/bin/ruff`）正靠这条归到 uv；对独立安装工具，规则 2 与规则 1 比的是同一个路径，不决定任何事 |
| `auto_updates` | `recipe.self_updates`（claude/agy `true`，grok/rustup `false`，§4.4） | **新增读取方** `UpdatesPage.tsx` `rowDescription` 的 `updates.selfUpdatingHint` 支（今天 `:452` 只对 blocked 行读它） |
| `uninstall_blocked` | `None`；配方 `uninstall: None` 时 `Some(NoSafeMethod)`（§6.1） | 闸门 `plans.rs:187`；`UNINSTALL_BLOCKED_KEYS`（`sources.ts:451`，`Record` 漏写编译失败）；`InstalledPage.tsx:144,329-345` |
| `size_bytes` / `installed_at` | `None`（`src/` 无读取方） | — |

### 2.4 锁

`ResourceLock(inst.id)`，与七个适配器及刷新一致（`refresh.rs:276`、`uv.rs:256`、`cargo.rs:313`）。
两个独立安装工具从不共锁；独立安装工具与它的 Homebrew 孪生也不共锁——它们是不同文件，两边适配器从不写同一路径（§6.7 的 `--zap` 规则）。

**唯一例外**：rustup 的 `Uninstall` **与 `Upgrade`** plan 都列两把锁：`ResourceLock("standalone-rustup")` 与
`ResourceLock(instance_id("cargo", Some(cargo_home)))`（cargo 的 id 形状在 `cargo.rs:161`，`cargo_home = env.cargo_home.unwrap_or(home/.cargo)`，`:133-136`）。
`rustup self uninstall` 删 `~/.cargo/{registry,git,.crates.toml,.crates2.json}`（VERIFIED，rustup 源码 `clean_cargo_home`，rustup.md §8），而 cargo 适配器的 inventory 读 `inst.prefix.join(".crates2.json")`（`cargo.rs:187`）；`rustup self update` 替换的是 `~/.cargo/bin/rustup`，而 `cargo` 就是指向它的代理（rustup.md §2、§5），cargo 刷新的 `cargo --version` 恰好撞上替换的那一刻会得到一个撕裂的二进制。`run_operation` 在启动前一次性取齐 `plan.locks` 里的每一把（`ops/mod.rs:437-450`），刷新对每个实例取 `ResourceLock(inst.id)`（`refresh.rs:275-277`），`acquire_resource_lock`（`ops/mod.rs:866`）不校验锁名——今天就能这么写。

锁名必须与 `CargoAdapter::detect` 产出的实例 id **逐字节相等**，否则两把锁互不相干而没有任何东西会报错。所以只留**一个生产者**：`cargo.rs` 暴露 `pub(crate) fn instance_id_for(cargo_home: &Path) -> String`，`detect()`（`:161`）与 rustup 的 `extra_locks`（拿 `Detected.cargo_home`，按 `:133-136` 同一条规则算出）都调它；步骤 E 加一条测试，在同一个 `HostEnv` 上（有、无 `CARGO_HOME` 各一次）跑 `CargoAdapter::detect` 与 rustup 的 `plan(Uninstall)`/`plan(Upgrade)`，断言 `plan.locks` 含那个实例的 `id`（§十三 #42）。

### 2.5 文件

```
adapters/meta/standalone-{claude,agy,grok,rustup}.toml     AdapterMeta 七个字段，不多一个键
adapters/fixtures/standalone-<tool>/<version>/              真机录制 + README（§9.3）
crates/banager-core/src/adapters/standalone/
    mod.rs        StandaloneAdapter：Adapter 实现、Detected 座、all()
    recipe.rs     Recipe 与它的枚举（§三）
    recipes.rs    四个 `pub static`（CLAUDE、AGY、GROK、RUSTUP）；RECIPES: &[&Recipe]
    route.rs      路径展开、共享排除、指纹（含悬空链接的词法归一）、PATH 遮蔽分类
    latest.rs     三种最新版本来源、点分整数比较、claude 通道
    removal.rs    路径清单卸载：五条检查、TrashPaths 计划、execute 复核与逐条移入废纸篓
crates/banager-core/src/trash/{mod.rs,real.rs,mock.rs}       Trasher trait、RealTrasher（NSFileManager）、MockTrasher（§6.2）
crates/banager-core/src/model.rs                             PlanAction（§6.2）、新 Warning/Fault/InstanceNote 变体、UninstallUnsafeReason
crates/banager-core/src/adapters/cargo.rs                    parse_crates2_bins、instance_id_for（步骤 E）
crates/banager-core/src/scan/mod.rs                          来源不明扫描（§八）
crates/banager-core/src/session/scan.rs                      Session::scan_unknown
crates/banager-core/tests/standalone_uninstall_test.rs       临时 HOME 上的端到端（MockTrasher）+ #[ignore] 的 RealTrasher 冒烟
crates/banager-core/tests/unknown_scan_test.rs               合成目录树
crates/banager-core/Cargo.toml                               objc2 + objc2-foundation（仅 macOS target，Q16）
src-tauri/src/ipc.rs, lib.rs                                 scan_unknown 命令 + 注册；uninstall_unsafe 错误 kind
src/lib/{types,api,queries,queryKeys,sources,warnings,format,updateState}.ts
src/components/CommandPreview.tsx                            TrashPaths 支
src/pages/UnknownPage.tsx, src/store/ui.ts, src/components/Sidebar.tsx, src/App.tsx
src/i18n/{en,zh-CN}.json, src/i18n/completeness.test.ts      （INTERPOLATED_SUBTREES：nav 与 operations.outcome 各加一项）
docs/what-we-run.md
```

`crates/banager-core/src/lib.rs` 加 `pub mod scan;`。spec §3 把扫描画在 `adapters/unknown.rs`，本稿改到 `scan/`，
理由是它不是 Adapter，读代码的人不该在 `adapters/` 里找到一个没有 `impl Adapter` 的模块（附录 C 记为有意偏离）。

---

## 三、配方：Rust `static` 数据，不是 TOML

spec §4.1：「适配器是有类型的 Rust 实现；TOML 只承载元数据。」删除清单、指纹、端点是**行为**，
写成 Rust 常量表：编译器检查每个枚举穷尽，没有「TOML 里多写一个键无人读」的静默死字段
（`AdapterMeta` 没有 `deny_unknown_fields`，`adapters/mod.rs:94-96`；顺手加上是一行可选加固）。

### 3.1 `Recipe`

```rust
// crates/banager-core/src/adapters/standalone/recipe.rs
/// 一个工具。全部 `'static` 数据，没有 trait 对象，一张表看完。每个字段的注释点名它的读取方；
/// 没有读取方的字段不得加入。
pub struct Recipe {
    pub id: &'static str,             // "claude"。adapter id = "standalone-{id}"；ArtifactKey.name；也是用户敲的命令名（route::shadow_note 用它 resolve_exe）。
                                      // 首批四个的命令名都等于 id；第一个不等的工具出现时再加 `binary` 字段。→ all()/detect()/plan()
    pub meta_toml: &'static str,      // include_str!("../../../../../adapters/meta/standalone-<id>.toml") → AdapterMeta::from_toml。
                                      // 显示名与主页从 meta.name / meta.homepage 取，不在这里抄第二遍（§十三 #45）
    pub route: Route,                 // → detect()
    pub version: VersionCmd,          // → detect()、reconcile()
    pub latest: Option<Latest>,       // None = 不检查（无 VERIFIED 端点）：无候选、无徽章，已安装行不说「已是最新」。→ check_updates()
    pub self_updates: bool,           // → inventory()（auto_updates）。grok 的 config.toml 读取推迟到「静默安装」核实后，形状记 §十一
    pub upgrade: Option<UpgradeCmd>,  // None = 每个候选带 UpdateBlocked::SelfUpdatesOnly。→ check_updates()、plan(Upgrade)
    pub uninstall: Option<Uninstall>, // None = 制品带 UninstallBlocked::NoSafeMethod。→ inventory()、plan(Uninstall)、execute()。字段本身步骤 C 才加（§十）
    pub backup_globs: &'static [Glob],// 自更新器留在启动器目录里的备份文件名。步骤 D 才加（首个生产者 agy）。→ removal（可选删除）、scan 归属规则 4
}

/// 配方里的每条路径都以 `~/` 或 `$CARGO_HOME/` 开头，由 `route::expand(&HostEnv, s)` 展开
/// （`$CARGO_HOME` = env.cargo_home 或 home/.cargo，与 cargo.rs:133-136 同一条规则）。没有 `$RUSTUP_HOME`：
/// Banager 不读 RUSTUP_HOME 下的任何东西（§十三 #44）。绝不在适配器里读 `std::env::var("HOME")`——
/// `HostEnv` 存在的理由（runner/path_env.rs:9-13）。一条测试遍历 RECIPES，断言每条路径都以这两者之一开头。
pub struct Route { pub kind: RouteKind, pub launcher: &'static str, pub root: &'static str }
pub enum RouteKind {
    /// 启动器是符号链接，完全解析后落在 root 之下（claude、grok；本机 VERIFIED）。
    /// 悬空时按链接文本词法归一后是否落在 root 下判定（§3.3 第 2 步）。
    SymlinkIntoRoot,
    /// 启动器是普通文件（非链接），真实路径不含任何包管理器标记（agy、rustup）。
    FlatFile,
}

pub struct VersionCmd { pub args: &'static [&'static str], pub env: &'static [(&'static str, &'static str)], pub parse: VersionParse }
pub enum VersionParse { FirstToken, SecondToken }   // 取 stdout 第一行；token 须匹配 ^\d+(\.\d+)*$，手写判断，不引 regex crate（adapters/mod.rs:339-341 的先例）

pub enum Latest {
    /// claude 专用：GET `{base}/{channel}`，channel 读 `~/.claude/settings.json` 的 `autoUpdatesChannel`
    /// （"latest" | "stable"，VERIFIED 文档键名；缺失、读不到、值不认识 → "latest"）。点名工具而不做
    /// 通用「读 JSON 键」机制：只有一个工具需要，第二个出现时再泛化。
    /// 「channel 值 → `claude-code-releases/<channel>` 端点」这一映射是**推断**（claude.md §4：
    /// 两个键值 VERIFIED 自文档，`claude doctor` 报 `Auto-update channel: latest`，但更新器内部
    /// 调用哪个 URL 未反编译核实，UNVERIFIED）。推断错的代价无害：stable 用户看到一个 `/latest` 徽章
    /// → 点 `claude update` → `UnchangedAfterUpgrade`（§4.4 第 5 条已覆盖）。
    ClaudeChannel { base: &'static str },
    /// GET url（darwin_arm64 清单），JSON 顶层 `field` 是版本串。x86_64 主机上不发请求，该行 uncheckable
    /// 并说明「尚未在 Intel Mac 上核实」：架构取 `std::env::consts::ARCH`（编译进当前运行的那一片；
    /// Rosetta 下的通用二进制报 x86_64，同样落到「无法检查」——安全方向；不给 HostEnv 加字段）。
    /// Intel runner curl 到 darwin_amd64 清单后再加 `url_x86_64`（§十一）。
    HttpJsonField { url: &'static str, field: &'static str },
    /// GET url，正文是 TOML，顶层 `version = '…'`。
    HttpTomlVersion { url: &'static str },
    /// 运行 `<launcher> args`，stdout 是 JSON：`latest_field` 为最新版本，`available_field` 为是否有更新。
    /// 只允许其 --help 自述「不安装」的子命令（grok `update --check --json`）。
    Command { args: &'static [&'static str], timeout_secs: u64, latest_field: &'static str, available_field: &'static str },
}
// 没有 `HttpText`：首批四个工具没有一个用纯文本端点（deno/mise 的在第二批，形状记 §十一）——
// 阶段 4 定义它就是一个死变体（§十三 #40）。

pub struct UpgradeCmd { pub args: &'static [&'static str], pub timeout_secs: u64, pub cancel: CancelPolicy }

pub enum Uninstall {
    /// 工具自己的官方卸载命令（rustup）。`probe` 是 plan() 先跑的只读命令，其 stdout 交给 `warnings`。
    /// 两个 fn 都以 `&Detected` 为参数：plan() 拿不到 HostEnv（adapters/mod.rs:442 的签名，§3.2）。
    Command {
        args: &'static [&'static str], timeout_secs: u64, cancel: CancelPolicy,
        probe: Option<Probe>,
        warnings: fn(d: &Detected, probe_stdout: Option<&str>) -> Vec<Warning>,
        extra_locks: fn(d: &Detected) -> Vec<ResourceLock>,
    },
    /// 没有命令；厂商文档给出路径清单。`remove` 的顺序就是执行顺序（启动器最后，§6.2）。
    /// 清单抄自哪份文档写在配方常量的 `///` 注释与 fixture README 里——它不是字段，因为没有生产
    /// 读取方（§十三 #8/#36）。
    Paths { remove: &'static [RemoveSpec], keep: &'static [KeepSpec] },
}
pub struct Probe { pub args: &'static [&'static str], pub timeout_secs: u64 }
pub struct RemoveSpec { pub path: &'static str, pub expect: Expect, pub what: RemovedWhat, pub optional: bool }
pub enum Expect { SymlinkIntoRoot, File, Dir }
pub struct KeepSpec { pub path: &'static str, pub what: KeptWhat }
/// 无 glob crate：`prefix` + 任意串 + `suffix`，只匹配同目录下的普通文件。定义在 `scan/mod.rs`
/// （扫描规则 4 也读它；adapters 依赖 scan 的一个类型，不是反过来），步骤 D 引入。
pub struct Glob { pub dir: &'static str, pub prefix: &'static str, pub suffix: &'static str, pub what: RemovedWhat }
```

`RemovedWhat`、`KeptWhat` 是 `Warning` 新变体的载荷（§6.5），定义在 `model.rs`，因为它们过 IPC。
TOML 里的 `kind = "standalone"` 是**文档性**的：今天没有代码按 `kind` 分支（架构图 §1.2 已核），也没有东西
从 TOML 生成 what-we-run.md；填它只为让人读 `adapters/meta/` 时一眼看出这四个不是包管理器。

### 3.2 `StandaloneAdapter` 与 `Detected` 座

```rust
pub struct StandaloneAdapter {
    recipe: &'static Recipe,
    meta: AdapterMeta,
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    /// detect() 写、plan()/execute() 读：它们拿不到 HostEnv。与 cargo 的 binstall 座同一套路
    /// （`cargo.rs:98-137`：detect 写 `binstall: Mutex<Option<PathBuf>>`，plan 读）。
    /// 路径清单卸载用的「移到废纸篓」（§6.2）；与 runner/http 一样是注入的 trait 对象。
    trasher: Arc<dyn Trasher>,
    detected: Mutex<Option<Detected>>,
}
/// `real` 为 `None` 就是 `LauncherOnly`（§3.3 第 2 步）。`cargo_home` 按 cargo.rs:133-136 同一条规则算。
pub struct Detected { pub home: PathBuf, pub euid: u32, pub cargo_home: PathBuf, pub launcher: PathBuf, pub real: Option<PathBuf> }
```

`plan()` 在 `detected` 为 `None` 时返回 `Refused`（走 IPC `refused`，有意不给专门文案）——理论上不可达：闸门要求实例在快照里，而实例只能来自 `detect()`。

`HostEnv`（`runner/path_env.rs:5-21`，今天有 `path_dirs / home / euid / cargo_home / ollama_host`）**不改**。初稿要加的
`rustup_home` 没有任何可观察的读取方（`FlatFile` 指纹不看 `root`；rustup 的 `prefix` 只被扫描规则 3 读而规则 3 对
rustup 不生效；四条卸载警告不带 `RUSTUP_HOME` 数据），而加一个字段要改 32 处 `HostEnv {` 字面构造
（`grep -rn "HostEnv {" crates src-tauri`）——为一个死字段不值（§十三 #16/#44）。rustup 的 `root` 改为 `$CARGO_HOME`。

### 3.3 探测与路线归属（`detect()`，每工具）

1. `launcher = expand(route.launcher)`。`symlink_metadata` 失败 → **无实例**（是「没装」，不是 `unavailable`）。刻意不用 `resolve_exe(binary, env)`（`path_env.rs:87-95`）：PATH 里 `/opt/homebrew/bin` 排在 `~/.local/bin` 前面时（Intel 老机器常见），`resolve_exe("claude")` 找到 brew 那份，指纹一拒，原生安装就隐形了。brew 的 `CANDIDATE_PATHS`（`brew/mod.rs:173-177`）是同一思路：按安装器写死的位置找。
2. **共享排除与悬空链接**：`real = canonicalize(launcher)`。成功 → `real` 的任一分量是 `Cellar`、`Caskroom`、`node_modules`、`corepack` → **无实例、无错误**：那是 brew/npm 适配器已经列出的行，或者扫描页会显示的东西；三个 brew 前缀根（`/opt/homebrew`、`/usr/local`、`/home/linuxbrew/.linuxbrew`）下的 `Cellar`/`Caskroom` 都被这条覆盖。**失败（悬空链接）** → 取 `read_link(launcher)` 的文本，相对文本按启动器所在目录做**词法**拼接归一（处理 `..`/`.`，不碰文件系统；grok 的 `../downloads/grok-1.0.41-macos-aarch64` 就是相对的），结果以 `expand(root)` 开头 → 实例存在：`version: None`、`status.unavailable: None`、`notes: [LauncherOnly]`、`Detected.real = None`——这是「程序目录已进废纸篓、启动器还在」的半卸载态，§6.2 的执行顺序保证它是中途停下后**唯一**可能的残留；闸门放行，再点一次卸载把链接移走（§6.3 检查 2）。不以 `root` 开头 → **无实例**：一个指向别处的悬空 `~/.local/bin/claude`（旧安装器的遗留，claude.md §2c）不是「Claude Code 没应答」，来源不明页把它列成失效的链接并显示指向哪里，诚实（§十三 #6/#33）。
3. **指纹**（`real` 存在时）：`SymlinkIntoRoot` → `launcher` 必须是符号链接且 `real.starts_with(canonicalize(expand(root)))`；`FlatFile` → 必须是普通文件（`FileType::is_file()` 且非链接）。不符 → 无实例。
4. 版本（`real` 存在时）：`runner.run(CommandSpec { program: launcher, args: version.args, env: version.env, timeout: 30 s, output_use: Parsed })`（`runner/mod.rs:45-54`），按 `VersionParse` 解析。失败 → `version: None`、`Unavailable::NotResponding`。`unavailable` **只**由版本读取决定，路线判断是分开的一轴。
5. PATH 通知：§七。
6. 写 `Detected`，返回实例（`unverified_version = meta.unverified_version(&version)`）。

双重列出在构造上不可能：brew 列 `brew info --installed` 报的，npm 列 `npm ls -g` 报的，本适配器只列真实路径落在一个**没有任何包管理器会写**的根之下的启动器。同一个**工具**可以合法出现两次（Homebrew 组的 `claude-code` cask 行 + `Claude Code` 组的一行），那正是 spec §4.2 的「检测到 2 个 claude」，§七说清楚哪份在跑。

安装时的环境变量（`GROK_BIN_DIR`、`UV_INSTALL_DIR`……）Banager 无从知道（Finder 启动的进程只有 `fix_path_env` 恢复的 PATH，`src-tauri/src/lib.rs:18`）；首批一律按安装脚本默认路径。自定义路径的安装落到来源不明页——诚实。

### 3.4 版本读取不得触发自更新

调研现场看到 `agy --version` 派生后台更新进程（agy.md §4，VERIFIED 日志 `Spawned background update process`）；claude 原生版启动时检查更新（claude.md §5）。spec §1 第 4 条「后台只读」在这里差点被一条 `--version` 打破。

| 工具 | `version.args` | `version.env` | `parse` | 本机实测输出 |
|---|---|---|---|---|
| claude | `--version` | `DISABLE_AUTOUPDATER=1`（VERIFIED：`claude doctor` 报「disabled (set by env: DISABLE_AUTOUPDATER)」，且文档说它只停后台检查，`claude update` 不受影响） | `FirstToken` | `2.1.281 (Claude Code)` |
| agy | `--version` | `AGY_CLI_DISABLE_AUTO_UPDATE=true`（VERIFIED via 官方 troubleshooting 文档；**本机 1.2.10 实测两次——带与不带该变量——`agy --version` 不写任何日志文件、不改 `updater/update_status.json` 的 mtime、不派生更新进程**：`--version` 在 1.2.10 上根本走不到更新器，变量因此**无法**用 `--version` 验证其效力，保留它是按厂商文档的保险带。研究时在 1.2.9 上观察到的派生（agy.md §4）来自同一会话里带提示词的运行，不能归到 `--version`） | `FirstToken` | `1.2.10` |
| grok | `--version` | 无 | `SecondToken` | `grok 1.0.41 (4220f3b224a6)` |
| rustup | `--version` | 无 | `SecondToken`（stdout 第一行；两行 `info:` 在 stderr） | `rustup 1.29.1 (d95a37b6a 2026-08-13)` |

这些环境变量只加在 detect/inventory/reconcile 的版本读取上，**不加在 Upgrade plan 的 `env`** 上（`claude update` 不该被要求停用自己）。升级前后的核对用同一读法，`ops/mod.rs:88-100` 比较的是同一解析器的两个字符串。

初稿在 agy 这一格写「即便无效，代价只是它像今天一样自己更新——不会更糟」，评审指出这句是假的（§十三 #10）：今天没有任何东西在后台跑 agy，阶段 4 之后 Banager 的每次刷新跑 `agy --version` 两到三次（detect、inventory、reconcile）。若某个将来版本的 `--version` 又开始派生更新器且变量无效，Banager 自己就成了后台写机器的触发器。处理：每个 agy `verified_versions` 的录制（§9.3）都必须包含「`--version` 后 `log/` 无新文件、`update_status.json` mtime 不变」这一步并写进 README；哪一版做不到，就把那一版的版本读取改成不 spawn（例如读 `~/.gemini/antigravity-cli/updater/` 里安装器记录的清单）或 `version: None` 并在行上说明，不是「不会更糟」。what-we-run.md 照实写：1.2.10 的 `--version` 不写日志；带提示词的运行才写。

### 3.5 四个首批配方（数据）

| | claude | agy | grok | rustup |
|---|---|---|---|---|
| `id`（= 命令名） | `claude` | `agy` | `grok` | `rustup` |
| meta TOML `name` / `homepage` | Claude Code / `https://code.claude.com/docs/en/setup` | Antigravity CLI / `https://antigravity.google/docs/cli/install/` | Grok Build / `https://x.ai/build` | rustup / `https://rust-lang.github.io/rustup/` |
| `verified_versions`（meta TOML） | `["2.1.281"]` | `["1.2.10"]` | `["1.0.41"]` | `["1.29.1"]` |
| `route` | `SymlinkIntoRoot`，`~/.local/bin/claude` → `~/.local/share/claude`（VERIFIED：链接指向 `versions/2.1.281`） | `FlatFile`，`~/.local/bin/agy`，root `~/.gemini/antigravity-cli`（VERIFIED：176 MB Mach-O 普通文件） | `SymlinkIntoRoot`，`~/.grok/bin/grok` → `~/.grok`（VERIFIED：指向 `../downloads/grok-1.0.41-macos-aarch64`，**相对**链接） | `FlatFile`，`$CARGO_HOME/bin/rustup`，root `$CARGO_HOME`（VERIFIED：11 MB Mach-O 普通文件） |
| `latest` | `ClaudeChannel { base: "https://downloads.claude.ai/claude-code-releases" }`（`/latest`→`2.1.281`、`/stable`→`2.1.273`，均 VERIFIED 直接 200 无重定向；通道→端点映射 UNVERIFIED-by-inference，§3.1） | `HttpJsonField { url: ".../manifests/darwin_arm64.json", field: "version" }`（arm64 VERIFIED live；x86_64 主机上不查） | `Command { ["update","--check","--json"], 60 s, "latestVersion", "updateAvailable" }`（VERIFIED：其 `--help` 自述「without installing」，本机实测输出） | `HttpTomlVersion { "https://static.rust-lang.org/rustup/release-stable.toml" }`（VERIFIED，rustup 源码 `DEFAULT_UPDATE_ROOT` 就读它） |
| `self_updates` | `true`（VERIFIED 文档） | `true`（VERIFIED 日志 + 本机现场自更新） | **`false`**（`auto_update = true` 只被证实为「启动时检查」，是否静默安装 UNVERIFIED——grok.md §5 开放问题 2；核实前不把它写进用户看到的句子，§4.4） | `false` |
| `upgrade` | `["update"]`，1800 s，`KillThenReconcile` | **None** → `SelfUpdatesOnly` | `["update"]`，1800 s，`KillThenReconcile` | `["self","update"]`，600 s，**`NoCancel`**，加 cargo 锁 |
| `uninstall` | `Paths`（§6.3） | `Paths` | `Paths` | `Command`（§6.4） |
| `backup_globs` | `[]` | `[Glob { dir: "~/.local/bin", prefix: "agy.", suffix: ".old", what: Backups }]` | `[]` | `[]` |

agy 的 amd64 清单 URL 由安装脚本的 `${os}_${arch}` 拼法推出（VERIFIED 脚本逻辑，UNVERIFIED 该文件存在）。CI 的 Intel runner `curl` 一次确认后，加 `url_x86_64` 是一行改动（§十一）；在那之前 Intel Mac 上 agy 是一行「无法检查」，原因写「尚未在 Intel Mac 上核实」。

### 3.6 `inventory` / `reconcile` / `search`

- `inventory(inst)`：重新跑 §3.3 的第 1–4 步（一次 `canonicalize`、一次 `--version`），返回唯一制品；启动器不存在或指纹不符 → 空列表；`LauncherOnly`（悬空但链接文本指向 root）→ 制品存在、`version: ""`、`path: None`。**不是** detect 结果的缓存：刷新在实例锁下调它（`refresh.rs:275-277`），操作后 `reconcile` 必须看到磁盘现状。
- `reconcile(inst, key)` = `reconcile_from(inventory(inst).await?, key)`（`adapters/mod.rs:385-399`）：`present` = 启动器通过与 detect **同一套**路线判断（`symlink_metadata` + 指纹，悬空但指向 root 也算在——它还在，卸载得把它移走）；`version` 来自版本读取，失败为 `None`。路径清单卸载全部完成后启动器没了 → `present: false` → `Succeeded`（`ops/mod.rs:688-690`）；中途停下、程序目录已走而启动器悬空 → `present: true` → `StillInstalledAfterUninstall`（`:691`），下次刷新它是一行 `LauncherOnly`（§十三 #6/#7/#21）。
- `search` = `Err(Unsupported)`（同 `uv.rs:244`）。发现是阶段 5。

---

## 四、最新版本检查

### 4.1 端点（只用 VERIFIED）

见 §3.5 的 `latest` 行。首批**刻意不用 GitHub API**（匿名 60 次/小时，uv.md §4 观察到的 `x-ratelimit-limit`）；四个工具都有厂商自己的静态端点。第二批：deno `dl.deno.land/release-latest.txt`、mise `mise.jdx.dev/VERSION`、uv `releases.astral.sh/.../uv.ndjson` 首行、pnpm `registry.npmjs.org/-/package/pnpm/dist-tags`、Ollama.app `ollama.com/api/update`（204/200）、bun 只有 GitHub Releases（到时加 1 h 内存缓存）——全部在各自文件里 VERIFIED。

超时：HTTP 每请求 30 s（`HttpRequest.timeout`，`http/mod.rs:22`；`RealHttpClient` 的客户端默认也是 30 s，`real.rs:51`）。失败、非 200、解析失败 → 一行 `uncheckable_candidate(key, current, channel, lookup_failure_reason(...))`（`adapters/mod.rs:269-284, 319-336`），`UpdatesPage` 已渲染为「无法检查」；网络失败**永不**让 `check_updates` 返回 `Err`（那会让整个来源标 stale）。

`UpdateCandidate` 字段：`channel` = grok `Native`（工具自己答的）、其余 `Registry`；`checkable: true`；`warnings: []`；`target` = 远端串（trim）；`current` = 安装串；`key` 与制品同 → `Settings.ignored_updates` 不改（`updateState.ts:65-73`）。

### 4.2 HTTPS 主机 allowlist

`RealHttpClient`（`http/real.rs`）今天做对了的：rustls、`banager/{version}` UA、不跟随重定向（3xx 即错，`:50`）、30 s、8 MiB 体上限（`:22`）。它**没有主机名单**；`:43-49` 那段「the four endpoints this client talks to」是注释，不是检查（那个「四」把本机 http 的 Ollama 守护进程也算进去了：HTTPS 主机其实是 3 个）。阶段 4 把 HTTPS 主机从 3 涨到 **6**（连本机 Ollama 的 http 端点算 7 个），第二批 11。

加：

```rust
// http/real.rs
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &[
    "crates.io", "pypi.org", "registry.ollama.ai",                              // 阶段 3
    "downloads.claude.ai",
    "antigravity-cli-auto-updater-974169037036.us-central1.run.app",
    "static.rust-lang.org",
];
```

`send()` 在发请求前解析 `req.url`：scheme 为 `https` 且 host 不在名单 → `HttpError::Network("host not allowed: …")`。`http://` **豁免**：本 crate 里唯一的 http 调用方是 Ollama 守护进程（`HostEnv.ollama_host`，可指向别机）。已知缝隙（记 §十二 Q8）：用户把 `OLLAMA_HOST` 设成 `https://` 指向另一台机器时会被名单挡住——`RealHttpClient::new()` 在 `Session::new`（`session/mod.rs:259`）里不带 `HostEnv`；真要放行需要 `with_extra_host(ollama_host)`，等有人报了再做。

读取方：`send()` 本身；一条单元测试遍历 `RECIPES` 每个 `Latest` 的 URL 断言 host 在名单内（新增配方带新主机 = 两处改动一起审）；`docs/what-we-run.md` 的「Banager 只连接这些主机」一节。同一改动里把 `:43-49` 那段「four endpoints」注释改成指向常量——它是一句会变假的代码断言。

### 4.3 版本比较

现有适配器用 `!=`（`cargo.rs` 的比较），因为它们的注册表从不报比已装更旧的版本。独立安装工具会：claude 的 `/stable` 指针（2.1.273）此刻就比本机 latest 频道的 2.1.281 旧。规则：**仅当 remote > local 才建候选**，按 `.` 拆成整数序列逐位比较（短者小；任一分量非数字 → 整串不可比 → `uncheckable_candidate`，原因写出两个串）。calver（mise `2026.9.12`）在这条规则下正确（mise.md §7.4：字符串比较会把 `9` 排到 `12` 后）。grok 直接信它自己的 `updateAvailable`。`latest.rs` 里约 20 行 + 一张表测试；不引 `semver` crate。

### 4.4 自更新工具的诚实语义（D5 定案）

事实：claude 原生版运行时后台更新（VERIFIED 文档），几周不运行就会落后；agy **每次调用**都检查（15 分钟节流，VERIFIED 日志，且本稿写作期间它就自更新了一次）；grok `auto_update = true` 表示「启动时检查」，是否静默安装 UNVERIFIED；rustup 不自更新。

三份设计的分歧：safety 要把这些工具藏在「包含自更新的应用」（`Settings.include_self_updating`，默认关）后面；reuse/journey 照常显示并解释。定案**照常显示**：

1. `InstalledArtifact.auto_updates` 按配方求值（claude/agy `true`，grok **`false`**，rustup `false`）。grok 是唯一一个事实 UNVERIFIED 的：`auto_update = true` 只被证实为「启动时检查」（grok.md §5 开放问题 2），而 `selfUpdatingHint` 会把「它通常会自己更新」当事实告诉用户——UNVERIFIED 的事实不能在用户看到的句子里成为承重墙（§十三 #25）。核实前 grok 走普通路：徽章 + 「更新」按钮 + `grok update`；哪怕它其实会静默自更新，结果也只是 `UnchangedAfterUpgrade`，准确。CI 录制到静默安装的证据后，把 `self_updates` 改成 `true`、恢复读 `config.toml` 的 `GrokConfig` 形状（§十一），一处改动。
2. `check_updates` **不看** `CheckOptions.include_self_updating`（`adapters/mod.rs:27-29` 的注释就写着「Homebrew only」，brew 的 `--greedy` 在 `brew/mod.rs:790-791`）。徽章说的是「这份此刻比最新版旧」，由启动器的活版本比出来，不是 `brew outdated` 那种查不到 cask 活版本的噪声。
3. **`auto_updates` 得到第一个非 Pinned 读取方**：`UpdatesPage.tsx` 的 `rowDescription`（`:444-475`）新增一支——候选可操作（`isActionable`）、`artifactsById` 里该制品 `auto_updates === true`、且 `instance.adapter_id` 以 `standalone-` 开头（不动 `--greedy` 列出的自更新 cask 的描述）→ `updates.selfUpdatingHint`。
4. 配方 `upgrade: None`（agy：`agy update` 存在但零文档、零选项、未被任何人运行过，agy.md §4）→ 每个候选 `blocked: Some(UpdateBlocked::SelfUpdatesOnly)`（新变体）。读取方：闸门 `blocked_upgrade`（`plans.rs:81`，已通用）在 `:184` 拒绝 Upgrade；`updateStateOf`（`updateState.ts:49`）判为 blocked 隐藏按钮与勾选框；`UPDATE_BLOCKED_KEYS`（`sources.ts:351`，`Record<UpdateBlocked, …>`，漏写编译失败）供徽章/描述/拒绝文案，其 `command` 返回 `instance.exe_path`（在终端里可直接运行的绝对路径——它会打开 agy 的界面，句子照实说「打开一次再退出」；不能改成 `<exe> --version`，本机 1.2.10 实测 `--version` 根本不触发更新器，§3.4），文案加上「它最多每 15 分钟检查一次」（agy.md §4 的节流，否则用户「运行了一次却没更新」是必然的困惑；§十三 #31），`selfUpdatingDescription*` 为 `null`（理由本身就是自更新）。为什么是 blocked 候选而不是「无候选」：无候选让已安装行说「已是最新」（`installed.upToDate`），1.2.11 存在时那是谎话；为什么不是 `checkable: false`：那表示「查不到」，而 Banager 查到了。
5. 「版本没动」：`claude update` 已最新时打印 `Claude Code is up to date (X)` 退出 0（VERIFIED 文档）。`run_operation` 前后各读一次（`ops/mod.rs:630-639`、`:655`），相等 → `NeedsAttention(UnchangedAfterUpgrade)`（`:697-700`），现有文案「程序并没有更新它，操作日志里也许能看到原因」——准确。真实竞态（检查与点击之间工具自己更新了）同样落到这里，同样准确。中途停止 → `Unconfirmed`（`:774-777`）。阶段 4 在 `ops/` 里不改一行。

已知缺口（记 §十一）：本机 claude 的自更新被 shell 里的 `DISABLE_AUTOUPDATER=1` 关掉，Finder 启动的 Banager 看不见这个变量，所以 `selfUpdatingHint` 说「**通常**会自己更新」；一个后续改进是 claude 专用地读 `~/.claude/.last-update-result.json` 的 `timestamp`（VERIFIED 存在）。

---

## 五、升级

`plan(inst, Upgrade)` = `ensure_instance_match`（`adapters/mod.rs:414-422`）→ `validate_package_name(req.name)` 且 `== recipe.id` → 无 `upgrade` 则 `Err(UpdateBlocked { SelfUpdatesOnly })`（与闸门同一变体，兜住快照过期的情况）→ 按表建 `Plan`（`action: Command { program, args, env: [] }`）；`execute` = `run_plan`（`:469-514`）不变。`needs_password: false`（四条路线全在 `$HOME`，无提权）；`warnings: []`；`affected: []`；`locks: [inst.id]`（rustup 多一把 cargo 锁，§2.4）。

| 工具 | `program` | `args` | 超时 | `cancel_policy` | 依据 |
|---|---|---|---|---|---|
| claude | `~/.local/bin/claude` | `update` | 1800 s | `KillThenReconcile` | 新版本下到 `versions/<new>` 新文件、校验后才重指链接（VERIFIED 读 install.sh 与文档）；杀在中途最多留一个未链接的文件。官方无「可中断」承诺，预览不承诺 |
| grok | `~/.grok/bin/grok` | `update` | 1800 s | `KillThenReconcile` | 新文件 + 重指链接，旧版留在 `downloads/`（VERIFIED 布局；重指原子性 UNVERIFIED，但旧二进制无论如何还在） |
| rustup | `$CARGO_HOME/bin/rustup` | `self update` | 600 s | **`NoCancel`**；`locks` 加 `cargo:<cargo_home>` | 单文件下载后替换，原子性 UNVERIFIED（rustup.md §7 开放问题 1）；一个撕裂的 `rustup` 会让 13 个代理全坏——包括 cargo 刷新正要跑的 `cargo`，所以持 cargo 锁（§2.4，§十三 #11）。**永不** `rustup update`（会动工具链；rust-lang/rustup#4724 证明中断留下半装的工具链）。超时仍会结束它并报 `Unconfirmed`（`run_plan:500-502` → `ops/mod.rs:787-799`） |
| agy | — | — | — | — | `SelfUpdatesOnly` |

1800 s 是 spec §4.1 的「安装/升级 30 min」；claude 下载约 220 MB。

**两个升级命令都没有在调研中被执行过**（grok.md §4「NOT executed」；claude.md §6 记录过 2.1.246 之前的 TTY 提示挂起）。`RealRunner` 已经把子进程的 stdin 接到 `/dev/null`（`runner/real.rs:670`），一个读 stdin 的确认提示会立刻拿到 EOF 而不是等 30 分钟——但工具对 EOF 的反应（退出 1、当作「否」、还是忽略后重试）没人看过。步骤 D 之前在 CI runner 上各录一次真实的 `grok update` / `claude update`（stdin 关闭），把观察到的非交互行为写进 what-we-run.md；哪个会挂，就给那个工具的 `UpgradeCmd` 加它自己的非交互开关（§十三 #34）。

`NoCancel` 的两个读取方**已在 HEAD**：`OperationManager::cancel` 在 Running 时拒绝（`ops/mod.rs:331`）、`OperationBar.tsx:38` 在 Running 时不给按钮（Queued 时给——什么都还没启动）。rustup 是第一个生产者：步骤 E 把四处「No adapter produces `NoCancel` yet」（§0.1）改为指向 rustup，并把 `ipc.rs:1663-1770` 与 `OperationBar.test.tsx` 里两个 NoCancel 用例的名字改成点名 rustup。确认框在预览下多一句 `operations.noCancelHint`，**两个读取方**：`UpdatesPage.tsx:972` 的 `CommandPreview` 之后，与 `UninstallDialog.tsx:253` 的 `CommandPreview` 之后（rustup 的卸载同样 `NoCancel`，§6.6 的样稿里那句话得有人渲染——初稿只给了 UpdatesPage 一个读取方，§十三 #9），都按 `plan.cancel_policy === "NoCancel"` 显示；文案「一旦开始就不能取消」与 Running-only 语义一致。

---

## 六、卸载

### 6.1 三种情形

- **Command**（rustup）：官方卸载命令，走 `run_plan` 不变（§6.4）。
- **Paths**（claude、agy、grok）：厂商文档的路径清单，`execute()` 逐条调系统「移到废纸篓」（`PlanAction::TrashPaths`，不经 `run_plan`；§6.2–6.3）。
- **Neither**（第二批工具在核实前；首批在步骤 C 落地前的 claude）：制品带 `uninstall_blocked: Some(UninstallBlocked::NoSafeMethod)`（新变体）。读取方：闸门 `blocked_uninstall`（`plans.rs:103`）在 `:187` 拒绝；`UNINSTALL_BLOCKED_KEYS`（`sources.ts:451`）；`InstalledPage.tsx:144`（徽章）与 `:329-345`（把理由放在描述位、藏起卸载按钮）。这是**每制品**的，不是实例的 `read_only_reason`：那表示「整个来源只能看」，会把升级也藏掉。界面绝不提供 Rust 会拒绝的操作，而 Rust 在闸门就拒绝了，配方代码根本跑不到。

### 6.2 路径清单卸载 = 一个 `TrashPaths` 计划，`execute()` 逐条调系统「移到废纸篓」

初稿在这里写的是「一条 `/bin/mv -n <路径…> ~/.Trash/Banager – <工具> <时间>/`」，理由是 `Plan` 就是一个程序一个 argv，
不该在唯一的执行模型旁边再立一个。评审用本机复现推翻了它（§十三 #19、#20、#23、#13）：

- claude 的两条路径基名都是 `claude`：`mv -n` 先把链接移成 `dest/claude`，再遇到同名目录就**静默跳过、退出 0**——626 MB 的 `~/.local/share/claude` 原地不动，`reconcile` 看启动器没了 → `Succeeded`。**一条 argv 表达不了这次卸载**；去掉 `-n` 也只是换成「Not a directory」退出 1。
- `~/.Trash` 受 TCC 保护：Finder 启动的、没有「完全磁盘访问权限」的 Banager，连它派生的 `/bin/mv` 也会在 rename 时得到 EPERM，而初稿检查 6 的 `access(W_OK)` 不问 TCC（本会话里 UNVERIFIED——这个终端已有 FDA；步骤 C 之前必须核，见下）。
- 单个源 `mv` 到不存在的目录会把源改名成那个名字并退出 0（agy 没有 `.old` 也没有 `~/.cache/antigravity` 时正是一条路径），`create_dir_all` 对已存在目录也成功，同一分钟里第二个 plan 会执行进一个已有内容的目录。

定案：

**`Plan` 加一个两臂枚举**（本稿唯一的 `Plan` 形状改动，Q16）：

```rust
// model.rs
pub enum PlanAction {
    /// 一个程序一个 argv：今天每个 Plan 都是它。`run_plan` 只接受它。
    Command { program: PathBuf, args: Vec<String>, env: Vec<(String, String)> },
    /// 没有命令：execute() 按顺序把每条路径交给系统的「移到废纸篓」。只有 Paths 卸载产生它。
    TrashPaths { paths: Vec<PathBuf> },
}
pub struct Plan { pub request: OpRequest, pub action: PlanAction, pub needs_password: bool, pub locks: Vec<ResourceLock>,
                  pub cancel_policy: CancelPolicy, pub warnings: Vec<Warning>, pub affected: Vec<String>, pub timeout_secs: u64 }
```

`program`/`args`/`env` 三个字段并入 `Command`。改动面（机械，一次改完）：32 处 `Plan {` 字面构造（7 个适配器 + `session/plans.rs`、`session/test_support.rs`、`ipc.rs` 与七个 `tests/ops_*_test.rs`）；读取方四处——`run_plan`（`adapters/mod.rs:469`；`TrashPaths` → `Err(Refused("not a command"))`，理论不可达）、`OpSummary.argv_preview`（`ops/mod.rs:284-285`：`Command` 照旧，`TrashPaths` → 空 `Vec`；这个字段今天在 `src/` 里没有渲染读取方，只有 `types.ts:202` 的镜像——不另造假命令行）、`CommandPreview`（`UninstallDialog.tsx:253`、`UpdatesPage.tsx:972`：新增一支 `TrashPaths`，见下）、`types.ts` 镜像（`action: { Command: { program; args; env } } | { TrashPaths: { paths: string[] } }`，外部标记，`types.test.ts` 加形状用例）。

**执行**（`execute()`，`removal.rs`）：
1. 取 `Detected`；把 `plan.action` 解成 `paths`（它就是 `plan()` 时从 `warnings` 里的 `WillTrash` 建出来的同一批，顺序相同）。
2. 对每条路径重跑 §6.3 检查 1–5，并记下 `(st_dev, st_ino)`；任一差异 → `Ok(Outcome::BanagerFailed(Fault::PathChanged { path }))`，**一条都不移**。
3. 逐条：先看 `cancel` 令牌（已取消 → 停，返回 `Ok(Outcome::Unconfirmed)`，`run_operation` 的 stopped 臂（`ops/mod.rs:787-799`）reconcile 后如实报 `Cancelled` 或 `StillInstalledAfterUninstall`）；再 `symlink_metadata` 取一次 `(st_dev, st_ino)`，与第 2 步不同 → `PathChanged`；然后 `trasher.trash(&path)`。成功 → `sink.emit(OperationEvent::Log { stream: Stdout, line: "Moved <path> to the Trash (<新位置>)" })`（`events.rs:50-54`，与 `run_plan` 给命令输出用的同一条通道）；失败 → 一行 NSError 文本进日志，返回 `Ok(Outcome::Failed { exit_code: None, summary })`（`model.rs:407-410`）——前面移走的留在废纸篓，启动器（最后一条）还在，下次刷新行还在，可以重来。
4. 全部成功 → `Ok(Outcome::Succeeded)`，`run_operation` 再 `reconcile`（`ops/mod.rs:688-691`）。

**`Trasher`**（`crates/banager-core/src/trash/`）：与 `CommandRunner`/`HttpClient` 同一套路。`pub trait Trasher: Send + Sync { fn trash(&self, path: &Path) -> Result<PathBuf, TrashError>; }`（返回项目在废纸篓里的新位置，日志用）；`RealTrasher`（`#[cfg(target_os = "macos")]`，`objc2-foundation` 的 `NSFileManager::trashItemAtURL_resultingItemURL_error`——对符号链接移链接本身、不解析目标，e2e 测试断言这一点）；`MockTrasher`（rename 进一个临时目录，记录调用序列，可注入「第 N 条失败」）。`StandaloneAdapter` 多一个 `trasher: Arc<dyn Trasher>`，`Session::new`（`session/mod.rs:259-271`）注入 `RealTrasher`。这是本稿唯一新增的依赖（`objc2` + `objc2-foundation`，只在 macOS target 下；Q16 列了 `trash` crate 的替代——它的 `DeleteMethod::NsFileManager` 是同一个调用，而它默认的 `Finder` 方法走 AppleScript、会弹「自动化」授权，不可用）。

**顺序**：配方 `remove` 的顺序 = 执行顺序：程序目录、缓存、备份在前，**启动器最后**。中途停下（取消、某条失败、Banager 被杀）的唯一残留形态是「程序目录已进废纸篓、启动器成了悬空链接」——它在下次刷新里仍是一行（§3.3 `LauncherOnly`），再点一次卸载把链接移走（已不在的程序目录列为 `AlreadyGone`，§6.3 检查 2）。反过来（启动器先）会让 626 MB 隐形：detect 找不到启动器，行消失，而 `~/.local/share/claude` 不是 bin 目录，来源不明页也看不到（§十三 #7/#21）。

**为什么是废纸篓、为什么是系统调用（Q1）**：同卷 rename、瞬间完成；访达「放回原处」可用（初稿的 `mv` 做不到，文案只敢说「拖回去」）；不需要 FDA；`/usr/bin/trash` 只在 15.0+ 存在，`rm -rf` 是终端用户跑的、选了 GUI 的人期望「卸载」像把 app 拖进废纸篓。跨卷不再是问题：系统调用会用那一卷自己的 `.Trashes`，仍是 rename。代价：这是 Banager 第一处进程内文件系统写入——what-we-run.md 与附录 B 都写明；测试断言 `MockTrasher` 的调用序列逐字等于 `plan` 的 `paths`、临时 HOME 里除此之外无变化。

**步骤 C 合并前的核实（阻塞）**：用一个 Finder 启动的、无 FDA 的开发构建（从终端 `pnpm tauri dev` 起的进程继承终端的 FDA，不算）：(1) 对临时目录里的一个文件与一个符号链接各调一次 `RealTrasher::trash` → 成功、项目出现在废纸篓、「放回原处」可用、链接移的是链接本身；(2) 同一进程 `std::fs::rename` 进 `~/.Trash/x` → 预期 EPERM（记录下来，证明初稿的 `mv` 方案在真实上下文里不可用；若意外成功，也不回到 `mv`——基名冲突与「放回原处」两条理由仍在）。两条结果写进 `what-we-run.md`。如果 (1) 也失败（不预期），回退是 `Trasher` 的另一个实现（同一 trait、同一 `Plan`、同一预览）：`std::fs::create_dir`（**不是** `create_dir_all`；`AlreadyExists` → `PathChanged` 类拒绝，目标必须是新建的）建 `~/.Trash/Banager – <工具> <时间>/<序号>/` 再逐条 `rename`——那时就得接受一次「完全磁盘访问」之旅，这是 spec §1 不允许的摩擦，由作者拍板。

`needs_password: false`；`timeout_secs: 120`（`execute()` 自己对 `Instant` 计时，超出 → 停下并 `Unconfirmed`）；`cancel_policy: KillThenReconcile`（在这里的含义：条目之间看令牌，没有进程可杀；一次 `trashItemAtURL:` 是一次 rename）；`locks: [inst.id]`；`affected: []`（那个字段表示「会坏掉的依赖方」，非空会禁用确认按钮，`UninstallDialog.tsx:87,209-248`）。

**预览**（`CommandPreview` 的 `TrashPaths` 支）：一句 `uninstall.trashPreview`（「Banager 会自己把上面列出的 {{count}} 项移到废纸篓——不运行任何命令，也不删除任何东西：清空废纸篓之前都能放回来。」），项目本身由「继续之前请注意」里的 `WillTrash` 逐条列出（§6.6）。初稿的 `uninstall.trashNote` 并入这句；它的读取方不再是「任一 `WillTrash` 存在」，而是 `plan.action` 的形状。

### 6.3 可以移什么：五条运行时检查 + 一条配方测试，`plan()` 一遍、`execute()` 再一遍

对每条 `RemoveSpec` 与每个 `backup_globs` 命中，路径按 `Detected.home` 展开：

| # | 检查 | 失败时（`AdapterError::UninstallUnsafe { path, reason }`，§9.1） |
|---|---|---|
| 1 | `canonical_home = canonicalize(home)`；`canonical_parent = canonicalize(path.parent())`——**每种 `Expect` 都做**（初稿只对 `Dir` 再 canonicalize 一次，父目录是链接时 `File`/`SymlinkIntoRoot` 会逃逸，§十三 #26）；`canonical_parent` 以 `canonical_home` 开头，且之下至少两层（永不是 `~`、`~/.local`、`~/.config`、`~/.cache`、`~/Library`、`~/.cargo`）。用户自设的 `~/.local/bin → /Volumes/Data/bin` 因此被拒——不是因为不安全，而是「Banager 只动个人文件夹里的东西」得是真的；`$HOME` 本身是链接时两边都 canonical，不误拒。首批没有一条路径在 `home` 外；这条是防将来配方悄悄扩大爆炸半径 | `outside_home` |
| 2 | `symlink_metadata(path)` 成功。失败时：`optional` → 跳过（`~/.cache/antigravity` 不在不是错）；非 optional、**实例是 `LauncherOnly`**（§3.3）且这条不是启动器 → 跳过并发 `Warning::AlreadyGone { path }`（上次中断留下的状态，这次只剩启动器要移）；否则 | `missing` |
| 3 | `st_uid == Detected.euid`（`HostEnv.euid`，`path_env.rs:8`）——是用户自己的 | `not_owned_by_you` |
| 4 | `expect`：`SymlinkIntoRoot` → 是符号链接，且 `canonicalize(path)` 以 `canonicalize(expand(root))` 开头，**或**（悬空时）`read_link` 文本按 §3.3 第 2 步的词法归一后以 `expand(root)` 开头；`File` → 普通文件，非链接；`Dir` → 目录，非链接，且 `canonicalize(path)` 本身也满足 1。**`optional: true` 的路径指纹不符 → 不是拒绝**：跳过并发 `WillKeep { path, what: NotOurs }`（grok 的 `~/.local/bin/agent` 是个通用名，本机就有好几个别的 agent CLI；grok 的安装器只在 `~/.grok/bin` 不在 PATH 上时才放这个回退链接，一个外来的 `agent` 至少和 grok 的一样常见，不该让 grok 永远卸不掉，§十三 #27）；非 optional 才拒绝 | `not_what_instructions_expect` |
| 5 | glob：`read_dir(dir)`，留下 `prefix` 开头 `suffix` 结尾的普通文件，每个再过 1、3、4；逐个进 `paths` 与 `WillTrash`，预览里每个文件都看得见 | 同上，按文件 |

初稿的检查 6（`~/.Trash` 可写）**删除**：无 FDA 的进程连 `access(W_OK)` 都答不对（§0.1），而系统调用自己会回答——失败在 `execute()` 里是一行日志 + `Outcome::Failed`（§6.2），`trash_unavailable` 这个 reason 随之不存在。初稿的检查 7（清单里没有一条路径是另一条的前缀）是**配方常量的性质**，不是运行时的：`recipes.rs` 的单元测试遍历 `RECIPES` 断言，不进 `plan()`（§十三 #14）。

全部跳过后 `remove` 为空 → `AdapterError::Refused`（走通用 `refused`，**有意**不给专门文案：制品在列表上就意味着启动器存在，这条不可达）。`detected` 为 `None` 同样通用 `Refused`（§3.2）。

**`execute()` 对同一批绝对路径再跑 1–5**，并在移每一条之前比对 `(st_dev, st_ino)`，任何与 `plan()` 所见的差异（链接被重指、文件变成目录、属主变了、被替换成同名的另一个 inode）→ `Ok(Outcome::BanagerFailed(Fault::PathChanged { path }))`（新 `Fault` 变体），**一条都不移**（第一条之前）或停在那一条（之后，前面的已在废纸篓）。这是 brew 为 `brew uses` 读取做的 `catalogue_stamp` TOCTOU 防线（`brew/mod.rs:578,1242` 附近）用在真正要紧的地方：路径清单是从文件系统里建出来的，预览到确认之间的几分钟里文件系统会变（`PLAN_LIFETIME` 600 s，`plans.rs:18`）。

**在构造上永不上清单的**：`$HOME` 之外的任何东西（grok 安装器可能在 `/usr/local/bin` 放回退链接；存在则作为 `WillKeep { OutsideHome }` 报告，告诉用户它会变成失效链接、可以自己删）；shell 启动文件（Banager 不编辑 `.zshrc`/`.zprofile`，安装器加的行留着，作为 `WillKeep { ShellConfigLines }` 报告）；共享配置与凭据（`~/.claude`、`~/.claude.json`、`~/.grok` 除 `bin/downloads/bundled/completions` 外、`~/.gemini/antigravity-cli`；`~/.gemini` 根与 Gemini CLI 共用，**永不整目录删**）——默认保留，阶段 4 无「全部清除」（Q4）。

三个 `Paths` 配方（`remove` 按执行顺序，启动器最后）：

| 工具 | `remove`（path · expect · what · optional） | `keep`（path · what） | 依据（写在配方常量的 `///` 注释与 fixture README，不是字段） |
|---|---|---|---|
| claude | `~/.local/share/claude` · Dir · Program；`~/.claude/downloads` · Dir · Cache · optional（厂商文档写明的更新暂存目录，install.sh 的 `DOWNLOAD_DIR`，claude.md §2a；它只被原生更新器用，cask 路线不自更新；本机今天存在但为空）；`~/.local/bin/claude` · SymlinkIntoRoot · Launcher（**最后**） | `~/.claude` · SettingsAndHistory（本机 4.4 GB：`projects/`、`file-history/`、`cache/`、`chrome/`、`shell-snapshots/`、`backups/`、`uploads/`……文案说「设置、登录信息、历史记录和工作文件」，不再只说「设置」）；`~/.claude.json` · Settings | code.claude.com/docs/en/setup「Uninstall Claude Code → Native」（VERIFIED 原文就是那两条 `rm`）+ install.sh 的 `DOWNLOAD_DIR` |
| agy | `~/.cache/antigravity` · Dir · Cache · optional；glob `~/.local/bin/agy.*.old` · Backups；`~/.local/bin/agy` · File · Program（最后；它就是启动器） | `~/.gemini/antigravity-cli` · **ToolState**（它是工具自己的根，不是「你的数据」：本机 `bin/` 12 MB 可执行文件、`builtin/`、`cache/`、`updater/`、`log/` 50 MB 与 `conversations/` 196 MB、`brain/` 103 MB 混在一起。没有厂商清单说哪些子目录可以单独删；唯一厂商编写的删除清单——cask 的 zap——把它当一个整体。Q4 保留整个目录，文案照实说「它的对话、历史和工作文件；程序自己的一些文件也在里面」，§十三 #24）；`~/.zshrc` · ShellConfigLines；`~/.zprofile` · ShellConfigLines（两处都有 `# Added by Antigravity CLI installer`，agy.md §2） | 无厂商卸载文档；清单是 install.sh 的 `TARGET_DIR`/staging 路径 + cask `antigravity-cli` 的 zap（只 trash `~/.gemini/antigravity-cli`）的综合（agy.md §5 自己标明是综合，不是厂商文本），README 里写明 |
| grok | `~/.grok/downloads` · Dir · Program（本机约 397 MB，含三个版本）；`~/.grok/bundled` · Dir · Program · optional；`~/.grok/completions` · Dir · Program · optional；`~/.config/fish/completions/grok.fish` · File · Program · optional（install.sh 也生成它，grok.md §2；本机存在，148 KB，初稿漏了）；`~/.local/bin/grok` · SymlinkIntoRoot · Launcher · optional；`~/.local/bin/agent` · SymlinkIntoRoot · Launcher · optional；`~/.grok/bin` · Dir · Launcher（**最后**；`Route.launcher` `~/.grok/bin/grok` 在它里面） | `~/.grok` · SettingsAndHistory（`config.toml`、`auth.json`、`sessions/`、`memory/`、`skills/`、`plugins/`）；`~/.zshrc` · ShellConfigLines；`/usr/local/bin/grok` 与 `/usr/local/bin/agent` 若存在 · OutsideHome | README「File Locations」表 + install.sh（无厂商卸载文档；de-facto 的 `rm -rf ~/.grok` 会删掉登录、会话、记忆——本稿拒绝） |

agy 的 `.old`：瞬态（§0.1），`optional` 语义天然覆盖「这半小时之外它不在」；在那半小时之内碰上就一并移走，预览里列出。

### 6.4 rustup：官方命令，四条固定警告 + 一条条件警告，两把锁

`rustup self uninstall -y`（VERIFIED 源码 `uninstall()` / `clean_cargo_home()`，rustup.md §8）删：所有工具链、整个 `RUSTUP_HOME`、`CARGO_HOME` 里除 `bin/` 的一切（`registry/`、`git/`、`.crates.toml`、`.crates2.json`、`env`……）、`bin/` 里的 rustup 与 **13 个代理**（`TOOLS` 10 + `DUP_TOOLS` 3，本机 `ls -la ~/.cargo/bin` 全部在；rustup.md §8 写的「12」数错了）；**不动** `cargo install` 装的真实二进制（本机 `hexyl`），因此 `~/.cargo/bin` 与 `~/.cargo` 非空而留下；不传 `--no-modify-path` 时它会自己从 shell 配置里删掉当初加的 PATH 行（Q6：让它删——传 `--no-modify-path` 会留下一行每开一个终端都报错的 `. "$HOME/.cargo/env"`，对小白更糟）。

只看 argv 的预览告诉不了小白这些。`Uninstall::Command` 的 `probe = ["toolchain", "list"]`（30 s，只读，VERIFIED 输出 `stable-aarch64-apple-darwin (active, default)`），`warnings(&Detected, probe_stdout)` 返回：

- `Warning::RemovesToolchains { names }`——每行第一个 token；probe 失败时 `names: []`，前端换一句不带名字的话；
- `Warning::DeletesCargoCaches`（单位变体）——Cargo 的下载与安装记录一起没了；
- `Warning::LeavesUnmanaged { names }`——读 `Detected.cargo_home/.crates2.json`。**不能**靠把 `parse_crates2_entries`（`cargo.rs:45`）改 `pub(crate)`：它从 `installs` 的**键**里取 `(name, version, source)`，值（`"bins":[…]` 所在）被扔掉（`cargo.rs:40-43` 的 `HashMap<String, serde_json::Value>`）——`hexyl` 碰巧 crate 名等于二进制名，`ripgrep` 就会说「ripgrep 会留在 Mac 上」而留下的其实是 `~/.cargo/bin/rg`，来源不明页列的也是 `rg`（§十三 #4）。新函数 `cargo::parse_crates2_bins(json) -> Result<Vec<(String, Vec<String>)>>`（serde 结构体反序列化 `installs.*.bins`，不用 `Value`），`names` = 所有 crate 的 `bins` 展平（本机：`hexyl`）；空则不发。fixture 用**已有的** `adapters/fixtures/cargo/1.98.1/crates2.json`（真机录制，含 `"bins":["hexyl"]`）。同一函数在步骤 E 里也给 cargo 适配器的 `inventory` 填 `InstalledArtifact.path = {cargo_home}/bin/{bin}`（§8.3 规则 2 的 cargo 输入）。这是真实的跨适配器后果：cargo 适配器下一轮 inventory 找不到 `.crates2.json` → 报「没装东西」（`cargo.rs:181` 注释：文件不在是「nothing installed」不是失败），而 `hexyl` 还在 `~/.cargo/bin`——正是来源不明页要显示的东西；文案还得说清第二个后果：rustup 删掉 rc 文件里的 PATH 行后，终端也**找不到** `hexyl` 了，除非用户自己把 `~/.cargo/bin` 加回 PATH（§十三 #28）；
- `Warning::EditsShellConfig`（单位变体）——它会改你的 shell 设置文件；
- `Warning::LeavesShellConfigLine { path }`（**条件发出**，每个文件一条）：`plan()` **只读**（永不写）`~/.zshenv`、`~/.zprofile`、`~/.zshrc`、`~/.bash_profile`、`~/.bash_login`、`~/.bashrc`、`~/.profile`、`~/.config/fish/config.fish`，凡含 `.cargo/env` 引用且**不在 rustup 自己会清理的 rc 集合里**的文件各发一条。rustup 的 `remove_from_path` 只改它 `src/cli/self_update/shell.rs` 认识的文件（按作者记忆：zsh 是 `$ZDOTDIR/.zshenv` 或 `~/.zshenv`；POSIX 是 `~/.profile`；bash 是 `~/.bash_profile`/`~/.bash_login`/`~/.bashrc`；fish 是 `~/.config/fish/conf.d/rustup.fish`——**这份集合 UNVERIFIED**，步骤 E 合并前对照那个文件核一遍，写进 README）。本机就是现成样本：`~/.zshenv:1` 与 `~/.profile:1` 是 rustup 写的，`~/.zshrc:17` 的 `. "$HOME/.cargo/env"` 不是——卸载后每开一个 zsh 都会打印 `no such file or directory: …/.cargo/env`，正是 Q6 选择让 rustup 自己删 PATH 行想避免的那种失败，所以要提前告诉用户哪个文件里还有一行要自己删。

`affected` 留空（非空会禁用确认，`UninstallDialog.tsx:209-248`；`hexyl` 不会「坏」，只是没人管了）。`NoCancel`，600 s，`extra_locks(&Detected)` 返回 `[ResourceLock(cargo::instance_id_for(&d.cargo_home))]`（§2.4，单一生产者）。事后 `reconcile` 找不到 `$CARGO_HOME/bin/rustup` → `Succeeded`；cargo 适配器下次 detect 找不到 `cargo` 代理 → 该实例消失（对它的过期 plan 报 `SourceGone`），正确。是否提供由 Q5 定。

**Homebrew 的 `rustup` formula 孪生**（rustup.md §2 Route B，本机没有）：keg-only，二进制在 Cellar 下，但 `RUSTUP_HOME`/`CARGO_HOME` 与安装路线无关（rustup.md §2 标 UNVERIFIED 为直接测试，由 rustup 的设计强烈蕴含）——原生的 `self uninstall` 删 `~/.rustup` 时，孪生的工具链一起没了。`RemovesToolchains` 说的「所有工具链」对它同样成立，后果已经在第一条警告里，所以不为它单加一条（§十三 #29 部分采纳）；`plan()` 看不到合并后的快照，也没有可靠的本地信号说「有孪生」。两个 UNVERIFIED 点记 §十一，CI runner 装一次 brew `rustup` 录制后再定要不要点名它。

### 6.5 新的 `Warning` 变体

`Warning`（`model.rs:263-283`）已有带载荷变体（`WouldBreak { names }`、`ThirdPartyRegistry { host }`），外部标记形状与 TS 镜像（`types.ts:79-85`）一致，`model.rs:700` 与 `types.test.ts:197` 各有一条形状测试。新增八个，`{{path}}` 在 Rust 侧已把 `$HOME` 前缀换成 `~`（那是数据不是句子；`TrashPaths.paths` 保持绝对路径）：

| 变体 | 载荷 | 生产者 | 读取方 | 用户看到 |
|---|---|---|---|---|
| `WillTrash { path, what: RemovedWhat }` | `RemovedWhat = Launcher \| Program \| Backups \| Cache` | `Paths` 的 `plan()`，每条一个，顺序 = 执行顺序 | `warningKey/warningArgs`（`warnings.ts`）→ `UninstallDialog.tsx:215-226` 的项目列表 | 「移到废纸篓：~/.local/bin/claude（命令本身）」 |
| `WillKeep { path, what: KeptWhat }` | `KeptWhat = Settings \| SettingsAndHistory \| ToolState \| ShellConfigLines \| OutsideHome \| NotOurs` | `Paths` 的 `plan()`，每条一个；`NotOurs` 来自检查 4 的 optional 路径 | 同上 | 「保留：~/.claude（你的设置、登录信息、历史记录和工作文件，其它应用也可能在用）」 |
| `AlreadyGone { path }` | `String` | `Paths` 的 `plan()`，检查 2 在 `LauncherOnly` 实例上 | 同上 | 「已经不在了：~/.local/share/claude（没有东西要移）」 |
| `RemovesToolchains { names }` | `Vec<String>` | rustup `warnings` | 同上；`names` 空时换键 `warnings.removesToolchainsUnlisted` | §6.6 |
| `DeletesCargoCaches` | 无 | rustup | 同上 | |
| `LeavesUnmanaged { names }` | `Vec<String>` | rustup（非空才发） | 同上 | 「hexyl 是用 Cargo 装的，会留在 Mac 上，但以后没有工具管理它了，终端也找不到它，除非你自己把 ~/.cargo/bin 加回 PATH。」 |
| `EditsShellConfig` | 无 | rustup | 同上 | |
| `LeavesShellConfigLine { path }` | `String` | rustup（每个含残留引用的 rc 文件一条） | 同上 | 「卸载后 ~/.zshrc 里还有一行会去加载 ~/.cargo/env，每开一个终端窗口都会报一句错，直到你自己把那一行删掉。」 |

**必须一起改**：`warningKey`（`warnings.ts:19-35`）对不认识的裸串变体 `default: return null`，而 `warningTexts` 把 `null` **静默丢掉**——这是最危险的一处：一个没写分支的 `WillTrash` 会让对话框只剩「会移到废纸篓」一句、不列出任何一项。改成 `switch` + `never` 默认分支（`format.ts:77-92` 的 `faultKey` 就是这么写的），让 `tsc` 来做这件事。`types.test.ts` 与 `model.rs` 的形状测试各加新变体。

不选 journey 的 `MovesToTrash`/`PermanentDelete`（没有永久删除分支）与 `RemovesDefaultToolchain`（工具链行不在射程）；不选 reuse 的 `LeftBehind { paths }`（并入 `WillKeep { OutsideHome }`）。

### 6.6 对话框（claude，本机）

由现有「继续之前请注意：」列表（`UninstallDialog.tsx:215-226`，经 `warningTexts`）与现有 `CommandPreview`（`:251`）渲染：

```
卸载 Claude Code？
继续之前请注意：
 • 移到废纸篓：~/.local/share/claude（程序文件）
 • 移到废纸篓：~/.claude/downloads（可重新下载的缓存）
 • 移到废纸篓：~/.local/bin/claude（命令本身）
 • 保留：~/.claude（你的设置、登录信息、历史记录和工作文件，其它应用也可能在用）
 • 保留：~/.claude.json（你的设置）
将执行：
 Banager 会自己把上面列出的 3 项移到废纸篓——不运行任何命令，也不删除任何东西：清空废纸篓之前都能放回来。
[取消] [卸载]
```

rustup（本机，含 `~/.zshrc:17` 那条残留引用）：

```
卸载 rustup？
继续之前请注意：
 • 这会把 Rust 本身删掉：所有工具链（stable-aarch64-apple-darwin）以及 rustup 下载的全部内容。需要 Rust 的项目要等重新安装后才能再编译。
 • 这也会删除 Cargo 下载的包，以及用 cargo install 装的程序清单。
 • hexyl 是用 Cargo 装的，会留在 Mac 上，但以后没有工具管理它了，终端也找不到它，除非你自己把 ~/.cargo/bin 加回 PATH。
 • rustup 会修改你的 shell 设置文件，删掉它当初加的那一行。
 • 卸载后 ~/.zshrc 里还有一行会去加载 ~/.cargo/env，每开一个终端窗口都会报一句错，直到你自己把那一行删掉。
将执行：
 ~/.cargo/bin/rustup self uninstall -y（预览里是展开后的绝对路径）
运行期间请不要关闭 Banager 或 Mac：中途停止会留下损坏的安装，所以这个操作一旦开始就不能取消。
```

最后那句由 `UninstallDialog` 按 `plan.cancel_policy === "NoCancel"` 渲染（`operations.noCancelHint`，§五）。

### 6.7 `brew uninstall --zap` 危害

`brew uninstall --zap --cask claude-code` 会连原生安装的 `~/.local/bin/claude`、`~/.local/share/claude` 与共享的 `~/.claude` 一起删（VERIFIED，cask 的 zap stanza，claude.md §2b）；`grok-build` 的 zap 是 `rmdir ~/.grok`；`antigravity-cli` 的 zap trash `~/.gemini/antigravity-cli`。Banager 的 brew 适配器**从不**传 `--zap`：卸载 argv 就是 `["uninstall", flag, name]`（`brew/mod.rs:1277`），全文件 grep `"--zap"`、`"--force"` 零命中，`--ignore-dependencies` 只出现在注释（`:567`）。阶段 4 把这条从「碰巧没写」变成「承诺」：`brew/mod.rs` 测试模块加一条，对 cask/formula 的 Install/Uninstall/Upgrade 三种 `plan()` 断言 `args` 不含这三个标志；`docs/what-we-run.md`「Banager 绝不做的事」写明。

反向危害不存在：每个 `Paths` 清单都是路线私有的（永不是 `~/.claude`，永不是 Homebrew 前缀），删掉原生副本，共存的 cask 副本与共享配置原样不动。

### 6.8 shell 启动文件

agy（`# Added by Antigravity CLI installer` + PATH 行）、grok（`# >>> grok installer >>>` … `<<<` 块）都改过 `~/.zshrc`/`~/.zprofile`。**阶段 4 不编辑任何 shell 启动文件**：那不是路径删除，`TrashPaths` 表达不了；残留的行无害（`~/.local/bin` 本来就该在 PATH 里；zsh 容忍不存在的目录与 `fpath`）。`WillKeep { ShellConfigLines }` 一句话说明。正确形状记 §十一。

---

## 七、哪一份在跑：四个 PATH `InstanceNote`（+ 一个 `LauncherOnly`）

```rust
// model.rs  InstanceNote（今天 IndexMayBeStale | IndexUpdating，:93-105）
    /// 敲这个工具的名字时找不到它：启动器所在目录不在 Banager 看到的 PATH 里。
    NotOnPath,
    /// 敲名字时跑的是 Homebrew 装的那份（PATH 上先找到的可执行文件解析后含 /Cellar/ 或 /Caskroom/）。
    ShadowedByHomebrew,
    /// 同上，npm 装的那份（含 /node_modules/）。
    ShadowedByNpm,
    /// 同上，一份 Banager 不认识的。
    ShadowedByOther,
    /// 启动器还在，但它指向的程序目录已不在（上次卸载中途停下，§6.2 的顺序保证这是唯一的残留形态）。
    /// 行保留、闸门放行，卸载会把剩下的链接移走。detect() 在 §3.3 第 2 步填它。
    LauncherOnly,
```

算法（`detect()`，`route::shadow_note`，`LauncherOnly` 时跳过）：`first = resolve_exe(recipe.id, env)`（`path_env.rs:87-95`，第一个 `is_file()` 的 PATH 命中）；`None` → `NotOnPath`；`canonicalize(first) == Detected.real` → 无 note；否则按 `canonicalize(first)` 的分量分类。无载荷（通道设计 v2 §2.3 的既定规则：数据变体会把裸字符串变成外部标记对象，手写 TS 镜像的风险面变大）；四句各自可行动的话比一句带路径的话对小白更有用——路径他也不认识。

读取方：`sourceNoticesFor` 的 notes 循环（`sources.ts:198-225`）五个新分支（`:222` 的 `never` 让漏写编译失败），`{{command}}` 取 `instance.exe_path` 的 `file_name()`，`{{source}}` 沿用 `sourceLabel`；PATH 四个 `variant: "info"`、`LauncherOnly` 是 `"warning"`；`axis: "state"`、无按钮；`types.ts:122` 联合；两份 locale；`sources.test.ts`。两页都渲染（`InstalledPage.tsx:181`、`UpdatesPage` 同一规则）。通知的标题与描述经 `SourceNotices.tsx:33` 的 `t(key, values)` 插值，是**纯文本**——`withCommand`/`COMMAND_SLOT` 只用在行描述与对话框上（`UpdatesPage.tsx:466`、`InstalledPage.tsx:330`、`UninstallDialog.tsx:108`），通知里的 `{{command}}` 不渲染成代码，与现有两条通知一致（§十三 #12）。

PATH 语义与免责：Banager 的 PATH 来自 `fix_path_env::fix()` 唤起登录 shell（`src-tauri/src/lib.rs:18`），本机顺序是 `~/.opencode/bin`、`~/.local/bin`、`~/.grok/bin`、…、`/opt/homebrew/bin`、`/usr/local/bin`（现场核实）；从某个终端启动时会继承那个进程特有的临时目录（unknown-scan.md §0 的 23 条会话目录）。文案因此说「**多半**跑的是…」，不点名赢家的路径。本机四个工具都是单副本、无遮蔽，这四个 note 的测试是临时目录上的单元测试，不是录制。

Homebrew 组里的 `claude-code` cask 行**不会**说「还有一份原生的」——那需要给 `InstalledArtifact` 加命令名字段或前端按名字猜（`claude-code` ≠ `claude`）；正确形状记 §十一。

---

## 八、来源不明扫描

### 8.1 不是 `Adapter`

没有 `plan`/`execute`/`reconcile`，没有实例，没有命令输出可录制。注册成 `Adapter` 会撞三件事：`fixtures_layout_test` 要求一个 fixture 目录（放什么？）、`Session::build` 把它当读写来源注册进 ops、`issue_plan` 闸门要为它答「可操作吗」。决定性的一条：归属需要**所有其它适配器**的实例，而 `detect()` 并发独立运行（`refresh.rs:154-168`），`inventory(&self, inst)` 只看到自己。

```
crates/banager-core/src/scan/mod.rs     pub fn scan_unknown(env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], globs: &[(String, &'static [Glob])], budget: ScanBudget) -> UnknownScan   同步、纯文件系统
                                        （`globs` 与 `Glob` 类型在步骤 D 才出现：F 先合时签名没有这个参数、规则只有 0–3；D 加参数与规则 4）
crates/banager-core/src/session/scan.rs impl Session { pub fn scan_unknown(&self, env: &HostEnv) -> UnknownScan }   克隆当前 Snapshot 的 instances/artifacts 做归属；D 起从 standalone::RECIPES 填 globs（adapter id → backup_globs）；不取锁
src-tauri/src/ipc.rs                    #[tauri::command] async fn scan_unknown(state) -> Result<UnknownScan, String>   tauri::async_runtime::spawn_blocking（同 open_ollama_app，ipc.rs:550-559）；lib.rs:37-48 注册
src/lib/api.ts                          scanUnknown(): Promise<UnknownScan>
src/lib/queries.ts, queryKeys.ts        useUnknownScan()，enabled: false，页面打开与「重新扫描」时 refetch；queryKeys.unknown
src/pages/UnknownPage.tsx               页面；Page 联合（store/ui.ts:4）加 "unknown"；Sidebar.tsx:9 的 PAGES 加一项；App.tsx 路由加一支
```

**不并入 `Snapshot`、不并入 `refresh`**：它不是关于受管来源的数据；最多 10 s 的文件系统遍历不该由刷新付；进 `Snapshot` 要么进 `same_content`（每次 mtime 变化都重播）要么被代际忽略。页面打开与按需扫描。

### 8.2 线格式（Rust + TS 镜像，每个字段点名读取方）

```rust
pub struct ScanBudget { pub max_entries: usize /* 2000 */, pub max_duration: Duration /* 10 s */ }
#[derive(Serialize, Deserialize, …)] pub struct UnknownScan {
    pub scanned: Vec<ScannedDir>,      // 实际读过的目录与条目数 → 页脚「查看了：…」，让空列表显得是「看了七处」而不是「没看」
    pub entries: Vec<UnknownEntry>,    // 没人认领的 → 行
    pub attributed: u32,               // 被已知来源认领因此不列的数量 → 「另有 N 个来自 Banager 认识的来源」
    pub stopped: Option<ScanStop>,     // 触到预算 → 横幅「列表可能不完整」，数字来自载荷
}
pub struct ScannedDir { pub path: PathBuf, pub entries: u32 }
/// 带上预算的数字：`ScanBudget` 是运行时参数，文案里写死「2,000」「10 秒」会和代码分道扬镳
/// （`HomebrewStillUpdating { minutes }`，model.rs:449-453，就是为此把数字塞进载荷的先例；§十三 #15）。
pub enum ScanStop { FileLimit { max_entries: u32 }, TimeLimit { max_secs: u32 } }
pub struct UnknownEntry {
    pub path: PathBuf,                 // 目录里的那一项 → 行名（file_name）与技术细节下的全路径
    pub kind: EntryKind,               // → 徽章「程序」/「程序（链接）」/「失效的链接」
    pub resolved: Option<PathBuf>,     // canonicalize 结果；BrokenSymlink 为 None → 技术细节下「指向 …」
    pub link_target: Option<String>,   // readlink 原文，只对链接 → 断链句的 {{target}}
    pub size_bytes: Option<u64>,       // 解析目标的大小 → 副标题
    pub modified_at: Option<i64>,      // unix 秒 → 副标题（Intl.DateTimeFormat，绝对日期；仓库刻意没有相对时间格式化器）
    pub owned_by_me: bool,             // st_uid == euid → 「由一个用了管理员权限的安装器放在这里」
    pub app_bundle: Option<String>,    // 路径（断链用 link_target）任一分量以 .app 结尾时的 app 名 → 「{{app}} 的一部分」
}
pub enum EntryKind { File, Symlink, BrokenSymlink }
```

```ts
export type EntryKind = "File" | "Symlink" | "BrokenSymlink";
export type ScanStop = { FileLimit: { max_entries: number } } | { TimeLimit: { max_secs: number } };
export interface ScannedDir { path: string; entries: number }
export interface UnknownEntry { path: string; kind: EntryKind; resolved: string | null; link_target: string | null;
  size_bytes: number | null; modified_at: number | null; owned_by_me: boolean; app_bundle: string | null }
export interface UnknownScan { scanned: ScannedDir[]; entries: UnknownEntry[]; attributed: number; stopped: ScanStop | null }
```

`EntryKind` 在页面里用 `Record<…, string>` 查表（tsc 强制穷尽）；`ScanStop` 是数据变体，页面用 `"FileLimit" in stopped` 分支 + `never` 兜底。不加 `elapsed_ms`、`scanned_at`——没有渲染位。

### 8.3 规则

目录 = spec §4.2 的七个（`~/.local/bin ~/bin /usr/local/bin ~/.cargo/bin ~/go/bin ~/.bun/bin ~/.deno/bin`）∪ `HostEnv.path_dirs` 中以 `home` 开头的；按 canonical 路径去重；**不存在的静默跳过**，不进 `scanned`（本机 36 条候选里 29 条不存在，unknown-scan.md §0/§3.4）。深度 1：条目是目录 → 跳过，不递归（`~/Library/pnpm` 下的 `bin/`、`store/`，§3.6）；普通文件无任何 `x` 位 → 跳过（本机无样本，README 标「理论边界」）。

每条：`symlink_metadata`；是链接 → `canonicalize`（多跳一次到位，`python3.12` 两跳）；失败 → `BrokenSymlink`（本机有一条指向已删除 app 内脚本的链接），`link_target` 保留 readlink 原文，永不报错。`nlink` 不看（rustup 代理是符号链接不是硬链接，VERIFIED）。

归属，按序，命中即计入 `attributed` 不列出：

0. `entry == instance.exe_path`（**原始路径**相等，不 canonicalize），任一实例。悬空的 `LauncherOnly` 启动器（§3.3）靠这条不被列两次——`canonicalize` 对它失败，规则 1–3 都碰不到它（§十三 #43）。
1. `canonicalize(entry) == canonicalize(instance.exe_path)`，任一实例——13 个 rustup 代理都解析到 `~/.cargo/bin/rustup` → rustup。
2. `canonicalize(entry)` **以** `canonicalize(artifact.path)` **开头**（`path` 是文件时即相等），任一制品 → 该制品的实例。这是 `InstalledArtifact.path` 的第一个读取方，而**填它的不只独立安装工具**：uv 今天就填（`uv.rs:65`，tool 的 venv 目录；fixture `uv/0.12.17/tool-list-show-paths.txt` 里 `ruff` → `~/.local/share/uv/tools/ruff`，它的 shim `~/.local/bin/ruff` 解析到 `…/tools/ruff/bin/ruff`）→ uv，初稿的「相等」永远匹配不上 uv 的数据，会把已安装页列在 uv 下的 `ruff` 说成来源不明（§十三 #35）；独立安装工具填真实二进制，与规则 1 比的是同一个路径，规则 2 对它们不决定任何事——照实说；cargo 从步骤 E 起填 `{cargo_home}/bin/{bin}`（§6.4 的 `parse_crates2_bins`）→ `hexyl` → cargo。
3. `canonicalize(entry)` 以某实例**拥有的根**开头 → 取最长者。**不是 `prefix`**（§十三 #1/#22）：uv/pipx/pip/npm 的 `prefix` 是「可执行文件所在目录」（`uv.rs:138-141`、`pipx.rs:213-216`、`pip.rs:125-128`、`npm.rs:194-197` 全是 `exe_path.parent()`）。本机 `~/.local/bin/python3.12 → ~/.local/share/uv/python/…`，而 pip 适配器的 `CANDIDATE_INTERPRETERS` 含 `python3.12`（`pip.rs:68-74`）且 `-m pip --version` 失败也照样推一个 `NotResponding` 实例（`:92-100,153-156`）——于是刷新产出一个 prefix 为 `~/.local/bin` 的 pip 实例，按 `prefix` 归属会把整个 `~/.local/bin`（`agy`、一个第三方 app 放进来的脚本……）都算成 pip 的，这页存在的理由全没了；brew 的 `prefix` 是整个 `/opt/homebrew`/`/usr/local`（`brew/mod.rs:1446,1833`），Intel 机上第三方安装器放进 `/usr/local/bin` 的东西（unknown-scan.md §3.7 记录的那条 root 拥有的第三方链接就是这类）会被算成 brew 的而 `brew info --installed` 永远不会列它。拥有的根按 adapter id 查表（`scan/mod.rs` 的 `owned_roots(inst) -> Vec<PathBuf>`，带测试；§十一 的 `Adapter::owned_roots()` 是它将来的归宿）：`brew` → `<prefix>/Cellar`、`<prefix>/Caskroom`、`<prefix>/opt`；`ollama` → `<prefix>`（`~/.ollama`，bin 目录里没有东西解析到那里，列上只为完整）；`standalone-claude/agy/grok` → `<prefix>`（工具根 `~/.local/share/claude`、`~/.gemini/antigravity-cli`、`~/.grok`）；`standalone-rustup`、`cargo`、`uv`、`pipx`、`pip`、`npm` → **空**（rustup 的一切靠规则 1，cargo 靠规则 1/2，uv 靠规则 2；其余今天没有能归属的东西。永不把 `parent()` 得来的 `prefix` 当拥有）。
4. （步骤 D 起）文件名匹配某个**有实例的**配方的 `backup_globs`（`agy.<ts>.old` 在 `~/.local/bin`）→ 该工具。实例不存在（agy 已卸载）时留下的 `.old` 是真的来源不明，照列。

其余是 `UnknownEntry`；路径分量含 `.app` 的填 `app_bundle`。本机对照（unknown-scan.md §2 + 本次 `ls -la ~/.local/bin`；实例有 brew、cargo、npm、ollama、pip **两个**（Homebrew 的 python3.14 与 `~/.local/bin/python3.12`，后者 prefix `~/.local/bin`）、pipx、uv，加四个 standalone）：`~/.local/bin/agy`、`claude` → 规则 1；`python3.12` → 规则 0（它就是那个 pip 实例的 `exe_path`）→ 算「认领」——诚实：它确实是 Banager 列出的一个来源的可执行文件，至于它是 uv 管的 Python，是 §十一 `owned_roots` 的事；一个第三方 app 放进来的脚本 → 规则 0–3 全不中（pip 实例的 `~/.local/bin` 不再是根）→ **列出**；一条指向已删除 app 内部的链接 → `BrokenSymlink` + `app_bundle` 为那个 app 的名字。`~/.cargo/bin`：13 个代理 + `rustup` → 规则 1；`hexyl` → 步骤 E 之后规则 2 → cargo；**E 之前**（F 可以先合）它会暂时出现在来源不明页——F 的交付说明写明这一条。`~/.opencode/bin/opencode`（144 MB 独立二进制，无配方——正是这页存在的理由）→ 列出；`~/.grok/bin/{grok,agent}` → 规则 1；`/usr/local/bin` 里一个第三方远程桌面 app 以 root 身份放的链接 → 列出，`app_bundle` 为该 app、`owned_by_me: false`；一个 app 私有 CLI 目录（在 PATH 上）里的链接 → 列出。

### 8.4 预算、并发、边界

`max_entries` 计**检查过**的条目（含被认领的）；`max_duration` 在每次 `read_dir` 与每个条目前对 `Instant` 检查；任一触发 → `stopped: Some(...)`，已扫的照常返回。本机 26 个条目 64 ms（§4）；上限为 5000 文件的 `~/bin` 而设，用合成测试覆盖（2001 个空文件；0 ms 预算），不是录制。

同步、跑在 Tauri 阻塞池；启动时克隆 `snapshot.instances/artifacts`，期间的刷新提交不影响已算的；不取资源锁（不读包管理器的文件，只读目录项与元数据）；连点两次跑两次，页面显示最新。单个 `stat` 失败只影响那一项；根目录读不了则不进 `scanned`。不执行（`Executable` 判断只看 mode 位，不跑 `file(1)`）、不写、不读列出目录之外的任何东西。

### 8.5 页面

侧栏 `nav.unknown`。标题 + 一句说明（`unknown.intro`）；`scanned` 逐目录带计数；`attributed` 计数句；`stopped` 横幅；行复用 `ArtifactRow`（无主按钮、无勾选）：名字加粗、`~` 缩写路径、kind 徽章、断链句、「{{app}} 的一部分」、大小 · 日期、`owned_by_me` 为假时「由一个用了管理员权限的安装器放在这里」；技术细节开时多一行 `resolved`；「重新扫描」按钮（这是全 app 第一个刷新控件——backlog 记过其它页没有；本稿不给别的页加）。阶段 4 无每行操作（「在访达中显示」= 一条 `/usr/bin/open -R` 命令，记 §十一）。

---

## 九、线格式、i18n、fixtures、测试、what-we-run.md

### 9.1 `src/lib/types.ts` 镜像（手写；每行都得手加）

```ts
export type UninstallBlocked = "Pinned" | "NoSafeMethod";
export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly";
export type InstanceNote = "IndexMayBeStale" | "IndexUpdating" | "NotOnPath" | "ShadowedByHomebrew" | "ShadowedByNpm" | "ShadowedByOther" | "LauncherOnly";
export type RemovedWhat = "Launcher" | "Program" | "Backups" | "Cache";
export type KeptWhat = "Settings" | "SettingsAndHistory" | "ToolState" | "ShellConfigLines" | "OutsideHome" | "NotOurs";
export type Warning = …现有六个…
  | { WillTrash: { path: string; what: RemovedWhat } }
  | { WillKeep: { path: string; what: KeptWhat } }
  | { AlreadyGone: { path: string } }
  | { RemovesToolchains: { names: string[] } }
  | "DeletesCargoCaches"
  | { LeavesUnmanaged: { names: string[] } }
  | "EditsShellConfig"
  | { LeavesShellConfigLine: { path: string } };
export type Fault = …现有五个… | { PathChanged: { path: string } };
export type PlanAction =
  | { Command: { program: string; args: string[]; env: [string, string][] } }
  | { TrashPaths: { paths: string[] } };
export interface Plan { request: OpRequest; action: PlanAction; needs_password: boolean; locks: string[];
  cancel_policy: CancelPolicy; warnings: Warning[]; affected: string[]; timeout_secs: number }   // program/args/env 并入 action（§6.2）
// §8.2 的四个扫描类型
```

`ArtifactKind`、`OpKind`、`OpRequest`、`Settings`、`ManagerInstance`、`InstalledArtifact`、`UpdateCandidate`、`Snapshot`、`Adapter` trait、`CheckOptions`、`CheckOutcome` **不变**；`Plan` 变（§6.2）。

编译器抓得住哪些漏文案，抓不住哪些（通道设计 v2 §三的告诫）：

| 类型 | 强制方式 |
|---|---|
| `UninstallBlocked`、`UpdateBlocked` | `Record<…, Copy>`（`sources.ts:351,451`）——漏一项 `tsc` 失败 |
| `InstanceNote` | `sources.ts:222` 的 `never`——`tsc` 失败 |
| `Fault` | `format.ts:77-92` 的 `never`——`tsc` 失败 |
| `Warning` | **今天抓不住**：`warningKey` 默认返回 `null`，对话框静默丢弃。步骤 A 改成 `never` 默认分支，之后与 `Fault` 同级 |
| `RemovedWhat`、`KeptWhat`、`EntryKind` | 页面/`warnings.ts` 里新建 `Record<…, string>`——构造上穷尽 |
| `ScanStop`、`PlanAction` | `in` 分支 + `never` 兜底（同 `Fault`） |

新 IPC 错误 kind 需要一个**新的 Rust 生产者**（初稿没写，§十三 #5/#38）：`AdapterError::Refused(String)` 只带一个字符串，`plan_operation_error`（`ipc.rs:183-216`）把它映成无载荷的 `{"kind":"refused"}`（`:215`），`planFailureMessage`（`sources.ts:625-640`）只会渲染通用的 `planRefused.refused`——五句专门文案永远到不了屏幕。加：

```rust
// adapters/mod.rs
AdapterError::UninstallUnsafe { path: String /* 已把 $HOME 换成 ~ */, reason: UninstallUnsafeReason },
// model.rs
pub enum UninstallUnsafeReason { OutsideHome, Missing, NotOwnedByYou, NotWhatInstructionsExpect }
```

`plan_operation_error` 加一臂，reason 由 `match` 逐个写成 snake_case 字面量（同 `not_actionable_json` 的做法，不用 `rename_all`）：`{"kind":"uninstall_unsafe","path":"~/.local/bin/claude","reason":"not_what_instructions_expect"}`；`planFailureMessage` 的 `switch` 加 `case "uninstall_unsafe"` → `planRefused.uninstallUnsafe.<reason>`（四句；`trash_unavailable` 随检查 6 删除）；`UninstallDialog` 收到它走现有 `refusalText`（`:102`）。`types.test.ts` 加形状用例。检查 7 与空清单**有意**留在通用 `refused`（§6.3）。

### 9.2 i18n（en + zh-CN 全文；`completeness.test.ts` 同时检查键集互比与每个键在非测试源码里被字面引用，所以查表一律用字面键的 `Record`，不拼字符串；`no-literal-strings.test.ts` 禁 JSX 里的英文字面量）

```
adapters.standalone-claude       "Claude Code"                    "Claude Code"
adapters.standalone-agy          "Antigravity CLI (agy)"          "Antigravity CLI（agy）"
adapters.standalone-grok         "Grok Build (grok)"              "Grok Build（grok）"
adapters.standalone-rustup       "rustup"                         "rustup"

standalone.summary.standalone-claude   （读取方：InstalledPage.tsx:344 前的 STANDALONE_SUMMARY_KEYS: Record<StandaloneAdapterId, string>）
  en "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm."
  zh "Anthropic 的终端编程助手。用它自己的安装器装的，不是 Homebrew 或 npm。"
standalone.summary.standalone-agy
  en "Google's Antigravity coding assistant for the terminal. Installed with its own installer."
  zh "Google 的 Antigravity 终端编程助手。用它自己的安装器装的。"
standalone.summary.standalone-grok
  en "xAI's Grok coding assistant for the terminal. Installed with its own installer."
  zh "xAI 的 Grok 终端编程助手。用它自己的安装器装的。"
standalone.summary.standalone-rustup
  en "Rust's toolchain manager: it installs and updates the Rust compiler and Cargo."
  zh "Rust 的工具链管理器：负责安装和更新 Rust 编译器与 Cargo。"

sourceNotice.notOnPath.title
  en "{{source}} isn't in your PATH"                          zh "{{source}} 不在 PATH 里"
sourceNotice.notOnPath.description
  en "It's installed, but typing {{command}} in Terminal probably won't find it: the folder it lives in isn't in your shell's search path (PATH). Opening a new Terminal window sometimes fixes this; otherwise follow the tool's own install guide for adding it to PATH."
  zh "它装好了，但在「终端」里输入 {{command}} 多半会找不到：它所在的文件夹不在 shell 的搜索路径（PATH）里。新开一个终端窗口有时就好了；不行的话，按这个工具自己的安装说明把它加进 PATH。"
sourceNotice.shadowedByHomebrew.title
  en "Another copy runs when you type {{command}}"            zh "输入 {{command}} 时运行的是另一份"
sourceNotice.shadowedByHomebrew.description
  en "You have {{source}} twice: this copy, and one installed with Homebrew. When you type {{command}} in Terminal, the Homebrew copy most likely runs, not this one. Updating or removing this one won't change what {{command}} does. If you only want one, uninstall the other — both are listed on this page."
  zh "{{source}} 装了两份：这一份，和一份用 Homebrew 装的。在「终端」里输入 {{command}} 时，多半运行的是 Homebrew 那份，不是这一份。更新或卸载这一份，不会改变 {{command}} 的行为。只想留一份的话，把另一份卸掉——两份在这一页上都能找到。"
sourceNotice.shadowedByNpm.title          （同 shadowedByHomebrew.title）
sourceNotice.shadowedByNpm.description    （同上，Homebrew → npm）
sourceNotice.shadowedByOther.title        （同 shadowedByHomebrew.title）
sourceNotice.shadowedByOther.description
  en "You have {{source}} twice: this copy, and another one Banager doesn't manage. When you type {{command}} in Terminal, that other copy most likely runs, not this one. Updating or removing this one won't change what {{command}} does. The Unknown page may show where it is."
  zh "{{source}} 装了两份：这一份，和一份 Banager 不管理的。在「终端」里输入 {{command}} 时，多半运行的是那一份，不是这一份。更新或卸载这一份，不会改变 {{command}} 的行为。「来源不明」页可能能看到它在哪。"
sourceNotice.launcherOnly.title
  en "Only the {{command}} link is left"                      zh "只剩下 {{command}} 这个链接了"
sourceNotice.launcherOnly.description
  en "The program files are gone (an earlier uninstall stopped partway; they may be in the Trash), but the {{command}} link is still there, so typing {{command}} in Terminal fails. Uninstall removes the link. If you didn't mean to remove {{source}}, put its folder back from the Trash and refresh."
  zh "程序文件已经不在了（上次卸载中途停下，它们可能在废纸篓里），但 {{command}} 这个链接还在，所以在「终端」里输入 {{command}} 会失败。卸载会把这个链接移走。如果你并不想删掉 {{source}}，把它的文件夹从废纸篓放回去，然后刷新。"

updates.selfUpdatingHint
  en "This copy is behind ({{current}} → {{target}}). {{source}} usually updates itself the next time you run it; you can update it now with Banager, or just run it."
  zh "这份落后了（{{current}} → {{target}}）。{{source}} 通常在下次运行时会自己更新；可以现在用 Banager 更新，也可以直接运行它。"
operations.noCancelHint      （读取方两处：UpdatesPage 确认框与 UninstallDialog，都在 CommandPreview 之后按 plan.cancel_policy === "NoCancel" 显示，§五）
  en "Don't close Banager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts."
  zh "运行期间请不要关闭 Banager 或 Mac。中途停止会留下损坏的安装，所以这个操作一旦开始就不能取消。"

updates.blocked.SelfUpdatesOnly.badge
  en "Updates itself"                                         zh "自己更新"
updates.blocked.SelfUpdatesOnly.description
  en "A newer version of {{source}} is out ({{current}} → {{target}}), and {{source}} installs updates itself in the background — Banager doesn't have a safe way to do it for you. Open it once (run {{command}} in Terminal, then quit it): it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background."
  zh "{{source}} 出了新版本（{{current}} → {{target}}），它会在后台自己安装更新——Banager 没有安全的办法替你做这件事。打开它一次（在「终端」里运行 {{command}}，然后退出）：它启动时会检查更新，最多每 15 分钟一次，并在后台自己装好新版本。"
updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable
  en "A newer version of {{source}} was seen the last time it answered, and {{source}} installs updates itself in the background — Banager doesn't have a safe way to do it for you. Open it once (run {{command}} in Terminal, then quit it): it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background."
  zh "上次 {{source}} 应答时就已经有新版本了，它会在后台自己安装更新——Banager 没有安全的办法替你做这件事。打开它一次（在「终端」里运行 {{command}}，然后退出）：它启动时会检查更新，最多每 15 分钟一次，并在后台自己装好新版本。"
updates.blocked.SelfUpdatesOnly.refused
  en "{{source}} updates itself, so Banager didn't try to update it. Nothing has been changed."
  zh "{{source}} 会自己更新，所以 Banager 没有去更新它。什么都没有改动。"
  （UPDATE_BLOCKED_KEYS.SelfUpdatesOnly 的 selfUpdatingDescription / …SourceUnavailable = null）

installed.blocked.NoSafeMethod.badge      en "Can't uninstall here"        zh "无法在这里卸载"
installed.blocked.NoSafeMethod.description
  en "{{source}} has no uninstall command, and Banager doesn't yet have a verified list of the files it would need to remove, so it doesn't offer to. The official instructions are on its website."
  zh "{{source}} 没有卸载命令，Banager 也还没有一份核实过的文件清单，所以不提供卸载。官方说明在它的网站上。"
installed.blocked.NoSafeMethod.descriptionSourceUnavailable   （同上）
installed.blocked.NoSafeMethod.refused
  en "Banager can't uninstall {{source}} yet, so it didn't. Nothing has been changed."
  zh "Banager 还不能卸载 {{source}}，所以没有动。什么都没有改动。"
  （command 返回空串；该句没有 {{command}} 槽）

warnings.willTrash.Launcher   en "Moves to the Trash: {{path}} (the command itself)"                 zh "移到废纸篓：{{path}}（命令本身）"
warnings.willTrash.Program    en "Moves to the Trash: {{path}} (the program's files)"                zh "移到废纸篓：{{path}}（程序文件）"
warnings.willTrash.Backups    en "Moves to the Trash: {{path}} (an old copy the updater left behind)" zh "移到废纸篓：{{path}}（更新程序留下的旧副本）"
warnings.willTrash.Cache      en "Moves to the Trash: {{path}} (downloaded files it can re-create)"  zh "移到废纸篓：{{path}}（可重新下载的缓存）"
warnings.willKeep.Settings            en "Keeps: {{path}} (your settings)"                            zh "保留：{{path}}（你的设置）"
warnings.willKeep.SettingsAndHistory  en "Keeps: {{path}} (your settings, login, history and working files — other apps may use it too)"   zh "保留：{{path}}（你的设置、登录信息、历史记录和工作文件，其它应用也可能在用）"
warnings.willKeep.ToolState           en "Keeps: {{path}} (its conversations, history and working files; some of the program's own files are in there too)"   zh "保留：{{path}}（它的对话、历史和工作文件；程序自己的一些文件也在里面）"
warnings.willKeep.ShellConfigLines    en "Keeps: the lines its installer added to {{path}} (harmless; Banager never edits that file)"   zh "保留：安装程序加进 {{path}} 的几行（无害；Banager 从不改这个文件）"
warnings.willKeep.OutsideHome         en "Keeps: {{path}} (outside your home folder, so Banager won't touch it; after uninstalling it's a dead link you can delete yourself)"   zh "保留：{{path}}（不在你的个人文件夹里，Banager 不会碰它；卸载后它是个失效的链接，你可以自己删）"
warnings.willKeep.NotOurs             en "Keeps: {{path}} (it isn't part of this install — something else put it there)"   zh "保留：{{path}}（它不属于这次安装，是别的东西放在那里的）"
warnings.alreadyGone                  en "Already gone: {{path}} (nothing left to move)"             zh "已经不在了：{{path}}（没有东西要移）"
warnings.removesToolchains
  en "This removes Rust itself: every toolchain ({{names}}) and everything rustup downloaded. Projects that need Rust will stop building until you install it again."
  zh "这会把 Rust 本身删掉：所有工具链（{{names}}）以及 rustup 下载的全部内容。需要 Rust 的项目要等重新安装后才能再编译。"
warnings.removesToolchainsUnlisted   （names 为空）
  en "This removes Rust itself: every toolchain and everything rustup downloaded. Projects that need Rust will stop building until you install it again."
  zh "这会把 Rust 本身删掉：所有工具链以及 rustup 下载的全部内容。需要 Rust 的项目要等重新安装后才能再编译。"
warnings.deletesCargoCaches
  en "This also removes Cargo's downloaded packages and its list of programs you installed with cargo install."
  zh "这也会删除 Cargo 下载的包，以及用 cargo install 装的程序清单。"
warnings.leavesUnmanaged_one
  en "{{names}} was installed with Cargo. It stays on your Mac, but nothing will manage it any more, and Terminal won't find it unless you add ~/.cargo/bin to your PATH yourself."
  zh "{{names}} 是用 Cargo 装的。它会留在 Mac 上，但以后没有工具管理它了，终端也找不到它，除非你自己把 ~/.cargo/bin 加回 PATH。"
warnings.leavesUnmanaged_other
  en "{{count}} programs installed with Cargo stay on your Mac, but nothing will manage them any more, and Terminal won't find them unless you add ~/.cargo/bin to your PATH yourself: {{names}}."
  zh "有 {{count}} 个用 Cargo 装的程序会留在 Mac 上，但以后没有工具管理它们了，终端也找不到它们，除非你自己把 ~/.cargo/bin 加回 PATH：{{names}}。"
warnings.editsShellConfig
  en "rustup will edit your shell settings files to remove the line it added."
  zh "rustup 会修改你的 shell 设置文件，删掉它当初加的那一行。"
warnings.leavesShellConfigLine
  en "After this, {{path}} still has a line that loads ~/.cargo/env, so every new Terminal window will print an error until you remove that line yourself."
  zh "卸载后 {{path}} 里还有一行会去加载 ~/.cargo/env，每开一个终端窗口都会报一句错，直到你自己把那一行删掉。"

uninstall.trashPreview_one / _other   （CommandPreview 的 TrashPaths 支，§6.2；取代初稿的 uninstall.trashNote）
  en "Banager moves the {{count}} item listed above to the Trash itself — no command runs, and nothing is deleted: you can put it back until you empty the Trash." / "…the {{count}} items listed above…put them back…"
  zh "Banager 会自己把上面列出的 {{count}} 项移到废纸篓——不运行任何命令，也不删除任何东西：清空废纸篓之前都能放回来。"

planRefused.uninstallUnsafe.outsideHome
  en "Banager won't remove {{path}}: it's outside your home folder. Nothing was changed."
  zh "Banager 不会移除 {{path}}：它不在你的个人文件夹里。什么都没有改动。"
planRefused.uninstallUnsafe.missing
  en "{{path}} isn't there any more, so Banager stopped. Nothing was changed."
  zh "{{path}} 已经不在了，Banager 停下了。什么都没有改动。"
planRefused.uninstallUnsafe.notOwnedByYou
  en "Banager won't remove {{path}}: it belongs to another user on this Mac. Nothing was changed."
  zh "Banager 不会移除 {{path}}：它属于这台 Mac 上的另一个用户。什么都没有改动。"
planRefused.uninstallUnsafe.notWhatInstructionsExpect
  en "Banager won't remove {{path}}: it isn't what the official instructions describe (a link to somewhere else, or a different kind of file), so removing it could hit the wrong thing. Nothing was changed."
  zh "Banager 不会移除 {{path}}：它和官方说明描述的不一样（链到了别处，或者不是同一类文件），移除可能误伤别的东西。什么都没有改动。"
  （初稿的 trashUnavailable 随检查 6 删除，§6.3；废纸篓不可用在执行时是 Outcome::Failed 的 summary，不是拒绝）

operations.outcome.BanagerFailed.PathChanged
  en "Failed: {{path}} changed between the preview and now, so Banager didn't move anything. Look at the preview again."
  zh "失败：{{path}} 在预览之后有了变化，所以 Banager 什么都没有移动。请重新查看预览。"

nav.unknown                  en "Unknown"                              zh "来源不明"
unknown.title                en "Programs Banager can't place"         zh "Banager 说不清来源的程序"
unknown.intro
  en "These command-line programs are on your Mac, but none of the sources Banager knows installed them. Banager only lists them — it never runs or deletes anything here."
  zh "这些命令行程序在你的 Mac 上，但 Banager 认识的来源都没有装过它们。Banager 只是列出来——这里什么都不会运行，也不会删除。"
unknown.lookedIn             en "Looked in:"                           zh "查看了："
unknown.dirCount_one / _other   en "{{path}} ({{count}} item)" / "{{path}} ({{count}} items)"   zh "{{path}}（{{count}} 项）"
unknown.attributed_one       en "{{count}} more program came from a source Banager knows and is listed under it."   zh "另有 {{count}} 个程序来自 Banager 认识的来源，已列在对应来源下。"
unknown.attributed_other     en "{{count}} more programs came from sources Banager knows and are listed under them."  zh "另有 {{count}} 个程序来自 Banager 认识的来源，已列在对应来源下。"
unknown.stopped.FileLimit    en "Banager stopped after looking at {{count}} items, so this list may be incomplete."   zh "Banager 查看 {{count}} 项后停下了，这个列表可能不完整。"   （count = stopped.FileLimit.max_entries）
unknown.stopped.TimeLimit    en "Banager stopped after {{seconds}} seconds, so this list may be incomplete."          zh "Banager 查看 {{seconds}} 秒后停下了，这个列表可能不完整。"   （seconds = stopped.TimeLimit.max_secs）
unknown.kind.File            en "Program"                              zh "程序"
unknown.kind.Symlink         en "Program (link)"                       zh "程序（链接）"
unknown.kind.BrokenSymlink   en "Broken link"                          zh "失效的链接"
unknown.brokenLink           en "Points at {{target}}, which no longer exists"   zh "指向 {{target}}，但那里已经没有了"
unknown.linksTo              en "Links to {{path}}"                    zh "指向 {{path}}"
unknown.partOfApp            en "Part of {{app}}"                      zh "{{app}} 的一部分"
unknown.adminOwned           en "Put here by an installer with administrator rights"   zh "由一个用了管理员权限的安装器放在这里"
unknown.sizeAndDate          en "{{size}} · {{date}}"                  zh "{{size}} · {{date}}"
unknown.scanAgain            en "Scan again"                           zh "重新扫描"
unknown.scanning             en "Scanning…"                            zh "正在扫描…"
unknown.scanFailed           en "Couldn't scan: {{message}}"           zh "没能扫描：{{message}}"
unknown.empty
  en "Nothing unexplained: every command-line program Banager found came from a source it knows."
  zh "没有说不清的：Banager 找到的命令行程序都来自它认识的来源。"

emptyStates.noSources.description（改写）
  en "Banager works with Homebrew, npm, pipx, uv, pip, Cargo, Ollama, and tools that come with their own installer (Claude Code, Antigravity, Grok, rustup). None of them are set up on this Mac yet — Homebrew is the easiest place to start."
  zh "Banager 支持 Homebrew、npm、pipx、uv、pip、Cargo、Ollama，以及自带安装器的工具（Claude Code、Antigravity、Grok、rustup）。这台 Mac 上一个都还没装，建议先从 Homebrew 开始。"
emptyStates.nothingInstalled.description（改写）
  en "Anything you install with Homebrew, npm, pipx, uv, pip, Cargo or Ollama, or with a tool's own installer, will show up here."
  zh "用 Homebrew、npm、pipx、uv、pip、Cargo、Ollama 装的东西，以及用工具自带安装器装的，都会出现在这里。"
```

`settings.includeSelfUpdating.*` **不动**（D5：独立安装工具不走那个开关）。`{{command}}` 在行描述与对话框里经现有 `withCommand`/`COMMAND_SLOT` 渲染成代码；在 `sourceNotice.*` 里是纯文本（§七）。`{{size}}` 需要一个 10 行的 `formatBytes`（`format.ts` 今天只有 `displayToken`/`outcomeKey`/`outcomeArgs`）。两个经模板插值到达的新键——`nav.unknown`（`Sidebar.tsx:9` 的 `t(\`nav.${p}\`)`）与 `operations.outcome.BanagerFailed.PathChanged`（`format.ts` 的 `outcomeKey`）——要同时登记进 `completeness.test.ts:187` 的 `INTERPOLATED_SUBTREES`（`nav` 在步骤 F，`operations.outcome` 在步骤 C），否则该测试判它们「无人引用」（§十三 #47）。

### 9.3 fixtures：只收真机录制

布局（`fixtures_layout_test.rs:13-57`：每个注册 id 一目录、≥ 1 版本目录、每版本一 README）。首批本机（BrulekMBA，2026-09-24 或之后，逐字节）：

```
adapters/fixtures/standalone-claude/2.1.281/   README.md  version.txt（`claude --version`）  latest.txt（`curl -sS …/latest` → 2.1.281）
                                               stable.txt（`…/stable` → 2.1.273：一份真实的「远端比本地旧」录制，§4.3 的比较规则为它而存在）
                                               layout.txt（`ls -la ~/.local/bin/claude ~/.local/share/claude/versions`，佐证不是解析输入，同 cargo/1.98.1/install-list.txt）
adapters/fixtures/standalone-agy/1.2.10/       README.md  version.txt  manifest-darwin_arm64.json  update_status.json（`cat ~/.gemini/antigravity-cli/updater/update_status.json`）  layout.txt（`ls -la ~/.local/bin/agy`——**只列工具自己的路径**，不是整个目录：`ls -la ~/.local/bin` 会把 一个第三方 app 放进来的脚本、一条指向第三方 app 内部的链接、`python3.12` 与用户名逐字带进公开仓库，§十三 #30）
                                               README 另记：`agy --version` 前后 `log/` 无新文件、`update_status.json` mtime 不变（§3.4 的观察，每个版本重做）
adapters/fixtures/standalone-grok/1.0.41/      README.md  version.txt  update-check.json（`grok update --check --json`）  layout.txt（`ls -la ~/.grok/bin ~/.grok/downloads`）
adapters/fixtures/standalone-rustup/1.29.1/    README.md  version.txt（stdout）  version-stderr.txt（两行 info:，证明解析器不读 stderr）  release-stable.toml  toolchain-list.txt  layout.txt（`ls -la ~/.cargo/bin`）
                                               （`.crates2.json` 的解析 fixture 不在这里：用已有的 adapters/fixtures/cargo/1.98.1/crates2.json，§6.4）
```

**不录制**：`~/.claude/settings.json`（个人配置；`auth.json` 永不）——通道解析用内联 JSON（阶段 3 计划：内联不算 fixture）。`~/.grok/config.toml` 首批不再被读（§4.4）。「有更新」的一对版本本机录不到（四个都是最新）——比较逻辑用内联字符串（Q13），不为了录制而降级作者的 claude。指纹测试在临时目录自建布局，**不得**写进 `adapters/fixtures/`。`scan` 没有 fixture 目录（不是 adapter，且集合相等测试禁止多出一个）。README 按 `uv/0.12.17/README.md` 的格式写明日期、主机、命令、以及本机有哪条路线、没有哪条（读者才知道为什么没有 cask 录制）。

第二批（§十 步骤 G）：一个手动触发的 GitHub Actions 工作流在 `macos-latest` 上用官方脚本把 bun/deno/mise/uv/pnpm 装进一次性 `HOME`（CI runner 是 spec §4.1 允许的录制环境；「不管进 sh」的规则约束的是 Banager），运行版本命令、`curl` 端点、上传目录为 artifact，作者审核后带 README 提交；Ollama.app 在 runner 上装 cask；Intel runner 顺手 `curl` agy 的 `darwin_amd64` 清单。没有一个字节是 AI 写的。

### 9.4 测试

Rust，`adapters/standalone/`：每个 `Recipe` 的路径都以 `~/`、`$CARGO_HOME/` 之一开头、`Latest` 主机在 `ALLOWED_HTTPS_HOSTS`、每个 `Paths` 配方里没有一条路径是另一条的前缀且启动器是 `remove` 的最后一条；每个 fixture 的版本解析（rustup 的 stderr 文件）；最新版本解析（TOML/JSON/grok 的 JSON）与比较表（`2.1.273 < 2.1.281`、`1.0.41 == 1.0.41`、`2026.9.9 < 2026.9.12`、`1.2.10 > 1.2.9`、`abc` 不可比）；claude 通道（内联 JSON：缺失、`stable`、畸形）；临时目录上的路线检测（链接进 root → 实例；链接进含 `/Caskroom/` 的路径 → 无；期望链接却是文件 → 无；启动器缺失 → 无；**悬空链接文本指向 root（绝对与相对 `../downloads/…` 各一）→ `LauncherOnly` 实例、`unavailable: None`、`path: None`；悬空链接指向别处 → 无实例**；`MockRunner` 让 `--version` 失败 → `NotResponding`；四种遮蔽各一）；`check_updates`（远端更旧 → 无候选；HTTP 失败 → uncheckable；agy → `SelfUpdatesOnly`；grok `updateAvailable: false` → 无；`include_self_updating` 无论真假结果相同；x86_64 下 agy 为 uncheckable——用 `cfg` 或注入的 arch 常量）；`plan(Upgrade)` 的 argv/env（无 `DISABLE_AUTOUPDATER`）/策略/超时，rustup 的 `locks` 含 cargo 实例 id；`plan(Uninstall)`：五条检查各一个失败用例（属主不匹配用注入错误 `euid` 的 `HostEnv`，测试建不出 root 文件；**父目录是链接指向 HOME 外 → `outside_home`**，每种 `Expect` 各一）、`optional` 缺失跳过、**`optional` 指纹不符（外来的 `~/.local/bin/agent`）→ `WillKeep { NotOurs }` 且计划照常**、`LauncherOnly` 实例上缺失的程序目录 → `AlreadyGone` 且计划只含启动器、glob 只匹配普通文件、`action` 逐字是 `TrashPaths { paths }` 且顺序 = 配方顺序 = `WillTrash` 顺序、rustup 列两把锁且第二把等于 `CargoAdapter::detect` 在同一 `HostEnv`（有/无 `CARGO_HOME`）产出的 id；rustup 的 `LeavesShellConfigLine` 在临时 HOME 里对 `~/.zshrc` 含引用 / 只有 `~/.zshenv` 含引用各一例；`parse_crates2_bins` 对 `adapters/fixtures/cargo/1.98.1/crates2.json` 给 `[("hexyl", ["hexyl"])]`；**TOCTOU**：`plan` 与 `execute` 之间重指链接 → `BanagerFailed(PathChanged)` 且 `MockTrasher.calls()` 为空；**中途停下**：`MockTrasher` 在第二条失败 / 令牌在第一条后取消 → 第一条已在 mock 的废纸篓、启动器仍在、`reconcile.present == true`，随后第二个 `plan(Uninstall)` 给出 `AlreadyGone` + 只含启动器的 `TrashPaths`，`execute` → `Succeeded`；**基名冲突**：真实 claude 布局（两条都叫 `claude`）→ `MockTrasher` 收到两条调用，两项都在。

Rust，`tests/standalone_uninstall_test.rs`：真 `StandaloneAdapter` + `MockTrasher`（rename 进临时目录），在临时 `HOME` 里合成 claude 布局，走 `plan → execute → reconcile`，断言 `Succeeded`、mock 废纸篓里是链接本身（不是目标）与程序目录、临时 HOME 之外无变化。另一个 `#[ignore]` 的用例用 `RealTrasher` 对临时目录里的一个文件和一个链接各调一次（CI 的 macOS runner 跑，它的废纸篓是一次性的；开发机手动跑一次），断言返回的新位置在 `~/.Trash` 下且链接移的是链接本身。

Rust，`tests/unknown_scan_test.rs`：断链、两跳链接、子目录跳过、不存在的根不进 `scanned`、无 `x` 位的文件跳过、2001 个文件 → `FileLimit { max_entries: 2000 }`、0 ms 预算 → `TimeLimit { max_secs: 0 }`、归属规则 0–3 各一（`testing::manager_instance`，`testing.rs:44`；**规则 3 的反例**：一个 prefix 为 `~/.local/bin` 的 pip 实例不得认领 `~/.local/bin/agy`，而规则 0 仍认领 `python3.12` 本身；**规则 2 的 uv 例**：制品 `path = ~/.local/share/uv/tools/ruff`，`~/.local/bin/ruff → …/tools/ruff/bin/ruff` → 认领；**规则 0 的悬空例**：`exe_path` 是悬空链接 → 认领、不列）、`.app` 分量；步骤 D 加规则 4 与 `.old` 在无实例时照列。root 属主不可测，`owned_by_me` 是一行 `st_uid == euid`。

Rust，其它：`brew/mod.rs` 三种 `plan()` 不含 `--zap`/`--force`/`--ignore-dependencies`；`http/real.rs` 名单外 https 主机被 `send()` 拒绝、`http://` 放行；`tests/ops_upgrade_version_test.rs` 加一例「`claude update` 打印 up to date 退出 0 → `UnchangedAfterUpgrade`」（与 `:232,416` 的 disabled cask / locked pipx 同款）；`session/mod.rs:500` 改十一；`fixtures_layout_test` 自然覆盖；`ipc.rs:1663-1770` 两个 NoCancel 用例改名点名 rustup；`Plan` 形状测试（`model.rs`）加 `TrashPaths`。

前端（vitest，每个改动的 `*.ts(x)` 配一个 `*.test.*`，本仓库 1:1 惯例）：`types.test.ts` 每个新变体、`PlanAction` 两臂、`ScanStop` 两臂、`UnknownScan` 的形状；`warnings.test.ts` 八个变体的 key/args/text，以及 `never` 重构后的穷尽；`sources.test.ts` 五个 note、`SelfUpdatesOnly`/`NoSafeMethod` 的 copy record、`STANDALONE_SUMMARY_KEYS`、`uninstall_unsafe` 四个 reason 的解码；`CommandPreview.test.tsx` `Command` 与 `TrashPaths` 两支；`UpdatesPage.test.tsx` `selfUpdatingHint` 只出现在独立安装工具的可操作行、`SelfUpdatesOnly` 行无按钮无勾选且计入「另有 N 个无法在这里更新」、`operations.noCancelHint`；`UninstallDialog.test.tsx` `WillTrash`/`WillKeep`/`AlreadyGone` 渲染为项目、`TrashPaths` 预览句带计数、`NoCancel` 计划显示 `operations.noCancelHint`、`uninstall_unsafe` 的拒绝文案；`InstalledPage.test.tsx` `NoSafeMethod` 行；`UnknownPage.test.tsx` 行、徽章、断链、`.app`、两种停止横幅带数字、空态、重新扫描；`Sidebar.test.tsx` 新页；`OperationBar.test.tsx` 现有 `NoCancel` 用例只改名；`completeness.test.ts` 的 `INTERPOLATED_SUBTREES` 两处登记（§9.2）。

### 9.5 `docs/what-we-run.md`

现状标题「Phase 0–1: Homebrew only」（`:1`），78 行只写 brew；npm/pipx/uv/pip/cargo/ollama 六个来源上线时没有加节。重写成每来源一节（只读表：后台检查、不要密码；写表：先预览后确认），从各适配器的 `plan()`/`detect()` 抄 argv 与超时，再加：

- **每个独立安装工具一节**：探测命令与环境变量（含 `DISABLE_AUTOUPDATER=1`、`AGY_CLI_DISABLE_AUTO_UPDATE=true` 的理由；agy 1.2.10 的 `--version` 不写日志、带提示词的运行才写）、更新检查（HTTP 主机或子命令）、升级 argv 与取消策略、卸载（路径清单：Banager **自己**逐条调 macOS 的「移到废纸篓」——与访达同一个调用，不运行命令，五条检查，启动器最后；或 `rustup self uninstall -y` 及它会删什么）。
- **Banager 读的文件**（只读、不保存、不上传）：`~/.claude/settings.json` 的 `autoUpdatesChannel` 一个键；rustup 卸载预览时读 shell 启动文件找 `.cargo/env` 引用（§6.4）；`~/.cargo/.crates2.json`（cargo 适配器今天就读）。
- **Banager 只连接这些主机**：`ALLOWED_HTTPS_HOSTS` 原文（6 个），每个 URL 取什么，「不跟随重定向」，「请求里除 UA 外不带本机任何信息」（grok 的 `update --check` 是它自己的请求）。
- **Banager 绝不做的事**：不跑 shell、不 `curl | sh`、不重跑任何安装脚本、不传 `brew … --zap`/`--force`/`--ignore-dependencies`、不删 `$HOME` 之外的文件、不永久删除任何文件（唯一的进程内文件系统写入就是「移到废纸篓」，且只对确认过的卸载）、不编辑 shell 启动文件、不删 `~/.claude`/`~/.gemini`/`~/.grok` 里的设置与登录、永不 `rustup update`、后台刷新不写机器。
- **步骤 C 的核实结果**（§6.2）：无 FDA 的 Finder 启动构建能否 `trashItemAtURL:`、不能 `rename` 进 `~/.Trash`。
- **来源不明扫描一节**：扫哪些目录、深度 1、预算、只读目录项与元数据、不运行不删除。

---

## 十、实施步骤（每步全绿、可独立合并、有用户可见的读取方）

**前置条件已满足**：`NoCancel` 由 `17d8ef7` + `99a9d6f` 完整落地（§0.1）。基线 `8ba6f52`。每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷，初稿的 B 就犯了（§十三 #41），下面按步切片。

| 步 | 内容 | 交付 | 依赖 |
|---|---|---|---|
| **A** | `docs/what-we-run.md` 为六个阶段 3 来源补齐；brew 三标志测试；`ALLOWED_HTTPS_HOSTS`（现有三主机）+ `send()` 检查 + 改掉 `:43-49` 注释；`warningKey` 改 `never` 穷尽 | 信任文件不再撒谎；三条今天就有读取方的承诺变成测试 | 无，可最先合 |
| **B** | 骨架 + claude，不含卸载。`Recipe` 在此步的切片：`id / meta_toml / route / version / latest（只有 ClaudeChannel、HttpTomlVersion、HttpJsonField、Command 四臂里 claude 用到的 ClaudeChannel，其余三臂各随其生产者的步骤加入）/ self_updates / upgrade`——**没有** `uninstall`、`backup_globs` 字段（C、D 才加）；`recipe.rs`/`recipes.rs`/`route.rs`（含悬空链接的词法归一与 `LauncherOnly`）/`latest.rs`/`mod.rs`；`standalone::all`；`adapters/meta/standalone-claude.toml` + fixture；五个 `InstanceNote` 变体 + 通知 + 文案；`UninstallBlocked::NoSafeMethod` + copy record（**claude 在此步产生它**：没有 `uninstall`，按钮诚实地藏起）；`auto_updates` 的 `selfUpdatingHint` 读取方；`STANDALONE_SUMMARY_KEYS`；`adapters.standalone-claude`；`emptyStates` 两句改写；`session/mod.rs:500` 改八。**不在此步**：`UpdateBlocked::SelfUpdatesOnly`（生产者 agy，D）、`HostEnv.rustup_home`（已删）、`Uninstall`/`RemoveSpec`/`KeepSpec`/`Probe`/`Expect`/`Glob` | 已安装页一个 Claude Code 组、诚实徽章、PATH 通知、`claude update` 按钮；卸载按钮不出现并说明原因；半卸载态有一行会说话 | A（allowlist） |
| **C** | 路径清单卸载：`PlanAction` + 32 处构造 + 四处读取方 + `types.ts` 镜像（§6.2）；`trash/` 三个文件 + `objc2-foundation` 依赖；`Recipe.uninstall` 字段 + `Uninstall::Paths`/`RemoveSpec`/`KeepSpec`/`Expect`；`removal.rs`、五条检查、`execute()` 复核与逐条移入；`Fault::PathChanged` + `faultKey` + 文案 + `INTERPOLATED_SUBTREES["operations.outcome"]`；`WillTrash`/`WillKeep`/`AlreadyGone` + `RemovedWhat`/`KeptWhat` + 文案；`AdapterError::UninstallUnsafe` + `UninstallUnsafeReason` + IPC 臂 + `planFailureMessage` + `types.test.ts`；`CommandPreview` 的 `TrashPaths` 支 + `uninstall.trashPreview`；临时 HOME 端到端测试 + `#[ignore]` 的 `RealTrasher` 冒烟；claude 配方填 `uninstall: Paths`，不再产生 `NoSafeMethod`。**合并前**：§6.2 的无 FDA 核实，结果写进 what-we-run.md | 安全地卸载原生 Claude Code；预览逐条说人话；中途停下可以重来 | B |
| **D** | grok + agy：两个配方、meta、fixtures（agy README 含「`--version` 不写日志」的观察）、`Latest::Command`、`Latest::HttpJsonField`（含 x86_64 → uncheckable）、`Recipe.backup_globs` + `Glob`（在 `scan/mod.rs`，若 F 已合；否则先放 `recipe.rs`，F 时搬）、`UpdateBlocked::SelfUpdatesOnly` + copy record（agy 产生）、`Uninstall::Paths` 的 `optional` 指纹不符 → `NotOurs`、扫描规则 4 与 `globs` 参数（若 F 已合）、`adapters.standalone-{grok,agy}` 与 summary。**合并前**：CI runner 各录一次 `grok update` / `claude update`（stdin 关闭）的非交互行为（§五） | 三个 AI CLI 全部在列；agy「自己更新」无按钮 | C（两者都用 Paths 卸载） |
| **E** | rustup：配方、meta、fixtures、`Uninstall::Command` + `Probe`、`RemovesToolchains`/`DeletesCargoCaches`/`LeavesUnmanaged`/`EditsShellConfig`/`LeavesShellConfigLine` + 文案、`cargo::parse_crates2_bins` + cargo `inventory` 填 `InstalledArtifact.path`、`cargo::instance_id_for` 单一生产者 + 交叉锁相等测试、Upgrade 与 Uninstall 两个 plan 都持 cargo 锁、第一个 `NoCancel` 生产者（改四处「No adapter produces `NoCancel` yet」+ 两个测试名）、`operations.noCancelHint` 的两个读取方。**合并前**：对照 rustup `shell.rs` 核实它管理的 rc 集合（§6.4） | rustup 行、`self update`、官方卸载带四到五条警告 | B（骨架）；与 C/D 无关，可并行；`operations.noCancelHint` 的 `UninstallDialog` 读取方在 C 之前合时先只挂 `UpdatesPage` |
| **F** | 来源不明扫描：`scan/`（规则 0–3、`owned_roots` 表）、`session/scan.rs`、IPC、`api/queries/queryKeys`、`UnknownPage`、导航 + `INTERPOLATED_SUBTREES["nav"]`、`formatBytes`、四个线类型（`ScanStop` 带载荷）+ 镜像 + 形状测试、合成目录树测试 | 侧栏「来源不明」页 | A 之后即可；与 B–E 无关（归属泛读 `exe_path`/`path`/拥有的根；B 合并后额外认领四个启动器；**E 之前 `~/.cargo/bin/hexyl` 会暂时出现在这页**，E 填 `path` 后归到 cargo；D 加规则 4） |
| **G** | 第二批：占位。每个工具（uv 本体、bun、deno、mise、pnpm 独立版、Ollama.app）在 §9.3 的 CI 录制到手后**各自作为一份小规格**落地，形状全部记在 §十一「第二批配方」；此处不重复列 | | 各自的录制 |

A → B → C → D 串行（共用骨架）；E 在 B 之后；F 在 A 之后随时。并行时各用独立 worktree（通道设计 v2 §五的教训，仓库记忆里也有）。每步的 what-we-run.md 小节随该步合并。

`NoSafeMethod` 在 C 之后首批无生产者、Ollama.app 再次产生——它有闸门与 copy record 两个读取方，且 B 阶段的 claude 真的产生过它；记在此处免得下一轮当死变体报。

---

## 十一、明确不在射程（记 backlog，附正确形状）

- **安装尚未存在的工具**（spec §13：阶段 5）。`plan(Install)` 返回 `Unsupported`；实例不存在时闸门根本到不了（`SourceGone`）。形状：阶段 5 的精选条目指向官方安装命令 + 「打开终端并粘贴」，绝不是 Banager 自己 `curl | sh`。
- **「也删掉设置」/ 永久删除**。形状：不是同一屏上的复选框——`OpRequest`（`model.rs:345-350`）没有字段能带它，需要新 `OpKind::Purge`、`run_operation` 两处 `match` 各加一臂、`removal.rs` 的 purge 清单、对话框第二步确认并引用厂商自己的警告原文（claude：「Removing configuration files will delete all your settings, allowed tools, MCP server configurations, and session history」）。
- **清理 shell 启动文件里安装器加的行**。形状：一个按标记块（`# >>> grok installer >>>`…`<<<`；`# Added by Antigravity CLI installer` + 下一行；uv 的 `. "$HOME/.local/bin/env"`）删行的操作，先把原文件备份进废纸篓，预览显示 diff——它不是子进程，是新的 `Plan` 形状。`WillKeep { ShellConfigLines }` 已点名文件。
- **「装了 2 份」的合并计数 / Homebrew 行上的「还有一份原生的」**。形状：`Snapshot` 级派生视图「命令名 → 提供它的制品列表」，需要每个配方的 `twins`（brew cask/formula 名、npm 包名：`claude-code`/`claude-code@latest`、`@anthropic-ai/claude-code`、`antigravity-cli`、`grok-build`、`rustup`……）——这正是阶段 5 精选清单的 `install: [{adapter_id, key}]`，建两次是错的顺序；或 `InstalledArtifact.command_names: Vec<String>`（45 处构造）。§七的一句话已经回答了更有用的那半个问题。
- **rustup 工具链行**（`rustup toolchain list` 每行一个 `Tool` 制品，`rustup update <tc>` NoCancel，`rustup toolchain uninstall <tc>`，`check --no-self-update` 的「Update available」行）。journey 的形状可行，但是三个新 trait 方法与 spec §13 之外的范围；`rustup check` 的「Update available」行格式 UNVERIFIED（本机全最新，录不到）。
- **每工具「自更新已被关掉」检测**。形状：claude 专用读 `~/.claude/.last-update-result.json` 的 `timestamp`，超过一周则把 `selfUpdatingHint` 换成「它已经很久没有自己更新了」；grok 读 `config.toml` 的 `auto_update` 已是模板。
- **第二批配方**（§9.3 录制后）：uv 本体（`~/.config/uv/uv-receipt.json` 存在且 `install_prefix` 含 `realpath(uv)`——uv 自己的 `check_receipt_is_for_this_executable`，VERIFIED 源码；最新 = `releases.astral.sh/github/versions/main/v1/uv.ndjson` 首行；`uv self update`（它自己重跑安装脚本，VERIFIED，因此是 uv 的行为不是 Banager 的）；卸载 `~/.local/bin/{uv,uvx}`，保留 receipt、`uv tool dir`（Banager 的 uv 适配器列的就是它）、`uv python dir`）；bun（`~/.bun/bin/bun`；只有 GitHub Releases → 1 h 缓存；`bun upgrade` 单次 rename，VERIFIED 源码；卸载 `~/.bun`）；deno（`~/.deno/bin/deno`；`dl.deno.land/release-latest.txt` 去 `v`；`deno upgrade`（`remove_file` 后 `rename`，极短无二进制窗口，VERIFIED 源码 → 记入预览）；卸载 `~/.deno`，保留 `~/Library/Caches/deno`；**永不** `deno uninstall`——那删的是用户脚本）；mise（`~/.local/bin/mise` 且两层上无 `lib/.disable-self-update`；`mise.jdx.dev/VERSION`，calver；`mise self-update -y --no-plugins`；卸载**只删二进制**或 `mise --yes implode` 先 `--dry-run` 预览并警告它会删 `~/.local/share/mise/installs` 里所有语言运行时——到那步再定）；pnpm 独立版（`realpath(pnpm)` 直接在 `~/Library/pnpm` 下且无 `corepack`/`Cellar`/`node_modules` 分量；`registry.npmjs.org/-/package/pnpm/dist-tags`；`pnpm self-update`；卸载只删 `~/Library/pnpm/pnpm`，`store/` 是全机共享的、`bin/` 是用户的全局包）；Ollama.app（`/Applications/Ollama.app` 且不被 brew 列为 `ollama-app` cask——唯一要看 brew inventory 的配方，只能在合并后的快照上判，不在 `detect()`；版本 `defaults read … CFBundleShortVersionString`；`ollama.com/api/update` 204/200；无更新命令 → `SelfUpdatesOnly`；卸载需 `sudo rm -rf /Applications/Ollama.app` 且要退出 app → `NoSafeMethod`，直到有「`$HOME` 外的管理员拥有的 app 包」规则）。
- **corepack 管的 pnpm**（本机的 pnpm 就是它：`/opt/homebrew/bin/pnpm → corepack/dist/pnpm.js`，既非 brew formula 也非 npm 全局包，Banager 今天看不见）。形状：npm 适配器在 `npm ls -g` 见到 `corepack` 时发 `InstanceNote::CorepackShims`，通知说「pnpm/yarn 由 corepack 管理」。
- **自定义安装目录**（`BUN_INSTALL`、`DENO_INSTALL`、`GROK_BIN_DIR`、`MISE_INSTALL_PATH`、`UV_INSTALL_DIR`）。v1 只认默认路径；形状：更多 `HostEnv` 字段，各自以配方为读取方。
- **来源不明页的每行操作**。形状：「在访达中显示」= `/usr/bin/open -R <path>` 一条命令（`open_ollama_app` 模式，`ipc.rs:421,550`），路径取自同一进程产出的扫描结果、绝不来自客户端；「移到废纸篓」需要 §6.3 全部五条检查与每文件预览——那是它自己的一个阶段，因为「Banager 不知道这是什么」正是不提供的理由。
- **uv 管的 Python 等工具管理的树的归属**（`~/.local/share/uv/python/…`）。形状：`Adapter` trait 加 `owned_roots(&self, env) -> Vec<PathBuf>`（uv：python 与 tool 目录；mise：`~/.local/share/mise`），扫描读它——一个 trait 方法、七个一行实现。阶段 4 已把它的**扫描侧一半**拉进来（§8.3 规则 3 的 `owned_roots(inst)` 查表：brew 的 Cellar/Caskroom/opt、standalone 的工具根），trait 方法是那张表将来的归宿；等真实机器上的来源不明列表显示 uv-python 这个模式常见了再做。
- **grok 的 `self_updates` 读 `~/.grok/config.toml`**（初稿的 `SelfUpdates::GrokConfig { default: true }`）。形状已在初稿；条件是 CI 录制到「`auto_update = true` 时启动会**静默安装**」的证据（grok.md §5 开放问题 2）。到那时 `self_updates: bool` 变回一个小枚举、`~/.grok/config.toml` 进 what-we-run.md 的「读的文件」。
- **`Latest::HttpText { url }`**（纯文本端点）与 **agy 的 `url_x86_64`**。前者的生产者是 deno（`dl.deno.land/release-latest.txt`）与 mise（`mise.jdx.dev/VERSION`），随它们的第二批规格加入；后者等 Intel runner `curl` 到 `darwin_amd64.json` 后加为 `HttpJsonField` 的第二个 URL 并把 x86_64 分支从「uncheckable」改成「查」。
- **Homebrew `rustup` formula 的孪生**（§6.4）。两个 UNVERIFIED 点交 CI runner：(1) brew 装的 rustup 是否与原生共用 `~/.rustup`/`~/.cargo`；(2) brew 用户是否也会得到一份 `$CARGO_HOME/bin/rustup`（rustup 自我复制的代理安装）从而多出一行 `standalone-rustup`。(1) 为真则 rustup 的 `warnings` 加一条点名孪生的警告（需要一个本地信号：`/opt/homebrew/opt/rustup` 或 `/usr/local/opt/rustup` 存在）；(2) 为真则 rustup 的路线指纹要排除「brew 也列了 `rustup` formula」——那是 §十一 Ollama.app 同款的合并后判断。
- **`HostEnv.arch`**。不需要：agy 的架构分支用 `std::env::consts::ARCH`（§3.1）；只有当某个配方要在**运行时**按架构改行为（而不是编译期）才值得加字段。
- **`ManagerInstance.display_name`**。考虑过用于分组标签，否决：标签是每工具每语言的，前端已按 id 键文案。将来若需要 Rust 侧的每实例标签（第二个 Homebrew 前缀是唯一另一例），字段加在 `ManagerInstance`，读取方 `InstalledPage`、`UpdatesPage`、`UninstallDialog`、`notActionableMessage` 四处。
- **`OLLAMA_HOST` 为 https 远端时被 allowlist 挡住**（§4.2）。形状：`RealHttpClient::with_extra_host`，由 `Session::new` 传入。
- **靠重跑 `curl | sh` 更新的工具**（无自更新命令）。首批十个里没有；出现时答案是 `SelfUpdatesOnly` 式文案指向官网，永不是 Banager 跑安装器。
- **Homebrew 的「彻底删除」（zap）**。永不（§6.7）。
- **Banager 自己下载并替换二进制**（替 agy 更新）。永不——那是厂商安装器的事。

---

## 十二、请作者拍板（每条附推荐默认）

| # | 问题 | 推荐默认 | 理由 |
|---|---|---|---|
| Q1 | 路径清单卸载怎么进废纸篓：(a) `execute()` 逐条调 macOS 的 `NSFileManager trashItemAtURL:`（进程内、不需要 FDA、访达「放回原处」可用）；(b) 初稿的一条 `/bin/mv -n … ~/.Trash/Banager – <工具> <日期>/`（**已被本机复现否决**：基名冲突静默留下程序目录；且 `~/.Trash` 受 TCC 保护，§0.1）；(c) macOS 15+ 用 `/usr/bin/trash`、13.3–14.x 退到 `rm -rf`？ | **(a)。** | (b) 在真实上下文里跑不通也表达不了 claude；(c) 两个机制、一半用户不可逆。(a) 的代价是 `Plan` 多一个枚举臂（Q16）和一个 macOS-only 依赖，换来「放回原处」和无摩擦。步骤 C 合并前的无 FDA 核实是阻塞项（§6.2）；若那次核实意外失败，回退是同一 `Trasher` trait 下的 `create_dir` + `rename` 实现，并接受一次 FDA 之旅——那时再回来拍板。 |
| Q2 | 自更新的工具（Claude Code、Antigravity CLI；Grok Build 待核实）照常显示更新徽章 + 「它平时会自己更新」，还是像自更新的 cask 一样藏在「包含自更新的应用」开关后面？ | **照常显示。** | 徽章是拿启动器的活版本比出来的，是真话；藏起来等于默认对 AI CLI 说不出「它落后了」——那是 Banager 的第一问；本机 claude 的自更新就是关着的。grok 是否静默安装 UNVERIFIED，核实前不给它这句提示（§4.4）。 |
| Q3 | agy 不提供「更新」按钮（`SelfUpdatesOnly`），显示新版本号并让用户在终端跑一次？ | **是。** | `agy update` 零文档、零选项、没人跑过（agy.md §4）。Google 一旦有文档，改配方一行。 |
| Q4 | 卸载默认保留设置、登录、历史（`~/.claude`、`~/.claude.json`、`~/.grok` 的大部分、`~/.gemini/antigravity-cli`），阶段 4 不提供「全部清除」？ | **是。** | `~/.claude` 与 VS Code 扩展、桌面 app 共用（VERIFIED 文档）；`~/.grok` 有 `auth.json`；`~/.gemini` 与 Gemini CLI 共用。清除是另一屏、另一个 `OpKind`（§十一）。 |
| Q5 | 提供 `rustup self uninstall -y`（`NoCancel`，四条警告，两把锁），还是让 rustup 永远 `NoSafeMethod`？ | **提供。** | 它是官方方法；藏起来意味着想删 Rust 的人没有路。四条警告把它会删什么全说了。觉得首发爆炸半径太大就选 `NoSafeMethod`，同样诚实。 |
| Q6 | Banager 永不编辑 shell 启动文件；但 rustup 自己的卸载可以（不传 `--no-modify-path`）？ | **两者皆是。** | 编辑 `.zshrc` 是有自己风险的一类改动（§十一）；传 `--no-modify-path` 会留一行每开终端都报错的 `. "$HOME/.cargo/env"`，对小白更糟。`EditsShellConfig` 警告说明。 |
| Q7 | 读 `~/.claude/settings.json` 的 `autoUpdatesChannel` 决定查 `/latest` 还是 `/stable`？ | **读。** | 一个 JSON 键；不读的话 stable 频道的用户会被告知一个 `claude update` 装不上的新版——`UnchangedAfterUpgrade` 虽然准确，但徽章误导。读不到按 latest。「键值 → 端点」的映射本身是推断（UNVERIFIED，§3.1），推断错的失败模式无害且已被 §4.4 第 5 条覆盖；这次读取记入 what-we-run.md 的「读的文件」。 |
| Q8 | 在 `RealHttpClient::send` 里加编译期 HTTPS 主机 allowlist（`http://` 豁免）？ | **加。** | 主机从 4 到 7 到 11；注释不是检查；一个 Cloud Run 长域名写进名单和 what-we-run.md 比藏在代码里让人放心。已知缝隙 `OLLAMA_HOST=https://…` 记 §十一。 |
| Q9 | 首批只做 claude、agy、grok、rustup？ | **是。** | 只有这四个能从本机录真 fixture；合起来覆盖了阶段 4 的每种机制（自更新、路径清单卸载、官方卸载、`NoCancel`、共享配置、PATH 遮蔽、`--zap` 危害）。 |
| Q10 | 第二批 fixture 用手动触发的 CI 工作流录，作者审核后提交？ | **是。** | 本机没有那六个的独立安装；在作者机器上装它们会改动作者自己的环境。 |
| Q11 | 来源不明页打开时扫描 + 「重新扫描」按钮，不并入刷新？ | **是。** | 刷新保持快；`Snapshot` 只关于受管来源；大 `~/bin` 的 10 s 不拖慢已安装页。 |
| Q12 | 扫描放 `crates/banager-core/src/scan/`，不是 spec §3 画的 `adapters/unknown.rs`？ | **是。** | 它不是 Adapter；读代码的人不该在 `adapters/` 里找到一个没有 `impl Adapter` 的模块。附录 C 记为有意偏离，像阶段 3 记 Ollama 那样。 |
| Q13 | 「有更新」的一对版本用内联字符串测比较逻辑，不为录制而把本机 claude 降到 `versions/2.1.277`？ | **内联。** | 阶段 3 计划允许内联测试数据；端到端的「显示更新→跑 `claude update`→版本移动」录制以后在 CI runner 上做。 |
| Q14 | 注册形状为 `standalone-<tool>` 每工具一个 adapter id（D1），而不是单一 `standalone` + `standalone:<tool>` 实例？ | **每工具一个。** | 前端标签、`unverified_version`、fixture 目录、并发探测零改动；不撞 `uv`/`ollama`。逻辑仍只有一份 `StandaloneAdapter`。 |
| Q15 | 两条空状态文案（`emptyStates.noSources` / `nothingInstalled`）改写为提到自带安装器的工具？ | **改。** | 一句话，两种语言，让「Claude Code 去哪了」有答案。 |
| Q16 | `Plan` 的命令部分改成 `action: PlanAction { Command \| TrashPaths }`（32 处构造、四处读取方、TS 镜像），还是保住「一个程序一个 argv」而让 `execute()` 跑 N 条 `/bin/mv`、预览只显示第一条？ | **改 `Plan`。** | N 条命令配一条预览是对「执行前先看到确切命令」撒谎；`mv` 又有 TCC 问题（Q1）。枚举让 `CommandPreview`、`run_plan` 都在编译期被迫处理两臂。改动是机械的，一次改完。依赖二选一：直接 `objc2-foundation`（一个调用、可控）或 `trash` crate 配 `DeleteMethod::NsFileManager`（同一个调用，多一层）；推荐前者。 |
| Q17 | 悬空但指向工具根的启动器（半卸载态）保留为一行 `LauncherOnly`、闸门放行让用户再点一次卸载；还是按「没应答」处理成 `NotResponding`（闸门拒绝，用户得自己到访达里删那个链接）？ | **`LauncherOnly`。** | 这个状态只会由 Banager 自己的卸载中途停下造成（§6.2 的顺序保证），Banager 得能收尾；一个 `InstanceNote` 变体、一句话、闸门不改。`NotResponding` 留给「程序在但不应答」。 |

---

## 附录 A：每个新增或改变用途的字段/变体的读取方

| 字段 / 变体 | 生产者 | 生产读取方 | 用户看到 |
|---|---|---|---|
| `ManagerInstance.id = standalone-<tool>` | `detect()` | 全工作区；`Settings.ignored_updates` | 每工具一个组 |
| `ManagerInstance.exe_path`（启动器） | `detect()` | `plan()`、`reconcile()`、`CommandPreview`、`sourceNoticesFor` 的 `{{command}}`、扫描规则 0 与 1 | 每个预览里的命令 |
| `ManagerInstance.prefix`（工具根；rustup 为 `$CARGO_HOME`） | `detect()` | 扫描规则 3（经 `owned_roots` 表，只对 standalone-claude/agy/grok 与 brew）、`SymlinkIntoRoot` 指纹 | （间接）更少的「来源不明」行 |
| `InstanceNote::{NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther}` | `detect()` | `sourceNoticesFor` 四个新分支 | 两页上的一条 info 通知 |
| `InstanceNote::LauncherOnly` | `detect()`（§3.3 第 2 步） | `sourceNoticesFor` 第五个分支；`plan(Uninstall)` 检查 2（`AlreadyGone` 的条件）；扫描规则 0 | 「只剩下 claude 这个链接了」+ 可以再点卸载 |
| `InstalledArtifact.path` | `inventory()`（standalone：真实二进制；uv 今天已填；cargo 步骤 E 起填） | 扫描规则 2（**第一个读取方**；对 uv 的 shim 决定归属） | （间接）`ruff` 不出现在来源不明 |
| `InstalledArtifact.auto_updates` | `inventory()` | `UpdatesPage.rowDescription` 的 `selfUpdatingHint`（**第一个非 Pinned 读取方**） | 「它平时会自己更新」 |
| `UpdateBlocked::SelfUpdatesOnly` | `check_updates()`（`upgrade: None`，步骤 D 的 agy） | 闸门 `plans.rs:184`；`updateStateOf`；`UPDATE_BLOCKED_KEYS` | 「自己更新」徽章，无按钮 |
| `UninstallBlocked::NoSafeMethod` | `inventory()`（无 `uninstall`） | 闸门 `plans.rs:187`；`UNINSTALL_BLOCKED_KEYS`；`InstalledPage.tsx:144,329` | 「无法在这里卸载」+ 一句话，无按钮 |
| `PlanAction::TrashPaths { paths }` | `plan(Uninstall)`（Paths） | `execute()`（逐条 `Trasher::trash`）；`run_plan`（拒绝）；`OpSummary.argv_preview`（空）；`CommandPreview` 的 `TrashPaths` 支 → `uninstall.trashPreview` | 「Banager 会自己把上面列出的 N 项移到废纸篓……」 |
| `Trasher` / `RealTrasher` / `MockTrasher` | `Session::new` 注入 | `StandaloneAdapter::execute` | 项目出现在废纸篓、可「放回原处」 |
| `Warning::WillTrash / WillKeep / AlreadyGone / RemovesToolchains / DeletesCargoCaches / LeavesUnmanaged / EditsShellConfig / LeavesShellConfigLine` | `plan(Uninstall)` | `warningKey/warningArgs` → `UninstallDialog` 列表 | 对话框的项目列表 |
| `KeptWhat::{ToolState, NotOurs}` | `plan(Uninstall)`（agy 的根；optional 指纹不符） | `warnings.ts` 的 `Record<KeptWhat, string>` | 「它的对话、历史和工作文件……」/「它不属于这次安装」 |
| `Fault::PathChanged { path }` | `execute()` 复核 | `faultKey/outcomeArgs` → 操作条 + 日志抽屉 | 「失败：… 在预览之后有了变化」 |
| `AdapterError::UninstallUnsafe { path, reason }` + `UninstallUnsafeReason` | `plan(Uninstall)` 检查 1–5 | `plan_operation_error` → IPC kind `uninstall_unsafe` → `planFailureMessage` → `UninstallDialog.refusalText` | 四句拒绝之一 |
| `Plan.locks` 含 cargo 的 id（rustup 的 Upgrade 与 Uninstall） | `plan()`，经 `cargo::instance_id_for` | `run_operation:437-450` | cargo 刷新静静等待 |
| `cargo::instance_id_for(cargo_home)` | — | `CargoAdapter::detect`（`:161`）与 rustup 的 `extra_locks`；相等测试 | — |
| `cargo::parse_crates2_bins` | — | rustup 的 `LeavesUnmanaged`；cargo `inventory` 填 `path` | 「hexyl 是用 Cargo 装的……」 |
| `Plan.cancel_policy = NoCancel`（rustup） | `plan()`——第一个生产者 | `ops/mod.rs:331`（Running 时拒绝）；`OpSummary.cancel_policy:205` → `OperationBar.tsx:38`；`operations.noCancelHint`（UpdatesPage 与 UninstallDialog） | Running 时无 Cancel 按钮；预览下一句提醒 |
| `Recipe.backup_globs` + `Glob` | 静态数据（步骤 D） | `removal.rs`（可选删除）、扫描规则 4 | agy `.old` 进废纸篓 / 不出现在来源不明 |
| `Recipe.latest.*` 的 URL | 静态数据 | `check_updates()`；allowlist 测试；what-we-run.md | 更新行 |
| `ALLOWED_HTTPS_HOSTS` | 常量 | `send()`；配方测试；what-we-run.md | （拒绝时）一行「无法检查」 |
| `UnknownScan.*` / `UnknownEntry.*` / `ScanStop::{FileLimit{max_entries}, TimeLimit{max_secs}}` | `scan_unknown` | `UnknownPage`（§8.2 逐字段；停止横幅里的数字） | 来源不明页 |
| IPC `scan_unknown` | — | `api.ts` → `useUnknownScan` → 页面 | 同上 |

初稿列过、现已**删除**的行（免得下一轮再报）：`HostEnv.rustup_home`（无可观察读取方，§3.2）、`Uninstall::Paths.source`（唯一的「读取方」是文档，改为 `///` 注释 + README，§3.1）、`Recipe.{binary, display_name, homepage}`（与 `id`/meta TOML 重复，§3.1）、`SelfUpdates::GrokConfig`（§十一）、`Latest::HttpText`（§十一）、`uninstall.trashNote` 的「任一 `WillTrash` 存在」读取方（换成 `plan.action` 的形状）、`uninstall_unsafe` 的 `trash_unavailable` reason（检查 6 删除）。

## 附录 B：本设计拒绝做什么，拒绝在哪里执行

| 拒绝 | 执行点 |
|---|---|
| 跑 `curl … \| sh` 或任何 shell | 配方里没有字段能命名启动器之外的程序；`PlanAction::Command.program` 只可能是实例的 `exe_path`；`TrashPaths` 不 spawn |
| 重跑任何安装脚本 | 同上（`uv self update` 重跑的是 uv 自己的行为，第二批 what-we-run.md 写明） |
| 给 brew 传 `--zap` / `--force` / `--ignore-dependencies` | brew 测试（步骤 A） |
| 删 `$HOME` 之外的东西（含父目录是链接逃逸出去） | 检查 1（canonical 父目录） |
| 删不属于用户的东西 | 检查 3 |
| 顺着链接删到工具根之外 | 检查 4 + `canonicalize`（悬空时词法归一） |
| 永久删除任何东西 | 唯一的删除动作是 `Trasher::trash`（系统「移到废纸篓」）；`Trasher` trait 没有别的方法 |
| 一条卸载移了一半报成功（同名基名、中途停下） | 逐条移入（无基名冲突）；启动器最后 + `reconcile.present` = 启动器的路线判断 → `StillInstalledAfterUninstall`；`LauncherOnly` 行让用户收尾 |
| 预览之后文件系统变了还照删 | `execute()` 复核 + 逐条 `(st_dev, st_ino)` → `Fault::PathChanged` |
| 进程内做除「移到废纸篓」之外的任何文件系统写入 | `execute()` 只调 `Trasher::trash`；测试断言 `MockTrasher.calls()` 逐字等于 `paths`，临时 HOME 里除此无变化 |
| 版本没动却说升级成功 | `run_operation:713`（现有），靠 `reconcile` 对应答的工具总有版本 |
| 启动器还在却说卸载成功 | `run_operation:688-691`（现有），`reconcile.present` = 启动器的 lstat + 指纹 |
| 中途停止后假装知道结果 | `run_plan:500-502` → `Unconfirmed`；`run_operation:787-799`；`TrashPaths` 的 `execute()` 在取消/失败时同样返回 `Unconfirmed`/`Failed` 让它 reconcile |
| 给 Banager 驱动不了的更新一个按钮 | `SelfUpdatesOnly` + 闸门 |
| 给没有核实方法的卸载一个按钮 | `NoSafeMethod` + 闸门 |
| 取消**正在运行的** rustup `self update` / `self uninstall` | `NoCancel` + `ops/mod.rs:331`（Queued 时可以取消——什么都还没启动） |
| 让 rustup 删 `.crates2.json` / 替换 `rustup` 二进制时 cargo 在读 | 两把锁（Upgrade 与 Uninstall）+ `run_operation:437-450` + 锁名相等测试 |
| 把 UNVERIFIED 的事实写进用户看到的句子 | grok 不得 `selfUpdatingHint`（§4.4）；claude 通道映射的失败模式已覆盖（§3.1）；rustup 的 rc 集合与 TCC 都是合并前核实项 |
| 一条 `--version` 触发工具自更新 | `version.env`（§3.4） |
| 连接没人审过的主机 | `ALLOWED_HTTPS_HOSTS` + `send()` + 配方测试 |
| 把 brew/npm 装的副本当原生列出 | 共享排除 + 指纹（§3.3） |
| 在来源不明扫描里执行、删除、写入 | `scan_unknown` 是 `read_dir`/`symlink_metadata`/`canonicalize` 上的纯函数，没有 `CommandRunner`、没有 `OperationManager` |
| 编辑 shell 启动文件 | 没有任何机制能表达它（§6.8） |

## 附录 C：对 spec / 架构图 / 三份设计的有意偏离

| 处 | 原说法 | 本稿 | 理由 |
|---|---|---|---|
| 来源不明扫描位置 | spec §3 `adapters/unknown.rs`，架构图问「是否 Adapter」 | `scan/mod.rs`，独立只读路径 | §8.1 |
| 「展示官方删除清单，用户确认逐项删除」 | spec §4.2 | 一个 `PlanAction::TrashPaths` 计划，`execute()` 逐条调系统「移到废纸篓」，每条一个 `WillTrash` 项目；逐项勾选留待「也删设置」那轮 | §6.2：一条 `mv` 表达不了同名基名，也进不了受 TCC 保护的 `~/.Trash` |
| 「检测到 2 个 claude」 | spec §4.2 一句计数 | 「输入 claude 时运行的是 Homebrew 那份」 | 更可行动；对称的一半记 §十一 |
| 首批工具 | spec §4.2 列十个 | 四个 | fixture 只收真机录制 |
| 元数据 TOML + Rust 配方 | spec §4.1 | 照做（safety 的 TOML 配方未采纳） | spec 原文 |
| `Plan.cancel_policy: SafeKill \| …` | spec §5 | 只有 `KillThenReconcile \| NoCancel` | `17d8ef7` 已删 `SafeKill` |
| `Plan` = 一个程序一个 argv | spec §6、架构图 §4.3、三份设计与本稿初稿 | `Plan.action: Command \| TrashPaths` | §6.2：基名冲突与 TCC（§十三 #19/#20），Q16 |
| 「NoCancel 在任何状态都拒绝」 | 本稿初稿（核对于 `17d8ef7`） | Running 时拒绝、Queued 时接受 | `99a9d6f`，§0.1 |
| 自更新工具走 `--greedy` 开关 | safety §3.4 | 照常显示 + `selfUpdatingHint` | D5 |
| `standalone:<tool>` 单适配器 | safety §1.1 | `standalone-<tool>` 每工具注册 | D1 / Q14 |
| `trash -s` / `rm -rf` 分支 | reuse §5.2、journey §5.2 | 系统 `NSFileManager trashItemAtURL:` 一种 | Q1；`trash` 15.0 才有，`rm -rf` 不可逆 |
| rustup 工具链行 | journey §2.3 | 不做 | §十一 |
| `Fault::PathChanged` 的进程内复核 | reuse/journey 无 | 有 | §6.3 |

---

## 十三、评审处理记录（2026-09-24）

初稿经三个视角（code-reality / safety-facts / dead-and-scope）对抗评审，48 条。每条都由本稿作者在 `8ba6f52`
与本机上重新核过再定：**采纳 46、部分采纳 2（#29、#31）、驳回 0**。「采纳」意味着本稿正文已按该条改写，
不是「记下来以后改」。评审自己也有几处小错（#2/#37 的基线、#28 的位置、#31 的机制、#34 的 stdin），一并记在理由里。

| # | 视角 · 严重度 | 一句话 | 处理 | 核实与理由 |
|---|---|---|---|---|
| 1 | code-reality · breaks | 规则 3 按 `prefix` 归属会吞掉整个 bin 目录 | **采纳** | `pip.rs:68-74,92-100,125-137,153-156`、`uv.rs:138-141`、`pipx.rs:213-216`、`npm.rs:194-197` 全是 `exe_path.parent()`；本机 `~/.local/bin/python3.12 → ~/.local/share/uv/python/…` 会产出 prefix 为 `~/.local/bin` 的 pip 实例。§8.3 规则 3 改为 `owned_roots` 查表，示例按含 pip 实例的真实情况重写，测试加反例。 |
| 2 | code-reality · wrong | `NoCancel` 语义与基线已变 | **采纳** | 评审写的基线 `a6fe7e7`+未提交改动已过时：HEAD 是 `8ba6f52`，`99a9d6f` 已提交 Running-only 语义（`ops/mod.rs:331`、`OperationBar.tsx:38`）。§0.1/D6/§五/附录 B 改写，四处「No adapter produces NoCancel yet」列入步骤 E。 |
| 3 | code-reality · wrong | `warnings`/`extra_locks` 以 `&HostEnv` 为参数，`plan()` 拿不到 | **采纳** | `adapters/mod.rs:442` 签名无 `HostEnv`。改为 `fn(&Detected, …)`，`Detected.cargo_home` 按 `cargo.rs:133-136` 同一规则算。 |
| 4 | code-reality · wrong | `parse_crates2_entries` 给不出 `bins` | **采纳，fixture 部分修正** | `cargo.rs:40-55` 从键取名、值被扔掉，确认。新 `parse_crates2_bins`；评审要求「录 `~/.cargo/.crates2.json` 进 fixture」——`adapters/fixtures/cargo/1.98.1/crates2.json` 已存在且含 `"bins":["hexyl"]`，直接用它。 |
| 5 | code-reality · wrong | `Refused(String)` 到不了 `uninstall_unsafe` | **采纳** | `adapters/mod.rs:123`、`ipc.rs:211-215`、`sources.ts:625-640` 确认。加 `AdapterError::UninstallUnsafe { path, reason }` + `UninstallUnsafeReason`（四臂）+ IPC 臂 + `switch` 臂 + 形状用例，§9.1。 |
| 6 | code-reality · wrong | 悬空启动器有两个互相矛盾的答案 | **采纳** | 定义一次：链接文本词法归一后指向 root → `LauncherOnly` 实例（`unavailable: None`，闸门放行），否则无实例；`present` = 同一套路线判断。§3.3、§3.6、Q17。 |
| 7 | code-reality · wrong | argv 顺序下「移一半」的故事是假的 | **采纳** | 被 #19 的逐条执行吸收：启动器最后；`LauncherOnly` 上缺失的程序目录列为 `AlreadyGone`；kill/失败 between items 测试，§6.2/§6.3/§9.4。 |
| 8 | code-reality · wrong | `Paths.source` 的对话框读取方不存在 | **采纳** | `Plan` 无此槽位、§9.2 无键。字段删除，改为配方常量的 `///` 注释 + fixture README（与 #36 同）。 |
| 9 | code-reality · gap | rustup 卸载的 `noCancelHint` 无人渲染 | **采纳** | `UninstallDialog.tsx:213-256` 不读 `cancel_policy`，确认。键改名 `operations.noCancelHint`，两个读取方（UpdatesPage、UninstallDialog）+ 测试。 |
| 10 | code-reality · gap | `AGY_CLI_DISABLE_AUTO_UPDATE` 是承重墙但 UNVERIFIED，「不会更糟」是假的 | **采纳** | 本机实测（带/不带变量各一次）：1.2.10 的 `agy --version` 不写日志、不改 `update_status.json`、不派生更新器——变量无法用 `--version` 验证，但更重要的是 `--version` 本身不触发更新器。「不会更糟」删除；每版录制必须复做这一观察；失败时的退路写明。§3.4。 |
| 11 | code-reality · gap | `self update` 只持 rustup 锁 | **采纳** | `rustup.md §2/§5`：`cargo` 是指向 `rustup` 的代理，替换时撞上 `cargo --version` 会撕裂。Upgrade plan 加 cargo 锁，§2.4/§五。 |
| 12 | code-reality · minor | 通知里的 `{{command}}` 不是代码 | **采纳** | `SourceNotices.tsx:33` 是 `t()` 纯文本；`withCommand` 只在 `UpdatesPage.tsx:466`、`InstalledPage.tsx:330`、`UninstallDialog.tsx:108`。§七/§9.2 改为「纯文本」，与现有通知一致。 |
| 13 | code-reality · minor | `create_dir_all` 对已有目录静默成功 | **采纳** | 本机 `mkdir` 复现。系统调用方案下不再建目录；保留的 `rename` 回退实现用 `create_dir` + `AlreadyExists` → 拒绝，§6.2。 |
| 14 | code-reality · minor | 「六条」实为七条 | **采纳** | 改为五条运行时检查 + 检查 7 变配方单元测试 + 检查 6 删除，`execute()` 复跑同一批，§6.3。 |
| 15 | code-reality · minor | 「2,000」「10 秒」写死在文案里 | **采纳** | `ScanStop { FileLimit { max_entries }, TimeLimit { max_secs } }`，文案用 `{{count}}`/`{{seconds}}`，§8.2/§9.2。 |
| 16 | code-reality · minor | `HostEnv` 加字段要改 32 处构造 | **采纳（以删除字段解决）** | `grep "HostEnv {"` = 32 处，确认。与 #44 一起：`rustup_home` 无读取方，字段不加，`HostEnv` 不动，§3.2。 |
| 17 | code-reality · minor | grok 的 `grok.fish` 漏列 | **采纳** | 本机 `~/.config/fish/completions/grok.fish` 存在（148 KB）。加为 `File · Program · optional`，§6.3。 |
| 18 | code-reality · minor | claude 通道映射未标 UNVERIFIED | **采纳** | claude.md §4 原文「UNVERIFIED as an exact internal mechanism」。§3.1 与 Q7 标出并写失败模式。 |
| 19 | safety-facts · breaks | 一条 `mv` 只移了启动器、静默留下 626 MB | **采纳** | 本机复现（§0.1 表）：`mv -n` 两个同名源，第二个静默跳过、退出 0；无 `-n` 则 `Not a directory` 退出 1；`du -sh ~/.local/share/claude` = 626M。§6.2 整段重写为逐条执行。 |
| 20 | safety-facts · breaks | 无 FDA 的进程进不了 `~/.Trash` | **采纳** | 本会话有 FDA（`ls ~/.Library/Mail` 可读），复现不了；与 Catalina 起的已知 TCC 行为一致。定案改用 `NSFileManager trashItemAtURL:`（`Trasher` trait），步骤 C 合并前在无 FDA 构建上核实为阻塞项，Q1 改写。 |
| 21 | safety-facts · breaks | 取消/被杀后报成功且残留不可清理 | **采纳** | `ops/mod.rs:797` 的 stopped 臂确认。逐条执行 + 启动器最后 + `LauncherOnly` + `AlreadyGone`，残留唯一形态是一个悬空链接且行仍在，§6.2/§3.3。 |
| 22 | safety-facts · wrong | 规则 3 对 brew/uv/pipx/pip 的 `prefix` 是假承诺 | **采纳** | 与 #1 同一处；brew → `Cellar/Caskroom/opt`，standalone → 工具根，其余空，§8.3。 |
| 23 | safety-facts · wrong | 「`mv` 到不存在的目录退出 1」对单源不成立 | **采纳** | 本机复现：单源被改名、退出 0。§0.1 表订正；回退实现的 `create_dir` 规则同 #13。 |
| 24 | safety-facts · wrong | agy 的根标成「你的数据」、claude 的 `~/.claude` 标成「设置」 | **采纳** | 本机 `du`：`~/.gemini/antigravity-cli/{bin 12M, log 50M, conversations 196M, brain 103M}`；`~/.claude` 4.4 GB。`KeptWhat::Data` → `ToolState` 并改文案；`~/.claude/downloads` 进 `remove`（Cache，optional）；`SettingsAndHistory` 文案加「工作文件」。不逐子目录删 agy 的根：没有厂商清单说哪些能单独删。 |
| 25 | safety-facts · gap | grok 的「通常会自己更新」是 UNVERIFIED 当事实 | **采纳** | grok.md §5 开放问题 2。grok `self_updates: false`，`GrokConfig` 变体移到 §十一等核实，§4.4/D5/Q2。 |
| 26 | safety-facts · gap | 父目录是链接时逃逸出 `$HOME` | **采纳** | 检查 1 对每种 `Expect` 都 canonicalize 父目录并与 canonical `home` 比；`st_dev` 检查不需要——系统调用跨卷也是 rename，§6.3。 |
| 27 | safety-facts · gap | 外来的 `~/.local/bin/agent` 让 grok 永远卸不掉 | **采纳** | 本机就有多个别的 agent CLI。optional 指纹不符 → 跳过 + `WillKeep { NotOurs }`，§6.3 检查 4。 |
| 28 | safety-facts · gap | rustup 不管的 rc 文件里残留 `.cargo/env` 行；`hexyl` 从 PATH 消失 | **采纳** | 本机 `~/.zshrc:17` 确有该行（评审说「在 Docker 块里」不准确——它在块之后第 17 行，结论不变）；`~/.zshenv:1`、`~/.profile:1` 是 rustup 的。加条件警告 `LeavesShellConfigLine { path }`，`leavesUnmanaged` 文案加 PATH 后果；rustup 管理的 rc 集合标 UNVERIFIED、步骤 E 前核。 |
| 29 | safety-facts · gap | 未点名 Homebrew rustup 孪生 | **部分采纳** | 孪生的工具链确实会一起没（rustup.md §2，UNVERIFIED），但 `RemovesToolchains` 的「所有工具链」已把后果说全，且 `plan()` 看不到合并后的快照、本机也无孪生可核。§6.4 加说明，两个 UNVERIFIED 点记 §十一 交 CI；不加第五条警告。 |
| 30 | safety-facts · minor | 两处文件读取未披露；`layout.txt` 会把个人目录列表带进公开仓库 | **采纳** | what-we-run.md 加「读的文件」；agy 的 `layout.txt` 只列 `~/.local/bin/agy`；grok 的 `config.toml` 随 #25 不再被读，§9.3/§9.5。 |
| 31 | safety-facts · minor | 让用户跑裸 `agy` 会开 TUI；15 分钟节流；卸载竞态 | **部分采纳** | 15 分钟写进文案，采纳。「改成 `<exe> --version`」**驳回**：本机实测 1.2.10 的 `--version` 不触发更新器（#10），打开一次 TUI 才是能触发的动作，文案改为「打开它一次再退出」。竞态：`--version` 不派生，无竞态；若将来某版派生，reconcile 会看到启动器复活并报 `StillInstalledAfterUninstall`，已诚实，不加 sleep。 |
| 32 | safety-facts · minor | 主机数与代理数不一致 | **采纳** | `ALLOWED_HTTPS_HOSTS` 6 项；`http/real.rs:43-49` 的「four」含 http 的 Ollama；`ls ~/.cargo/bin` 13 个代理（rustup.md §8 自己写错成 12）。§4.2/§6.4/§8.3 统一。 |
| 33 | safety-facts · minor | 任何悬空 `~/.local/bin/claude` 都成了「没应答」的行 | **采纳** | 与 #6 同一处：链接文本不指向 root → 无实例，来源不明页列为失效链接，§3.3。 |
| 34 | safety-facts · minor | 升级命令未跑过，提示会挂 30 分钟 | **采纳（修正机制）** | `runner/real.rs:670` 已 `stdin(Stdio::null())`，提示会拿到 EOF 而不是挂——但工具对 EOF 的反应没人看过。CI 录制两条升级命令的非交互行为仍列为步骤 D 前置，§五。 |
| 35 | dead-and-scope · wrong | 规则 2 的相等匹配不上 uv 的 `path`，且被规则 1 完全遮蔽 | **采纳** | `uv.rs:65` 填 venv 目录、fixture 里 `ruff` 的 shim 确认。规则 2 改「以…开头」，uv 是第一个真实输入，测试加 uv 例，§2.3/§8.3/附录 A。 |
| 36 | dead-and-scope · wrong | `Paths.source` 无线上载体 | **采纳** | 同 #8。 |
| 37 | dead-and-scope · wrong | 基线钉错、行号漂移 | **采纳** | 评审写的 `99a9d6f` 也已过时；重钉 `8ba6f52`，行号全部按它刷新（`acquire_resource_lock:866`、`no_cancel:341`、`UnchangedAfterUpgrade:713`、`Unconfirmed` 臂 `:787-799`、`OperationBar.tsx:38`、`plan.locks:437-450`）。 |
| 38 | dead-and-scope · gap | `uninstall_unsafe` 无 Rust 生产者；检查 7/空清单无 reason | **采纳** | 同 #5；检查 7 改配方测试，空清单**有意**留通用 `refused`，§6.3/§9.1。 |
| 39 | dead-and-scope · gap | 规则 4 需要 `Glob` 而 F 声称独立于 B–E | **采纳** | F 只带规则 0–3；D 加 `globs` 参数与规则 4，`Glob` 定义在 `scan/`，§8.1/§十。 |
| 40 | dead-and-scope · gap | `Latest::HttpText` 与 `url_x86_64` 的 `Some` 臂无生产者 | **采纳** | 两者移出阶段 4，形状记 §十一；`HttpJsonField` 只剩一个 URL。 |
| 41 | dead-and-scope · gap | 步骤 B 合并了没有生产者的变体与字段 | **采纳** | §十按步切片：B 无 `uninstall`/`backup_globs`/`SelfUpdatesOnly`；C 加 `Uninstall` 族；D 加 `Glob`/`SelfUpdatesOnly`；E 加 `Command`/`Probe`。 |
| 42 | dead-and-scope · gap | cargo 交叉锁的字符串无人校验 | **采纳** | `cargo::instance_id_for` 单一生产者 + 同 `HostEnv` 上的相等测试，§2.4。 |
| 43 | dead-and-scope · gap | 悬空启动器被列两次 | **采纳** | 规则 0（原始路径相等）+ 测试，§8.3。 |
| 44 | dead-and-scope · minor | `HostEnv.rustup_home` 无可观察读取方 | **采纳** | 字段删除，rustup 的 `root`/`prefix` = `$CARGO_HOME`，规则 3 对 rustup 不生效（一切靠规则 1），§2.2/§3.2。 |
| 45 | dead-and-scope · minor | `Recipe` 重复 meta TOML；`kind` 无读取方 | **采纳** | 删 `binary`/`display_name`/`homepage`，用 `id`/`meta.name`/`meta.homepage`；`kind` 明说是文档性的，§3.1。 |
| 46 | dead-and-scope · minor | G 与 §十一 重复 | **采纳** | G 改占位并指向 §十一，§十。 |
| 47 | dead-and-scope · minor | 两个插值键会挂 `completeness.test.ts` | **采纳** | `completeness.test.ts:187` 的 `INTERPOLATED_SUBTREES` 确认。F 登记 `nav`，C 登记 `operations.outcome`，§9.2/§十。 |
| 48 | dead-and-scope · minor | 架构判定来源未指明 | **采纳** | `std::env::consts::ARCH`（编译期；Rosetta 报 x86_64 → 不查，安全方向），不加 `HostEnv` 字段，§3.1/§十一。 |

评审之外、处理时顺手发现并写进正文的事实：`OpSummary.argv_preview` 今天在 `src/` 里没有渲染读取方（只有
`types.ts:202` 的镜像）——本项目的签名缺陷已经在生产代码里，`TrashPaths` 给它空数组、不另造假命令行；
`ls -la ~/.local/bin` 显示本机今天没有 `~/.local/bin/grok` 回退链接（grok.md §1 一致），grok 的两个 optional
启动器链接在本机都不存在，配方照列。
