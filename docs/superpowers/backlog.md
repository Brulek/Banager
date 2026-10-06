# Backlog（来自 2026-09-18 阶段 0–1 分支终审）

终审结论：可合并，需先修 5 项（已在 feat/phase-0-1 上修复）。以下为终审与各任务评审中**推迟到后续计划**的事项，按归属计划分组。写新计划时先读这里。

## 2026-09-24 逐条核对（分支 feat/pre-release-polish）

下文很多条目写于数周前，之后被后续提交修掉，但条目本身没有更新。2026-09-24 按当时代码逐条核对，
每条都给了能证明状态的 file:line。**下列条目已关闭，下一轮不要当新发现重报**（括号内是修掉它的提交）：

- 已在早先分支修掉：`issued_plans` 泄漏、`records` 无上限（`93871e5`）；并发刷新重复广播
  `SnapshotChanged`（`93871e5`、`67d86b6`）；runner 的四条打磨（`3ccf239`、`6158bba`、`ba80106`、`2ee9244`）；
  `RunnerError::NotFound` / `Spawn` 无测试（`c39dd13`）；非 Unix `compile_error!`（`e337df2`）；
  tokio `rt-multi-thread` 移入 dev-dependencies（`08fe468`；`macros` 留在正式依赖是对的，
  runner 与 ops 的生产代码用 `tokio::select!`）；`session/mod.rs` 拆分（`af6057c`）；Codex M3 brew
  根结构体 `serde(default)`（`34b8fea`）；Codex N1 detect 测试依赖本机（`8569473`）；`uses-jq.txt`
  为空（`63aa31c` 录了有依赖者的 fixture）；更新确认框的版本跳变与 Ollama 摘要（`60188b4`、`71bbacb`）；
  `[profile.release]` 不生效（`64da96b`）；`fix-path-env` 无 `rev`（`6b8a0af`）；动态 i18n 键无法静态校验
  （`32e0505`、`51a3e18`）；更新列表虚拟化（`7a3ab59`）；`useRefresh` 绕过合并器（`93871e5`）；
  日志抽屉焦点陷阱与 Esc（`dc8683d`）；设置页单选按钮（`f35623b`）；`showDependencies` 全局开关
  （`5c2c653`）；更新页测试夹具（`7940893`、`a8f50b3`）；`greedy_casks` /「包含自更新的应用」
  （`6d0e38c`、`5e387b9`、`ffcbffd`）；CI 的 `concurrency` 与 `timeout-minutes`（`6a4005b`）。
- 本分支修掉：`SAVE_TMP_SEQ` 改 `Relaxed`（`989a495`）；`UninstallDialog` 无效的 eslint 抑制注释
  （`b1c7818`）；`item.planError ?? ""` 死代码改为类型收窄（`338f704`）；卸载对话框两次快速点击与
  残留的提交错误（`34e80fb`，连点实际比条目写的更糟：第二次 `mutate()` 让第一次的 `onSubmitted`
  永远不触发）；`clearSelectedUpdates` / `clearLogs` 查实没有需要它们的 bug，删除（`8739b5a`、`b67f5f6`）；
  字面量扫描扩到整个 `src`、两个字符的词也算（`1d27015`、`82e77ae`）；Codex M4
  `cancel_policy` 从没人读：`NoCancel` 运行中拒绝取消、界面不给「取消」按钮，排队中仍可取消，
  `SafeKill` 没有生产方也没有独立语义，删除（`17d8ef7`、`99a9d6f`）；生产代码不再带 Node 类型，
  测试单独一套 `tsconfig.test.json`，门禁改为 `pnpm typecheck`（`a6fe7e7`）；核对阶段「取消」按钮
  点了没反应（`201f760`）；重新签发的卸载预览在确认被禁用时仍叫用户「再确认一次」（`67ee5fc`）。

**核对后仍开着的**（形状见下文各自条目）：~~npm `prefix -g` 失败时的合成实例 ID；pip 分不清「没带 pip」
与「pip 坏了」~~（两条都已于 2026-10-02 做了，见下文）；`SUDO_ASKPASS` 透传（作者拍板）；GitHub Actions 的 Node 20 运行时（`@v4`，等额度恢复能跑
CI 时再升，升了没法在本地验证）；~~8pt 网格（约 51 处，需要看着界面改）~~（已不适用：2026-09-29 的审美规格
另定了尺寸——行高 52、内容边距 20、按钮高只有 20/24/28 三档，`docs/superpowers/2026-09-29-aesthetics-spec.md:13`、`:34-35`）；`releaseDraft`、
空机器首启（作者拍板或属于阶段 5）。刷新按钮已于 2026-09-28 加上（见下文「整个应用没有刷新按钮」）。另有规格写了、各阶段计划都没排进去的四块，
2026-10-02 核对时各做了一部分，仍开着的只有：本地快照缓存（spec §3/§5，启动时先渲染上次的结果；第一次检查先出清单
已做，见文末「第一次检查」一条）；操作日志落盘（§5/§6/§7；历史那半已做：`history.json` 记下 Banager 做过的更新与卸载，
更新页「最近的更新记录」读它，`crates/banager-core/src/history/mod.rs`、`src-tauri/src/history.rs`，日志一行也不落盘，
见 `docs/what-we-run.md`「Files Banager writes」）；「报告问题」只剩打开问题反馈页那半（「拷贝诊断信息」已做，在帮助菜单与
设置页，`crates/banager-core/src/diagnostics.rs`、`src/lib/diagnostics.ts`；仓库是私有的，去处待作者定）；菜单栏图标与
登录时启动（§8，属阶段 6；后台检查与通知已做，`crates/banager-core/src/auto_check.rs`、`src-tauri/src/auto_check.rs`、
`src-tauri/src/notify.rs`）。

## 阶段 4 已知缺口（2026-09-25，分支 feat/phase-4-standalone）

- **来源不明页认不出 `pip install --user` 装的命令。** 框架版 Python（Homebrew、python.org）的 `--user`
  把脚本放进 `~/Library/Python/3.X/bin`；那个目录在 PATH 上时，里面的脚本会列在「说不清来源」页，而它们的包
  列在已安装页的 pip 下。pip 的制品没有 `path`（`pip list --format=json` 不给脚本位置），pip 也没有「拥有的根」。
  整体复审确认为真（important）；修复做到一半时额度耗尽，**没有收下**：它给所有适配器共用的 `ManagerInstance`
  加了一个只有 pip 用的线格式字段 `user_scripts_dir`，并让 pip 的 detect 多跑一条 `python -c` 探测命令。未提交的
  改动存在 `~/dev/Banager/.superpowers/phase4/abandoned-pip-user-scheme-fix/`（补丁 + 一份录制，含本机路径）。
  已做的：页面导语不再断言「Banager 认识的来源都没有装过它们」，改成「没能对应到任何一个它认识的来源」
  （`uvx` 同样认不出，阶段 4 步骤 F 的交付说明已写）。正确形状：spec §十一 的 `Adapter::owned_roots(&self, env)`
  trait 方法——pip 的实现调用一次 `sysconfig.get_path("scripts", "osx_framework_user")`，结果只进扫描，不上线格式。

- **卸载之后回来的程序文件，已安装页上看不见**（2026-09-26；修复的 A、B 两部分在 `f3712ca` 与紧随其后的提交
  「Report what came back after a path-list uninstall's last pause, and name it in the log」，C 部分待作者决定）。
  path-list 卸载把启动器（清单最后一项）移进废纸篓之后还要停最多 3 秒（`removal::PUT_BACK_SETTLE`），还在运行的
  工具可能在这段时间里重建程序文件夹或下载缓存。已做的：A——`Adapter::reconcile_after_uninstall` 多收一个 `plan`，
  独立安装工具的读取在启动器不在时再用 `removal::left_behind` 看清单上的其它路径和备份文件，留下的算「还在」，
  看不清的算读不出（`Unconfirmed`）；B——`removal::execute_removal` 在最后一次停顿之后自己再看一遍，回来的每条
  路径在操作日志里各写一行（`LogNote::BackAfterUninstall`），结果报 `NeedsAttention(BackAfterUninstall)`，看不清时
  报 `Unconfirmed`。两者都不算预览按自己的规则判为「不是这个工具的」、这次也没移走的可选路径（与
  `removal::listed_path_back` 同一个例外）。**仍然存在的缺口**：启动器已经不在，而 `detect` 只认启动器
  （`route::probe` 为 `Absent` 就不列实例），所以下一次刷新没有这一行——回来的文件只出现在这次操作的结果和日志
  里，已安装页上没有可以再点一次的「卸载」；Claude Code 的程序文件夹与下载缓存也不在来源不明页固定扫描的那几个
  bin 文件夹里。**C 的形状**（没有做，是作者的产品决定）：`detect` 在启动器不在、清单上的程序路径
  （`RemovedWhat::Program`）还在时也列出这个实例，带一个新的无载荷 `InstanceNote`（例如
  `ProgramWithoutLauncher`，与 `LauncherOnly` 相对）——一次线格式变更，连带 TypeScript 镜像、两种语言的文案与
  `src/lib/sources.ts` 的读取方；`plan_removal` 的检查 2 现在对缺失的启动器报 `Missing`，要改成在这种状态下把
  它列为「已经不在」（`Warning::AlreadyGone`），移走其余的。**副作用**：手动删掉启动器、留下程序文件的安装
  （例如只执行过 `rm ~/.local/bin/claude`）也会因此出现一行——现在它不出现。

## 阶段 2（界面 / IPC）之前必须处理

2026-10-02 核对：本节五条全部早已修掉，逐条划去。

- ~~`crates/banager-core/src/adapters/mod.rs` `validate_package_name`：拒绝以 `/` 或 `.` 开头、含 `..` 段、以 `.rb` 结尾的名字，否则 `brew install --formula /tmp/evil.rb` 可执行任意本地 formula。IPC 暴露 install 之前必须修。~~
  —— **已于 2026-09-19 解决**（`b4d6722`）：`adapters/mod.rs:379-396` 拒绝以 `-`、`/`、`.` 开头、以 `.rb` 结尾、含 `..` 段的名字。
- ~~`src-tauri/tauri.conf.json`：`csp` 目前为 `null`；spec §6 要求禁止远程脚本与导航。界面计划的清单项。~~
  —— **已于 2026-09-19 解决**（`9e992fc`）：`src-tauri/tauri.conf.json:26` 是 `default-src 'self'`、`connect-src 'self'` 的真 CSP。
- ~~清理 create-tauri-app 模板残留：`src/App.tsx`（logo、外链、greet 表单）、`src-tauri/src/lib.rs` 的 `greet` 命令、`index.html` 标题。~~
  —— **已于 2026-09-19 解决**（`9e992fc`）：`src`、`src-tauri/src` 里没有 `greet`，`App.tsx` 是应用外壳，`index.html:11` 标题为 Banager。
- ~~`OpRecord.cancel` 是 `pub`，调用方可绕过 `cancel()` 的状态簿记；IPC 层接入时改为私有 + `Notify` 替代 `wait()` 的 20 ms 轮询。~~
  —— **已于 2026-09-19 解决**（`99a7bd7`）：`OpRecord`（`ops/mod.rs:116-121`）不带取消令牌，令牌在私有的 `OpInternal`（`:171-176`）；
  `wait()`（`:411-419`）等 `done_notify: Notify`，不再轮询。
- ~~`detect()` 在 euid 0 时返回空向量，与"未安装 brew"无法区分；界面需要区分显示。~~
  —— **已于 2026-09-22 解决**（`4a7a4c5`）：root 下 brew 的 `detect` 照样列出实例，标 `Unavailable::RefusesAsRoot`，不跑 brew
  （`adapters/brew/mod.rs:1092-1100`、`:1142-1143`，`model.rs:85`）。

## 阶段 3（其余来源）/ 存储与刷新层

2026-10-02 核对：本节四条全部早已修掉，逐条划去。

- ~~Settings 需要一个「包含自更新的应用」开关（阶段 2 计划已把 `greedy_casks` 从 `Settings` 中整体移除：只存不用，Session 从不读取，Homebrew 检查更新固定跑 `outdated --json=v2`）。实现时要把该选项从 Settings 经 Session 传到 Homebrew 的 `check_updates`（对应 `brew outdated --greedy`），届时一并调整 `Adapter` trait 的 `check_updates` 签名（会牵动已合并的阶段 0–1 代码）。~~
  —— **已于 2026-09-20 解决**（`9f9a43f`、`6d0e38c`）：`Settings.include_self_updating`（`settings.rs:82-89`）经 `CheckOptions`
  传到 brew 的 `check_updates`，打开时加 `--greedy`（`adapters/brew/mod.rs:1224-1227`）。
- ~~`brew/mod.rs` `maybe_update`：`brew update` 失败或超时（离线、首次 tap 同步慢）会让整个 `check_updates` 失败，应退化为"沿用旧索引 + 标记可能过期"（spec §3）；并发调用存在 TOCTOU 双重 `brew update`，需串行化。~~
  —— **已于 2026-09-20 解决**（`e4d10d9`，提醒挂到实例上是 2026-09-22 的 `b1a2ca9`）：失败或超时得 `IndexFreshness::MayBeStale`，
  照常读 `brew outdated`，实例带 `InstanceNote::IndexMayBeStale`（`adapters/brew/mod.rs:1219-1223`）；每个实例一把更新锁
  （`update_locks`，`:66`、`:761`）。
- ~~`AdapterMeta.verified_versions` 从未与 `ManagerInstance.version` 比较（spec §4.1 "未验证版本"角标）。~~
  —— **已于 2026-09-20 解决**（`eb4b34d`）：`AdapterMeta::unverified_version`（`adapters/mod.rs:113`），上线格式
  `ManagerInstance.unverified_version`（`model.rs:213`）。
- ~~spec §4.2 需更正：brew 7.0.3 的 `installed[]` 只有 `installed_on_request`，没有 `installed_as_dependency`（解析器与其文档注释是对的，spec 过时）。~~
  —— **已于 2026-09-22 解决**（`fd2f180`）：spec 已订正（`docs/superpowers/specs/2026-09-17-banager-design.md:116`）。

## 阶段 3 终审遗留：分支 feat/phase-3-sources 合并前必做（2026-09-20 立）

