# Canager 阶段 5 提案：发现页与空机器正门（2026-09-29）

> 提案，不是规格。对应 spec §13 的阶段 5（发现页 + 空机器首启 + Homebrew 安装引导）。请作者先拍板 §四 的五条，再据此写规格与分步计划；本稿只动文档。
> 代码断言核对于 `feat/ui-round-2` 的 `5f53098`，Rust 路径相对 `crates/canager-core/src/`；外部事实给出处，GitHub 上的源码按提交或标签钉住；**未核**的写明在哪一步核。
> 写完后已修（同一分支）：第零节第 6 条，窗口不能再要求安装（`cb3ef0a`）；第 3 条，没装命令行工具时不再运行 `/usr/bin/python3`（`6a23756`）。第 2 条仍成立：没有命令行工具时 pip 不再列出，空 Mac 更容易到达空状态，但正门条件仍按 §1.1 在步骤 C 定。
> 已定、本稿不再讨论：macOS 优先；面向小白；空机器的正门是带精选的首启；pip 只读；签名构建（spec §0、§1、§7）。

## 零、今天的样子

| # | 事实 | 出处 |
|---|---|---|
| 1 | 代码给空 Mac 的是一句「没有找到可管理的工具 / 建议先装 Homebrew。」，没有按钮 | `src/components/SnapshotStatus.tsx:98-109`，`src/i18n/zh-CN.json:567-570` |
| 2 | 真正的空 Mac 到不了那一屏：只有所有来源都没有实例时 `detect` 才是 `Missing`；系统自带的 `/usr/bin/python3` 总在 PATH 上，pip 探测对它跑 `-m pip --version`，失败了也照样列出一个「没有应答」的 pip | `session/refresh.rs:367-371`，`adapters/pip.rs:92-130` |
| 3 | `/usr/bin/python3` 与 `/usr/bin/git` 是同一个文件（本机同一 inode），Homebrew 的源码说它「没装开发者工具时是个弹窗的桩」。所以没装 Xcode 命令行工具时，pip 探测很可能每次刷新都弹出系统的「安装命令行开发者工具」对话框：**未核**（本机装着 Xcode） | 本机 `ls -i`；本机 Homebrew 源码 `shims/shared/git:45`；S0 核 |
| 4 | 六个来源已有安装计划：brew（formula、cask）、npm、pipx、uv、cargo、ollama pull；装完重读，没装上报「需要留意」 | `docs/what-we-run.md` 各来源的写命令表（如 :603-613、:923-929）；`ops/mod.rs:705-711` |
| 5 | 界面没有任何地方发起安装，也没有搜索、清单的 IPC 命令 | `src-tauri/src/lib.rs:109-125`；`src/` 里的 `"Install"` 只在类型、文案与 mock |
| 6 | Rust 侧却没拦：`plan_operation` 收整个 `OpRequest`，闸门对 `Install` 只查来源可写、可用，不查装的是什么，发一个 `Install` 会照常出预览 | `src-tauri/src/ipc.rs:410-415`，`session/plans.rs:81-115,172-192` |
| 7 | 安装要求来源已经存在，否则 `SourceGone`，所以「装 Homebrew 本身」走不了这条路；独立安装工具的 `plan(Install)` 直接拒绝 | `session/plans.rs:149-151`，`adapters/standalone/mod.rs:800-803` |
| 8 | 信任文件的承诺：不经 shell 跑命令、不把下载管进 sh、不跑安装脚本、不碰密码、窗口不能打开网址、只连白名单主机、不跟随跳转、响应上限 8 MiB、只写自己的两个文件 | `docs/what-we-run.md:44-60, 1954-1962, 1969-1974, 2021-2023, 2066-2070` |
| 9 | backlog 点名阶段 5 前要修：brew 搜索搜不到 jq、类型会猜错；含空格的搜索词被拒 | `docs/superpowers/backlog.md:366-369` |

## 一、小白看到什么

### 1.1 空的 Mac：打开就是「发现」

