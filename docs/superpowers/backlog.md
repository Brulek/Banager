# Backlog（来自 2026-09-18 阶段 0–1 分支终审）

终审结论：可合并，需先修 5 项（已在 feat/phase-0-1 上修复）。以下为终审与各任务评审中**推迟到后续计划**的事项，按归属计划分组。写新计划时先读这里。

## 阶段 2（界面 / IPC）之前必须处理

- `crates/canager-core/src/adapters/mod.rs` `validate_package_name`：拒绝以 `/` 或 `.` 开头、含 `..` 段、以 `.rb` 结尾的名字，否则 `brew install --formula /tmp/evil.rb` 可执行任意本地 formula。IPC 暴露 install 之前必须修。
- `src-tauri/tauri.conf.json`：`csp` 目前为 `null`；spec §6 要求禁止远程脚本与导航。界面计划的清单项。
- 清理 create-tauri-app 模板残留：`src/App.tsx`（logo、外链、greet 表单）、`src-tauri/src/lib.rs` 的 `greet` 命令、`index.html` 标题。
- `OpRecord.cancel` 是 `pub`，调用方可绕过 `cancel()` 的状态簿记；IPC 层接入时改为私有 + `Notify` 替代 `wait()` 的 20 ms 轮询。
- `detect()` 在 euid 0 时返回空向量，与"未安装 brew"无法区分；界面需要区分显示。

## 阶段 3（其余来源）/ 存储与刷新层

- Settings 需要一个「包含自更新的应用」开关（阶段 2 计划已把 `greedy_casks` 从 `Settings` 中整体移除：只存不用，Session 从不读取，Homebrew 检查更新固定跑 `outdated --json=v2`）。实现时要把该选项从 Settings 经 Session 传到 Homebrew 的 `check_updates`（对应 `brew outdated --greedy`），届时一并调整 `Adapter` trait 的 `check_updates` 签名（会牵动已合并的阶段 0–1 代码）。
- `brew/mod.rs` `maybe_update`：`brew update` 失败或超时（离线、首次 tap 同步慢）会让整个 `check_updates` 失败，应退化为"沿用旧索引 + 标记可能过期"（spec §3）；并发调用存在 TOCTOU 双重 `brew update`，需串行化。
- `AdapterMeta.verified_versions` 从未与 `ManagerInstance.version` 比较（spec §4.1 "未验证版本"角标）。
- spec §4.2 需更正：brew 7.0.3 的 `installed[]` 只有 `installed_on_request`，没有 `installed_as_dependency`（解析器与其文档注释是对的，spec 过时）。

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
  命令中途被停下（取消或超时）的升级一律报「结果未确认」，不看版本。工具是在升级中途、不是结束时写下 Canager
  读的版本，被 `SIGTERM` 停下时又不回滚，所以版本变了不等于装完（brew 公式新 keg 已倒入、还没 link；cask 在
  `stage` 写了新版本元数据、新 app 还没装上），没变也不等于没动（pipx 先装包、最后才写元数据；cask 先把旧 app
  移出 /Applications、之后才写元数据）。Homebrew 与 pipx 的出处逐行写在 `run_operation`（`ops/mod.rs`）
  `Ok(Outcome::Unconfirmed)` 分支的注释里；`tests/ops_upgrade_version_test.rs` 里四个被停下的端到端用例、
  `tests/ops_cancel_test.rs` 里三个用例守着它。执行前那次读取仍保留，读到的版本只用于退出 0 的升级（见下面
  「假「成功」」一条）；在它读的时候按了取消，命令还没开始，报「你已取消」是真的，这一条也保留。
- **npm 在 `npm prefix -g` 失败时用可执行文件路径合成一个不可用实例的 ID。** 更理想的是沿用上一轮
  快照里的实例，但那要把上一轮快照穿进 `Adapter::detect` 的签名，七个适配器都得改。