四路整分支通读复审（死字段／跨任务矛盾／重复与 fixture／注入面）共报 12 条独立缺陷，5 条 critical。
其中 13 条已在 `c7dcc29`（Rust 安全）与 `ccbea0a`（前端诚实化）修掉。**以下是明确留到下一轮的，
按依赖顺序排列。** 分支已推送，CI 见 GitHub；本地安全标签 `prewrite-d300730` 指向历史重写前的旧头，
确认无误后可删。

**一、实例级通道 —— 已定案并在实施中**
规格：`docs/superpowers/2026-09-22-instance-level-channel-spec.md`（草案经三路对抗评审重写，
评审证伪了草案的 7 处断言并抓出 2 处「照草案实施会直接坏掉」）。分五步落地：

- [x] 步骤 1 `643efc6` — npm 可写性查错目录：改为沿 `{prefix}/lib/node_modules` → `{prefix}/lib`
      → `{prefix}` 取最近存在的祖先再判权限。
- [x] 步骤 2 `b0ebc0f`..`0e6eceb` — 能力轴：`ManagerInstance.read_only_reason` 上线格式；
      **`Capabilities` 连同 trait 方法、七份实现与十一处测试 fake 一并删除**（本条同时结清下面
      「阶段 4 之前」那条同名条目）；`Session::issue_plan` 设统一闸门；前端删掉
      `READ_ONLY_ADAPTER_IDS` 与 `UpdatesPage` 的 stopgap，pip 与 npm 各自的只读文案分开。
- [x] 步骤 3 `02cc340`..`54a4399` — 状态轴：`InstanceStatus` 上线格式，`healthy` 删除，
      `refresh.rs` 四处改动全部落地，来源通知经 `SourceNotices` + `sources.ts` 的纯映射两页共用。
- [x] 步骤 4 `926fe13`..`b1a2ca9` — `CheckOutcome { candidates, notes }`（纯核心内部、不跨 IPC）
      + brew 的 `IndexMayBeStale`，渲染在更新页——「所有内容都已是最新」那句话所在的那一页。
- [x] 步骤 5 `aebb975`..`616cbb7` — 卸载确认屏与更新页的固定英文警告改成可本地化枚举。

**合取不变量已落地并有测试**：`session/plans.rs:51` 的 `if !instance.writable() || !instance.available()`，
测试遍历两个 `Unavailable` 变体 × 每个 `OpKind`。这是评审抓到的那条——修发现(6) 的「沿用旧产物」
若没有它，就会把发现(4) 原样造回来（Ollama 没跑时实例仍可写，结转回来的每行都带一个点了必失败的卸载按钮）。
全绿：286 个 Rust 测试、181 个前端测试。

规格 §8 列了明确不在射程的四项（每实例 `refreshed_at`、每包可操作性、`{{message}}` 动态英文透传、
全新 Mac 的安装引导），都给了正确形状，避免下一轮当新发现重报。
2026-10-02 核对：其中两项已做——每包可操作性（`UpdateCandidate.blocked`，见下文「每包可操作性」一节）；`{{message}}` 透传
照 spec §6 的形状做了，原话只在「显示技术细节」打开时出现，关着时认得出原因就说原因、否则只说没做成什么
（`planErrorMessage`，`src/lib/sources.ts:1274-1293`；设置保存、扫描、载入失败几处同样分开）。仍开着两项：每实例
`refreshed_at`（`ManagerInstance` 没有这个字段，`crates/banager-core/src/model.rs:202-225`）与全新 Mac 的安装引导（阶段 5 提案待拍板）。
—— 每实例那项**已于 2026-10-02 做完**（分支 `r5/k4-last-answered`，backlog-triage R12；草稿出自 Codex，改写后合入）：字段叫
`ManagerInstance.answered_at`（`crates/banager-core/src/model.rs:225`，不叫 `refreshed_at`——`Snapshot.refreshed_at` 的意思是
「这次刷新跑过」，这个是「这个来源回答过」），只在内存里，不写文件。`refresh` 在扇出前按 ID 从上一轮接续
（`session/refresh.rs:411`），这一轮清单和更新检查**都**回答了才盖新章，章是开始问它的时间（`:525`、`:714`）；两半都没换掉行
（都失败/拒读、任务崩溃、没响应、被操作占着、`detect` 崩溃）沿用；只有一半回答（行一半新一半旧）清空——时间只标它自己那次回答的行
（复审 k4 第 4 条）；找不到为空，首轮为空。`same_content` 改投影比较
（`model.rs:250`、`session/mod.rs:174`），只差时间的一轮不升代。只在「没有响应」的提示里说：
「显示的是它今天09:12响应时的结果」（`src/lib/sources.ts:380`、`:464`），空页面与「工具环境」那一行同一说法
（`src/lib/toolSetupCheck.ts:191`），概览的提示行也是这一句；没运行、root、https、没有pip 与软件清单提示不带时间。

**二～四、已于 2026-09-22 全部清掉**（`829ae63`..`7940893`，300 个 Rust 测试 / 193 个前端测试全绿）

- 适配器失败语义统一到 cargo 的模型：远程查不到 = 该项 `checkable: false` 带理由，不是整个来源失败。
  npm 不再谎报「全部最新」——判据改成「非零退出且没有任何发现」，比原定的「exit 1 且 stdout 空」更严，
  堵掉了 `{}` 与不可解析响应两个洞。
- 两块逐字重复抽成 `reconcile_from` 与 `ensure_instance_match`；**npm 那份漏 `kind` 检查的漂移
  由构造消除**（七个适配器现在共用同一份）。
- brew 的 root 拒绝回到 `BrewAdapter::detect`，不再禁掉其余六个来源；`RefusedAsRoot` 因此变成不可达，
  连同 TypeScript 镜像、界面分支与两种语言的文案一并删除。
- 闸门的拒绝不再是 `{:?}` 出来的原始英文；`PlanId` 改成 128 位随机令牌；`open_ollama_app` 的三个
  stdio 流置空。

**实施者自己抓到并修掉的两处**（简报没要求，但都是本项目的招牌缺陷）：
一是它的改动会让 pip 成为最大的「查不了」行来源，而那些理由文本因为一个三选一互斥分支加上
`truncate` 类**永远渲染不出来**——定义了、镜像了、没人看得见；二是更新页的标题计数只用了
`isActionable` 三个组成部分里的两个，会在六行没有按钮的行上方写「6 个可用更新」。

**仍然挡着合并的只剩作者本人的事项**（见下一节）。技术项全部清空。

**六、合并闸门（2026-09-23）已清零，以下是明确推迟、不挡合并的已知项**

合并前最后一轮四视角整分支通读（`bfcdf3b` 之后 42 个提交），加 Astra（GPT-6）对 Rust↔TS 线格式与
七路并发刷新的两轮独立复审，共报 5 条 critical、十余条 important，**全部修完**，最终代码上的
状态机视角复核结论为「无 critical」：每种运行结束方式恰好回收一次子进程、`killpg` 至多一次、
无无界挂起、输出不丢；14 条会把 stdout 交给解析器的命令全部标记 `OutputUse::Parsed`；四条结转
路径对同一实例互不重叠。

推迟项（都有明确形状，下一轮不要当新发现重报）：
- ~~**实例 ID 跨适配器唯一性只靠约定。**~~ —— **已于 2026-09-23 在 `1d424ad` 修复**（分支
  feat/phase-3-hardening）：七个适配器统一走 `model::instance_id` 构造（产出与旧 ID 逐字相同，因为
  ID 持久化在 `Settings.ignored_updates` 里）；`Session::with_adapters` 拒绝重复的适配器 ID；
  refresh 按适配器 ID 顺序探测，同 ID 只留第一个并记一条点名双方的 `SourceError`。
- ~~**丢弃一个 refresh future 只会 detach 其 worker，不会取消。**~~ —— **已于 2026-09-23 在 `9c643fe`
  修复**（分支 feat/phase-3-hardening）：refresh 的两层扇出改持 `AbortOnDropHandle`（按扇出顺序
  join 不变，所以结转/备注回并/panic 结转四条路径的 join 结果与原先逐一相同；没用 `JoinSet` 是因为
  它按完成顺序产出）；另外 `RealRunner::run` 在 future 被丢弃时 `killpg` 整组，否则 abort 只放了锁、
  命令还在锁外跑。~~`killpg` 仍至多一次，且只打未回收的 pid。~~ —— **已于 2026-09-23 在 `2ee9244`
  改变**：取消与超时改成先 `SIGTERM` 整组、给 `STOP_GRACE`（5 秒）让它自己收尾，收尾期间用
  `killpg(pgid, 0)` 探活，还没退的才补一次 `SIGKILL`；一次停止最多两次信号，不再是至多一次。
- ~~**升级途中取消仍报「结果未确认」而不是「你已取消」。** 这是有意的：升级后包仍在，证明不了新版本
  没在 kill 之前装上。真修需要在执行前先 reconcile 一次记下旧版本。~~ —— 2026-09-24 `c6ecf5b` 曾改成按
  执行前后两次读到的版本判：没变且是用户取消的报「你已取消」，变了报「已成功」。**同日撤回**，原来的判断是对的：
  命令中途被停下（取消或超时）的升级一律报「结果未确认」，不看版本。工具是在升级中途、不是结束时写下 Banager
  读的版本，被 `SIGTERM` 停下时又不回滚，所以版本变了不等于装完（brew 公式新 keg 已倒入、还没 link；cask 在
  `stage` 写了新版本元数据、新 app 还没装上），没变也不等于没动（pipx 先装包、最后才写元数据；cask 先把旧 app
  移出 /Applications、之后才写元数据）。Homebrew 与 pipx 的出处逐行写在 `run_operation`（`ops/mod.rs`）
  `Ok(Outcome::Unconfirmed)` 分支的注释里；`tests/ops_upgrade_version_test.rs` 里四个被停下的端到端用例、
  `tests/ops_cancel_test.rs` 里三个用例守着它。执行前那次读取仍保留，读到的版本只用于退出 0 的升级（见下面
  「假「成功」」一条）；在它读的时候按了取消，命令还没开始，报「你已取消」是真的，这一条也保留。
- ~~**npm 在 `npm prefix -g` 失败时用可执行文件路径合成一个不可用实例的 ID。** 更理想的是沿用上一轮
  快照里的实例，但那要把上一轮快照穿进 `Adapter::detect` 的签名，七个适配器都得改。~~ —— **已于 2026-10-02 做了**
  （`b5bf9ded`）：不改 `detect` 的签名，在 `refresh` 层做——`resume_unanswered_npm`（`session/refresh.rs`）认出
  `npm::unanswered_instance_id` 这个替身 ID，上一轮同一个可执行文件恰有一个 npm 时沿用它的 ID、前缀与版本，
  标「没有响应」，于是它的行和隐藏的更新照常结转。
- ~~**pip 的「不可用」分不清「这个 Python 根本没带 pip」和「pip 装了但坏了」**——两者退出码相同。~~ —— **已于
  2026-10-02 做了**（`81bf514c`、`4c50c361`）：`-m pip --version` 的 stderr 有一行是 Python 自己的
  「No module named pip」时报新的 `Unavailable::NoPip`，界面写「“python3.13”没有附带pip」（提示，不是警告，
  无按钮）；`pip.__main__` 之类的坏 pip 照旧「没有响应」。只读已有命令的输出，没有新命令。
- ~~**`Session` 的 `testing` 模块里有一个会改动实时状态的 `expire_issued_plans`**，而该模块刻意不是
  `#[cfg(test)]`，所以会进发布版的库。~~ —— **已于 2026-09-23 在 `11e5ac8` 修复**：`expire_issued_plans`
  收进 `test-support` this-crate-only 的 Cargo feature（`crates/banager-core/Cargo.toml`），默认不开，
  `src-tauri` 的测试通过 `[dev-dependencies]` 单独开它，resolver = "2" 保证不进发布版二进制。

**五、需要作者本人拍板**
- ~~**整个应用没有刷新按钮。** 刷新触发点一共四个：启动、操作完成、两个错误态里的「重试」，以及
  后台运行的 Homebrew 索引更新自行结束时（`ipc::refresh_on_background_change`，
  `src-tauri/src/lib.rs:31-34`）——最后这条不需要用户动手。
  「打开 Ollama」和设置开关这两条明确承诺过会刷新的路径已经修好，但一个软件管家没有常驻刷新控件
  是个洞。加常驻控件涉及位置与形态，是产品决定，没有代为决定。
  阶段 4 步骤 F 给「来源不明」页加了它专属的「重新扫描」——只重跑那一页的扫描，不刷新各来源——各来源仍无常驻刷新控件，这条待拍板的问题不变。~~
  —— **已于 2026-09-28 在分支 feat/ui-redesign 解决**：作者 2026-09-27 定了界面重构方向
  （`docs/superpowers/2026-09-27-ui-redesign.md`），位置与形态按它来——每一页顶上的页头
  （`src/components/PageHeader.tsx`）有「重新检查」，走的就是「重试」那一次刷新（`useRefresh` →
  `refreshIntoCache`），任何刷新在跑时它都停用、页头写「正在检查…」，平时写上次检查是多久以前。

## 每包可操作性（2026-09-24 立，分支 feat/per-package-actionability）

**已做**：`UpdateCandidate.blocked: Option<UpdateBlocked>`，唯一变体 `Pinned`，生产方有两个：brew 的
`parse_outdated`（读 `brew outdated --json=v2` 的 `pinned`，公式与 cask 都有），和 pipx 的 `parse_outdated`
（读 `pipx list --outdated` 行里名字后面的 ` [pinned]`，把它从名字上切掉）。Rust 侧闸门
`blocked_upgrade`（`session/plans.rs`）在 `issue_plan` 与 `submit` 复检里拒绝 `Upgrade`；
更新页 `isActionable` 多一个条件，按钮、勾选、「更新所选」和两个计数一起去掉；行上写「已固定」
并给出 `<该 brew 的绝对路径> unpin <名字>`（cask 为 `--cask`；路径取自该实例的 `exe_path`，
以代码样式显示）；pipx 的行写 `<该 pipx 的绝对路径> unpin <名字>`，说明句里的来源名按实例给（Homebrew / pipx）。
自己会更新的 cask（`auto_updates`）另有一句，不承诺它停在现在的版本。Banager 不代为解除固定。
pipx 被固定的例子是改过的录制（原在 `adapters/fixtures/pipx/1.17.3/list-outdated-pinned.txt`，只插了 ` [pinned]`，
README 写明、测试核对），和 brew 7.0.6 的 `outdated-pinned.json` 一样，与设计文档「只收真机录制」的字面冲突。
—— **已于 2026-10-06 按作者决定 R7 移走**：两份都在 `adapters/fixtures-derived/`（`brew/7.0.6/outdated-pinned.json`、
`pipx/1.17.3/list-outdated-pinned.txt`），各带 README；`fixtures_layout_test.rs` 核对每份都有 README 和同版本的录制。
brew 7.0.6 `receipts/` 里不是录制的 27 份收据（25 份按 cask 目录构造、2 份由 `package-manager-manager.json` 改出）也一并移到
`adapters/fixtures-derived/brew/7.0.6/receipts/`；`adapters/fixtures/brew/7.0.6/receipts/` 只剩本机录的 5 份，`cask_receipt.rs` 有测试核对。

