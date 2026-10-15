# 实例级通道设计 v2（2026-09-22）

v1 由控制者撰写，经三路对抗评审（死字段 / 用户可见 / 改动半径），三方均判
`sound_with_changes`，共证伪 v1 的 7 处断言、发现 2 处「照 v1 实施会直接坏掉」的问题。
本稿是据此重写的**可实施规格**。评审报告：`.superpowers/sdd3/review-spec-{dead-fields,user-visible,blast-radius}.md`。

---

## 零、v1 错在哪（保留记录，避免下一轮重新发现）

| v1 的说法 | 真相 | 谁查出来的 |
|---|---|---|
| `healthy` 只有 2 个读取方 | **6 处**，含 `src/lib/queries.ts:139`（「打开 Ollama」后的守护进程轮询）——v1 全文未提，漏改会让按钮永不收敛 | 死字段 / 改动半径 |
| `status.refreshed_at` 由 `SnapshotStatus.tsx` 渲染成「2 天前」 | **今天不可能**：`EmptyState` 只收单个 title+description，`SnapshotStatus` 无任何按来源渲染位，全仓零 `Intl.RelativeTimeFormat`/日期库，`refreshed_at` 在 `src/` 里只被当布尔判空 | 三方一致 |
| `SourceNotice.tsx` 是读取方 | 它是**纯展示组件**（自述 "Purely presentational -- callers decide when it applies"），拿不到 `ManagerInstance`。真读取方是 `InstalledPage.tsx:178-212` | 死字段 / 改动半径 |
| 发现(3)（brew 提醒被丢弃）已解决 | **未解决**。伤害发生在更新页 `UpdatesPage.tsx:307-309` 的「所有内容都已是最新」，v1 只把提醒搬到了已安装页——从一个看不见的地方搬到另一个 | 死字段 / 用户可见 |
| 删掉的是「其余五个」 | 算术错，纯删**四个**；`per_item_upgrade` 与 `uninstall` 是被合并，不是被删 | 三方一致 |
| 删除面是「七份实现」 | **约 22 处**（1 声明 + 7 生产 + 11 测试 fake + `fake_capabilities()` + 2 条自证测试） | 死字段 / 改动半径 |
| `set_writability` 能守住不变量 | **守不住，而且它自己就是零调用方**：`ManagerInstance` 字段全 `pub`，Rust 结构体字面量强制写全字段，七个 `detect()` 一个都不会调它。这正是本项目连犯两阶段的错，换了个形状 | 改动半径 |
| Q2 是「又一次跨七适配器的改动」 | **不是**。A 类集中在 brew 与 cargo **两个文件**；其中 brew 那两条在**卸载确认屏**上，中文用户正被要求读英文风险提示后点「卸载」 | 用户可见 |

v1 另有一处事实表述需更正：refresh.rs 的「保留上一轮产物」承诺在**报错路径上是做到了的**
（`inventory()`/`check_updates()` 返回 `Err` 时会 `extend(previous...)`）。没做到的只有
`healthy == false` 被 `continue` 整段跳过那一条路。改的是那一行，不是错误分支。

---

## 一、决定

两轴分解**保留**——评审确认界面今天就在按两轴渲染：pip 是 `variant="info"` + 「改用 pipx 或 uv」，
不可达是 `variant="warning"`（Ollama 另加启动按钮）。压成一个状态块等于把这两种底色、
两种文案、有无按钮的区别重新抹平。

相对 v1 的四处建模修正：

1. **砍掉 `writable` 字段**，只留 `read_only_reason` + 一个方法。单一真源，矛盾状态不可表示，
   线格式少一字段，七处构造少写一行，代价为零。
2. **砍掉 `status.refreshed_at`**（本轮）。三方独立判定其读取方不存在，且它会打破
   `same_content` 比较（见 §2.4 注）。完整形状记入 backlog。
3. **新增「可操作 = 能力 ∧ 状态」合取不变量**，在 `Session::issue_plan` 一处闸门实现。
   v1 完全没有这条，导致 §2.4 的「沿用旧产物」会把发现(4) 原样造回来。
4. **实例级通知必须两页都渲染**。v1 只覆盖已安装页，而三个场景的谎话在更新页。

---

## 二、Rust

### 2.1 删除 `Capabilities`（约 22 处，编译器全程护航）