```
┌──────────────┬──────────────────────────────────────────────────────────────────┐
│ ○ 概览       │  发现                                                            │
│ ○ 更新       │                                                                  │
│ ○ 已安装     │     这台 Mac 上还没有命令行工具，从这里开始                      │
│ ● 发现       │                                                                  │
│ ○ 来源不明   │  ┌────────────────────────────────────────────────────────────┐  │
│              │  │ 第一步  [图标] Homebrew                                    │  │
│              │  │ Mac 上最常用的软件安装器，下面的工具都靠它安装和更新       │  │
│              │  │ 要在「终端」里安装，需要这台 Mac 的登录密码                │  │
│              │  │                                         [ 安装 Homebrew ]  │  │
│              │  └────────────────────────────────────────────────────────────┘  │
│              │                                                                  │
│              │  第二步  装好 Homebrew 之后，一键安装                            │
│              │  AI 助手                                                         │
│              │  [图标] Claude Code  在终端里写代码的 AI 助手   需先装 Homebrew  │
│              │  [图标] Codex        OpenAI 的终端编程助手      需先装 Homebrew  │
│              │  本地模型                                                        │
│              │  [图标] Ollama       在这台 Mac 上运行大模型    需先装 Homebrew  │
│              │  常用工具                                                        │
│ ≡ 设置       │  [图标] Node.js      很多 AI 工具要用它         需先装 Homebrew  │
└──────────────┴──────────────────────────────────────────────────────────────────┘
```

- 正门的条件不用 `detect === Missing`（第零节第 2 条让它几乎不成立），改为：没有 Homebrew，且除系统 Python 的 pip 以外什么都没列出。精确规则在步骤 C 定。其它没有 Homebrew 的 Mac，发现页顶部同样有这张第一步卡，只是启动时不自动跳到这一页。
- 第二步的条目灰着，写「需先装 Homebrew」；macOS 14 以下的 Ollama 写「需要 macOS 14」（cask 的 `depends_on`）。
- 「更新」「已安装」两页在空 Mac 上各一句话，加一个「去发现页」。
- macOS 15 以下照实多一句：Homebrew 官方已不支持这些版本，装得上但可能出问题（`Homebrew/install@0a396a4` 的 install.sh:267、:618-640）。Intel Mac 更进一步：官方脚本直接拒绝（:169-173），pkg 也只装 Apple Silicon，所以第一步卡在 Intel 上只写「Homebrew 已不支持这台 Mac」，没有按钮。Canager 自己仍是 universal、最低 13.3（spec §0、§2），管 Intel 上已有的 Homebrew 不受影响。

### 1.2 点「安装 Homebrew」

```
┌─ 安装 Homebrew ─────────────────────────────────────────────────────┐
│                                                                    │
│  Homebrew 要在「终端」里安装，Canager 不运行它，也不经手密码。     │
│                                                                    │
│  1  点「打开终端」，安装命令会先拷贝好                             │
│  2  在终端里按 ⌘V，再按回车                                        │
│  3  输入这台 Mac 的登录密码，按回车；输入时不显示字符              │
│  4  出现 Press RETURN 时再按一次回车，等它装完，别关终端           │
│                                                                    │
│  › 这条命令会做什么          › 查看命令原文                        │
│                                                                    │
│  正在等 Homebrew 装好…                  [ 取消 ]  [ 打开终端 ]     │
└────────────────────────────────────────────────────────────────────┘
```

- 「这条命令会做什么」展开是 §三.1 的五行白话；「查看命令原文」是 brew.sh 上那一行，与放进剪贴板的逐字相同；打开「显示技术细节」时默认展开。
- 表开着时，Canager 每 2 秒查一次 `/opt/homebrew/bin/brew` 在不在（只查文件，不运行）。brew 出现、且 Homebrew 的更新锁空着（脚本最后那次 `brew update` 持这把锁，本机 Homebrew 源码 `cmd/update.sh:680`；探测已有，`adapters/brew/mod.rs:1210`）才算装好：表变成「Homebrew 已装好」和「继续」，刷新，第一步打勾，第二步的按钮亮起。表关了也没关系，下一次「重新检查」同样会发现它。
- Canager 的 PATH 是启动时读的，那时还没有 Homebrew。装好后要把 `<prefix>/bin` 补进 Canager 自己的 PATH，否则之后用 Homebrew 装的 Node.js 带来的 npm 在重启 Canager 之前不出现：npm 按 PATH 找（`what-we-run.md:82-84`），它自己也要从 PATH 上找到 node。

### 1.3 已经有工具的 Mac