**已知未做**（事实依据见 `.superpowers/actionability-facts.md`，那是本机未入库的调查记录；下一轮不要当新发现）：
- ~~**pipx 的 `unpin` 连注入包一起解除**：`pipx unpin <环境>` 会把该环境里注入的包也一并解除固定
  （pipx 1.17.3 `commands/pin.py:82-92`，没有只解主包的选项）。Banager 不列注入包（不传
  `--include-injected`），行上的说明没提这一点。~~
  —— **已于 2026-10-02 做了**：pipx 被固定的行（更新页与已安装页共用 `blockedDetail`）在解除固定命令下多一行
  「这条命令也会解除注入到它环境里的包的固定。」（`UPDATE_BLOCKED_KEYS.Pinned.note`，只对 pipx；不说有几个）。
- ~~**假「成功」**：pipx 被锁定的工具（有 lock 文件）、uv 用 `==` 装的工具、brew 已停用的 cask（C2）、
  brew 装着的 caskfile 读不出来（C4）——工具都跳过更新却退出 0，Banager 报「成功」而什么都没变。~~
  —— **已于 2026-09-24 在 `c6ecf5b` 修复**，走的是「核对版本真的变了」这条路：升级前后各用同一个
  `reconcile` 读一次版本，退出 0 而版本没变时报新结果 `NeedsAttention(UnchangedAfterUpgrade)`
  （「更新命令显示成功，但版本和更新前一样……」，指向操作日志，四种情况工具都在日志里说了原因）。
  四种情况各有一个端到端测试（`tests/ops_upgrade_version_test.rs`）。代价是每次升级多一次清单读取；
  brew 在后台 `brew update` 还没跑完时这次读取会被拒（`IndexUpdating`），那时照旧只看在不在。
  **仍未做**：更新页事先不知道这些状态，行上照样有「更新」按钮，点了才知道；要提前标出仍得多读上面那几份输出。
  其中 ~~brew 已停用的公式与 cask~~ —— **已于 2026-10-02 做了**：`check_updates` 本来就为限定名字读一次
  `brew info --installed --json=v2`，据其 `disabled` 给候选加新的 `UpdateBlocked::Disabled`，更新页标「已停用」、
  不给按钮、说明 Homebrew 不再提供更新（有替代就点名），闸门照样拒绝；没加命令。pipx lock 文件、uv `==`、
  caskfile 读不出三种仍未做（要读新输出、先录真机 fixture，见分诊 NEEDS-AUTHOR）。
- ~~**卸载被固定的包**~~ —— **已于 2026-09-24 修复**：Homebrew 7.0.6 不加 `--force` 时拒绝卸载被固定的包
  （`uninstall.rb:48-49`、`cask/uninstall.rb:40-44`），用的是 `onoe` 不是 `ofail`，公式这边退出 0。
  现在从清单读 `pinned`（`brew info --installed --json=v2` 的公式与 cask 条目都有，`formula.rb:3140`、
  `cask/cask.rb:574`），写进新字段 `InstalledArtifact.uninstall_blocked: Option<UninstallBlocked>`（唯一变体
  `Pinned`，唯一生产方 brew 的 `parse_info_installed`）。闸门 `blocked_uninstall`（`session/plans.rs`）在
  `issue_plan` 与 `submit` 复检里拒绝 `Uninstall`，按实例、类型、名字三者匹配；IPC 报
  `{"kind":"uninstall_blocked"}`。已安装页该行没有「卸载」按钮，说明句替换简介，给出由该实例 `exe_path`
  拼的 `unpin` 命令（与更新页共用 `unpinCommand`，以代码样式显示）；卸载确认框遇到这条拒绝时单独措辞。
  说明句原本一律承诺「下次检查时就会提供卸载，最晚在你下次启动 Banager 的时候」，可 Homebrew 没应答时
  这一行是结转下来的，Homebrew 再次应答之前不会有「卸载」按钮；同日改成：来源没应答的行换用
  `descriptionSourceUnavailable`，只说「之后 Banager 检查时只要 Homebrew 有应答，就会提供卸载」。
  更新页被固定的行（`updates.blocked.Pinned.description` 与 `descriptionSelfUpdating`）当时对没应答的来源
  也作同样的假承诺，已在分支 feat/per-package-actionability 上同样补了
  `descriptionSourceUnavailable` / `descriptionSelfUpdatingSourceUnavailable` 修掉：`UpdatesPage.tsx` 的
  `rowDescription` 按候选自己实例（`snapshot.instances.find`，与其命令用的是同一份查找）是否应答挑选句子。
  Banager 不传 `--force`，也不代为解除固定。pipx 不产生它：`pipx uninstall` 照样删除被固定的工具
  （pipx 1.17.3 `commands/uninstall.py` 不读 `pinned`）。
- ~~**已安装页的「有更新」徽标**~~ —— **已于 2026-09-24 修复**：原先 `updatableIds` 把 `snapshot.updates`
  里每一条都算作有更新，包括被固定的、`checkable: false` 的和被忽略的。现在两页共用
  `src/lib/updateState.ts`：`notIgnored` 决定更新页列出哪些，`updateStateOf` 决定每一条是可更新、只读、
  查不了、被工具拒绝（`blocked`）还是来源没响应；更新页的按钮、勾选、计数和徽标，已安装页的徽标，都从它来，
  两处 `switch` 都没有 `default`，新增状态不写文案 `tsc` 就不过。已安装页只在更新页会给「更新」按钮时写
  「有可用更新」；被固定的写「已固定」（有更新时读 `blocked`，已是最新时读 `uninstall_blocked`），查不了的写
  「无法检查」，只读来源写「仅供查看」，被忽略的写「已忽略更新」，来源没应答的（比如 Ollama 没在运行，更新是之前
  结转下来的）写「有新版本，暂时无法更新」。最后这一种起初仍写「有可用更新」，更新页却没有按钮，与上面那句不符，
  同日改掉：已安装页只在 `updateStateOf` 为 `actionable` 时写「有可用更新」，这正是更新页给按钮的
  `isUpdateActionable`。有测试把两页逐行对照，其中包括一个没在运行的 Ollama。
  更新页一处可见变化：来源没响应的行，徽标仍写「更新」，但颜色从 `info` 改成 `neutral`，与其余没有按钮的行一致。
  2026-09-27 起（分支 feat/updates-page-feedback）：更新页的「忽略」拆成「跳过这个版本」和「不再提醒」，
  `notIgnored` 换成同一文件里的 `hidingRule`（更新页经 `notHidden` 读它）：键在 `Settings.ignored_updates`
  里的、或（键，目标版本）在 `Settings.skipped_versions` 里且能跳过的（`canSkipVersion`），都不列出。已安装页
  对前者写「不再提醒更新」，对后者写「已跳过 {{version}}」（Ollama 模型写「已跳过新版本」，不印摘要）；逐行对照的
  测试也加上了跳过的行。能跳过，指目标版本只代表一个版本。查不了的行不算（它的目标版本就是已装版本）；声明为
  `version :latest` 的 Homebrew cask 也不算：`brew outdated` 对它贪婪时（Banager 在「包含自更新的应用」打开时传
  `--greedy`，Homebrew 自己的 HOMEBREW_UPGRADE_GREEDY、HOMEBREW_UPGRADE_GREEDY_CASKS 也能让它贪婪），下载一变
  就把它列出来，目标版本却永远是 latest，跳过 latest 等于永不提醒，按钮说的「下个版本发布时再提醒你」
  做不到。这两种行只给「不再提醒」，已存下的跳过也不隐藏它们。

## 阶段 4 之前

- ~~`Adapter::capabilities()` 七份实现零调用方~~ —— **已于 2026-09-22 在 `e4b13b4` 整体删除**。六个字段里界面唯一需要的「能不能写」是每实例的事实（npm 取决于 prefix 权限），静态的每适配器 trait 方法承载不了，所以移到 `ManagerInstance.read_only_reason`；`search` / `upgrade_all` / `background_check` / `cancel_safe` 四个零调用方直接删。阶段 3 曾因这条砍掉 `needs_network` 标志，该裁决依然正确。

- ~~`crates/banager-core/src/adapters/brew/mod.rs:274-277` **「brew update 失败」的提醒在没有可更新项时被丢掉**。阶段 3 任务 3 把 `brew update` 的失败从「整个来源检查失败」降级成一条提醒，但提醒只能挂在 `UpdateCandidate.warnings` 上，而 `UpdateCandidate` 必须带一个真实的 `ArtifactKey`。于是当 `brew update` 失败、`brew outdated` 又报告零个可更新项时，`for candidate in &mut candidates` 无可遍历，提醒被静默丢弃——偏偏这正是最需要它的情形：本地公式索引陈旧，所以「没有更新」这个结论本身可能就是错的。任务 3 的评审与修复代理都独立认定这是计划自身的设计缺口而非实现偏差，修复代理据此返回 BLOCKED 而没有擅自发明接口，这是对的（2026-09-20 控制者裁决：接受现状，记在这里）。
  修的代价：要给 `ManagerInstance`（或 `Snapshot`）加一条实例级 warnings 通道，连带 TypeScript 镜像、线格式表、界面渲染与测试——本身就是一个完整任务，不该塞进阶段 3 的任何一格。
  可接受的理由：后果是少说了一句提示，不是做错了动作；一旦真有可更新项，提醒照常显示。**不阻塞 v0.1**，但要在做 `Capabilities` 那条（同样需要实例级字段）时一起做掉——两者是同一个通道。~~
  —— **已于 2026-09-22 解决**（实例级通道步骤 4，`926fe13`..`b1a2ca9`）：提醒不再挂在候选上，而是 `CheckOutcome.notes` 里的
  `InstanceNote::IndexMayBeStale`（`adapters/brew/mod.rs:1219-1223`），由 `refresh` 并进实例状态（`session/refresh.rs:563`），
  没有一个可更新项时照样显示在更新页（`src/lib/sources.ts:417`）；测试 `test_check_updates_reports_a_failed_brew_update_as_a_note_on_the_source`。

## 阶段 4（独立安装工具）进行中的遗留（2026-09-25 立，分支 feat/phase-4-standalone）

- **`OLLAMA_HOST` 为 `https://` 时被 https 名单挡住，界面上却只说「没有响应」**（spec §4.2、§十一）。步骤 A 的 `host_allowed`（`crates/banager-core/src/http/real.rs`）只豁免 `http`；`normalize_ollama_host`（`runner/path_env.rs`）原样保留 `https://` 值；`OllamaAdapter::detect`（`adapters/ollama/mod.rs`）把 `send` 的拒绝 `unwrap_or(false)` 成「没应答」，于是显示为 NotResponding（地址是本机且装了 Ollama.app 时是 NotRunning，带一个按了也没用的「打开 Ollama」按钮），没有一个字说是 Banager 自己拒绝的。用户于是去查自己的反向代理而不是 Banager。
  **现状已写明**（2026-09-25）：`docs/what-we-run.md` 的 Ollama 与 Network 两节各有一段说 `https://` 的 `OLLAMA_HOST` 会被拒绝；`crates/banager-core/tests/what_we_run_test.rs` 的 `test_what_we_run_says_an_https_ollama_host_is_refused_and_it_is` 把这两句话钉在 `host_allowed` 的实际行为上——修掉缝隙时测试与两句话要一起改。
  **修法**（spec §十一 定的形状）：`RealHttpClient::with_extra_host(ollama_host)`，由 `Session::new` 传入；`src-tauri/src/lib.rs` 的 `run()` 启动时已 `HostEnv::discover()` 过一次，值可以从那里经 `AppState::new`（`src-tauri/src/state.rs`）带到 `Session::new`。要不要放行取决于有没有真实用户这样配（spec：「等有人报了再做」）。
  ~~**若暂不放行，至少让通知说实话**：`InstanceNote` 按设计不带载荷（`model.rs`，线格式是裸字符串），塞不进一条 `Message`，得加一个新的无载荷变体（例如 `DaemonHostRefused`），连带 TypeScript 镜像、两种语言的文案与 `src/lib/sources.ts` 的读取方——一次线格式变更，单独成一个任务。~~ —— **已于 2026-10-02 做了说实话那半**（`81bf514c`、`4c50c361`、`1e76391a`）：新的 `Unavailable::HttpsHostRefused`，`OllamaAdapter::detect` 用 `host_allowed` 同一条规则判定、根本不发这次请求；通知写「不支持通过https连接Ollama」并说明可改用 http:// 地址后重新打开，不给「重新检查」也不给「打开Ollama」。放行 https 那半仍等作者。