删 `struct Capabilities`、trait 方法 `fn capabilities()`、7 份生产实现、11 处测试 fake
（`session/{mod,refresh,plans}.rs`、`src-tauri/src/ipc.rs:237`、六个 `tests/ops_*.rs`）、
`session/test_support.rs:72` 的 `fake_capabilities()`、两条自证测试（`pip.rs:619`、`npm.rs:947`）。

六个字段的去向：`per_item_upgrade` 与 `uninstall` **合并**进 `read_only_reason`
（评审逐个核过七份实现：两者取值完全一致，pip 双 false、其余六个双 true，不存在
「能卸不能升」的实例）；`search` / `upgrade_all` / `background_check` / `cancel_safe` **纯删**，
零调用方且短期不会有。

> cargo 看似反例——`cargo install --force` 对 git/path 来源会换源或装不上，而 `cargo uninstall`
> 总能跑。但这层不对称在**每制品**轴上，不在每实例轴：`cargo.rs:245-256` 已对非 registry 来源
> 发 `checkable: false`，`UpdatesPage.tsx:131-132` 已挡住。实例轴上一个布尔是对的尺寸。

### 2.2 能力轴（`model.rs`）

```rust
/// 为什么这个来源只能看不能动。用枚举而非字符串：这些理由要显示给用户，
/// 而 Rust 侧拼好的英文句子无法本地化（存量 warnings 就踩了这个坑，见 §6）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadOnlyReason {
    /// 这个工具本身没有 Banager 能安全驱动的安装/卸载路径（pip）。
    ByDesign,
    /// 工具能装能卸，但它要写的目录当前用户写不了（nodejs.org 装的 Node）。
    PrefixNotWritable,
}
```

`ManagerInstance` **只加一个字段**：

```rust
    /// `None` = 可写。唯一真源——没有第二个字段能与它不一致。
    pub read_only_reason: Option<ReadOnlyReason>,
```

```rust
impl ManagerInstance {
    pub fn writable(&self) -> bool { self.read_only_reason.is_none() }
}
```

各 `detect()` 填法：pip → `Some(ByDesign)`；npm → 不可写时 `Some(PrefixNotWritable)`；其余五个 → `None`。

**npm 可写性修正**（评审确认 v1 方向对、回退链错）：`npm prefix -g` 返回 prefix 根
（`/opt/homebrew`），npm 实际写 `{prefix}/lib/node_modules`。目录不存在时 npm 要**创建**它，
需要的是对父目录的写权限。正确规则是沿 `{prefix}/lib/node_modules` → `{prefix}/lib` → `{prefix}`
取**最近的存在的祖先**再 `access(W_OK)`。

`plan()` 里现有的 `Refused` 闸门**保留**：detect 与用户点按钮之间隔着几十秒到几小时，
权限会变（TOCTOU）。纵深防御，不是重复。