```
┌──────────────┬──────────────────────────────────────────────────────────────────┐
│ ○ 概览       │  发现                            [ 搜索 Homebrew 里的软件    ]   │
│ ○ 更新     3 │                                                                  │
│ ○ 已安装  42 │  AI 助手                                                         │
│ ● 发现       │  [图标] Claude Code  在终端里写代码的 AI 助手        已安装      │
│ ○ 来源不明 2 │  [图标] Codex        OpenAI 的终端编程助手          [ 安装 ]     │
│              │  [图标] Gemini CLI   Google 的终端 AI 助手          [ 安装 ]     │
│              │  本地模型                                                        │
│              │  [图标] Ollama       在这台 Mac 上运行大模型        [ 安装 ]     │
│              │  常用工具                                                        │
│              │  [图标] Node.js      很多 AI 工具要用它              已安装      │
│              │  [图标] uv           安装 Python 写的工具           [ 安装 ]     │
│ ≡ 设置       │  [图标] ffmpeg       转换、剪切音频和视频    ▓▓▓▓░░ 正在安装     │
└──────────────┴──────────────────────────────────────────────────────────────────┘
```

- 概览不变（大圆环加一个主按钮），不在概览里推销发现页；侧栏多一项「发现」，位置按 spec §7。
- 每条的状态由快照算。认得这件工具的任何一种装法就写「已安装」、不给按钮：阶段 4 的「输入 claude 时运行的是另一份」提醒，就是两份并存惹的麻烦，发现页不该再造一份。
- 行内进度、完成打勾，与更新页同一套；装好后这一行多一句「在新的终端窗口里输入 claude」和「拷贝」。

### 1.4 点「安装」

```
┌─ 安装 Gemini CLI？ ─────────────────────────────────────────┐
│                                                            │
│  [图标] Gemini CLI   Google 的终端 AI 助手                 │
│  来自 Homebrew，下载后由 Homebrew 核对校验值               │
│  会一起装：Node.js 等 25 个组件                            │
│  装好后在终端里输入 gemini 使用                            │
│                                                            │
│  › 查看将执行的命令                                        │
│                                      [ 取消 ]  [ 安装 ]    │
└────────────────────────────────────────────────────────────┘
```

- 沿用更新、卸载用的那种确认表，命令部分就是现有的 `CommandPreview`：一句来源、一句会一起装什么、一句怎么用，命令收在「查看将执行的命令」里。
- 「会一起装」要连「会一起升级」一起说：Homebrew 装新包时可能顺带升级已装的依赖。

## 二、第一批精选（9 条）

清单每条只写三样：用哪个来源装哪个名字；怎么认出已经装了（所有已知装法）；三句中英文案（是什么、你需要它吗、怎么用）。不写命令（spec §6：精选清单只含 `adapter_id + key`），命令由适配器生成，预览里的就是要跑的。下表 `<brew>` 是探测到的 brew 绝对路径，每条 brew 命令都带 `what-we-run.md:355-362` 的五个环境变量；版本、依赖与校验值取自本机 Homebrew 7.0.6 的索引（`brew info --json=v2`、`brew deps`，2026-09-29）。

| 条目 | 为什么收 | 从哪来 | 点「安装」后运行 |
|---|---|---|---|
| Homebrew | spec §1 点名；其余条目都靠它安装和更新；Canager 管得最全的来源 | brew.sh 的官方命令，脚本在 GitHub `Homebrew/install` | Canager 只拷贝命令、运行 `/usr/bin/open -a Terminal`，由用户在终端里跑（§三.1、Q1） |
| Claude Code | spec §1 点名；阶段 4 已认得它的三种装法 | cask `claude-code`（稳定通道；下载 downloads.claude.ai 上同一个二进制，cask 记着 sha256） | `<brew> install --cask claude-code` |
| Codex | spec §7「AI 助手」一类点名；OpenAI 出品 | cask `codex`（GitHub `openai/codex` 的发布包，带 sha256） | `<brew> install --cask codex` |
| Gemini CLI | spec §7「AI 助手」一类点名；Google 出品 | formula `gemini-cli`（依赖 Node.js，空 Homebrew 上连带 25 个） | `<brew> install --formula gemini-cli` |
| Ollama | spec §1 点名；Canager 已经能管模型 | cask `ollama-app`（GitHub 发布包，带 sha256；要 macOS 14；App 自己更新） | `<brew> install --cask ollama-app`，之后用现有的「打开 Ollama」 |
| 推荐模型 | 装了 Ollama 没有模型等于没装 | registry.ollama.ai；按本机内存挑一个，名字与大小在步骤 D 核 | `<ollama> pull <模型>`（现有计划，`what-we-run.md:923-929`） |
| Node.js | npm 上的工具都靠它运行；装好后 npm 来源自己出现 | formula `node`（直接依赖 19 个，连带 24 个） | `<brew> install --formula node` |
| uv | Python 写的工具该用它装：Canager 对 pip 只读 | formula `uv`（无依赖） | `<brew> install --formula uv` |
| ffmpeg | 转换、剪切音频和视频，用途不限于写代码 | formula `ffmpeg`（直接依赖 11 个，连带 14 个） | `<brew> install --formula ffmpeg` |