- ~~**cask 的命令行链接只认第一个 `app`**（2026-09-25，步骤 F 整体评审项）。`/usr/local` 的 Homebrew 上，cask 的 `binary` 把 `/usr/local/bin/code` 链到 `/Applications/Visual Studio Code.app/…` 里面，不在扫描给 brew 的三个根（`Cellar`/`Caskroom`/`opt`）之下，而 `/usr/local/bin` 每次都扫，于是已安装页列在 Homebrew 下的 cask，其命令在来源不明页被说成「没有来源装过」。现在 `parse_info_installed`（`adapters/brew/parse.rs`）把 cask 的 `InstalledArtifact.path` 填成 `brew info --installed --json=v2` 里 `app` 条目旁的绝对 `target`（`/Applications/X.app`，随 `--appdir` 走），扫描规则 2 据此认领。仍会列出的（`docs/what-we-run.md` 扫描一节已写明）：同一 cask 第二个 `app` 里的命令、`pkg` 装到 `.app` 与 `Caskroom` 之外的命令、`app` 条目没有绝对 `target` 的 cask。`path` 只有一个位置；改成多值是 Rust + TypeScript 镜像的线格式变更，单独成任务。~~ —— **已于 2026-10-02 做了**（`903016e7`，分支 `r5/g2-standalone-scan`）：没改线格式，扫描规则 2 另认 cask 的 `binary` 链接本身（内存里的 `facts.command_inputs.provided`），去向须落在该条目指名的文件、`Caskroom/<token>` 或 cask 的 app 里（与「输入命令跑的是哪一份」同一条规则，`commands::cask_places`）；第二个 `.app` 里的命令、`app` 没有绝对 `target` 的 cask 的命令不再列出，`pkg` 装的命令照旧列出（`tests/unknown_scan_test.rs` 的 `test_rule_2_claims_a_cask_binary_link_into_a_second_app_or_an_app_with_no_target`）。
  仍开着：`pkg` 装到 `.app` 与 `Caskroom` 之外的命令照旧列在「其他程序」页，单独成任务。

- **「放回原处」的记录只在一次卸载之内隔开**（2026-09-25，步骤 C；2026-09-26 改成跨卸载也隔开，见本条「已做」；
  末段两件没核实的事仍开着）。无「完全磁盘访问」时，`trashItemAtURL:`
  间隔 ≤1.5 秒的连续调用只有第一项在 `~/.Trash/.DS_Store` 里留下 Finder 的「放回原处」记录，间隔 ≥2 秒时每项都有
  （spike：15/15 与 4/4，一台 Mac、macOS 27.0，机制不明；记录在调用返回之后才写）。`removal::execute_removal`
  原先在同一次卸载里每移一项之后停 `PUT_BACK_SETTLE` = 3 秒（最后一项之后也停，再报告完成），但这个停顿**只管一次操作之内**：
  操作管理器同时跑最多 3 个操作（`ops/mod.rs` 的 `Semaphore::new(3)`），两个 path-list 卸载并发时，两边的移动仍可能挤进
  2 秒之内，后一项就会丢掉记录——文件照样在废纸篓里，只是只能手动拖回；卸载还在运行时退出 Banager，刚移的那一项也可能丢掉记录。
  即使在一次卸载之内，3 秒也只是让每一项「多半」有记录（四次观察），不是保证；文案与信任文件都这样说（裁定 29）。
  **修法的形状**：把「上一次移到废纸篓的时刻」放进全进程共享的一处（`Session::new` 交给所有独立安装工具适配器的是同一个
  `Arc<RealTrasher>`），每次移动前补足到 3 秒，而不是只在 `execute_removal` 的循环里停；`MockTrasher` 与测试不受影响。
  **现在不做的理由**：C 只有 Claude Code 一个 path-list 卸载，两次卸载都要各自预览、确认；步骤 D 加入 grok 与 agy 之后再做。
  （2026-09-26：步骤 D 已加入两者，path-list 卸载现在有 Claude Code、Antigravity CLI、Grok Build 三个，各自只锁自己的实例，
  所以两个不同工具的卸载可以同时跑；本条按上面的约定到期，仍未做。）
  **已做**（2026-09-26，步骤 D 整体评审项）：照上面的形状做了，差一处——时刻不放进 `RealTrasher`，而放进新的
  `removal::LastMove`，由 `standalone::all` 把同一个交给它建的每个适配器（与同一个 trasher 并排；`Session::new` 只调一次
  `all`，所以它就是全进程共享的那一处）。原因：`RealTrasher::trash` 是在一项最后一次检查**之后**才被调用的，在那里补等 3 秒，
  就把「检查紧挨着移动」拉开成最多 3 秒的空档（`take_turn`；信任文件写明检查与调用之间什么也不看）。现在每一项在检查**之前**等：
  先拿到 `LastMove` 的锁，再等到离上一次移动（本次卸载的或另一个卸载的）满 `settle`，然后检查、移动、记下时刻，最后才放锁——
  另一个卸载的移动插不进来，每次移动都在上一次移动返回之后至少 `settle`（生产里是 3 秒）才开始。取消与时间预算照旧能打断这两段等待，
  等另一个卸载的时间也算进本次卸载的预算。`new` 单独建的适配器各有自己的一个，所以原有测试的断言一条没改。测试：`removal::tests` 的
  `test_execute_removal_waits_the_gap_after_another_uninstalls_move_before_its_first_item` 与
  `test_execute_removal_waits_out_another_uninstalls_turn_unless_cancelled_or_out_of_time`，`standalone::tests` 的
  `test_grok_and_agy_uninstalls_running_at_once_keep_the_gap_between_all_their_moves`（用 `one_per_recipe`，即 `all` 建的
  那组适配器）。预览里「访达的『放回原处』多半也能用」一句因此对同时跑的两个卸载也成立，没有改。
  **同一处记两件没核实的事**（仍开着）：没有人真的点过「放回原处」（spike 只核对了 Finder 的记录；作者合并前在 Finder 启动的构建上
  手动核一次，结果写进 `docs/what-we-run.md` 的「Moving files to the Trash」）；macOS 27.0 以外的版本与 Intel Mac 没跑过
  （`tests/standalone_uninstall_test.rs` 的 `#[ignore]` 冒烟测试在作者的终端与 CI 上覆盖「移得进去」，悬空链接也在内，
  但那两处都不是无 FDA 的环境，不覆盖「放得回来」）。
- ~~**检查 1 的「永不」清单挡住了 agy 的 `~/.cache/antigravity`**（2026-09-25，步骤 C）。步骤 C 照 spec §6.3 检查 1
  括号里的清单执行：路径的 canonical 父目录不得是 `~` 本身，也不得是 `~/.local`、`~/.config`、`~/.cache`、`~/Library`、
  `~/.cargo`（`recipe::SHARED_FOLDERS`；`removal::plan_removal` 按解析后的路径查，`recipes::tests` 按配方的写法查），
  拒绝理由是 `SharedFolder`。claude 与 grok 的清单都通过；spec 给 agy 列的 `~/.cache/antigravity`（父目录 `~/.cache`）过不了。
  **步骤 D 要做的决定**：给这一条路径一个有测试的明确例外（只对 optional 的 `Cache`），或者改 spec 的清单——不要悄悄放宽整条规则。~~
  —— **已于 2026-09-26 解决**（2026-10-02 核对：Antigravity CLI 的配方把它列为保留项 `KeptWhat::InstallerCache`，
  `crates/banager-core/src/adapters/standalone/recipes.rs:214-217`；`SHARED_FOLDERS` 仍含 `.cache`，`recipe.rs:431`）。
  **步骤 D 定案：改清单，不改规则**（步骤 D 计划裁定 1；`314b093`、`13cf03d` 于 2026-09-26 落地，本条关闭）。
  `~/.cache/antigravity` 不移，列为保留项（新变体 `KeptWhat::InstallerCache`，文案说它是安装器的下载暂存文件夹、通常是空的、
  Banager 不会移动直接放在 `~/.cache` 里的东西、可以自己删），只在它存在时列出；检查 1 与 `SHARED_FOLDERS` 一字未改。
  本机它是空的（2026-09-26 录制时 `staging/` 0 项，见 fixture README）；安装脚本先把下载放在这里、校验后才复制到位
  （agy.md §3a），所以中断的安装可能在里面留下下载到一半的文件（程序本体约 176 MB）；后台更新器是否也在这里暂存没有核实。
  作者可见的后果：卸载 Antigravity CLI 后 `~/.cache/antigravity` 留在原地，对话框会说；来源不明页不会列它（它不在那一页扫描的
  bin 文件夹里）。
- **`~/.local/bin` 整个是链接时，卸载会被拒绝**（2026-09-25，步骤 C，裁定 24；原标题还有「或 `~/.claude`」，那一半
  2026-09-26 由步骤 D 改掉，见本条末）。步骤 C 要求家目录到清单上每条路径之间的每一层都是真目录（`removal::check_item`
  的祖先规则），所以用 dotfiles 工具把 `~/.local/bin` 整个链到别处（哪怕仍在家目录里）的用户，Claude Code 这一行照常显示
  （`route::probe` 先解析启动器所在的目录），但卸载在预览时就被拒绝，理由是 `not_what_instructions_expect`（文案说
  「和预期的不一样」，ⓘ 里说「它或它所在的文件夹链接到了别处」）。步骤 D 之后 Antigravity CLI 也一样：它的启动器 `~/.local/bin/agy`
  在同一个文件夹里，也不是 optional。spec §6.3 的检查 1 原本接受这种链接。
  **修法的形状**（真有人碰到再做）：只对启动器所在的那一层，允许它是一个指向家目录之内、又不在 `SHARED_FOLDERS` 里的链接，
  并把它解析后的目录与预览时记下的一起比对（`ItemIdentity` 已经随计划带着），配测试；不要整体放宽祖先规则——
  `~/.claude -> ~/Documents` 这类别名正是它挡住的。**现在不做的理由**：没有观察到这样的安装，放宽需要单独评审。
  **`~/.claude` 那一半已不成立**（步骤 D，`e15a340`）：清单上 optional 的路径若确认不了是这个工具的——它所在的文件夹是链接
  也算——就保留并说明（`removal::keeps_instead`，`WillKeep { NotOurs }`），不再拒绝整个卸载。所以 `~/.claude` 是链接、
  里面又有 `downloads` 时，`~/.claude/downloads` 留在原地、预览说一句，其余照常移走（测试
  `test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere`）；祖先规则仍然挡住把 `~/Documents/downloads`
  当成 Claude Code 的缓存移走。每个配方的启动器（`route.launcher`，清单的最后一项）从不是 optional
  （`recipes::tests` 钉着），所以上面 `~/.local/bin` 那一半照旧。
- ~~**升级后的读取仍把「看不清」当成「不在了」**（2026-09-25，步骤 C 顺带发现）。B 的 `StandaloneAdapter::reconcile`
  （升级前后的读取）经 `look` 用 `route::probe`（`4156e23` 起它不再经 `inventory`，两者共用 `look`），权限错误、
  循环链接这类「看不清」一律成了 `Absent`：`claude update` 退出 0 之后如果恰好读不了 `~/.local/bin`，`run_operation`
  会报 `NeedsAttention(GoneAfterUpgrade)`——说升级后不见了，而事实是看不清。步骤 C 只把卸载之后的读取换成了
  `route::probe_strict`（裁定 27）。**修法的形状**：`look`（`inventory` 与 `reconcile` 都经它读）改用 `probe_strict`，
  把 `Err` 映成 `AdapterError`，升级前后的读取随之得到 `Err` → `Unconfirmed`（刷新时的 `inventory` 自 `4156e23` 起已经把
  看不见的启动器当作与 detect 所列不符而拒绝：这个来源计入「部分数据可能不是最新的」横幅，这一行不消失）；要连同 B 的
  「探测失败是『没装』，不是『没响应』」这条规则一起评审。~~ —— **已于 2026-10-02 做了**（`fafd457d`，分支 `r5/g2-standalone-scan`）：`look` 改经 `route::probe_strict`，权限错误、循环链接成了错误而不是 `Absent`——`reconcile` 返回 `Parse`（升级退出 0 后报「结果未确认」，不再报 `GoneAfterUpgrade`），`inventory` 以「cannot look at」拒绝（这一轮该来源计入「部分数据可能不是最新的」）；`detect` 仍用 `probe`，看不清照旧是「没装」不是「没响应」（测试 `test_inventory_and_reconcile_call_a_launcher_they_cannot_look_at_unknown_not_gone`、`test_a_claude_update_exiting_zero_with_a_launcher_banager_cannot_look_at_is_unconfirmed_not_gone`）。
- **grok 回退链接的链接文本未核实**（2026-09-26，步骤 D）。`~/.local/bin/grok`、`~/.local/bin/agent` 只在 `~/.grok/bin`
  不在 PATH 上时由安装器创建（grok.md §2：它依次试 `~/.local/bin` 与 `/usr/local/bin`，用第一个可写的），本机没有，
  链接文本指向 `~/.grok/bin/grok` 还是直接指向 `downloads/` 里的文件不知道。配方把这两条列为 optional 且**排在最前**
  （步骤 D 计划裁定 3）——这是预防，不是纠错：检查 4 对这两条（以及 `~/.grok/bin/agent`）用 `route::leads_to_program`
  （`Expect::SymlinkToProgram`，配方里的 `GROK_PROGRAM_LINK`）：链接自己的文本（`one_hop`，只把**已存在**的前缀解析掉）
  得落在 `~/.grok/downloads` 里，或正是 `~/.grok/bin/grok`、`~/.grok/bin/agent` 之一；能解析时，解析结果还得落在
  `~/.grok/downloads` 里，或正是启动器指向的那个文件。只「落在 `~/.grok` 里」不算：`~/.grok` 是这次卸载保留的文件夹，
  用户的插件、技能也在里面（步骤 D 整步评审）。两种文本在 `downloads/` 进废纸篓之后都仍被接受（悬空时只看文本）；
  排在最前，是让它们在文本可能经过的每个文件夹都还在时就走掉，检查 4 看到的是一条能解析的链接，不只凭文本；中途停下
  也不会留下一条看起来像别人的悬空 `~/.local/bin/grok`。若它不是 grok 的（另一个 CLI 的 `agent`，或用户自己指向
  `~/.grok` 里某个插件、技能的程序的链接），按 `NotOurs` 保留并说明。`/usr/local/bin` 里的同名路径
  只在**链接进 `~/.grok`、且移动做完后确实指向空处**时才报「会变成失效链接」（`removal::dead_after`：已经悬空，
  或通往目标的路上经过清单要移走的路径；Intel Mac 上它可能是 Homebrew `grok-build` 的活链接，步骤 D 计划裁定 6；
  指向保留的 `~/.grok` 里某个插件程序的链接卸载后仍能用，不报，步骤 D 整步评审）。**核实办法**：在 CI runner 上
  让 `~/.grok/bin` 不在 PATH 上装一次（安装器只在这时才建回退链接），再 `readlink` 两个候选位置——可以加进步骤 D
  计划「The author's pre-merge verification」的工作流。