### 2.3 状态轴（`model.rs`）

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unavailable {
    /// 服务没在跑，用户可以自己启动（Ollama）。
    NotRunning,
    /// 可执行文件在 PATH 上，但跑不起来或版本认不出。
    NotResponding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceNote {
    /// `brew update` 失败，本地索引可能过期，所以「没有更新」可能是错的。
    IndexMayBeStale,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceStatus {
    pub unavailable: Option<Unavailable>,
    pub notes: Vec<InstanceNote>,
}
```

`ManagerInstance` 加 `pub status: InstanceStatus`，**删除 `pub healthy: bool`**
（两者逻辑等价，共存期即漂移期；三方一致同意一步到位）。

> `InstanceNote` 无载荷会丢掉今天 warning 里的 stderr 原因文本。对小白是净收益，
> 排障信息丢失是已知代价——**不加载荷**，否则裸字符串变体变成外部标记数据变体，
> 手写 TS 镜像的风险面变大。记录在此，免得下一轮当新发现再报一次。

**`healthy` 的 6 个读取方**（v1 说 2 个，错）：
`session/refresh.rs:107`、`src/lib/sources.ts:45`、`src/pages/InstalledPage.tsx:81/120`、
`src/pages/InstalledPage.tsx:192`（`{!item.healthy ? ...}`，独立分支，不走 `hasSourceNotice`）、
`src/components/SnapshotStatus.tsx:217`、**`src/lib/queries.ts:139`**（「打开 Ollama」后的轮询，
靠 `adapter_id === "ollama" && healthy` 判断守护进程起来没有——漏改则按钮点完通知永不消失）。

### 2.4 `session/refresh.rs`

1. **`refreshed_at` 无条件盖章**（刷新跑完就盖），不再是「七源全成功」。
   `Snapshot::empty()` 必须**继续保持 `refreshed_at: None`**——`SnapshotStatus.tsx:67-70`
   的「启动中」分支依赖它，评审已核过改后仍成立。
2. **`stale = !errors.is_empty()`**（**2026-09-22 修订**：本条最初写的是
   `任一实例 unavailable || !errors.is_empty()`，实施后又收窄回只看 errors）。
   收窄的理由：加宽的那一半**没有任何读取方能观察到**——唯一的生产读取方是
   `SnapshotStatus.tsx` 的 `stale && errors.length > 0`，而 errors 非空必然蕴含 stale，
   于是那个合取恒等于 `errors.length > 0`。真要给它一个读取方，只能是一条页面级横幅，
   而它会用更差的措辞、配一个不对的按钮，重复用户在两个页面上已经看到的每来源提示；
   更糟的是那条横幅按 `errors` 计数，会显示「上次刷新有 0 个来源没能完成」。
   一个不可用的来源自己会在两个页面上说话，不需要再让全局横幅替它说一遍。
   v1 的公式只看 detect 阶段，会丢掉 fan-out 失败（`brew outdated` 挂了、坏 tap），
   那类实例是可用的但 `errors` 非空——只看「实例是否 unavailable」的公式会给出 `stale == false`，
   `SnapshotStatus.tsx:170` 的横幅不出，用户拿着旧数据且毫无提示。**净回归，必须避免。**
3. **不可用的实例沿用上一轮的 artifacts 与 updates**（只改 `healthy == false` 那条
   `continue` 路径，错误路径本来就做对了）。这让现有文案
   `sourceNotice.unreachable.description`「这里显示的是上次查到的内容」**从谎话变成真话**——
   今天 Ollama 没启动时，用户看到一个分组标题、这句话，和**下面一行都没有**。
4. **notes 回填**：`instances` 在扇出前被 clone（`refresh.rs:117`），notes 产生在各 spawn 里，
   join 之后必须按 `instance_id` 合并回 `instances` 再构造 `Snapshot`。
   v1 漏了这一步——正是「`SourceError` 带着 `instance_id` 却没人拿去合并」的同款陷阱。

> **为什么砍掉每实例 `refreshed_at`**：`session/mod.rs:88-96` 的 `same_content` 包含
> `self.instances == other.instances`，而 `ManagerInstance` 派生 `PartialEq`。往里塞一个
> 每轮都变的 unix 秒 → `instances` 永不相等 → `commit()` 每轮 bump generation →
> 每轮广播 `SnapshotChanged` → 前端每轮重绘。现有测试
> `test_refresh_impl_does_not_rebroadcast_when_the_generation_is_unchanged`
> （`src-tauri/src/ipc.rs:558-593`）会直接变红，而它的注释恰好写着
> 「`refreshed_at` is deliberately excluded from that comparison」。
> 加上读取方不存在（无格式化器、无渲染位、无日期库），本轮砍掉，形状记 backlog。

### 2.5 可操作性合取闸门（v1 缺失，评审新增）

`writable` 是能力轴，与 `unavailable` 无关：**Ollama 没在跑时实例仍然 `writable`**。
§2.4-3 把旧 artifacts 结转回来之后，这些行会各自带一个卸载按钮，点下去必然失败
（守护进程没跑，`ollama rm` 不可能成功）——发现(4) 在发现(6) 的修法里原样长回来。

**不变量：一个操作可以出现，当且仅当 `read_only_reason.is_none() && status.unavailable.is_none()`。**

在 `Session::issue_plan`（`session/plans.rs:19-33`）加一道**统一闸门**：它已经从
`self.snapshot` 取到了 instance，是全工作区唯一所有操作必经的点。一处 4 行，
胜过在七个 `plan()` 里各加一段（本轮评审已点名两块跨五适配器的逐字重复，不要再造第三块），
并且让 `read_only_reason` 在 **Rust 侧**也有生产读取方，而不只是 IPC 对面的 TS。

### 2.6 `CheckOutcome`（Q1 定案：方案 b）

```rust
pub struct CheckOutcome {
    pub candidates: Vec<UpdateCandidate>,
    pub notes: Vec<InstanceNote>,
}
impl From<Vec<UpdateCandidate>> for CheckOutcome { /* notes: vec![] */ }
```

`check_updates` 返回 `Result<CheckOutcome, AdapterError>`。

- **(c) `&mut ManagerInstance` 直接否掉**：`refresh.rs:117-155` 把 `inst` move 进
  `tokio::spawn`，七路并发各持 clone。要可变借用就得加锁或改回串行，把任务 11 的并发拆掉。
- **(a) 元组否**：第二个位置无名，第三样东西要来时又是七处。
- **(b) 采纳的决定性理由**：`check_updates` 只有一个生产调用方（`refresh.rs:144`），
  **不跨 IPC**（`ipc.rs` 只在测试里提到它），所以 `CheckOutcome` 是**纯核心内部类型，
  不需要 TS 镜像**——不碰这个项目最脆的那一层。
- `From` 让六个适配器从 `Ok(out)` 变成 `Ok(out.into())`，只有 brew 真正填 notes。
  重复的是一次类型转换，不是一段逻辑。
- 这是本阶段**第二次**动这个签名（任务 2 刚为 `CheckOptions` 动过），所以要选
  「以后加字段不再改签名」的形状。

> `inventory()` 没有对称通道；它将来要报实例级提醒又是七处。本轮不动，记在此处。

### 2.7 brew

`maybe_update` 失败时，**从 `UpdateCandidate.warnings` 中彻底移除**该提醒，只推
`status.notes` 的 `IndexMayBeStale`。

必须是「彻底移除」而非「不只放在 warnings」：`UpdatesPage.tsx:336-350` 在 `checkable: true`
的行上走 `descriptionFor()`，**警告文本从不渲染**，只有一个写着「1 条警告」的徽标。
留着只会多一个读不出内容的徽标。

---

## 三、TypeScript（`src/lib/types.ts`）

```ts
export type ReadOnlyReason = "ByDesign" | "PrefixNotWritable";
export type Unavailable = "NotRunning" | "NotResponding";
export type InstanceNote = "IndexMayBeStale";

export interface InstanceStatus {
  unavailable: Unavailable | null;
  notes: InstanceNote[];
}

export interface ManagerInstance {
  id: string;
  adapter_id: string;
  exe_path: string;
  prefix: string;
  scope: "User" | "System";
  version: string | null;
  // healthy: boolean;                        <- 删除
  unverified_version: string | null;
  read_only_reason: ReadOnlyReason | null;    // 新增
  status: InstanceStatus;                     // 新增
}
```

镜像易漂三处：`InstanceStatus` 派生 `Default`，永远是对象不会是 `null`；`notes` 空时是 `[]`；
新增枚举变体时 TS 联合类型不报错，只在运行时落进 default 分支——`src/lib/types.test.ts` 补形状测试。
`Snapshot` 只活在内存 `Mutex` 里不落盘，**不需要 `#[serde(default)]`**（那条规矩只管 `Settings`）。

---

## 四、读取方表格（已按评审更正）

| 字段 | 生产方 | 生产读取方 | 用户看到什么 |
|---|---|---|---|
| `read_only_reason` | 七个 `detect()` | **`session/plans.rs`**（Rust 侧闸门 §2.5）、`src/lib/sources.ts` `canWrite()`、`src/pages/InstalledPage.tsx:178-212`（通知 + 卸载按钮）、**`src/pages/UpdatesPage.tsx:363-374`（按变体分支文案）** | pip：「改用 pipx 或 uv」；npm：「改用 Homebrew 装 Node」。**两条文案必须分开**——今天 `UpdatesPage` 对只读行硬编码 pip 的建议，改成按 `writable` 判断后会让 npm 行也建议用户改用 pipx/uv，对非程序员是纯误导 |
| `status.unavailable` | `detect()` + `refresh.rs` | `session/plans.rs`（闸门）、`src/lib/sources.ts`、`src/pages/InstalledPage.tsx:178-212`、**`src/pages/UpdatesPage.tsx` 顶部通知区 + 早退条件** | Ollama：「Ollama 没有运行」+ 按钮；其余：「Banager 现在连不上 X，下面是上次查到的内容」 |
| `status.notes` | `adapters/brew/mod.rs` | **`src/pages/UpdatesPage.tsx:307-309` 早退分支 + 顶部横幅**（首要）、`src/pages/InstalledPage.tsx` 分组头 | 「Homebrew 这里的『已是最新』可能不准」+「重试」 |
| `Snapshot.refreshed_at`（语义变更） | `refresh.rs` | `SnapshotStatus.tsx` | 六好一坏的机器不再永远显示「有来源没有应答」 |
| `Snapshot.stale`（改为导出） | `refresh.rs` | `SnapshotStatus.tsx:170` | fan-out 失败时横幅仍然出现（不回归） |

**删除项**：`Capabilities`（约 22 处）、`ManagerInstance.healthy`（6 个读取方）、
`src/lib/sources.ts` 的 `READ_ONLY_ADAPTER_IDS`、`UpdatesPage` 的 stopgap、
`SnapshotStatus.tsx:100` 那条变成不可达的分支及其 `emptyStates.incompleteCheck.*` 四条文案、
`SnapshotStatus.tsx:44-95` 两段描述旧语义的注释（必须重写，否则下一个读代码的人照错前提判断）。

**显式约束**：改 `hasSourceNotice(adapterId, healthy)` 签名时不能弄丢
`SnapshotStatus.tsx:217` 那个调用方——它是「零产物时不要显示『还没有安装任何东西』」的守门人。
弄丢了，一台只装了未启动 Ollama 的 Mac 会退回到那个刚修好的 bug。

---

## 五、实施顺序（Q4 定案：按轴切四步，不是「大爆炸 vs 渐进」）

每步全绿、每步有用户可见的读取方。

1. **只修 npm 可写性查错目录的 bug**（含祖先回退链）。独立、有测试、不动签名，可先合。
2. **能力轴**：加 `read_only_reason`；删 `Capabilities`（22 处）；`issue_plan` 加闸门；
   前端删 `READ_ONLY_ADAPTER_IDS` 与 stopgap；两页按变体分支文案；更新页计数改两段式。
   **交付发现 1 / 2 / 4 / 5。**
3. **状态轴**：加 `InstanceStatus` + 删 `healthy`（同一提交）；`refresh.rs` 三处改动 + notes 回填；
   `SnapshotStatus.tsx` 三条分支 + 两段注释 + 死文案；6 个 `healthy` 读取方；实例通知提升为
   两页共用。**交付发现 6。** 这一步确实大，且是唯一需要人肉盯 TS 镜像的一步。
4. **`CheckOutcome` + brew 的 `IndexMayBeStale`**。依赖第 3 步。**交付发现 3。**
5. **brew 卸载对话框的两条英文警告改枚举**（见 §6）。单文件，独立。

第 2、3 步互不依赖可并行；第 1、5 步随时可先合。
**并行时必须各自用独立 git worktree 或限定 `git add` 路径**——本会话已因共享工作树
吃过一次提交夹带的亏。

**测试构造成本**（v1 未提）：`ManagerInstance {` 全仓 **45 处**，分布 16 个 Rust 文件
（含 6 个 `tests/ops_*.rs` 与 `ipc.rs` 两处 fixture）+ 7 个 TS fixture 文件。
建议：**生产 `detect()` 保持结构体字面量**（要的就是编译器穷尽检查），
其余约 38 处测试构造统一走一个公开测试构造器（集成测试在 `tests/` 用不了 `#[cfg(test)]`，
需 `pub fn` 或 `test-support` feature）。下次加字段即回本。

---

## 六、Q2 定案：拆开做，不整体推迟

v1 判断「又一次跨七适配器的改动」**不成立**。评审核出实际分布：

- **本轮必做**：`brew/mod.rs:389` 与 `:396` 的两条依赖警告
  （"could not determine what depends on X…"、"Removing X will break: a, b"）。
  它们出现在**全 app 唯一的破坏性确认屏**上——`UninstallDialog.tsx:131-142` 在中文标题
  「继续之前请注意:」下原样列出英文。中文用户等于在没看懂风险提示的情况下点「卸载」。
  改成 `Warning::DependentsUnknown` / `Warning::WouldBreak { names }`。**单文件。**
- **本轮顺手**：`cargo.rs:319`、`cargo.rs:258` 两条（同样单文件）；删掉
  `brew/parse.rs:196` 的 `"pinned"`——它今天渲染不出来（见 §2.7），留着只造徽标。
- **记 backlog**：所有 `{{message}}` 插值透传的动态英文。正确形状不是枚举，而是
  **默认显示本地化通用句子，原始串收进 `show_technical_details` 开关后面**——
  该设置已存在且语义完全对得上，比全面枚举化便宜得多、收益更大。

---

## 七、文案（en + zh，B4 要求；缺一句就无法评审这份设计是否解决了问题）

`ByDesign`（pip）沿用现有 `sourceNotice.pipReadOnly`，只换触发条件。

```
sourceNotice.prefixNotWritable.title
  en: "Read-only: npm packages"
  zh: "只读：npm 包"
sourceNotice.prefixNotWritable.description
  en: "Banager can list these but can't update or remove them: npm keeps them in a folder your
       account isn't allowed to change. That usually means Node was installed with the installer
       from nodejs.org. Installing Node with Homebrew instead lets Banager manage them."
  zh: "Banager 只能列出这些，无法更新或卸载——npm 把它们放在了你的账户无权改动的文件夹里。
       这通常是因为 Node 是用 nodejs.org 的安装包装的。改用 Homebrew 安装 Node，Banager 就能管理它们了。"

sourceNotice.notRunning.title         en: "{{source}} isn't running"          zh: "{{source}} 没有在运行"
sourceNotice.notRunning.description
  en: "Start {{source}}, then come back — Banager will list what's in it."
  zh: "启动 {{source}} 之后回到这里，Banager 就能列出它里面的内容。"
  （ollama 沿用现有 sourceNotice.ollamaNotRunning，含「打开 Ollama」按钮；
    按钮条件是 adapter_id === "ollama" && unavailable === "NotRunning"）

sourceNotice.unreachable.description   （改写现有，去掉 {{when}}——本轮无每实例时间戳）
  en: "{{source}} is installed but didn't answer. Below is what Banager saw last time; anything
       added or removed since then won't show up. Reopening Banager usually fixes this."
  zh: "{{source}} 装着，但没有应答。下面是 Banager 上次看到的内容，之后的变化不会显示。
       重新打开 Banager 通常就能恢复。"

sourceNotice.indexMayBeStale.title
  en: "“Up to date” may not be accurate for Homebrew"
  zh: "Homebrew 这里的「已是最新」可能不准"
sourceNotice.indexMayBeStale.description
  en: "Banager couldn't download Homebrew's latest list of software, so there may be updates it
       can't see yet. Check your internet connection, then try again."
  zh: "Banager 没能下载 Homebrew 最新的软件目录，所以可能有更新它还看不到。检查一下网络连接，然后重试。"
sourceNotice.indexMayBeStale.action    en: "Try again"    zh: "重试"

updates.upToDate        （保留，仅当所有实例 available 且无 notes 时才说）
updates.noneCheckable   en: "No updates in the sources Banager could check"
                        zh: "已检查的来源里没有可更新的内容"
updates.countUnmanageable  en: "{{count}} more can't be updated here"   zh: "另有 {{count}} 个无法在这里更新"
updates.noneActionable  en: "Nothing here can be updated by Banager"    zh: "这里没有 Banager 能更新的内容"
```

**更新页计数改两段式**（B5）：今天 `actionableCount` 把只读行从计数里抹掉，
pip 有六个过期包时标题显示「0 个可用更新」、底下列着六行。改成「N 个可用更新」+
「另有 M 个无法在这里更新」；N 为 0 且 M > 0 时标题用 `updates.noneActionable`。

---

## 八、明确不在本轮射程（记 backlog，避免下一轮当新发现）

- **每实例 `refreshed_at`**（「上次应答 2 天前」）。完整形状：移出 `ManagerInstance` 或把
  `same_content` 改成投影比较并加测试锁住；新建相对时间格式化器 + i18n 键；
  `refresh.rs` 按 `inst.id` 从 `previous.instances` 接续（成功盖新章、失败沿用旧值、
  找不到为 `None`）；渲染位在两页的来源通知里，不在 `SnapshotStatus`。
- **每包可操作性**。`brew outdated` 会列出 pinned 公式（`parse.rs:194`），`checkable: true`
  → 给出「更新」按钮 → `brew upgrade` 拒绝 → 用户拿到一行原始英文。这是「提供了然后拒绝」
  的第七例，但**实例级通道收不住它**，它是每包的。正确形状是 `UpdateCandidate.actionable: bool`
  + 理由枚举（`Pinned` 是第一个变体），不是又一轮前端硬编码。
  —— **已于 2026-09-24 在分支 feat/per-package-actionability 落地**，形状略有不同：没有另设
  `actionable: bool`，而是 `blocked: Option<UpdateBlocked>` 一个字段（`None` 即可操作），
  免得布尔值和理由互相矛盾。详见 backlog「每包可操作性」一节。
- **`{{message}}` 动态英文透传**（见 §6）。
- **全新 Mac 的安装引导**：现有 `emptyStates.noSources` 点名了七个管理器并建议从 Homebrew 开始，
  但没告诉用户**怎么装**。一个「怎么安装 Homebrew」的按钮比一句建议有用。
- **`inventory()` 的实例级通道**（见 §2.6 注）。
