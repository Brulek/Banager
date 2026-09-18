# Backlog（来自 2026-09-18 阶段 0–1 分支终审）

终审结论：可合并，需先修 5 项（已在 feat/phase-0-1 上修复）。以下为终审与各任务评审中**推迟到后续计划**的事项，按归属计划分组。写新计划时先读这里。

## 阶段 2（界面 / IPC）之前必须处理

- `crates/canager-core/src/adapters/mod.rs` `validate_package_name`：拒绝以 `/` 或 `.` 开头、含 `..` 段、以 `.rb` 结尾的名字，否则 `brew install --formula /tmp/evil.rb` 可执行任意本地 formula。IPC 暴露 install 之前必须修。
- `src-tauri/tauri.conf.json`：`csp` 目前为 `null`；spec §6 要求禁止远程脚本与导航。界面计划的清单项。
- 清理 create-tauri-app 模板残留：`src/App.tsx`（logo、外链、greet 表单）、`src-tauri/src/lib.rs` 的 `greet` 命令、`index.html` 标题。
- `OpRecord.cancel` 是 `pub`，调用方可绕过 `cancel()` 的状态簿记；IPC 层接入时改为私有 + `Notify` 替代 `wait()` 的 20 ms 轮询。
- `detect()` 在 euid 0 时返回空向量，与"未安装 brew"无法区分；界面需要区分显示。

## 阶段 3（其余来源）/ 存储与刷新层

- `brew/mod.rs` `maybe_update`：`brew update` 失败或超时（离线、首次 tap 同步慢）会让整个 `check_updates` 失败，应退化为"沿用旧索引 + 标记可能过期"（spec §3）；并发调用存在 TOCTOU 双重 `brew update`，需串行化。
- `AdapterMeta.verified_versions` 从未与 `ManagerInstance.version` 比较（spec §4.1 "未验证版本"角标）。
- spec §4.2 需更正：brew 7.0.3 的 `installed[]` 只有 `installed_on_request`，没有 `installed_as_dependency`（解析器与其文档注释是对的，spec 过时）。

## 阶段 5（发现页）之前必须处理

- `brew/mod.rs` `search`：只要 `--desc` 搜索有结果就丢弃名字匹配，搜 "jq" 搜不到 jq（fixture 可复现：`search-jq.txt` 第 3 行是 jq，`search-desc-jq.txt` 无 `jq:` 行）；且无表头输出的"第一组是 formulae"启发式会把纯 cask 结果标成 Formula。改法：按 (kind, name) 合并；用 `brew search --formula {q}` 与 `brew search --cask {q}` 得到无歧义的类型，`--desc` 只用来补描述；同步更新 `docs/what-we-run.md`。
- 搜索词校验目前套用包名规则，含空格的查询（"json processor"）被拒；spec §4.1 要求独立的 query 规则。

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

## 需要作者本人操作的事项（阶段 0–1 遗留）

- 任务 3：创建 Developer ID Application 证书并导出 .p12、生成 App 专用密码、查 Team ID、`pnpm tauri signer generate -w ~/.tauri/canager.key` 并把公钥填入 `tauri.conf.json`（替换 `REPLACE_WITH_UPDATER_PUBKEY`）、逐个 `gh secret set`；然后打 `v0.0.1` 标签验证公证。
- 任务 1：在 Terminal.app 里跑一次 `pnpm tauri build` 确认 .dmg 打包（自动化会话里 Finder AppleEvent 超时 -1712，属 TCC 自动化权限问题）。
- 任务 6：`pnpm tauri dev` 目视确认窗口打开且日志里 `[canager] discovered PATH dirs` 含 `/opt/homebrew/bin`。
- 任务 13：按 `docs/spikes/2026-09-askpass.md` 亲自跑 `sudo -A` 对话框试验并填结果表。