- **grok 的 `~/.grok/bin` 不整目录移动**（2026-09-26，步骤 D 计划裁定 4，与 spec §6.3 的 `~/.grok/bin · Dir` 不同）。
  （决定记录，不是待办；2026-10-02 核对仍照此实现：`crates/banager-core/src/adapters/standalone/recipes.rs:296-301`。）
  安装器把它加进了 PATH，用户自己的脚本可能放在里面；清单列的是安装器放进去的两条链接（`agent`、最后 `grok`），文件夹本身
  留在被保留的 `~/.grok` 里（用户没往里放东西时是空的；安装器写进 shell 配置文件的 PATH 行照旧指向它，无害）。若日后要连
  文件夹一起移，形状是「文件夹里只剩清单上的条目才移」的检查，不是放宽启动器最后的不变量。
- **`grok update` / `claude update` 无交互时的行为待 CI 录制**（2026-09-26，步骤 D，spec §五；本仓库的 GitHub Actions 额度
  2026-10-01 恢复）。步骤 D 计划「The author's pre-merge verification」一节给了工作流、观察项与每种结果对应的配方改法；
  在录制到之前，`docs/what-we-run.md` 的 Grok Build 一节照实说还没观察过（「has not been observed by this project」）。
  **grok 的结果挡合并**（`GROK.upgrade` 是本步新加的）；**claude 的结果不挡本步合并**（B 已经交付了按钮，若会提示或挂起，
  是发布前要修的回归）——这与 spec §五「步骤 D 之前在 CI runner 上各录一次」不同，步骤 D 计划的偏差 13 记了。若 grok
  会提示或挂起，计划推荐的形状是新的 `UpdateBlocked::NeedsTerminal`（自己的 copy record：请用户自己在「终端」里跑
  `grok update`），不是 `SelfUpdatesOnly`（它那句「会在后台自己安装更新」用在 grok 上没有根据：grok 会不会自己装更新没有核实）；
  另一条路是保留按钮，让 `Failed` 引用 grok 的提示原文。claude 同理。
- **`grok --version` 是否触发启动时更新器、那个更新器会不会静默安装**（2026-09-26，步骤 D；grok.md §5 开放问题 2）。
  每次刷新跑两次 `grok --version`（detect、inventory 各一次）和一次 grok 自己的 `update --check --json`。1.0.41 的录制在
  `--version` 前后各看了一次 `~/.grok/bin` 与 `~/.grok/downloads` 的 `ls -lan`、`readlink ~/.grok/bin/grok` 和
  `version.json` 的 mtime：都没变，事后 `find ~/.grok -newer` 也说明这次读版本没在 `~/.grok` 里写任何东西（fixture
  README）。所以 1.0.41 的版本读取没留下碰到启动时路径的痕迹，那个更新器会不会装仍然不知道。以后每录一个
  `verified_versions` 都照做：链接或 `downloads/` 变了就停，由作者决定版本读取怎么改；只有 mtime 动了，照实写进 fixture
  README 与 `## Grok Build`。这条没关之前，grok 的 `self_updates` 不能改成 true。另：grok 自己的检查每次刷新都会在
  `~/.grok` 里写东西（替换 `version.json`、往 `logs/unified.jsonl` 加两行、刷新它自带的用户指南 27 个文件的修改时间），
  `docs/what-we-run.md` 已写明那是 grok 的写入，不是 Banager 的。

## 阶段 5（发现页）之前必须处理

- `brew/mod.rs` `search`：只要 `--desc` 搜索有结果就丢弃名字匹配，搜 "jq" 搜不到 jq（fixture 可复现：`search-jq.txt` 第 3 行是 jq，`search-desc-jq.txt` 无 `jq:` 行）；且无表头输出的"第一组是 formulae"启发式会把纯 cask 结果标成 Formula。改法：按 (kind, name) 合并；用 `brew search --formula {q}` 与 `brew search --cask {q}` 得到无歧义的类型，`--desc` 只用来补描述；同步更新 `docs/what-we-run.md`。
- 搜索词校验目前套用包名规则，含空格的查询（"json processor"）被拒；spec §4.1 要求独立的 query 规则。
- ~~阶段 2 的 Task 16 只做两项 i18n 自动检查：en/zh-CN 键集互比 + 组件里的 JSX 字面量扫描，**不**扫描代码里 `t("…")` 用到的键是否真的存在。若将来要补「静态校验 `t()` 键存在性」，需先把动态键改成静态查表：`Sidebar` 的 `t(\`nav.${p}\`)`、`OperationBar` 的 `operations.kind.${…}` / `operations.status.${…}`、`OperationBar`/`LogDrawer` 的 `operations.outcome.${…}`（改成 `PAGE_LABEL_KEYS` 一类的 `Record` 常量表）。动态键拼错在运行时立刻可见，不是隐蔽 bug，故阶段 2 未做。~~
  —— **2026-10-02 核对，大半已解决，改写为只剩的缺口**：`nav.*`、`operations.kind.*`、`operations.status.*` 已改成常量表
  （`PAGE_LABEL_KEYS`，`src/components/Sidebar.tsx:32`，`a98ba42`；`OP_KIND_KEYS`、`OP_STATUS_KEYS`，`src/lib/operations.ts:27`、`:48`，`e4b57a4`）；
  反方向「en.json 里有、却没人用的键」由 `src/i18n/completeness.test.ts:242-255` 查，它认得拼接键的固定开头（`51a3e18`、`32e0505`）。
  仍开着：`operations.outcome.${…}` 仍是拼出来的（`src/lib/operations.ts:147`），靠 `outcomeKey`（`src/lib/format.ts:20-40`）的
  `never` 穷尽检查保证每种结果都有句子；「`t()` 用到的键在 en.json 里确实存在」仍没有测试。
- ~~阶段 2 的更新页（`src/pages/UpdatesPage.tsx`，Task 12）直接遍历渲染，没接 `useVirtualizer`：只有 Homebrew 一个来源时更新通常只有几条到几十条，虚拟化收益极低而改动不小（每行的 `snapshot.artifacts.find` 已改为 `useMemo` 建 `Map` 的 O(1) 查找，真正的 O(n²) 已除）。接入更多来源后若更新列表可能变长，按已安装页（`InstalledPage`）的写法补 `useVirtualizer`。~~
  —— **已于 2026-09-22 解决**（`7a3ab59`）：更新页的列表是 `VirtualList`（`src/pages/UpdatesPage.tsx:1239`），它内部用 `useVirtualizer`（`src/components/VirtualList.tsx:302`），与已安装页同一个组件。

## Runner 打磨（任意时机）

- ~~`runner/real.rs`：最后的 `child.wait()` 未受剩余超时约束；末尾无换行的半行不会推给 `on_line`；kill 后不排空已缓冲的管道数据；每个字节被复制两次。~~
  —— **已于 2026-09-22 解决**（`3ccf239`、`6158bba`、`ba80106`，9-23 的 `2ee9244` 又改成先 SIGTERM）：`reap` 受 `POST_KILL_WAIT` 约束
  （`crates/banager-core/src/runner/real.rs:630-633`）；测试 `test_delivers_a_trailing_line_that_never_got_its_newline`（`:1670`）、
  `test_delivers_what_was_still_in_the_pipe_when_the_child_was_killed`（`:1712`）。
- ~~`RunnerError::NotFound` / `Spawn` 两条错误路径无测试。~~ —— **已于 2026-09-22 解决**（`c39dd13`）：`runner/real.rs:1819`、`:1849` 各一个测试。
- ~~`lib.rs` 加 `#[cfg(not(unix))] compile_error!("banager-core targets Unix (macOS) in v1")` 与 crate 文档说明；`path_env.rs`（`geteuid`、`HOME`）、`resolve_exe`（无 `.exe`）、`libc` 无条件依赖都隐含 Unix。~~
  —— **已于 2026-09-22 解决**（`e337df2`）：`crates/banager-core/src/lib.rs:44-45` 的 `compile_error!` 与其上的 crate 文档。
- ~~`crates/banager-core/Cargo.toml`：tokio 的 `rt-multi-thread`、`macros` 只有测试与示例用，应移到 `[dev-dependencies]`，使"core 不创建运行时"成为机械事实。~~
  —— **已于 2026-09-22 解决**（`08fe468`）：`rt-multi-thread` 只在 `[dev-dependencies]`（`crates/banager-core/Cargo.toml:121`）；`macros` 留在正式依赖
  （`:23`）是对的，生产代码用 `tokio::select!`（见本文开头 2026-09-24 那张表）。
- `brew/mod.rs` 的 `SUDO_ASKPASS` 透传与 `needs_password` 无关且子进程本就继承环境，实际只起预览作用（留不留待作者拍板；仍在
  `crates/banager-core/src/adapters/brew/mod.rs:1714`、`:1826`）。~~相关测试修改进程全局环境变量，未加串行化，将来可能抖动。~~
  —— 测试那半**已于 2026-09-22 解决**（`51c49ba`）：改用 fn 指针接缝 `with_askpass_fn`（`adapters/brew/mod.rs:375-376`），测试不再碰进程环境。

## 测试数据

- ~~`parse.rs` 的 `pinned` 分支无覆盖~~ —— **已于 2026-09-24 在分支 feat/per-package-actionability 解决**：`pinned` 现在被读成 `UpdateCandidate.blocked = Some(Pinned)`，有内联 JSON 单元测试，也有 `adapters/fixtures/brew/7.0.6/outdated-pinned.json`（由真实录制改了四个 pin 字段而来，README 写明，`brew_fixtures.rs` 有测试核对只差这四个值；2026-10-06 按 R7 移到 `adapters/fixtures-derived/brew/7.0.6/`）。
- ~~`adapters/fixtures/brew/7.0.3/uses-jq.txt` 为空；下次为新 brew 版本重录 fixtures 时，选一个有已装依赖者的 formula（如 `openssl@3`）录 `uses-<formula>.txt`，不得伪造。~~
  —— **已于 2026-09-22 解决**（`63aa31c`）：真机录了有依赖者的 `adapters/fixtures/brew/7.0.3/uses-pcre2.txt`（4 行），
  `crates/banager-core/tests/brew_fixtures.rs:188-203` 读它；`uses-jq.txt` 照旧是空的，作为「没有依赖者」的录制保留。

## 工作流与发布

- `.github/workflows/release.yml` `releaseDraft: true` 与 `tauri.conf.json` 的 `releases/latest/download/latest.json` 端点冲突：草稿永远不是 latest。终审建议：**先保留草稿**（签名/公证流水线尚未跑过、updater 公钥仍是占位符），在 `release.yml` 加注释并写 `docs/releasing.md` 说明"发布草稿是最后一步，发布后 updater 才能看到"；首个草稿经手工公证验证后再改 `releaseDraft: false`。待作者拍板。
- `.github/workflows/ci.yml`：`feat/**` 推送触发是本分支验证期的临时加项，PR 会让整套 macOS 作业（含真实 `brew install`）跑两遍；合并后去掉~~或加 `concurrency` 组与 `timeout-minutes`~~。
  （2026-10-02 核对：`concurrency` 组与 `timeout-minutes: 30` 已于 2026-09-22 在 `6a4005b` 加上，`.github/workflows/ci.yml:14-19`、`:26`；
  只剩去掉 `feat/**` 触发，`:12`，何时去掉由作者定。）
  —— **已于 2026-10-06 去掉**（作者决定 R3）：推送只触发 `main`，功能分支靠它的 PR 跑 CI（`.github/workflows/ci.yml:5-12`）。
- ~~`src-tauri/Cargo.toml`：`fix-path-env` 是无 `rev` 的 git 依赖，仅靠 Cargo.lock 钉住；应加 `rev`。~~ —— **已于 2026-09-22 解决**（`6b8a0af`）：`src-tauri/Cargo.toml:46` 带 `rev`。
- GitHub Actions 提示 checkout@v4 / setup-node@v4 / pnpm action-setup@v4 使用即将弃用的 Node 20 运行时；GitHub 定下时间表后升级。

- ~~`src-tauri/Cargo.toml` 的 `[profile.release]` 在 workspace 中被 Cargo 忽略（每次 cargo 命令都打印 "profiles for the non root package will be ignored"），意味着 create-tauri-app 给的 release 优化（lto、opt-level、strip 等）目前对发布构建**不生效**；应把该段移到根 `Cargo.toml`。首个正式发布前必须处理，否则体积目标失真。~~
  —— **已于 2026-09-22 解决**（`64da96b`）：`[profile.release]` 在根 `Cargo.toml:20-24`，`src-tauri/Cargo.toml:134-135` 只留一句说明。

## Codex 评审（2026-09-18）推迟项

Codex 独立评审发现 3 项 P1 + 9 项 P2，控制者逐条核实属实；其中 10 项已在本分支修复（见 `.superpowers/sdd/codex-fix-report.md`）。以下 4 项推迟：

- ~~**M3**：`adapters/brew/parse.rs` 两个根结构体对 `formulae`/`casks` 都用 `#[serde(default)]`，因此 `{}` 或只含未知字段的对象会被解析成「空集合」而非报错，把格式异常解释为「没装任何东西」或「全部最新」；`installed_on_request` 缺失时默认 false，把未知安装原因归类为依赖（应为 `Unknown`）。改法：对 JSON v2 要求必要顶层字段存在，区分合法空数组与字段缺失；补 `{}`、缺分区、字段类型错误、截断 JSON 的断言。~~
  —— **已于 2026-09-22 解决**（`34b8fea`）：两个根结构体（`adapters/brew/parse.rs:23-26`、`:435-438`）不再 `serde(default)`，缺分区的
  回答报错（测试 `parse_info_installed_refuses_a_reply_with_no_partitions`、`parse_outdated_refuses_a_reply_with_no_partitions`，`:872`、`:891`）；
  `installed_on_request` 缺失时归为 `InstallReason::Unknown`（`:342-346`）。
- ~~**M4**：`ops/mod.rs` 的 `cancel()` 从不读 `plan.cancel_policy`，执行路径也不按该字段分支，`NoCancel` 计划运行中仍会收到取消令牌。当前 BrewAdapter 只产生 `KillThenReconcile`，故暂不影响；后续适配器用到 `NoCancel` 前必须实现，并补策略矩阵测试。~~
  —— **已于 2026-09-24 解决**（`17d8ef7`、`99a9d6f`）：运行中的 `NoCancel` 操作拒绝取消（`ops/mod.rs:398`），排队中的仍可取消；
  测试 `tests/ops_cancel_test.rs:765`、`:802`、`:858`。