- **pip 的「不可用」分不清「这个 Python 根本没带 pip」和「pip 装了但坏了」**——两者退出码相同。
- ~~**`Session` 的 `testing` 模块里有一个会改动实时状态的 `expire_issued_plans`**，而该模块刻意不是
  `#[cfg(test)]`，所以会进发布版的库。~~ —— **已于 2026-09-23 在 `11e5ac8` 修复**：`expire_issued_plans`
  收进 `test-support` this-crate-only 的 Cargo feature（`crates/canager-core/Cargo.toml`），默认不开，
  `src-tauri` 的测试通过 `[dev-dependencies]` 单独开它，resolver = "2" 保证不进发布版二进制。

**五、需要作者本人拍板**
- **整个应用没有刷新按钮。** 刷新触发点一共四个：启动、操作完成、两个错误态里的「重试」，以及
  后台运行的 Homebrew 索引更新自行结束时（`ipc::refresh_on_background_change`，
  `src-tauri/src/lib.rs:31-34`）——最后这条不需要用户动手。
  「打开 Ollama」和设置开关这两条明确承诺过会刷新的路径已经修好，但一个软件管家没有常驻刷新控件
  是个洞。加常驻控件涉及位置与形态，是产品决定，没有代为决定。

## 每包可操作性（2026-09-24 立，分支 feat/per-package-actionability）

**已做**：`UpdateCandidate.blocked: Option<UpdateBlocked>`，唯一变体 `Pinned`，生产方有两个：brew 的
`parse_outdated`（读 `brew outdated --json=v2` 的 `pinned`，公式与 cask 都有），和 pipx 的 `parse_outdated`
（读 `pipx list --outdated` 行里名字后面的 ` [pinned]`，把它从名字上切掉）。Rust 侧闸门
`blocked_upgrade`（`session/plans.rs`）在 `issue_plan` 与 `submit` 复检里拒绝 `Upgrade`；
更新页 `isActionable` 多一个条件，按钮、勾选、「更新所选」和两个计数一起去掉；行上写「已固定」
并给出 `<该 brew 的绝对路径> unpin <名字>`（cask 为 `--cask`；路径取自该实例的 `exe_path`，
以代码样式显示）；pipx 的行写 `<该 pipx 的绝对路径> unpin <名字>`，说明句里的来源名按实例给（Homebrew / pipx）。
自己会更新的 cask（`auto_updates`）另有一句，不承诺它停在现在的版本。Canager 不代为解除固定。
pipx 被固定的例子是改过的录制（`adapters/fixtures/pipx/1.17.3/list-outdated-pinned.txt`，只插了 ` [pinned]`，
README 写明、测试核对），和 brew 7.0.6 的 `outdated-pinned.json` 一样，与设计文档「只收真机录制」的字面冲突，
放哪儿仍待作者拍板。

**已知未做**（事实依据见 `.superpowers/actionability-facts.md`，那是本机未入库的调查记录；下一轮不要当新发现）：
- **pipx 的 `unpin` 连注入包一起解除**：`pipx unpin <环境>` 会把该环境里注入的包也一并解除固定
  （pipx 1.17.3 `commands/pin.py:82-92`，没有只解主包的选项）。Canager 不列注入包（不传
  `--include-injected`），行上的说明没提这一点。
- ~~**假「成功」**：pipx 被锁定的工具（有 lock 文件）、uv 用 `==` 装的工具、brew 已停用的 cask（C2）、
  brew 装着的 caskfile 读不出来（C4）——工具都跳过更新却退出 0，Canager 报「成功」而什么都没变。~~
  —— **已于 2026-09-24 在 `c6ecf5b` 修复**，走的是「核对版本真的变了」这条路：升级前后各用同一个
  `reconcile` 读一次版本，退出 0 而版本没变时报新结果 `NeedsAttention(UnchangedAfterUpgrade)`
  （「更新命令显示成功，但版本和更新前一样……」，指向操作日志，四种情况工具都在日志里说了原因）。
  四种情况各有一个端到端测试（`tests/ops_upgrade_version_test.rs`）。代价是每次升级多一次清单读取；
  brew 在后台 `brew update` 还没跑完时这次读取会被拒（`IndexUpdating`），那时照旧只看在不在。
  **仍未做**：更新页事先不知道这些状态，行上照样有「更新」按钮，点了才知道；要提前标出仍得多读上面那几份输出。