- 「怎么认」要列全，例如 Claude Code = cask `claude-code` 与 `claude-code@latest`、`standalone-claude`、npm `@anthropic-ai/claude-code`；Ollama = cask `ollama-app` 与 `ollama-binary`、formula `ollama`。阶段 4 规格 §十一 说过，这张表就是「装了 2 份」合并计数要的那份对照，只建一次。
- 第二批候选：Antigravity CLI（cask `antigravity-cli`）、Grok Build（cask `grok-build`）、git（pkg 路线不带它，见 §三.1）。

## 三、信任与安全

### 3.1 装 Homebrew：网上的脚本加管理员密码

官方命令（`Homebrew/install` 的 README.md:6）：

    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

按 `Homebrew/install@0a396a4` 的 install.sh，它会：

1. 不是 Apple Silicon 就停（:169-173）；拒绝以 root 运行（:432）；要能用 `sudo`（macOS 上即管理员账户），在终端里要密码，权限不够就停（:590）。
2. 列出要装的东西，等用户按回车（:807-808）。
3. 没有 Xcode 命令行工具就 `sudo softwareupdate -i` 装上（:900），不行再弹系统的 `xcode-select --install`（:911）。
4. 从 GitHub 取 Homebrew 放进 `/opt/homebrew`，写 `/etc/paths.d/homebrew` 让新终端找得到 brew（:1082）。
5. 跑 `brew update --force --quiet`（:1090）；装完 Homebrew 默认开着匿名统计（:1106）。

- 这些都在用户自己的终端里，由 sudo 要密码。从按下按钮到装好，Canager 只做三件事：把命令原文放进剪贴板；运行 `/usr/bin/open -a Terminal`（同 `open -a Ollama`，只在按按钮时，`what-we-run.md:938-944`）；查 brew 在不在、更新锁空不空。信任文件里「不跑安装脚本」「不碰密码」两条保持为真。
- 核不了的照实说：命令取的是 GitHub 上 `HEAD` 的脚本，没有校验值可比。上面五件事是按某一次提交读的，每次 Canager 发版前重读一遍（发布清单加一项）；界面只写这五件事。
- 匿名统计：Canager 不替用户改 Homebrew 的设置，只在「这条命令会做什么」里说一句；要不要给开关，规格阶段再定。
- 官方 pkg（Q1 的 b）能核得更多，限制也都已核。签名与公证：Homebrew 的 `release.yml` 用 `pkgbuild`/`productbuild --sign` 签名、`notarytool` 公证（`7.0.7` 标签 :181-202、:357）；Canager 可把版本、sha256 与签名的 Team ID 钉在代码里，下载后逐项比对，再交给系统安装器，密码由 macOS 要。限制：只装 Apple Silicon、只装 macOS 15 以上（`package/Distribution.xml:4-8`）；不带 Xcode 命令行工具（`package/scripts/postinstall:8`、`:139-153`），之后第一次 `brew update` 会绕开那个弹窗的桩、先自己 `brew install git`（本机 Homebrew 源码 `shims/shared/git:43-64`、`cmd/update.sh:607-617`），即一次刷新会多装一个包；下载地址 302 跳到 `release-assets.githubusercontent.com`，文件 149,550,332 字节，而今天的 HTTP 客户端不跟随跳转、上限 8 MiB，要另开一条下载路径；Homebrew 自己只把 pkg 推荐给 Apple Silicon 上的 MDM 批量部署（README.md:11）。

### 3.2 执行前看清会运行什么

- 清单不写命令；确认表里「查看将执行的命令」就是将运行的那一条，连同环境变量。
- 「会一起装、会一起升级」的读法：本机 Homebrew 7.0.6 的 `brew install` 有 `--dry-run`（「Show what would be installed」），它是否完全只读**未核**，S0 录 fixture 核；不行就用 `brew deps` 减去已装的，只说「会一起装」，不说升级。
- 需要密码的 cask 不进清单：第一批三个 cask 都没有 pkg 安装器（本机 `brew info` 的 `artifacts`）；`SUDO_ASKPASS` 透传仍等作者拍板（`backlog.md:33`）。
- 装完照旧重读：没装上报「需要留意」，不信退出码（`ops/mod.rs:705-711`）。