- ~~**N1**：`brew/mod.rs` 的 detect 单测虽用 MockRunner，仍查询真实文件系统并硬编码「恰好一个实例且为 /opt/homebrew」；Intel Mac、无 Homebrew、双 Homebrew 环境都会失败。改法：把候选路径与存在性检查抽成可注入依赖，分别测零/一/双实例，真实路径验证移入显式门控的集成测试。~~
  —— **已于 2026-09-22 解决**（`8569473`）：路径存在性经可注入的 `path_exists_fn`（`adapters/brew/mod.rs:120`、`:386-387`），
  零/一/双实例各有测试（`:2124-2194`）。
- ~~**N2**：`release.yml` 安装两个编译目标并产出 universal 包，但没有 spec §10 要求的 Intel runner 启动冒烟；交叉编译成功不等于 x86_64 半边能跑。发布验收前补 Intel 启动验证，或明确记为未完成的验收项。~~
  —— **已于 2026-10-06 加上**（作者决定 R6）：`release.yml` 的 `intel-launch` 作业在 `macos-15-intel`（真 Intel 机器）上从刚出的 .dmg 启动 x86_64 半边，
  30 秒内退出或日志里有 panic 就让这次运行失败。草稿 Release 一建好标题就写着"Intel 启动检查未完成，不要发布"，只有检查通过才改回 `Banager vX`；
  检查失败、超时、被取消时改成"未通过，不要发布"，检查根本没跑（例如上一步上传失败）时保持"未完成"。还没在 CI 上跑过（要等第一次打 `v*` 标签）；
  GitHub 提供的 Intel 镜像最老是 macOS 15，13.3–14 没有覆盖。

## 阶段 2 终审（2026-09-19）推迟项

Opus max 全分支终审：3 项必修（已修），其余推迟。按主题分组。

**规格与实现不一致（下一个计划开头就处理）**
- ~~更新确认对话框没有展示版本跳变（`current → target`）与 `UpdateCandidate.warnings` 的文字内容（现在只有一个数量徽章）。spec §6 两项都要求。位置 `src/pages/UpdatesPage.tsx:361-378`。
  **做这条时必须一并处理 Ollama**：`UpdateChannel::Digest` 的行不能按 `current → target` 渲染。阶段 3 任务 10 的评审查实（对着提交进仓的 fixture 逐字比对）：Ollama 的 `current` 是 `/api/tags` 的清单摘要、`target` 是注册表清单里的 config 摘要，**是两个不同的哈希空间**，互不包含，拉取成功后新的 `current` 也不会等于旧的 `target`。这两个字段只是「变了／没变」的标记，真正的判定在 `compare_digests` 对层摘要集合的比较上。给小白看两串 64 位十六进制本来也毫无意义——这类行应该说「有新版本可拉取」，而不是打印哈希。
  为什么不在阶段 3 改：修它的两条路都拿一种不一致换另一种。改 `current` 为本地 config 摘要，会和「已安装」页显示的 `artifact.version`（`/api/tags` 摘要）自相矛盾；改 `target` 为注册表的清单摘要才是真正对的，但那要读 `Docker-Content-Digest` 响应头，而 `HttpResponse` 只有 `status` 和 `body`，得改 trait、Mock、真实实现和全部测试。当前无人渲染这两个字段，`reconcile` 也不比较它们，所以没有实际故障，只有一个等着被踩的坑——坑口已经写在 `adapters/ollama/mod.rs` 的注释里（2026-09-20 控制者裁决）。~~
  —— **已于 2026-09-22 解决**（`60188b4`；Ollama 那半是 2026-09-20 的 `71bbacb`）：确认框写版本跳变（`versionJump`，
  `src/components/UpdateConfirm.tsx:401-405`），`Digest` 的行写「此模型有新版本」，不印摘要（`:402`）；计划的提醒逐条成句
  （`warningLines`，`:607`），没有数量徽章了；候选自带的 `warnings` 在行的说明里成句（`src/components/updateDetails.tsx:42-60`）。
- ~~`greedy_casks`：计划的任务表把它列为任务 15 的交付物、spec §4.2 与 §5 也定义了它，但计划里那份权威 `Settings` 结构体没有它，于是实现也没有。补它是一次跨 Rust、TypeScript 与磁盘 JSON 的线格式变更，越晚越贵。**需要作者拍板**。~~
  —— **已于 2026-09-20 解决**（`9f9a43f`、`6d0e38c`）：设置里的「包含自更新的应用」，`Settings.include_self_updating`（`settings.rs:82-89`），
  见上文「阶段 3（其余来源）」第一条。
- ~~更新列表未虚拟化（`src/pages/UpdatesPage.tsx:283-315`），而已安装列表用了 `useVirtualizer`。Global Constraints 与 spec §7 都写了长列表要虚拟化。~~
  —— **已于 2026-09-22 解决**（`7a3ab59`）：见上文「阶段 5（发现页）之前」同名一条，更新页用 `VirtualList`（`src/pages/UpdatesPage.tsx:1239`）。

**资源增长（接入更多来源前处理）**
- ~~`crates/banager-core/src/session/mod.rs:138` 的 `issued_plans` 只在成功提交时清理，被放弃的预览（关掉对话框、被取代的批次、StrictMode 双次签发）会泄漏到进程结束。插入时顺带清掉超过 600 秒的条目。~~
  —— **已于 2026-09-20 解决**（`93871e5`）：签发新计划时清掉超过 `PLAN_LIFETIME`（600 秒，`session/plans.rs:18`）的条目（`:174-176`）。
- ~~`crates/banager-core/src/ops/mod.rs:154` 的 `records` 只增不减，于是 `summaries()` 无限增长，底部操作条在一次会话里做完第一个操作后就再也回不到空闲态。给历史加个上限（比如最新 100 条）。~~
  —— **已于 2026-09-20 解决**（`93871e5`）：上限 `DEFAULT_MAX_RECORDS = 200`（`ops/mod.rs:20`）。

**并发与一致性打磨**
- ~~`src/lib/queries.ts:33-41` 的 `useRefresh` 绕过了 `src/lib/events.ts` 里的模块级合并器，手动重试可能与事件驱动的刷新赛跑。改为走 `refreshIntoCache`。~~
  —— **已于 2026-09-20 解决**（`93871e5`）：`useRefresh` 走 `refreshIntoCache`（`src/lib/queries.ts:121-133`）。
- ~~`src-tauri/src/ipc.rs:27-32`：两个并发的 refresh 都在完成前读了 `generation_before`，一次真实变化可能广播两次 `SnapshotChanged`（幂等，但注释声称的不变量比实际强）。~~
  —— **已于 2026-09-22 解决**（`93871e5`、`67d86b6`）：一代快照只由认领到它的那次调用广播（`claim_broadcast`，`src-tauri/src/ipc.rs:201`；
  `announce`，`:137-159`）；每日自动检查那一轮另行照常通知。
- ~~`crates/banager-core/src/session/mod.rs:216-218`：`RefusedAsRoot` 分支清空了 artifacts 与 updates，而逐实例失败路径是保留旧数据并标记陈旧。实际不可达（进程内 euid 不变），但与既定规则不一致。~~
  —— **已不适用**（2026-09-22 起）：`RefusedAsRoot` 整体删除，root 下由 brew 的 `detect` 给实例标 `RefusesAsRoot`（见上文「二～四」
  第三条与「阶段 2 之前」第五条）；代码里只剩两处说明历史的注释（`session/mod.rs:65`、`session/refresh.rs:201`）。
- ~~`src/components/UninstallDialog.tsx:56` 缺同步的重入闩，两次极快的点击都会进入；服务端一次性 PlanId 挡住了重复卸载，但失败那次会重新签发计划。另外 `:123` 的提交错误文字会停留在新预览旁边，读起来像「还是坏的」——在 `onError` 重新签发时顺手 `submitMutation.reset()`。~~
  —— **已于 2026-09-24 解决**（`34e80fb`）：同步的 `submitLatch`（`src/components/UninstallDialog.tsx:127`），重新签发的预览回来时
  `submitMutation.reset()`（`:275-278`）。

**测试与工具链**
- ~~`src/pages/UpdatesPage.tsx` 的 14 个测试里 `snapshot.artifacts` 全是空数组，所以 `artifactsById` 从来没命中过，非技术细节视图的描述路径从未被真正执行。补一个带 artifact 的夹具。全部规划失败那条页面错误分支也没有任何测试。~~
  —— **已于 2026-09-22 解决**（`7940893`、`a8f50b3`）：多处测试带 artifact 夹具（如 `src/pages/UpdatesPage.test.tsx:531`、`:925`、`:3275`），
  全部规划被拒那条页面错误分支有测试（`:4720`）。
- ~~`tsconfig.json:16` 的 `"types": ["node"]` 把 Node 全局类型套给了整个 `src/`，而这是个 WebView 应用。改用 `tsconfig.test.json` 把这个让步限制在测试里；同时复查 `vite.config.ts:5` 那个在 `tsc -b` 下已过时的 `@ts-expect-error`。~~
  —— **已于 2026-09-24 解决**（`a6fe7e7`）：`tsconfig.json:19` 为 `"types": []`，Node 类型只给测试（`tsconfig.test.json:10`）。
  （`vite.config.ts` 那半**已于 2026-10-02 做了**：删掉那行，`pnpm typecheck` 加跑 `tsc -p tsconfig.node.json --noEmit --composite false`
  （`package.json:15`），以后再过时会报错。）
- ~~`src/i18n/no-literal-strings.test.ts:8` 只扫 `components` 与 `pages`，漏了 `App.tsx`、`lib/` 与 `store/`；正则要求至少 4 个字符，"OK"、"Done" 这类短文案会溜过去。~~
  —— **已于 2026-09-24 解决**（`1d27015`、`82e77ae`）：从 `src` 根扫起（`src/i18n/no-literal-strings.test.ts:7-11`），两个字母就算
  （`:24-32`）。
- ~~`crates/banager-core/src/session/mod.rs` 已 1152 行，阶段 3 值得拆分。~~
  —— **已于 2026-09-20 解决**（`af6057c`）：拆成 `session/` 下的 `mod.rs`（现 870 行）、`refresh.rs`、`plans.rs` 等九个文件。

**界面打磨**
- ~~`src/pages/SettingsPage.tsx:98-108` 的 `role="radio"` 按钮没有 roving tabindex 也没有方向键处理，键盘用户只能逐个 Tab；这些按钮与 `EmptyState` 的操作按钮都完全没有样式类。~~
  —— **已于 2026-09-22 解决**（`f35623b`），之后的界面重构又改了样子：设置页的选项现在是 `PopupButton` 与 `Switch`
  （`src/pages/SettingsPage.tsx:384`、`:405`），没有 `role="radio"` 按钮了；`EmptyState` 的按钮用 `BUTTON.regular.grey`
  （`src/components/EmptyState.tsx:92`）。
- ~~`src/components/LogDrawer.tsx:36-39` 是 `role="dialog"` 却没有焦点陷阱、也不能按 Esc 关闭。~~
  —— **已于 2026-09-22 解决**（`dc8683d`）：日志窗口用共用的 `Dialog`（`src/components/LogDrawer.tsx:180`），Tab 留在里面、Esc 关闭
  （`:90-91`，`src/components/ui/Dialog.tsx:184`）。
- ~~spec §7 的 8pt 网格：`px-3`、`py-1`、`gap-3` 等多处不在网格上。~~
  —— **已不适用**：2026-09-29 的审美规格另定了尺寸（行高 52、内容边距 20、按钮高 20/24/28），`docs/superpowers/2026-09-29-aesthetics-spec.md:13`、`:34-35`。
- ~~`src/store/ui.ts` 的 `showDependencies` 是一个全局开关，而列表项带着 `instanceId`；两个 brew 前缀（spec §4.2 提到的 Intel 迁移场景）下两组会一起展开收起。~~
  —— **已于 2026-09-22 解决**（`5c2c653`）：按实例记 `expandedDependencies`（`src/store/ui.ts:91`、`:254-259`）。

**杂项**
- ~~提交 `b4d6722` 的署名是 `Claude Sonnet 5`，28 个提交里唯一一个不一致。改它要重写 27 个后代提交，建议明确接受现状而不是返工。~~
  —— **已不适用**（2026-10-02 核对）：`b4d6722` 早已在 `main` 里（`git merge-base --is-ancestor b4d6722 main` 为真），接受现状，不再改。
- ~~`clearSelectedUpdates` 与 `clearLogs` 在计划的接口里、有测试，但生产代码从不调用。~~
  —— **已于 2026-09-24 解决**（`8739b5a`、`b67f5f6`）：两者已删，`src` 里没有了。
- ~~`src/pages/UpdatesPage.tsx:264` 的 `item.planError ?? ""` 按构造是死代码。~~
  —— **已于 2026-09-24 解决**（`338f704`）：改为类型收窄，`src/pages/UpdatesPage.tsx` 里没有 `planError ??` 了。
- ~~`src/components/UninstallDialog.tsx:36` 带着一个 eslint 抑制注释，而本仓库并未配置 eslint。~~
  —— **已于 2026-09-24 解决**（`b1c7818`）：`src` 里没有 `eslint-disable` 了。
- ~~`crates/banager-core/src/settings.rs:57` 用了 `Ordering::SeqCst`，`Relaxed` 就够。~~
  —— **已于 2026-09-24 解决**（`989a495`）：`settings.rs:227` 用 `Relaxed`。

## 界面重构终审推迟项（2026-09-28 立，分支 feat/ui-redesign）

来源：GPT-6 Astra 的小白视角复审（`~/dev/Banager/.superpowers/phase4/astra/review-ui.md`）与 Claude 的行为/文案复审。
下面几条是核实过、但这一轮没做的：

- ~~**npm 等卸载确认没说删什么、留什么**（Astra 2）。npm 的计划没有任何 warnings，确认框只有名字、版本和折叠的命令。
  要加"卸载范围"一行，必须先查清每个来源的卸载命令到底碰不碰用户的设置文件，没查清的不许写"会保留"。~~
  —— **已于 2026-09-28 解决**（`149bcd4`）：卸载确认在工具下写一句卸载范围（`Warning::UninstallScope`，`crates/banager-core/src/model.rs:937`），
  npm、pipx、uv、cargo、Ollama、Homebrew 各有自己的句子（i18n `warnings.uninstallScope.*`）；npm 只对 7 及以上写，因为 npm 6 会运行包自己的
  卸载脚本，读不出版本时不写（`crates/banager-core/src/adapters/npm.rs:77-93`）。