- ~~**卸载被固定的包**~~ —— **已于 2026-09-24 修复**：Homebrew 7.0.6 不加 `--force` 时拒绝卸载被固定的包
  （`uninstall.rb:48-49`、`cask/uninstall.rb:40-44`），用的是 `onoe` 不是 `ofail`，公式这边退出 0。
  现在从清单读 `pinned`（`brew info --installed --json=v2` 的公式与 cask 条目都有，`formula.rb:3140`、
  `cask/cask.rb:574`），写进新字段 `InstalledArtifact.uninstall_blocked: Option<UninstallBlocked>`（唯一变体
  `Pinned`，唯一生产方 brew 的 `parse_info_installed`）。闸门 `blocked_uninstall`（`session/plans.rs`）在
  `issue_plan` 与 `submit` 复检里拒绝 `Uninstall`，按实例、类型、名字三者匹配；IPC 报
  `{"kind":"uninstall_blocked"}`。已安装页该行没有「卸载」按钮，说明句替换简介，给出由该实例 `exe_path`
  拼的 `unpin` 命令（与更新页共用 `unpinCommand`，以代码样式显示）；卸载确认框遇到这条拒绝时单独措辞。
  说明句原本一律承诺「下次检查时就会提供卸载，最晚在你下次启动 Canager 的时候」，可 Homebrew 没应答时
  这一行是结转下来的，Homebrew 再次应答之前不会有「卸载」按钮；同日改成：来源没应答的行换用
  `descriptionSourceUnavailable`，只说「之后 Canager 检查时只要 Homebrew 有应答，就会提供卸载」。
  更新页被固定的行（`updates.blocked.Pinned.description` 与 `descriptionSelfUpdating`）当时对没应答的来源
  也作同样的假承诺，已在分支 feat/per-package-actionability 上同样补了
  `descriptionSourceUnavailable` / `descriptionSelfUpdatingSourceUnavailable` 修掉：`UpdatesPage.tsx` 的
  `rowDescription` 按候选自己实例（`snapshot.instances.find`，与其命令用的是同一份查找）是否应答挑选句子。
  Canager 不传 `--force`，也不代为解除固定。pipx 不产生它：`pipx uninstall` 照样删除被固定的工具
  （pipx 1.17.3 `commands/uninstall.py` 不读 `pinned`）。
- ~~**已安装页的「有更新」徽标**~~ —— **已于 2026-09-24 修复**：原先 `updatableIds` 把 `snapshot.updates`
  里每一条都算作有更新，包括被固定的、`checkable: false` 的和被忽略的。现在两页共用
  `src/lib/updateState.ts`：`notIgnored` 决定更新页列出哪些，`updateStateOf` 决定每一条是可更新、只读、
  查不了、被工具拒绝（`blocked`）还是来源没响应；更新页的按钮、勾选、计数和徽标，已安装页的徽标，都从它来，
  两处 `switch` 都没有 `default`，新增状态不写文案 `tsc` 就不过。已安装页只在更新页会给「更新」按钮时写
  「有可用更新」；被固定的写「已固定」（有更新时读 `blocked`，已是最新时读 `uninstall_blocked`），查不了的写
  「无法检查」，只读来源写「只读」，被忽略的写「已忽略更新」，来源没应答的（比如 Ollama 没在运行，更新是之前
  结转下来的）写「有新版本，暂时无法更新」。最后这一种起初仍写「有可用更新」，更新页却没有按钮，与上面那句不符，
  同日改掉：已安装页只在 `updateStateOf` 为 `actionable` 时写「有可用更新」，这正是更新页给按钮的
  `isUpdateActionable`。有测试把两页逐行对照，其中包括一个没在运行的 Ollama。
  更新页一处可见变化：来源没响应的行，徽标仍写「更新」，但颜色从 `info` 改成 `neutral`，与其余没有按钮的行一致。

## 阶段 4 之前