### 3.3 下载校验

- Homebrew 装的条目由 Homebrew 按索引里的 sha256 核对下载（本机源码 `downloadable.rb:91-99`）。清单规则：不收 `sha256 :no_check` 的 cask；第一批三个 cask 本机已核都带 sha256；清单测试在 CI 的 macOS runner 上对每条跑一次 `brew info --json=v2` 查这一项。
- 模型按层的 sha256 digest 寻址；Canager 查更新比的就是这些 digest（`what-we-run.md:914-921`）。
- Homebrew 本身：终端路线只有 HTTPS；pkg 路线见 §三.1。
- Canager 自己签名并公证（已定）；清单编译在签过名的应用里，改清单就破签名（Q5）。

### 3.4 安装入口只收清单里的东西

第零节第 6 条的口子在阶段 5 堵上：`plan_operation` 拒收 `Install`；新命令 `plan_install(catalog_id)` 由 Rust 查清单得出来源与名字；搜索结果也只发会话内的不透明 id（同 `PlanId` 的做法，`session/plans.rs:27-41`）。窗口永远不能自己报一个包名让 Canager 装。

### 3.5 `what-we-run.md` 要改的地方

- 新增「发现」一节：清单在应用里，读它不联网、不跑命令；安装跑的就是各来源写命令表里已有的命令；新增的只读读取（`--dry-run` 或 `brew deps`）。
- 新增「Homebrew 本身」一节：Canager 不运行安装脚本；按钮做的三件事；怎么判断装好；补 PATH 的做法。若补法是再读一次登录 shell，:50-60「启动时只跑一次 shell」要改成两次；直接补 `<prefix>/bin` 则不用改。
- 「What Canager never does」加一条：只安装清单与本次搜索结果里的东西。
- pip 一节：若 S0 证实会弹框，写明没有命令行工具时不运行 `/usr/bin/python3`。
- 只有 Q1 选 b 才改：Network 节加两个主机与唯一一次跳转；「Files Canager writes」加下载的 pkg 与何时删除；加 `pkgutil --check-signature`、`spctl -a -t install`、`open <pkg>` 三条命令。

## 四、请作者拍板

| # | 问题 | 推荐 | 代价 |
|---|---|---|---|
| Q1 | Homebrew 怎么装：(a) 拷贝官方命令、打开终端，用户自己粘贴并输入密码；(b) Canager 下载官方 pkg、核对后交给系统安装器；(c) Canager 在后台跑官方脚本、弹自己的密码框 | **先做 (a)**，Apple Silicon 都能用（macOS 15 以下有 Homebrew 自己的警告）；S0 通过后，在 macOS 15 以上把 (b) 做成主按钮，(a) 作 14 及以下与失败时的退路；不做 (c) | (a) 小白要自己粘贴、输入看不见的密码、按回车，出错时 Canager 只知道「还没装好」。(b) 一键、密码交给系统，但信任文件要新开四个口子（新主机、一次跳转、写一个 150 MB 的文件、拉起安装器），只覆盖 15 以上，不带命令行工具。(c) 违背「不跑安装脚本」「不碰密码」两条，askpass spike 至今没做完（`docs/spikes/2026-09-askpass.md`）。三条在 Intel Mac 上都走不通：Homebrew 官方已不装 Intel（§1.1） |
| Q2 | AI 助手（Claude Code、Codex、Gemini CLI）从哪装 | **Homebrew** | 一种机制、预览完整、Homebrew 核校验值、更新走 Canager 的更新页；但 Homebrew 版不会自己更新，而 Anthropic 文档写明原生安装会在后台自动更新（阶段 4 调研记录 `claude.md` §2b、§5，文档原文，未入库）。另一条路是像 Q1(a) 那样把厂商脚本交给终端：每个工具一套说明，Canager 看不到过程；但 Intel 空 Mac 上只剩这条路（前提是厂商还出 Intel 版） |
| Q3 | 搜索开放到哪 | **只搜 Homebrew**，先修 backlog 那两条 | 比 spec §0 的「brew、npm 都能搜」窄。npm 上名字相近的仿冒包多，小白分不清官方包名；Homebrew 的包有人审、带校验值 |
| Q4 | 首批多少条 | **先上 §二 的 9 条**，之后每条一个 PR | 少于 spec §13 的 ≥ 30 条；但每条要三句双语文案和一次真机装卸记录，30 条会拖住首发，也要等 CI 额度 |
| Q5 | 清单怎么更新 | **编译进应用，随版本更新** | 新条目要等发版；换来不加主机、清单跟应用一起签名，没人能远程改「一键装什么」。联网清单更新快，但要新主机、清单签名与离线回退 |

