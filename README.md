# Canager

**A Mac app for everything you installed from the terminal and then forgot about.**

If you've followed a few tutorials, you probably have Homebrew formulae, a couple of global npm
packages, some Python tools, a Rust binary and an Ollama model or two scattered across your Mac.
Each was installed with a different command. Updating them needs a different command again.
Removing them needs a third. Most people never do either, and the tools quietly rot.

Canager puts all of it in one window: what you have, what has an update, and a button for each.

> **Status: pre-release.** The core and the UI work and are covered by 359 Rust tests (plus 2 more
> that touch a real Homebrew and only run with `--ignored`) and 238 front-end tests, but there is
> no downloadable build yet — v0.1 is being prepared. Nothing here is ready to rely on.

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

Adding a source is one Rust file implementing one trait, plus a TOML metadata file.

## What makes it safe to point at your machine

This app runs package managers on your behalf, so the boundary matters more than the features:

- **There is no shell.** Every command is built as an argument vector and handed to the OS
  directly. Nothing is ever concatenated into a string a shell would interpret.
- **The window cannot ask for a command.** The UI sends an operation kind and a single-use,
  expiring identifier for a plan the Rust side built itself. There is no general "run this" path,
  so a compromised web view cannot invent one.
- **You see the exact command before it runs.** Every update and uninstall shows its real argv
  and whether it needs your password. An uninstall also says what it will affect — an update
  never touches anything else, so it has nothing to report there.
- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones,
  in your language.

## What it deliberately does not do yet

Being honest about this is part of the point:

- **No search and no catalogue, and no way to install something new.** You can manage what you
  already have; you cannot yet discover or add new things through Canager.
- **No on-demand refresh.** Canager checks at launch and after each operation. The only "Try
  again" buttons appear when something already needs one — a failed refresh, or a Homebrew index
  Canager couldn't update — not as a standalone control you can press at any time.
- **macOS only.** The core crate is portable and the architecture is cross-platform, but
  everything below the trait boundary assumes Unix today, and only macOS is tested. Windows and
  Linux are roadmap, not "nearly working".

## Building from source

Needs Rust (stable), Node with pnpm, and Xcode's command line tools.

```bash
pnpm install
pnpm tauri dev
```

Tests — all five must pass before anything is committed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm test
pnpm exec tsc -p tsconfig.json
```

`cargo test --workspace` has two `#[ignore]`d tests in `crates/canager-core/tests/brew_live.rs`,
both skipped by a plain `cargo test`: one only reads the real Homebrew on the machine running it,
the other installs and removes the `hello` formula and refuses to touch anything without
`CANAGER_LIVE=1`. CI runs both; run them yourself with:

```bash
CANAGER_LIVE=1 cargo test -p canager-core --test brew_live -- --ignored
```

## Language

English by default, with a full Simplified Chinese translation. Every label, heading, button and
message frame goes through i18n, and a test keeps the two locales in step — a sentence a Chinese
user cannot read is treated as a bug.

Rust's refusals are translated too, not just the frames around them. A plan built against a source
that is read-only, unavailable or gone, an operation Canager can't prepare (a name it won't pass to
a tool, a program that has gone missing), a preview that has expired or already been used, a
settings change it couldn't save, an operation Canager itself couldn't carry out (the program was
removed between the check and the run, say), Canager's own remarks in the operation log (waiting for
Homebrew to finish updating, a stream it could no longer read) and its verdicts on a result (the
command said it worked but the package isn't there) each arrive as a small structured payload the
front end renders in the user's language.

Three kinds of text are shown as-is:

- **Another program's own words.** Every line `brew` or `npm` prints in the operation log, and
  the last lines of its stderr when an operation fails or when it objects while Canager is
  preparing one; the reason macOS gives when it can't start a tool, whether Canager is preparing an
  operation or running one, or can't save Canager's settings for a cause Canager doesn't recognise.
  That is another program's text, and there is no way to translate it. Outside the log it is quoted
  inside a sentence in your language that says what happened.
- **The app framework's own error**, in the one case where the window can't get an answer from the
  rest of Canager at all while loading or refreshing the list. Canager itself never fails a refresh
  as a whole: a source it couldn't read is reported in your language, through its own notice and
  the "Some data might be out of date" banner.
- **One technical detail that is still Canager's own**, which appears in English inside an
  otherwise translated sentence. This is a known gap, not a design choice: with "Show technical
  details" turned on, Canager's own half of why a package couldn't be checked for updates ("exited
  with code 1"). It should become a structured payload like the refusals, and until it does, the
  message a Chinese user sees there is partly English.

## Design notes

The design and the reasoning behind it live in [`docs/superpowers/`](docs/superpowers/) — the
spec, the implementation plans, and the review findings that changed them. They are working
documents rather than polished writing, but they record why things are the way they are.

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

**目前处于发布前阶段**，核心与界面已经可用、有 359 个 Rust 测试（另有 2 个要连着真实的
Homebrew 才跑，平时是跳过的）和 238 个前端测试，但还没有可下载的版本，v0.1 正在准备。现在还
不适合依赖它。

界面默认英文，内置完整简体中文。所有标签、标题、按钮和提示框都走 i18n，两种语言由测试保证同步——
中文用户读不懂的句子算 bug。

Rust 侧返回的拒绝理由也会翻译，不只是外面那层框。操作所针对的来源只读、连不上或已不存在，操作无法
准备（某个名字 Canager 不肯交给工具、某个程序不见了），预览已过期或已用过，设置没能保存，操作因为
Canager 自己这边的原因没能执行（比如程序在检查之后、运行之前被删掉了），Canager 自己在操作日志里说的话
（等待 Homebrew 更新完毕、某个输出流读不下去了），以及它对结果的判断（命令说成功了，但那个包并不在），
都以一个结构化的小数据传到前端，用你选的语言显示。

有三类文字会原样显示：

- **其他程序自己的话。** brew、npm 在操作日志里打印的每一行，操作失败时、或者 Canager 准备操作时它提出
  异议时它 stderr 的最后几行；以及 macOS 无法启动某个工具（不论 Canager 是在准备操作还是在执行操作）、
  或因为 Canager 不认识的原因无法保存设置时给出的原因。那是另一个程序自己的文字，没法翻译。日志之外，它会被引用在一句用你的语言说明发生了什么的话里。
- **应用框架自己的报错**，只出现在一种情况：读取或刷新列表时，窗口完全联系不上 Canager 的其余部分。
  Canager 自己从不会让整次刷新失败：读不了的来源会用你的语言，通过它自己的提示和“部分数据可能不是最新的”横幅告诉你。
- **一处仍属于 Canager 自己的技术细节**，会以英文出现在一句已翻译的话里。这是已知的缺口，不是有意
  为之：打开“显示技术细节”后，某个包无法检查更新时 Canager 自己说明的那一半（“exited with code 1”）。
  它应该像上面的拒绝理由那样改成结构化数据，在那之前，中文用户在这里看到的提示会夹带英文。

尚未支持：搜索与软件目录、安装新东西、macOS 以外的平台。按需刷新也还没有——只有刷新失败，或者
Homebrew 的索引过期了，才会出现“重试”按钮，不是随时可按的独立刷新控件。