- ~~`Adapter::capabilities()` 七份实现零调用方~~ —— **已于 2026-09-22 在 `e4b13b4` 整体删除**。六个字段里界面唯一需要的「能不能写」是每实例的事实（npm 取决于 prefix 权限），静态的每适配器 trait 方法承载不了，所以移到 `ManagerInstance.read_only_reason`；`search` / `upgrade_all` / `background_check` / `cancel_safe` 四个零调用方直接删。阶段 3 曾因这条砍掉 `needs_network` 标志，该裁决依然正确。

- `crates/canager-core/src/adapters/brew/mod.rs:274-277` **「brew update 失败」的提醒在没有可更新项时被丢掉**。阶段 3 任务 3 把 `brew update` 的失败从「整个来源检查失败」降级成一条提醒，但提醒只能挂在 `UpdateCandidate.warnings` 上，而 `UpdateCandidate` 必须带一个真实的 `ArtifactKey`。于是当 `brew update` 失败、`brew outdated` 又报告零个可更新项时，`for candidate in &mut candidates` 无可遍历，提醒被静默丢弃——偏偏这正是最需要它的情形：本地公式索引陈旧，所以「没有更新」这个结论本身可能就是错的。任务 3 的评审与修复代理都独立认定这是计划自身的设计缺口而非实现偏差，修复代理据此返回 BLOCKED 而没有擅自发明接口，这是对的（2026-09-20 控制者裁决：接受现状，记在这里）。
  修的代价：要给 `ManagerInstance`（或 `Snapshot`）加一条实例级 warnings 通道，连带 TypeScript 镜像、线格式表、界面渲染与测试——本身就是一个完整任务，不该塞进阶段 3 的任何一格。
  可接受的理由：后果是少说了一句提示，不是做错了动作；一旦真有可更新项，提醒照常显示。**不阻塞 v0.1**，但要在做 `Capabilities` 那条（同样需要实例级字段）时一起做掉——两者是同一个通道。

## 阶段 5（发现页）之前必须处理

- `brew/mod.rs` `search`：只要 `--desc` 搜索有结果就丢弃名字匹配，搜 "jq" 搜不到 jq（fixture 可复现：`search-jq.txt` 第 3 行是 jq，`search-desc-jq.txt` 无 `jq:` 行）；且无表头输出的"第一组是 formulae"启发式会把纯 cask 结果标成 Formula。改法：按 (kind, name) 合并；用 `brew search --formula {q}` 与 `brew search --cask {q}` 得到无歧义的类型，`--desc` 只用来补描述；同步更新 `docs/what-we-run.md`。
- 搜索词校验目前套用包名规则，含空格的查询（"json processor"）被拒；spec §4.1 要求独立的 query 规则。
- 阶段 2 的 Task 16 只做两项 i18n 自动检查：en/zh-CN 键集互比 + 组件里的 JSX 字面量扫描，**不**扫描代码里 `t("…")` 用到的键是否真的存在。若将来要补「静态校验 `t()` 键存在性」，需先把动态键改成静态查表：`Sidebar` 的 `t(\`nav.${p}\`)`、`OperationBar` 的 `operations.kind.${…}` / `operations.status.${…}`、`OperationBar`/`LogDrawer` 的 `operations.outcome.${…}`（改成 `PAGE_LABEL_KEYS` 一类的 `Record` 常量表）。动态键拼错在运行时立刻可见，不是隐蔽 bug，故阶段 2 未做。
- 阶段 2 的更新页（`src/pages/UpdatesPage.tsx`，Task 12）直接遍历渲染，没接 `useVirtualizer`：只有 Homebrew 一个来源时更新通常只有几条到几十条，虚拟化收益极低而改动不小（每行的 `snapshot.artifacts.find` 已改为 `useMemo` 建 `Map` 的 O(1) 查找，真正的 O(n²) 已除）。接入更多来源后若更新列表可能变长，按已安装页（`InstalledPage`）的写法补 `useVirtualizer`。

## Runner 打磨（任意时机）