- ~~**不能更新的原因藏得深**（Astra 7）：断网原因默认折叠；解除固定的命令要打开「显示技术细节」才能复制；
  Antigravity CLI 的"打开它一次"没说怎么打开。~~
  —— **已于 2026-10-02 做了**：「无法检查」的原话认得出原因（`failureCause`：没网、磁盘满…）时，行的说明和
  （各行原因相同时）上方那句都直接说出来，技术细节关着也说；Antigravity CLI 在终端里输入 `agy` 跑的就是这一份时，
  句子写「在终端里输入agy打开它一次」（用命令名不用路径），否则照旧。解除固定的命令早已在句子里。
- ~~**来源不明页只列文件、不帮辨认**（Astra 8）：缺「在访达中显示」和复制路径（需要新的 IPC）；Docker 这类已知归属
  藏在「链接」标签的 ⓘ 里。另可考虑把入口改叫「未识别的工具」，免得用户以为是危险软件清单。~~
  —— **已于 2026-09-28 至 10-02 解决**：每行 ⋯ 菜单有「在访达中显示」「拷贝路径」（`d2af08c`；10-02 的 `e4f3690` 改成经 Banager 自己的
  命令、只显示扫描找到的，`src-tauri/src/reveal.rs`，`docs/what-we-run.md:2211-2215`）；页名改为「其他程序」并排到侧栏「来源」末行
  （`3f51cbe`，i18n `nav.unknown`）；归属与所在 App 直接写在行上，不只在 ⓘ 里（`bdf192d`，`src/pages/UnknownPage.tsx:207`、`:327`）。
- ~~**中文界面里的英文简介**（Astra 10）：Homebrew 的 formula 简介是上游英文（git、jq 等），需要一份常用工具的中文用途表。~~
  —— **已于 2026-09-28 解决**（`d45a201`，之后又扩充）：`src/assets/tool-descriptions/zh-CN.json`（现有 3,390 条，从各来源自己的描述译来），
  读取见 `src/lib/toolDescriptions.ts:1-16`。
- ~~**失败行挤掉名字**：一行同时有「会自动更新」标签和「更新失败 / 查看日志」时，800px 窗口下名字只剩几个字母（Claude Code）。
  结果出现时可以收起次要标签。~~
  —— **代码上已于 2026-09-29 解决**（`1031c5ae`，审美规格 R9）：一行只有一个状态词，操作结果出现时占用状态词那一栏
  （`src/pages/UpdatesPage.tsx:1047-1050`）；窗口变窄时先截描述，名字和状态词不截（`src/components/rowFit.ts:9-31`）。
  仍待：在 800 px 宽的真窗口里看一眼 Claude Code 那一行（只能在真窗口里核对）。
- ~~**全部取消遇到不能取消的 rustup**：按钮仍叫「全部取消」，rustup 那一项会继续跑完；操作条会接着显示它，但按钮文字与结果不完全一致。~~
  —— **已于 2026-09-28 解决**（`21be318`）：有一项运行中取消不了时按钮写「取消其余」（`src/components/OperationBar.tsx:131-139`、`:196`），
  那一项在操作条上点名。
- ~~**概览「2 个已隐藏」没有去处**：更新页不列出隐藏项，可以让这句话带用户去设置里的「已隐藏的更新」。~~
  —— **已于 2026-09-28 解决**（`3825af2`）：这句是链接，打开设置里的已隐藏更新（`src/pages/OverviewPage.tsx:84-88`）。
- **图标服务器（Icon server）**（2026-09-28，分支 feat/tool-logos）。现在的标志全部内置：`pnpm icons:build` 按
  `scripts/tool-icons/mapping.json` 生成 `src/assets/tool-icons/`，随应用一起打包，显示时不发网络请求；整个文件夹以
  5 MB 为限（`scripts/tool-icons/build.mjs` 的 `BUDGET_BYTES`，`src/lib/toolIcons.test.ts` 也卡着）。以后可以在服务器上
  以静态文件提供一个更大的包：应用整包下载，在本地用 `toolIconKey` 匹配，服务器因此不知道这台 Mac 装了什么。以
  `pack.json` 的 `version` 区分版本——现在 `icons:build` 固定写 1，应用还不读它。做的时候要动的：应用现在两条路都到不了
  一台 https 服务器（Rust 的 `RealHttpClient` 拒绝 `ALLOWED_HTTPS_HOSTS` 以外的 https 主机，窗口的内容安全策略是
  `connect-src 'self'`），主机加进名单后 `what_we_run_test` 要求 `docs/what-we-run.md` 写上它；下载的包若存到磁盘上，
  那份文件的「Files Banager writes」一节（现在只有 `settings.json`）也要改。

## 需要作者本人操作的事项（阶段 0–1 遗留）

- 任务 3：创建 Developer ID Application 证书并导出 .p12、生成 App 专用密码、查 Team ID、`pnpm tauri signer generate -w ~/.tauri/banager.key` 并把公钥填入 `tauri.conf.json`（替换 `REPLACE_WITH_UPDATER_PUBKEY`）、逐个 `gh secret set`；然后打 `v0.0.1` 标签验证公证。
- 任务 1：在 Terminal.app 里跑一次 `pnpm tauri build` 确认 .dmg 打包（自动化会话里 Finder AppleEvent 超时 -1712，属 TCC 自动化权限问题）。
- 任务 6：`pnpm tauri dev` 目视确认窗口打开且日志里 `[banager] discovered PATH dirs` 含 `/opt/homebrew/bin`。
- 任务 13：按 `docs/spikes/2026-09-askpass.md` 亲自跑 `sudo -A` 对话框试验并填结果表。

## 卸载说明的残留边角（2026-09-29 立，分支 feat/ui-round-2）

三轮对抗式核对后仍剩的少见情况，都不会把"会删"说成"不删"，只是说得不够全（证据见
`~/dev/Banager/.superpowers/round2/uninstall-scope.md` 与各轮 review）：
- ~~第三方 tap 的 cask 装好后 tap 被取消信任：Homebrew 只按记录卸载放置的文件，不执行记下的卸载步骤
  （`cask/installer.rb:999-1031`），确认框却说"并执行它记下的卸载步骤"。~~ —— **已于 2026-10-02 做了**
  （`UninstallScope::HomebrewCaskStepsIfTrusted` / `HomebrewCaskStepsOnlyIfTrusted`，`adapters/brew/trust.rs`）：
  Homebrew 7.0.7 起默认要求信任，只有存成 `.rb` 的记录才会因为不信任而整个不加载（`installer.rb:1010-1043`）；
  这种 cask 来自非 Homebrew 自己的 tap、信任列表里既没写它也没写它的 tap（或读不了列表）时，句子改说「它的卸载步骤，
  只在Homebrew信任它的来源时才执行」。设了 `HOMEBREW_NO_REQUIRE_TAP_TRUST` 时照旧。
- ~~旧 `.rb` caskfile 读不出来时 Homebrew 改用当前定义（`installer.rb:1040-1042`），执行的是今天的卸载步骤，
  不是记录里的。~~ —— **已于 2026-10-02 做了**（`HomebrewCaskRuby` / `HomebrewCaskStepsOnlyRuby`）：7.0.7 读不出时
  先按收据重建，收据或现在的定义有 Ruby 块时才改用现在的定义（`installer.rb:1046-1055`、`cask_loader.rb:879-920`）；
  记录是 `.rb` 的 cask 改说「并执行它的卸载步骤；其中部分步骤还会删除什么，无法事先得知。Homebrew读不出安装时记下的
  步骤时，会按它现在的定义执行。」，不再说什么保留不动；批量卸载把它留给单个卸载。记录里没有步骤的 `.rb`
  （`HomebrewCaskPlainRuby`）只说删除放置的文件、读不出时按现在的定义执行，不说执行卸载步骤。
- ~~`HomebrewCaskPlain` 仍写"删除 Homebrew 为 X 装的文件"：目前 7,763 个官方 cask 里没有带 pkg/installer 又被判为
  plain 的，第三方 tap 可能有。~~ —— **已于 2026-10-02 做了**（`HomebrewCaskPlainThirdParty`）：来自非 Homebrew 自己
  tap 的 plain cask 改说「删除Homebrew为“X”放置的文件；它的设置和数据保留不动，它的安装器如果另外装了文件，也不删除。」
  官方 cask 的句子不变。本机的 claudebar、codexbar 等第三方 cask 卸载时会看到新句子。
- ~~`HOMEBREW_NO_CLEANUP_FORMULAE` 点名的软件不会被安装后清理，`brew_env.rs` 没读这个变量，提示仍说会清理。~~
  —— **已于 2026-10-02 做了**（`Warning::HomebrewNoCleanupFormulae`）：brew.env 重放现在也读这个变量（按 Homebrew
  的逗号切法），清理或自动删除那几句后面加一句「HOMEBREW_NO_CLEANUP_FORMULAE列出的…除外：…」。
- ~~`brew uninstall` 还会删掉该 cask 自己在 `~/.homebrew/trust.json` 里的信任条目（`cmd/uninstall.rb:122-127`）。~~
  —— **已于 2026-10-02 做了**（`Warning::HomebrewForgetsTrust`）：卸载预览只读 `trust.json`（在用户的 Homebrew 配置
  目录，已写进 what-we-run 的「Files Banager reads」），列表里单独写了这个包、又肯定没写它的 tap 时，说「Homebrew还会
  从它的信任列表中删除“X”」；读不了列表或 tap 可能按网址写着时不说。

## 第二轮推迟项（2026-09-29 立，分支 feat/ui-round-2）

- **菜单栏图标、登录时启动**：按 Astra 对后台检查规格的建议推迟（`~/dev/Banager/.superpowers/round2/astra-bg.md`）。
  菜单栏图标要先定：数量为 0 / 检查中 / 检查失败各显示什么，「立即检查」要不要弹窗，退出时有操作在跑怎么办。
  登录启动用 SMAppService（macOS 13+），读系统的真实授权状态，登录启动时不弹窗口。
- **点通知真正打开「更新」页**：现在靠"通知在等 + Banager 被带到前台 + 窗口隐藏"来推断（`src-tauri/src/window.rs`），
  窗口只是被别的 App 挡住时点通知不会切页，⌘Tab 也会误触发。要用 UNUserNotificationCenter 的点击回调才能分清，
  它需要打包后的 app（开发版没有 bundle），见 Astra 第二轮复审第 4 条。
- **英文描述**：npm/PyPI/cargo 以外的来源在英文界面用的是来源自己的描述（Homebrew 的 desc），风格与中文表不完全一致。
- **长列表**（注意事项，不是待办；2026-10-02 核对仍成立）：`VirtualList` 复用已画好的行，前提是页面每次都传新的内联 `renderItem`；以后若改成 `useCallback`，
  要同时把行依赖的数据放进 key，否则会显示旧内容（`src/components/VirtualList.tsx:309-316` 的 `drawn` 只在 `items` 或 `renderItem` 换了时清空）。
- **Astra 第二轮复审**：`~/dev/Banager/.superpowers/round2/astra-round2.md`，第 1、2、3、5 条已修，第 4 条见上。

## 整体小白走查（2026-09-29 中午）

走查确认、今天没改的几条。【大改动】要动较多代码；【待作者定】要作者先拍板；【可先做】前半是小改动，后半要等打包后的
app 或更多工作。
- ~~【大改动】**别的来源要靠它运行的程序也给「卸载」**（node、npm 本身、pipx、ollama、python）：在 Rust 的卸载预览里
  判断这个包是不是列表里另一个来源运行所靠的——解析每个来源的 `exe_path`（npm → node，pip → python，pipx / uv 环境背后的
  解释器，ollama），看它是否落在这个 formula 的 keg / opt 路径下；是就把那个来源和它的工具数列在「这些软件还要用它」下
  （如「npm 和它的 4 个工具」），「卸载」保持不可点，与 Homebrew 的依赖者一样。npm 适配器拒绝卸载 `npm` 自己，行上用
  标签代替按钮。~~ —— **已于 2026-10-02 做了**（`crates/banager-core/src/needed_by.rs`、
  `crates/banager-core/src/session/needed_by.rs`、`src/lib/neededBy.ts`；分支 `r5/b1-runtime-guard`）。预览只顺着链接
  读取（`protected::resolve`），不运行命令，最多 2,000 条路径、1 秒；列在「依赖此工具的软件」下，写作「npm及其4个工具」
  「pipx装的2个工具」，下面一句写明先卸载哪些工具；`Session::submit` 也拒绝这样的预览（`UninstallBlocked::NeededBySource`），
  批量卸载把它放进「不会卸载」，用同样的话。与上面的设想有三处不同：(1) cask 也查（它的 Caskroom 文件夹和 App：Ollama.app
  的 `ollama`）；(2) 只在那个来源还有自己的工具时才拦——不算 npm 的 `npm`、`corepack`，Homebrew Python 自带的 `pip`、
  `setuptools`、`wheel`，以及 pip 的依赖；只剩 npm 自己时 node 可以卸载；(3) 原来按命令名猜的提醒（`hostedLines`）删掉了，
  批量的 X5 规则（按命令名）保留，排在新的规则之后。
- 【待作者定】**每次更新都留下旧版本，之后的卸载会"又回来"**：`HOMEBREW_NO_INSTALL_CLEANUP=1`
  （`crates/banager-core/src/adapters/brew/mod.rs:225`）让更新不删旧版本，不带 `--force` 的 `brew uninstall` 只删当前
  那一版（Homebrew `cmd/uninstall.rb:45`、`uninstall.rb:63-69`），剩下的旧版本又出现在已安装里。定一条规则：(a) 更新时
  删掉它所更新的那个包的旧版本，并在更新确认框里说（「会删除旧版本 1.25.0」），autoremove 与定期全面清理仍然关着；
  (b) 保留旧版本，但「卸载」删掉所有已装版本，确认框逐个列出（「删除 wget 的 2 个版本：1.25.0、1.26.0」），另加
  「清理旧版本」并显示能腾出多少空间。无论哪条，Banager 自己造成的状态都不能落到「显示卸载了，但它还在」。
