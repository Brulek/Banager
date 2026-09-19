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

## 阶段 4 之前

- `crates/canager-core/src/adapters/mod.rs` `Adapter::capabilities()` 在整个工作区**没有任何调用方**：`session/`、`ops/`、`src-tauri/src/ipc.rs` 都不调，`Capabilities` 也不在 `src/lib/types.ts` 里，从不跨 IPC。七个适配器各写一份 `capabilities()`，全是死代码。要么把它接进界面（离线时不可检查的来源、只读来源的提示、不支持搜索的来源——这需要给 `ManagerInstance` 的线格式加字段、加 TypeScript 镜像、加界面状态与测试），要么直接从 trait 上删掉。在有消费者之前，**别再让 `Capabilities` 长出新字段**：阶段 3 计划原本要加一个 `needs_network` 网络依赖标志，正因为这条而砍掉（2026-09-20 控制者裁决）。

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

- `parse.rs` 的 `pinned` 分支无覆盖：加内联 JSON 单元测试（"不手写 fixture"规则只约束 `adapters/fixtures/` 目录）。
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
