# Canager

**A Mac app for everything you installed from the terminal and then forgot about.**

If you've followed a few tutorials, you probably have Homebrew formulae, a couple of global npm
packages, some Python tools, a Rust binary and an Ollama model or two scattered across your Mac.
Each was installed with a different command. Updating them needs a different command again.
Removing them needs a third. Most people never do either, and the tools quietly rot.

Canager puts all of it in one window: what you have, what has an update, and a button for each.

> **Status: pre-release.** The core and the UI work and are covered by 889 Rust tests (plus 4 more
> that touch a real Homebrew, the real Trash or AppKit and only run with `--ignored`) and 858
> front-end tests, but there is no downloadable build yet — v0.1 is being prepared. Nothing here is
> ready to rely on.

<!-- A screenshot belongs here before the first release. -->

## What it manages

| Source | Reads | Installs / updates / removes |
|---|---|---|
| Homebrew — formulae and casks | yes | yes |
| npm — global packages | yes | yes, when the prefix is yours to write |
| pipx | yes | yes |
| uv — tools | yes | yes |
| pip | yes | **no** — Canager will not drive pip's installer; it points you at pipx or uv |
| cargo | yes | yes, with a warning that it compiles locally |
| Ollama — models | yes | yes |
| Claude Code — the native install, via its own installer | yes | updates yes; install no (the installer is Anthropic's, and Canager never runs it); uninstall yes — its program files, download cache and launcher go to the Trash, and your settings and history stay |
| rustup — the Rust toolchain manager, via its own installer | yes | updates yes (`rustup self update`); install no (the installer is rust-lang's, and Canager never runs it); uninstall yes (`rustup self uninstall -y`), offered only when Rust is in its standard folders (`~/.cargo`, `~/.rustup`) and previewed with everything it removes — permanently, not to the Trash: every toolchain by name, the whole Cargo folder with its settings and saved login, and the programs in its `bin` folder, named where known. Neither can be cancelled once it is running, and the preview says so |
| Antigravity CLI (`agy`) — Google's terminal agent, via its own installer | yes | updates **no** — it installs its updates itself in the background and its own `agy update` is undocumented, so a newer version is listed under "Can't update here" with an "Updates itself" chip, whose detail says to open the tool once (Canager looks the newer version up on Apple silicon only: on an Intel Mac the row reads "Can't check" and nothing is sent); install no (the installer is Google's, and Canager never runs it); uninstall yes — the `agy` program, and any `agy.<time>.old` backup its updater left beside it, go to the Trash; its conversations, history and working files in `~/.gemini/antigravity-cli` stay, and so do its staging folder in `~/.cache` and the `PATH` lines its installer added |
| Grok Build (`grok`) — xAI's terminal agent, via its own installer | yes | updates yes (`grok update`, offered when grok's own `update --check --json` says a newer version exists; how `grok update` behaves when nothing can answer a prompt is yet to be recorded on CI); install no (the installer is xAI's, and Canager never runs it); uninstall yes — its downloaded versions, its bundled agents and shell completions, any fallback links its installer made in `~/.local/bin`, and the two links in its `bin` folder go to the Trash (the folder itself, which its installer put on your `PATH`, stays); `~/.grok`'s settings, login, sessions and memory stay |

Programs that none of these sources installed — a tool's own installer dropped a binary into
`~/.local/bin`, an app put a helper into `/usr/local/bin`, a link whose target is gone — are
listed, read-only, on the **Unknown** page. Canager never runs, moves or deletes anything there;
`docs/what-we-run.md` says exactly what it reads. A program a source installed but reported no
path for is listed there too (uv's own `uvx`, for one): the gap is the source's, and the page
says what it sees. Cargo reports one program per crate — the one named after the crate, else the
first its record lists — so the other programs of a crate that installs several
(`cargo-binstall`'s `detect-targets`) stay on that page until it can report them all.

Canager checks every source when it opens, after each operation, and whenever you press **Check
again** in the header of the Overview, Updates and Installed pages, which also says how long ago the
last check finished, or choose **Check Again** (⌘R) in the menu bar's View menu, on any page; while
a check runs, neither starts another. The "Try again" of a failed refresh, or of a Homebrew index
Canager couldn't update, runs the same check, and a Homebrew index update left running in the
background starts one on its own when it ends (`ipc::refresh_on_background_change`,
`src-tauri/src/lib.rs:62-65`). The Unknown page's header has *Scan again* in its place, with how
long ago that page last scanned: it re-runs only that page's scan of your bin folders, against the
sources' last known state — it does not refresh the sources. Settings' header has neither.

Closing the window — its red button, or Close Window (⌘W) in the menu bar's File menu — leaves
Canager running, and an operation under way carries on; its icon in the Dock brings the window back
as you left it, without a new check. Quit Canager (⌘Q) quits it.

Adding a source is one Rust file implementing one trait, plus a TOML metadata file.

## What makes it safe to point at your machine

This app runs package managers on your behalf, so the boundary matters more than the features:

- **There is no shell.** Every command is built as an argument vector and handed to the OS
  directly. Nothing is ever concatenated into a string a shell would interpret.
- **The window cannot ask for a command.** The UI sends an operation kind and a single-use,
  expiring identifier for a plan the Rust side built itself. There is no general "run this" path,
  so a compromised web view cannot invent one.
- **You see the exact command before it runs.** Every update and uninstall lets you see the exact
  command before it runs, with the variables Canager sets for it — one press on "Show the command"
  in its confirmation, or open from the start with Settings' "Show technical details" on — and
  says whether it may ask for your password; an uninstall that runs no command lists instead the
  exact paths it will move to the Trash. An uninstall also says what it will affect. An update says
  so only when a Homebrew `brew.env` file turns Homebrew's periodic clean-up and its autoremove
  back on, since that clean-up, whenever it is due, then also uninstalls the packages that were
  installed only as dependencies and that nothing needs any more.
- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones,
  in your language. Canager runs Homebrew with its autoremove off, so a Homebrew uninstall does
  not also uninstall the other packages that were installed only as dependencies and that nothing
  needs any more; when a `brew.env` file turns autoremove back on, the preview says Homebrew will.
- **A tool with no uninstall command goes to the Trash, not away.** Claude Code's makers document
  its removal as a list of paths. Canager moves those paths, plus its installer's download cache,
  to the Trash itself, with the call Finder uses, so until you empty the Trash you can drag them
  back — and Finder's Put Back will likely work too; the preview lists each path it will move and
  each one it keeps (your settings and history, in `~/.claude` and `~/.claude.json`). Antigravity
  CLI and Grok Build publish no removal instructions at all, so their lists are Canager's own
  reading of how each was installed, and their paths go to the Trash the same way. Moving files to
  the Trash is the only change Canager makes to a file itself besides saving its own settings and
  its window's size and position;
  `docs/what-we-run.md` says how, and names every path each list moves or keeps and where it
  comes from.
- **Only the paths you were shown are moved.** Each path must be inside your home folder — never
  directly in it or in a folder other apps share, such as `~/.local` or `~/Library`, and with no
  folder that is a link between where your home folder really is and the path (the home folder
  itself may be reached through a link) — yours, what that tool's uninstall list describes, and
  clear of what it keeps. Canager remembers what each path was when you saw the preview; when you
  confirm, and again right before each path moves, it checks everything once more, and if anything
  differs it stops before moving that path, and the operation log lists anything it had already
  moved.

## What it deliberately does not do yet

Being honest about this is part of the point:

- **No search and no catalogue, and no way to install something new.** You can manage what you
  already have; you cannot yet discover or add new things through Canager.
- **macOS only.** The core crate is portable and the architecture is cross-platform, but
  everything below the trait boundary assumes Unix today, and only macOS is tested. Windows and
  Linux are roadmap, not "nearly working".

## Building from source

Needs Rust (stable), Node with pnpm, and Xcode's command line tools.

```bash
pnpm install
pnpm tauri dev
```

To look at the UI in an ordinary browser instead, with a mock backend in place of Tauri (for
screenshots; development only, never in a build), run `pnpm dev:mock` and open
<http://localhost:1430/> — [docs/ui-preview.md](docs/ui-preview.md) has the rest. `pnpm tauri:mock`
puts the same mock front end in the app's real window, title bar and all, and Canager runs no
command for it; the same page says why.

Tests — all five must pass before anything is committed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm test
pnpm typecheck
```

`pnpm typecheck` runs two TypeScript programs. `tsconfig.json` checks the production code under `src/`
with no ambient Node types, so `process`, `Buffer` or a `node:` import in code that will run inside the
WebView is a type error; `tsconfig.test.json` checks the vitest files with `@types/node`, which
`src/i18n/completeness.test.ts` and `src/i18n/no-literal-strings.test.ts` need to read the source tree
through `node:fs`. `pnpm build` runs the same two programs before `vite build`.

`cargo test --workspace` has four `#[ignore]`d tests, all skipped by a plain `cargo test`. Two are
in `crates/canager-core/tests/brew_live.rs`: one only reads the real Homebrew on the machine
running it, the other installs and removes the `hello` formula. The third, in
`crates/canager-core/tests/standalone_uninstall_test.rs`, moves five throwaway items it creates
(named `canager-trash-smoke-…`) into the real Trash of the Mac running it and leaves them there.
The fourth, in `crates/canager-core/src/icon/real.rs`, has AppKit draw Calculator's icon and
only reads. The two that change the machine refuse to touch anything without `CANAGER_LIVE=1`. CI
runs the first three; run them yourself with:

```bash
CANAGER_LIVE=1 cargo test -p canager-core --test brew_live -- --ignored
CANAGER_LIVE=1 cargo test -p canager-core --test standalone_uninstall_test -- --ignored
cargo test -p canager-core --lib icon::real -- --ignored
```

## Language

English by default, with a full Simplified Chinese translation. Every label, heading, button and
message frame in the window goes through i18n, and a test keeps the two locales in step — a sentence
a Chinese user cannot read is treated as a bug. The menu bar follows the window's language, Settings'
choice included. Its words are Rust's (`src-tauri/src/menu.rs`), macOS's own for the items every Mac
app has, and a test there keeps its two languages in step too.

Rust's refusals are translated too, not just the frames around them. A plan built against a source
that is read-only, unavailable or gone, an operation Canager can't prepare (a name it won't pass to
a tool, a program that has gone missing, a path on an uninstall list that is outside your home
folder, in a folder other apps share, missing, not yours or not what that list describes), a
preview that has expired or already been used, a settings change it couldn't save, an operation
Canager itself couldn't carry out (the program was removed between the check and the run, say, or a
path changed between the preview and the click), Canager's own remarks in the operation log
(waiting for Homebrew to finish updating, a stream it could no longer read, each item it moved to
the Trash) and its verdicts on a result (the command said it worked but the package isn't there)
each arrive as a small structured payload the front end renders in the user's language.

Three kinds of text are shown as-is:

- **Another program's own words.** Every line `brew` or `npm` prints in the operation log, and
  the last lines of its stderr when an operation fails; the reason macOS gives when it can't start
  a tool, whether Canager is preparing an operation or running one, can't save Canager's settings
  for a cause Canager doesn't recognise, or refuses to move an item to the Trash.
  That is another program's text, and there is no way to translate it. Outside the log it is quoted
  inside a sentence in your language that says what happened.
- **The app framework's own error**, in the one case where the window can't get an answer from the
  rest of Canager at all while loading or refreshing the list — its own text is shown untranslated,
  next to the retry button. Short of that, Canager itself never fails a refresh as a whole, but not
  every source with trouble gets a notice of its own. A source that has gone unavailable to Canager (not
  running, unreachable, or refusing to run as root) is reported in your language, through its own
  notice. A source that Canager could still reach, but whose software list or update check failed,
  is not: it only adds to the count in the "Some checks didn't finish" banner, which never says
  which source it was.
- **A number of technical details that are still Canager's own**, which appear in English inside an
  otherwise translated sentence. This is a known gap, not a design choice, and it is not just the
  one case the wording used to name: with "Show technical details" turned on, whenever a package
  can't be checked for updates Canager's own explanation of why is shown as plain English rather
  than translated — with the switch off you see only a short generic sentence instead. There are
  more than a dozen such explanations: a generic one like "npm outdated -g exited with code 1" (or
  "... did not finish", or the tool's own first line of stderr) from any lookup that runs a
  command, Grok Build's own update check among them (which has a few more of its own: a check
  Canager could not run, an answer that is not grok's JSON, or an error grok itself reported); from
  the six lookups Canager makes over HTTP instead of a command line, that request's own wording —
  pipx's PyPI lookup ("PyPI request failed: ...", "PyPI returned status 503", "could not parse PyPI
  response: ..."), Cargo's equivalent for crates.io, Ollama's for its own registry, Claude Code's
  for its release channel, rustup's for its release file, and Antigravity CLI's for its manifest
  (or, on an Intel Mac, why it made no request); and the two about the installed version: "cannot
  read the installed version now", from the code Claude Code, Antigravity CLI, Grok Build and
  rustup share, and "cannot compare the installed version ... with the published ...", from Claude
  Code, Antigravity CLI and rustup only, since Grok Build's check takes grok's own answer and
  compares no versions. They should all become structured payloads like the refusals above, and
  until they do, what a Chinese user sees there with the switch on is in English.

## Design notes

The design and the reasoning behind it live in [`docs/superpowers/`](docs/superpowers/) — the
spec, the implementation plans, and the review findings that changed them. They are working
documents rather than polished writing, but they record why things are the way they are.

## Logos

The logos Canager shows for tools and sources are trademarks of their owners, shown only to
identify the tool or source each stands for; a tool may show its maker's logo in place of one of
its own. Those drawn in white or near-black on their brand's colour come from
[Simple Icons](https://simpleicons.org/), which is released under CC0 — though, as Simple Icons
says, not every icon in it is: an icon under a license of its own has that license named in
Simple Icons' data. Such a logo keeps its license. Canager ships it unmodified, its path exactly
as Simple Icons has it, and credits it in Settings, under About → Icon credits, with its license
and the addresses of the license's text and of the page Simple Icons took the logo from. The
logos not from Simple Icons are the GitHub avatar of the organization or account behind the
project, or behind its maker. All of them are built into the app, from `src/assets/tool-icons/`,
and showing one makes no network request. `pnpm icons:build` regenerates that folder from
`scripts/tool-icons/mapping.json`, taking Simple Icons' logos from the pinned `simple-icons`
package and downloading the avatars from GitHub. It fails if the mapping names a logo under a
license Canager does not ship — it ships CC0-1.0, MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
ISC, CC-BY and CC-BY-SA, and no other: nothing noncommercial, no-derivatives, GPL-family or
custom — and if the folder comes to more than 5 MB, the limit a test holds it to as well. Neither
the app nor the tests run it.

## Descriptions

Under a tool's name, its row says in one line what the tool is: the description the tool's source
gives it, such as Homebrew's for a formula or a cask; where the source gives none, what kind of
thing that source lists ("npm package"); and for a tool with its own installer, a line of Canager's
own, in both languages. In Chinese, a row says a line in Chinese instead wherever Canager has one:
about 2,000 of them, for Homebrew's formulae and casks and for npm, PyPI and crates.io packages,
each translated from the description the tool's own source gives it. They are built into the app,
in `src/assets/tool-descriptions/zh-CN.json`, which is read only once the window is in Chinese, and
fetched from nowhere: showing one makes no network request. A tool's details show the Chinese line
with its source's own description under it, so nothing the source said is lost; a tool Canager has
no Chinese line for reads as it did before.

## License

Not chosen yet. Until a license file is added this repository is "all rights reserved" by
default, so please don't build on it yet — and I can't accept contributions until it's settled.

---

## 中文

**把你在终端里装过、然后忘掉的东西管起来。**

跟着几篇教程走下来，Mac 上多半散落着一些 Homebrew 软件、几个全局 npm 包、几个 Python 工具、
一个 Rust 编译出来的命令，还有一两个 Ollama 模型。每样都是用不同的命令装的，更新要换一条命令，
卸载又要换一条。大多数人两件都不做，这些东西就在那儿慢慢烂掉。

Canager 把它们放进同一个窗口：装了什么、哪个有更新、每个都配一个按钮。

每次更新和卸载，都能在它运行之前看到确切的命令，连同 Canager 为它设的环境变量：在确认框里点「查看将执行的命令」，或者在设置里打开「显示技术细节」，
让它一开始就展开；可能要输入 Mac 密码的，确认框也会先说。不运行命令的卸载，改为列出它要移到废纸篓的每一条路径。

**目前处于发布前阶段**，核心与界面已经可用、有 889 个 Rust 测试（另有 4 个要连着真实的
Homebrew、真实的废纸篓或 AppKit 才跑，平时是跳过的）和 858 个前端测试，但还没有可下载的版本，v0.1 正在
准备。现在还不适合依赖它。

界面默认英文，内置完整简体中文。窗口里所有标签、标题、按钮和提示框都走 i18n，两种语言由测试保证同步——
中文用户读不懂的句子算 bug。菜单栏跟着窗口的语言走，设置里选的语言也算。它的文字写在 Rust 里
（`src-tauri/src/menu.rs`），每个 Mac 应用都有的菜单项用 macOS 自己的叫法，那里也有测试保证两种语言同步。

Rust 侧返回的拒绝理由也会翻译，不只是外面那层框。操作所针对的来源只读、连不上或已不存在，操作无法
准备（某个名字 Canager 不肯交给工具、某个程序不见了、卸载清单上的某条路径不在你的个人文件夹里、
放在其它应用共用的文件夹里、不存在、不属于你或者和说明写的不一样），预览已过期或已用过，设置没能保存，
操作因为 Canager 自己这边的原因没能执行（比如程序在检查之后、运行之前被删掉了，或者某条路径在预览之后、
点击之前变了），Canager 自己在操作日志里说的话（等待 Homebrew 更新完毕、某个输出流读不下去了、
把哪一项移到了废纸篓），以及它对结果的判断（命令说成功了，但那个包并不在），
都以一个结构化的小数据传到前端，用你选的语言显示。

有三类文字会原样显示：

- **其他程序自己的话。** brew、npm 在操作日志里打印的每一行，操作失败时它 stderr 的最后
  几行；以及 macOS 无法启动某个工具（不论 Canager 是在准备操作还是在执行操作）、
  或因为 Canager 不认识的原因无法保存设置、或拒绝把某一项移到废纸篓时给出的原因。那是另一个程序自己的文字，没法翻译。日志之外，它会被引用在一句用你的语言说明发生了什么的话里。
- **应用框架自己的报错**，只出现在一种情况：加载或刷新列表时，窗口完全联系不上 Canager 的其余部分——这时它
  自己的文字会原样显示在重试按钮旁边。除此之外，Canager 自己从不会让整次刷新失败，但不是每个出问题的
  来源都有自己的提示。一个来源如果对 Canager 而言已经不可用了（没在运行、连不上、或者因为以 root 身份
  运行而被拒绝），会用你的语言、通过它自己的提示告诉你；一个来源如果本身能联系上，只是软件列表或更新
  检查失败了，就没有自己的提示——只会计入“部分检查没完成”横幅里的数字，横幅不会说是哪个来源。
- **还有几处技术细节仍属于 Canager 自己**，会以英文出现在一句已翻译的话里。这是已知的缺口，不是有意
  为之，而且不只是以前说的那一处：打开“显示技术细节”后，只要某个包没法检查更新，Canager 自己给出的
  原因就会原样显示成英文，而不是翻译过的句子——关掉开关时，看到的只是一句简短的通用提示。这样的原因
  有十几处：一类是像“npm outdated -g exited with code 1”这样的通用提示（也可能是“... did not
  finish”，或者工具自己 stderr 的第一行），出自任何要跑命令去检查更新的来源，Grok Build 用它自己的命令检查更新也在其中
  （它还另有几句：Canager 没能运行这个检查、回答不是 grok 该给的 JSON，或者 grok 自己报了错）；另一类来自另外六个改用 HTTP 直接查询的来源——
  pipx 查 PyPI、Cargo 查 crates.io、Ollama 查它自己的软件源、Claude Code 查它的发布通道、rustup 查它的发布文件、
  Antigravity CLI 查它的版本清单（在 Intel Mac 上则是它为什么没发请求）——各自请求失败、返回状态异常、
  解析失败时的原文提示；还有两句关于已安装版本的原文提示：读不到已安装版本，出自 Claude Code、
  Antigravity CLI、Grok Build 与 rustup 共用的代码；已安装版本与发布版本无法比较，只出自 Claude Code、
  Antigravity CLI 与 rustup，因为 Grok Build 的检查直接采信 grok 自己的回答，不比较版本。
  这些都应该像上面的拒绝理由一样改成结构化数据，在那之前，中文用户在开关打开时看到的，就是英文。

每个软件名下那一行简介，默认是它所在来源自己给的说明（比如 Homebrew 给 formula 和 cask 写的那句英文）；
来源没给的，写这个来源列出的是什么（“npm 软件包”）；自带安装器的工具，是 Canager 自己写的一句，
中英文都有。中文界面里，只要 Canager 有这个软件的中文说明，就改显示中文：约 2,000 条，涵盖 Homebrew 的
formula 与 cask，以及 npm、PyPI、crates.io 上的包，每条都译自该软件所在来源自己的说明。这些说明内置在应用里
（`src/assets/tool-descriptions/zh-CN.json`），界面是中文时才读取，不从任何地方下载，显示时不发任何网络请求。
软件详情里，中文说明下面用小字附上来源的原文，来源说过的话一句不丢；没有中文说明的软件，照旧显示原来那一行。

尚未支持：搜索与软件目录、安装新东西、macOS 以外的平台。

Canager 在打开时、每次操作完成后，以及你按下“概览”“更新”“已安装”三页页头的“重新检查”、或在任一页
从菜单栏选“显示”菜单里的“重新检查”（⌘R）时检查各来源，页头上也写着上次检查是多久以前；正在检查时，
再按也不会多查一遍。刷新失败或 Homebrew 的索引过期时出现的“重试”按钮做的是同一次检查；
后台运行的 Homebrew 索引更新自行结束时，它也会自己再查一遍（`ipc::refresh_on_background_change`，
`src-tauri/src/lib.rs:62-65`，不需要用户动手）。“来源不明”页的页头换成“重新扫描”和上次扫描是多久以前，
它只属于那一页：只重新扫描那一页看的几个 bin 文件夹，按各来源上次已知的状态判断——并不刷新各来源。
“设置”页的页头两者都没有。
（来源装了却没报路径的程序也会列在那一页，比如 uv 自带的 `uvx`：缺口在来源那边，页面照实说。Cargo
每个 crate 只报一个程序——与 crate 同名的那个，没有就报记录里的第一个——所以一个 crate 装了好几个程序时，
其余的（如 `cargo-binstall` 的 `detect-targets`）会留在那一页，直到它能把全部报出来。）

关掉窗口——点它的红色按钮，或从菜单栏选“文件”菜单里的“关闭窗口”（⌘W）——Canager 仍在运行，进行中的操作照常
继续；点程序坞里的图标，窗口按你离开时的样子回来，不会重新检查。选“退出 Canager”（⌘Q）才会退出。