- ~~【可先做】**更新失败只给英文原始报错、没有下一步，操作条还把失败叫「需要留意」**（`operations.batch.needsAttention`）：
  先做——有更新失败时操作条说「N 个更新失败，M 个已成功」，「需要留意」只留给 NeedsAttention 与 Unconfirmed；每个
  Failed 结果在工具原话下面加一句固定的中文下一步，如：上面是 wget 自己的报错。可以稍后点「重试」；还是失败，就点
  「拷贝日志」发给懂的人看。日志抽屉底部加「拷贝日志」。以后——认出常见原因（没网或 DNS、要管理员密码、磁盘满、
  Homebrew 被锁），各用一句中文说。~~ —— **已于 2026-10-02 做了**（`src/lib/runResult.ts`、
  `src/components/FailureNextStep.tsx`）：操作条有失败时说「2个更新失败，3个已成功」，另有需要查看的、已取消的接在
  后面；全是卸载时与结果块的标题一字不差（「已卸载1个，2个没有卸载」，已取消的也算没有卸载）；「需要查看」只留给
  没有失败的一批。日志里有工具原话的失败，日志下面加一句「上面是Homebrew自己的报错。……」：认不出原因时说怎样再试
  （更新点按「重试」、卸载/安装重新做一次）再说「拷贝日志」；认得出原因时，日志上方已有那条原因的下一步，下面只说
  「照上面说的做了还是失败，就点按“拷贝日志”……」；要 Mac 密码的，指向日志上方那条在终端里运行的命令（没有命令的
  来源只说「拷贝日志」），不提那一行没有的「重试」。点名的是来源（Homebrew、npm），不是工具本身，因为报错是来源写的。
  「拷贝日志」与常见原因此前几轮已经有了。**没覆盖到的**：只在日志里加了这句；打开「显示技术细节」后，操作条单个
  操作那一行、日志标题下面、批量卸载结果块各行也会显示工具原话，那几处下面没有这句（各有「查看日志」通向日志）。
  ~~批量卸载结果块各行与日志标题下面~~ —— **已于 2026-10-02 做了**（`src/components/FailureNextStep.tsx` 的
  `ResultRowStep`、`SubtitleStep`，键 `failure.toolWords.*`）：开着技术细节时，结果块里显示工具原话的那一行下面加一句
  「上面是Homebrew自己的报错。……」，认不出原因的说怎样再试、再点「查看日志」「拷贝日志」，认得出原因的接那条原因的
  做法（技术细节关着时那一行本来就说这句），要密码的指向日志里那条终端命令；日志窗口只在这个窗口的日志里已经没有工具
  那几行（日志只留最近 2000 行）、标题下只剩原话时，才在标题下说一句，不说两遍；macOS 不肯移到废纸篓的那种失败
  （没有退出码）不算来源的原话，不加。操作条单行没地方，没加。
- 【待作者定】**管理员密码的承诺兑现不了：要 sudo 的 App 直接失败**（`commandPreview.needsPassword`「部分 App 在这一步
  会要求输入 Mac 密码。」）：先做完搁着的 askpass 试验（`docs/spikes/2026-09-askpass.md`，上文「需要作者本人操作的
  事项」任务 13），再二选一：(1) 带一个 askpass 助手，让 macOS 真的弹出密码框；(2) 说实话：只在 cask 记录里有 pkg 或
  sudo 步骤时才显示提示，大意是「这个 App 要管理员密码，Banager 不能代你输入；到时这一项会失败，并给出在终端运行的
  命令」（原则 3 不写"可能"）。失败确实来自 sudo 时，用中文说明，并给出确切命令和「拷贝」按钮。
- ~~【大改动】**更新进行中退出没有任何提醒，确认框却叫人别退出**（`operations.noCancelHint`）：有操作在排队或运行时
  拦下退出（`RunEvent::ExitRequested` → `prevent_exit`；现在 `src-tauri/src/window.rs:164` 的 `on_run_event` 只处理
  Reopen），菜单栏的「退出」也经过页面。问「还有 3 个更新没完成，现在退出会中断它们。」，两个按钮：「完成后退出」
  （默认，最后一个结束时退出）和「仍然退出」（先取消排队中的，并点名取消不了的）。~~ —— **已于 2026-09-29 做了**
  （`src-tauri/src/quit.rs`、`src/components/QuitQuestion.tsx`），与上面的设想有三处不同：
  (1) 拦不在 `RunEvent::ExitRequested`：⌘Q、程序坞的「退出」、退出登录都走 AppKit 的 `terminate:`，tao 的 delegate
  没有 `applicationShouldTerminate:`，tauri 只在已经退出时发 `RunEvent::Exit`；改为启动时给 delegate 的类补上这个方法。
  (2) 按钮按任务定为「继续等待」（默认、Esc）和「仍然退出」；「仍然退出」先取消能取消的再退出（见下面第二次复审）；
  取消不了的（rustup）单独一行点名。「完成后退出」没做。
  (3) 退出登录、重启、关机时当场回答「不退出」，系统的这次操作被取消，选「仍然退出」后要再操作一次；
  `NSTerminateLater` 能让系统等用户回答，但回答要从网页经 IPC 回来，测试跑不到，暂不用。
  复审后补了一条：没人能回答时不拦。页面卸下（渲染出错被 React 整页卸掉）时告诉 Rust 不再问（`ask_before_quit(false)`）；
  问了之后页面 2 秒内没说问话已显示（`quit_question_shown`；重新载入、网页进程崩了都会这样），Rust 就照「仍然退出」直接退出。
  第二次复审（`.superpowers/round2/astra-quit.md`）后又补了三条：(a)「仍然退出」和 2 秒兜底退出前，先像操作栏
  「全部取消」那样取消能取消的（排队中的、运行中且可取消的），等它们的命令停下，最多 7 秒，再退出（`quit_now`），
  等待期间再退出（⌘Q、程序坞、退出登录）会被拦下，不打断等待；
  取消不了的 rustup 不停，Banager 退出后它的命令照常运行，输出管道没人读（`docs/what-we-run.md`）；问话正文只说退出会
  中断的那些。(b) delegate 的类（含父类）已经回答 `applicationShouldTerminate:` 时不补，守卫保持关闭并记一行日志；
  `src-tauri/tests/tao_delegate.rs` 在主线程建 tao 的事件循环，检查 tao 的 delegate 没有这个方法，tao 升级加了就失败。
  (c)「继续等待」、Esc、全部完成后自动关闭时告诉 Rust（`quit_kept_waiting`），这个问话的 2 秒兜底不再退出；问话还没
  确认显示时再按 ⌘Q，用同一个编号，不另起计时；「已显示」和「继续等待」这两个回执失败时各重发一次。
- 【待作者定】**2 秒兜底仍会在页面慢或回执两次都失败时替用户退出**（`quit_unless_shown`）：更完整的做法是兜底时不直接
  退出，改弹原生 NSAlert（「还有 N 个操作没完成」，「继续等待」/「仍然退出」），页面死了也能问、也能回答。要在主线程
  另起一个模态框，和 AppKit 的退出流程交织，测试跑不到，这次没做。
- 【待作者定】**概览几乎到不了明确的「都好了」，还怪到不点名的「来源」头上**：按 Banager 能动手的来判断圆环。没有可更新
  的、每个来源都回答了，就显示绿勾和「能在这里更新的都已是最新」，其余放进一行安静的小字（「1 个已隐藏、1 个只能
  查看」）。有来源没回答时点它的名，不说「来源」：「uv 这次没检查，其余都是最新的」或「Ollama 没有运行，没检查」。
  更新页的 `updates.noneCheckable`（与 `overview.nothingToUpdateChecked` 同为「已检查的来源里没有可更新的工具」）用
  同样的说法。
- ~~【可先做】**每天自动检查和通知会悄悄停掉**（`settings.autoCheck.description`）：先做——说明改成「关掉窗口也会每天
  检查；退出 Banager 或重启 Mac 后不再检查，直到你再打开它。」，开关下显示「上次自动检查：今天 9:12」（或「还没自动
  检查过」）。~~ —— **已于 2026-10-02 做了说明那半**（`0f318f98`）：现在写「Banager按所选频率检查更新，关掉窗口也会
  检查，查到的更新不会自动安装。退出Banager或重启Mac后不再检查，直到再次打开它。」（英文同义）。开关下已有 t20 的
  「下次自动检查：明天9:12左右」，没再加「上次自动检查」。仍待做：以后——在打包后的 app 里读真实的通知权限
  （UNUserNotificationCenter 的设置），关着时就显示 `settings.notifyUpdates.refused` 那行「在系统设置 → 通知里允许 Banager」。
- ~~【大改动】**第一次检查要等每个来源都答完才显示任何东西**（今天只补了说明：三页同一个圆环和「第一次检查要联网查每个
  工具的新版本，有时要一两分钟。」）：清单一读到就先提交一版快照，让「已安装」马上有内容，更新随各来源回答陆续补上。
  现在一轮只在末尾提交一次（`crates/banager-core/src/session/refresh.rs:681`），Homebrew 一轮最多等 `brew update`
  120 秒（`adapters/brew/mod.rs:233`）。上文的本地快照缓存只帮得到以后的启动，帮不到第一次。~~
  —— **已于 2026-10-01 解决**（`35025d0`，第四轮 t5）：第一次检查（或之前一轮都没提交过的检查）在各来源都列完清单、更新检查
  还在跑时，先把清单发给窗口（`InventoryPreview`，`crates/banager-core/src/session/refresh.rs:128-140`、`:660-692`，`session/mod.rs:183`），
  已安装页先列出来、更新和卸载按钮先关着，直到整轮提交（`src/lib/inventoryPreview.ts:1-16`）。与设想的两处不同：等所有来源都列完
  才发一次，不是逐个来源；更新不陆续补上，仍在整轮提交时一起出现（更新页、概览、程序坞角标只读提交后的快照）。
- ~~【可先做】**搜索把选中的工具藏掉时，详情仍开着**（t21 复审第 4 条）：「显示」藏掉选中工具时会关详情、安放焦点
  （`src/pages/InstalledPage.tsx` 中 `shownBefore` 那段 effect），搜索不会，同一种情况两种表现。做法：把 effect 的条件
  从“「显示」变了”改成“选中的工具不在 `rowItems` 里”；要先定打字途中暂时藏掉、再打一个字又出现时是否也关。~~ ——
  **已于 2026-10-02 做了**（`b9ae49b9`）：不管是「显示」、搜索还是新一轮检查藏掉的，选中的工具不在「显示」和搜索留下的
  工具里就关详情。打字途中不关：搜索框停顿约 0.8 秒（`SEARCH_SETTLE_MS`，`src/lib/settled.ts`）后的搜索和屏幕上的搜索
  都藏掉它才关，所以打错一个字再删掉不关，通知的「显示」一下清空搜索时也不会关掉它刚打开的详情。与设想的一处不同：
  不按 `rowItems` 判断，收起的「N 个组件」不算藏掉（通知的「显示」可能选中一个组件，它就在收起的组件里）。焦点只在
  丢了或在被关的详情里时才移到列表第一行，搜索框保留焦点。测试：`src/pages/searchHidesSelection.test.tsx`。
- 【待作者定】**聚焦列表上选中行（强调色底）的白字对比度约 4.0:1**（t21 复审第 5 条）：默认强调色 `#007aff` 上的白字
  （名称、版本）低于 13 pt 字所需的 4.5:1；这是 Mac 列表的画法，「增强对比度」下浅色的 `#0060df`、深色的 `#0068d9`（r6 起）才超过 4.5:1。
  行上灰按钮的蓝字已单独加深到约 5.3:1（`src/index.css` 选中行按钮规则）。要整体达标，得把选中填充改用加深的强调色。
- 【待作者定】**关掉窗口后页面可能被 WebKit 挂起，长操作结束时的通知就发不出**（原生代码复审 N1，`.superpowers/adv/review3/native-review.md`）：`src-tauri/tauri.conf.json` 的窗口没设 `backgroundThrottling`，macOS 14 起页面隐藏约 5 分钟后可能被挂起，操作结束要等窗口回来才上报，那时焦点已在窗口上、算作看过而不通知；提议给这个窗口加 `"backgroundThrottling": "disabled"`（macOS 13 上 wry 跳过，代价是隐藏页面的计时器照常跑），改前先在真窗口里核对一次：打开「操作完成时通知」，关窗口，跑一个超过 6 分钟的操作，切到别的 app，结束后几秒内应收到通知。
- 【待作者定】**用户自己的强调色是浅色时，强调色上的白字远低于 4.5:1**（r6 复审第 1 条）：`--color-accent-foreground`
  固定为白色，`--color-accent` 跟随系统设置里的强调色（`AccentColor`）。「增强对比度」下只把它加深 15%，这个数值只对
  蓝色量过。按系统强调色的近似值（color-mix 在 sRGB 里乘 0.85）算白字对比度：深色外观里黄约 2.0:1、绿约 2.6:1、橙约
  2.8:1、石墨约 3.9:1，紫、粉、红约 4.6–4.7:1；浅色外观里黄约 2.1:1、绿约 2.9:1、橙约 3.0:1、石墨约 4.4:1。不开增强
  对比度时更低（黄约 1.4–1.5:1）。受影响的是「全部更新」、对话框默认按钮、菜单高亮项和聚焦列表的选中行。做法二选一：
  按强调色算前景色（浅色强调色上用深色字，`src/index.css`）；或在「增强对比度」下不用 `AccentColor`，固定用
  `#0060df` / `#0068d9`。改前先在真窗口里用几种强调色核对（WebKit 给出的 `AccentColor` 值可能与上面的近似值略有不同）。
- 【待作者定】**确认框是 `role=dialog`，不是 `alertdialog`**（r6 复审第 5 条）：「要退出吗」（`QuitQuestion`）、卸载
  （`UninstallDialog`）、「要更新N个工具吗」（`UpdateConfirm`）仿的是 NSAlert，读屏会把 NSAlert 报成警告。改成 Radix
  `AlertDialog` 会同时改掉两件事：点对话框外面不再关闭（现在会关）；打开时焦点默认落在「取消」上（现在由 `src/components/ui/Dialog.tsx`
  的 `onOpenAutoFocus` 按框安放）。要先定这三个框要不要保留现在的表现，或者只加 `role="alertdialog"` 不换组件。另：卸载框里那个常在的 sr-only `role=status`，r6 复审后已有测试确认文字落进同一个节点。