- `runner/real.rs`：最后的 `child.wait()` 未受剩余超时约束；末尾无换行的半行不会推给 `on_line`；kill 后不排空已缓冲的管道数据；每个字节被复制两次。
- `RunnerError::NotFound` / `Spawn` 两条错误路径无测试。
- `lib.rs` 加 `#[cfg(not(unix))] compile_error!("canager-core targets Unix (macOS) in v1")` 与 crate 文档说明；`path_env.rs`（`geteuid`、`HOME`）、`resolve_exe`（无 `.exe`）、`libc` 无条件依赖都隐含 Unix。
- `crates/canager-core/Cargo.toml`：tokio 的 `rt-multi-thread`、`macros` 只有测试与示例用，应移到 `[dev-dependencies]`，使"core 不创建运行时"成为机械事实。
- `brew/mod.rs` 的 `SUDO_ASKPASS` 透传与 `needs_password` 无关且子进程本就继承环境，实际只起预览作用；相关测试修改进程全局环境变量，未加串行化，将来可能抖动。

## 测试数据

- ~~`parse.rs` 的 `pinned` 分支无覆盖~~ —— **已于 2026-09-24 在分支 feat/per-package-actionability 解决**：`pinned` 现在被读成 `UpdateCandidate.blocked = Some(Pinned)`，有内联 JSON 单元测试，也有 `adapters/fixtures/brew/7.0.6/outdated-pinned.json`（由真实录制改了四个 pin 字段而来，README 写明，`brew_fixtures.rs` 有测试核对只差这四个值）。
- `adapters/fixtures/brew/7.0.3/uses-jq.txt` 为空；下次为新 brew 版本重录 fixtures 时，选一个有已装依赖者的 formula（如 `openssl@3`）录 `uses-<formula>.txt`，不得伪造。

## 工作流与发布

- `.github/workflows/release.yml` `releaseDraft: true` 与 `tauri.conf.json` 的 `releases/latest/download/latest.json` 端点冲突：草稿永远不是 latest。终审建议：**先保留草稿**（签名/公证流水线尚未跑过、updater 公钥仍是占位符），在 `release.yml` 加注释并写 `docs/releasing.md` 说明"发布草稿是最后一步，发布后 updater 才能看到"；首个草稿经手工公证验证后再改 `releaseDraft: false`。待作者拍板。
- `.github/workflows/ci.yml`：`feat/**` 推送触发是本分支验证期的临时加项，PR 会让整套 macOS 作业（含真实 `brew install`）跑两遍；合并后去掉或加 `concurrency` 组与 `timeout-minutes`。
- `src-tauri/Cargo.toml`：`fix-path-env` 是无 `rev` 的 git 依赖，仅靠 Cargo.lock 钉住；应加 `rev`。
- GitHub Actions 提示 checkout@v4 / setup-node@v4 / pnpm action-setup@v4 使用即将弃用的 Node 20 运行时；GitHub 定下时间表后升级。

- `src-tauri/Cargo.toml` 的 `[profile.release]` 在 workspace 中被 Cargo 忽略（每次 cargo 命令都打印 "profiles for the non root package will be ignored"），意味着 create-tauri-app 给的 release 优化（lto、opt-level、strip 等）目前对发布构建**不生效**；应把该段移到根 `Cargo.toml`。首个正式发布前必须处理，否则体积目标失真。

## Codex 评审（2026-09-18）推迟项

Codex 独立评审发现 3 项 P1 + 9 项 P2，控制者逐条核实属实；其中 10 项已在本分支修复（见 `.superpowers/sdd/codex-fix-report.md`）。以下 4 项推迟：

- **M3**：`adapters/brew/parse.rs` 两个根结构体对 `formulae`/`casks` 都用 `#[serde(default)]`，因此 `{}` 或只含未知字段的对象会被解析成「空集合」而非报错，把格式异常解释为「没装任何东西」或「全部最新」；`installed_on_request` 缺失时默认 false，把未知安装原因归类为依赖（应为 `Unknown`）。改法：对 JSON v2 要求必要顶层字段存在，区分合法空数组与字段缺失；补 `{}`、缺分区、字段类型错误、截断 JSON 的断言。
- **M4**：`ops/mod.rs` 的 `cancel()` 从不读 `plan.cancel_policy`，执行路径也不按该字段分支，`NoCancel` 计划运行中仍会收到取消令牌。当前 BrewAdapter 只产生 `KillThenReconcile`，故暂不影响；后续适配器用到 `NoCancel` 前必须实现，并补策略矩阵测试。
- **N1**：`brew/mod.rs` 的 detect 单测虽用 MockRunner，仍查询真实文件系统并硬编码「恰好一个实例且为 /opt/homebrew」；Intel Mac、无 Homebrew、双 Homebrew 环境都会失败。改法：把候选路径与存在性检查抽成可注入依赖，分别测零/一/双实例，真实路径验证移入显式门控的集成测试。
- **N2**：`release.yml` 安装两个编译目标并产出 universal 包，但没有 spec §10 要求的 Intel runner 启动冒烟；交叉编译成功不等于 x86_64 半边能跑。发布验收前补 Intel 启动验证，或明确记为未完成的验收项。