## 五、分步（沿用阶段 4 的做法）

每步全绿、可独立合并、有用户看得到的读取方，只带本步有生产者的字段；`what-we-run.md` 的小节随该步合并；每步实现 → 对抗评审 → 逐条核实，再下一步。A → B → C 串行；D、E 在 B 之后可并行（各用独立 worktree）；F 等 Q1 与 S0。

| 步 | 内容 | 用户看得到 | 依赖 |
|---|---|---|---|
| **S0** | 核实，不合代码，也不在作者本机装任何东西。干净的 macOS 虚拟机（Apple Silicon，15 以上与 14 各一台）：① 终端走一遍官方命令，记下每个提示并截图，给 §1.2 的文案用；② 空机器上 Canager 刷新一次，看 `/usr/bin/python3` 会不会弹框；③ 下载 Homebrew.pkg，记签名的 Team ID 与 `spctl -a -t install` 的结果，装好后不装命令行工具，看 `brew update`、`brew install node`、`brew install --cask claude-code` 的表现；④ `brew install --dry-run` 是否只读，录 fixture。结果写进 `docs/spikes/` | 无（给 Q1、C、F 用的事实） | 无 |
| **A** | 清单与发现页（只读）：`catalog/` 数据与双语文案，编译进应用；清单测试（来源已注册、名字过 `validate_package_name`、双语齐全、每条有「怎么认」）；Rust 按快照算每条状态（已安装、可安装、需先装 Homebrew、需要 macOS 14）；IPC `get_catalog`；侧栏「发现」；`what-we-run.md`「发现」节 | 发现页：9 条、分类、哪些已经有了；还没有安装按钮 | 无 |
| **B** | 从清单安装：`plan_install(catalog_id)`；`plan_operation` 拒收 `Install`（加测试）；确认表 = 现有预览加「会一起装、会一起升级」；行内进度、完成态与「怎么用」 | 有 Homebrew 的 Mac 上一键装 6 条：Claude Code、Codex、Gemini CLI、Node.js、uv、ffmpeg | A；S0 ④ |
| **C** | 空机器正门与装 Homebrew（终端交接）：正门条件（§1.1）；打开即发现页；第一步卡与安装表；剪贴板、`open -a Terminal`、查路径与更新锁；装好后补 PATH 并刷新；macOS 15 以下的提醒；pip 探测按 S0 ② 改；`what-we-run.md`「Homebrew 本身」节。**合并前**：作者在干净虚拟机上亲手走一遍（中文清单，同阶段 4 步骤 C 的访达检查） | 空 Mac 从装 Homebrew 到装上第一个工具 | B；S0 ①② |
| **D** | Ollama 与模型：`ollama-app`（macOS 14 以下没有按钮），装好接现有「打开 Ollama」；模型走现有 `ollama pull` 计划，Ollama 没在运行时按钮换成「先打开 Ollama」（现有闸门本就会拒）；预览加模型大小（registry 清单各层 size 之和，主机已在白名单）与剩余空间 | 装 Ollama、下载一个模型 | B |
| **E** | 搜索（范围按 Q3）：先修 backlog 那两条；IPC `search`，结果只给不透明 id，安装走 B 的路；结果行标「不在精选里」 | 发现页顶部的搜索框 | B |
| **F** | 仅当 Q1 选 (b) 且 S0 ③ 通过：pkg 下载（流式写盘、只跟随一次到固定主机的跳转、比对钉住的 sha256 与 Team ID），再 `open` 交给系统安装器；macOS 15 以上用它，其余仍走 C | 不用终端也能装 Homebrew | C；S0 ③ |
| **G** | 清单扩到 Q4 定的条数：每条一个 PR，三句双语文案和一次真机装卸记录（CI runner 或虚拟机，Actions 额度 10/1 恢复后） | 更多条目 | B |

## 六、明确不在本阶段

- Canager 自己运行安装脚本或 `curl | sh`，或代收、转交密码。
- 需要密码的 cask（等 `SUDO_ASKPASS` 拍板）；用 pip 安装（已定只读）。
- 一次勾选多条连装；装完 Homebrew 自动接着装别的。
- 历史页、菜单栏（阶段 6）；远程清单（Q5）、npm 搜索（Q3），视拍板而定。