## 阶段 2 终审（2026-09-19）推迟项

Opus max 全分支终审：3 项必修（已修），其余推迟。按主题分组。

**规格与实现不一致（下一个计划开头就处理）**
- 更新确认对话框没有展示版本跳变（`current → target`）与 `UpdateCandidate.warnings` 的文字内容（现在只有一个数量徽章）。spec §6 两项都要求。位置 `src/pages/UpdatesPage.tsx:361-378`。
  **做这条时必须一并处理 Ollama**：`UpdateChannel::Digest` 的行不能按 `current → target` 渲染。阶段 3 任务 10 的评审查实（对着提交进仓的 fixture 逐字比对）：Ollama 的 `current` 是 `/api/tags` 的清单摘要、`target` 是注册表清单里的 config 摘要，**是两个不同的哈希空间**，互不包含，拉取成功后新的 `current` 也不会等于旧的 `target`。这两个字段只是「变了／没变」的标记，真正的判定在 `compare_digests` 对层摘要集合的比较上。给小白看两串 64 位十六进制本来也毫无意义——这类行应该说「有新版本可拉取」，而不是打印哈希。
  为什么不在阶段 3 改：修它的两条路都拿一种不一致换另一种。改 `current` 为本地 config 摘要，会和「已安装」页显示的 `artifact.version`（`/api/tags` 摘要）自相矛盾；改 `target` 为注册表的清单摘要才是真正对的，但那要读 `Docker-Content-Digest` 响应头，而 `HttpResponse` 只有 `status` 和 `body`，得改 trait、Mock、真实实现和全部测试。当前无人渲染这两个字段，`reconcile` 也不比较它们，所以没有实际故障，只有一个等着被踩的坑——坑口已经写在 `adapters/ollama/mod.rs` 的注释里（2026-09-20 控制者裁决）。
- `greedy_casks`：计划的任务表把它列为任务 15 的交付物、spec §4.2 与 §5 也定义了它，但计划里那份权威 `Settings` 结构体没有它，于是实现也没有。补它是一次跨 Rust、TypeScript 与磁盘 JSON 的线格式变更，越晚越贵。**需要作者拍板**。
- 更新列表未虚拟化（`src/pages/UpdatesPage.tsx:283-315`），而已安装列表用了 `useVirtualizer`。Global Constraints 与 spec §7 都写了长列表要虚拟化。

**资源增长（接入更多来源前处理）**
- `crates/canager-core/src/session/mod.rs:138` 的 `issued_plans` 只在成功提交时清理，被放弃的预览（关掉对话框、被取代的批次、StrictMode 双次签发）会泄漏到进程结束。插入时顺带清掉超过 600 秒的条目。
- `crates/canager-core/src/ops/mod.rs:154` 的 `records` 只增不减，于是 `summaries()` 无限增长，底部操作条在一次会话里做完第一个操作后就再也回不到空闲态。给历史加个上限（比如最新 100 条）。

**并发与一致性打磨**
- `src/lib/queries.ts:33-41` 的 `useRefresh` 绕过了 `src/lib/events.ts` 里的模块级合并器，手动重试可能与事件驱动的刷新赛跑。改为走 `refreshIntoCache`。
- `src-tauri/src/ipc.rs:27-32`：两个并发的 refresh 都在完成前读了 `generation_before`，一次真实变化可能广播两次 `SnapshotChanged`（幂等，但注释声称的不变量比实际强）。
- `crates/canager-core/src/session/mod.rs:216-218`：`RefusedAsRoot` 分支清空了 artifacts 与 updates，而逐实例失败路径是保留旧数据并标记陈旧。实际不可达（进程内 euid 不变），但与既定规则不一致。
- `src/components/UninstallDialog.tsx:56` 缺同步的重入闩，两次极快的点击都会进入；服务端一次性 PlanId 挡住了重复卸载，但失败那次会重新签发计划。另外 `:123` 的提交错误文字会停留在新预览旁边，读起来像「还是坏的」——在 `onError` 重新签发时顺手 `submitMutation.reset()`。

**测试与工具链**
- `src/pages/UpdatesPage.tsx` 的 14 个测试里 `snapshot.artifacts` 全是空数组，所以 `artifactsById` 从来没命中过，非技术细节视图的描述路径从未被真正执行。补一个带 artifact 的夹具。全部规划失败那条页面错误分支也没有任何测试。
- `tsconfig.json:16` 的 `"types": ["node"]` 把 Node 全局类型套给了整个 `src/`，而这是个 WebView 应用。改用 `tsconfig.test.json` 把这个让步限制在测试里；同时复查 `vite.config.ts:5` 那个在 `tsc -b` 下已过时的 `@ts-expect-error`。
- `src/i18n/no-literal-strings.test.ts:8` 只扫 `components` 与 `pages`，漏了 `App.tsx`、`lib/` 与 `store/`；正则要求至少 4 个字符，"OK"、"Done" 这类短文案会溜过去。
- `crates/canager-core/src/session/mod.rs` 已 1152 行，阶段 3 值得拆分。

**界面打磨**
- `src/pages/SettingsPage.tsx:98-108` 的 `role="radio"` 按钮没有 roving tabindex 也没有方向键处理，键盘用户只能逐个 Tab；这些按钮与 `EmptyState` 的操作按钮都完全没有样式类。
- `src/components/LogDrawer.tsx:36-39` 是 `role="dialog"` 却没有焦点陷阱、也不能按 Esc 关闭。
- spec §7 的 8pt 网格：`px-3`、`py-1`、`gap-3` 等多处不在网格上。
- `src/store/ui.ts` 的 `showDependencies` 是一个全局开关，而列表项带着 `instanceId`；两个 brew 前缀（spec §4.2 提到的 Intel 迁移场景）下两组会一起展开收起。

**杂项**
- 提交 `b4d6722` 的署名是 `Claude Sonnet 5`，28 个提交里唯一一个不一致。改它要重写 27 个后代提交，建议明确接受现状而不是返工。
- `clearSelectedUpdates` 与 `clearLogs` 在计划的接口里、有测试，但生产代码从不调用。
- `src/pages/UpdatesPage.tsx:264` 的 `item.planError ?? ""` 按构造是死代码。
- `src/components/UninstallDialog.tsx:36` 带着一个 eslint 抑制注释，而本仓库并未配置 eslint。
- `crates/canager-core/src/settings.rs:57` 用了 `Ordering::SeqCst`，`Relaxed` 就够。

## 需要作者本人操作的事项（阶段 0–1 遗留）

- 任务 3：创建 Developer ID Application 证书并导出 .p12、生成 App 专用密码、查 Team ID、`pnpm tauri signer generate -w ~/.tauri/canager.key` 并把公钥填入 `tauri.conf.json`（替换 `REPLACE_WITH_UPDATER_PUBKEY`）、逐个 `gh secret set`；然后打 `v0.0.1` 标签验证公证。
- 任务 1：在 Terminal.app 里跑一次 `pnpm tauri build` 确认 .dmg 打包（自动化会话里 Finder AppleEvent 超时 -1712，属 TCC 自动化权限问题）。
- 任务 6：`pnpm tauri dev` 目视确认窗口打开且日志里 `[canager] discovered PATH dirs` 含 `/opt/homebrew/bin`。
- 任务 13：按 `docs/spikes/2026-09-askpass.md` 亲自跑 `sudo -A` 对话框试验并填结果表。
