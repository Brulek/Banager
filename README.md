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

English by default, with a full Simplified Chinese translation. Every string in the UI goes
through i18n, and a test keeps the two locales in step — a string a Chinese user cannot read is
treated as a bug. That now includes the actionability refusals Rust sends back: a plan built
against a source that's read-only, unavailable or gone, or a preview that's expired or already
been used, each arrives as a small `{"kind": ...}` payload the front end recognises and renders
in the user's language, not the error's own English `Display`. What is *not* translated is the
package manager's own output — every line `brew` or `npm` prints in the operation log, and the
last lines of its stderr when an operation fails, are shown as-is, because there is no way to
translate another program's text. Canager's own remarks in that log (waiting for Homebrew to
finish updating, a stream it could no longer read) and its verdicts on the result (the command
said it worked but the package isn't there) are translated like the rest of the UI. One gap is
left: when an operation fails for a reason of Canager's own rather than the tool's — the program
was removed between the check and the run, say — the failure line carries a short technical
error in English.

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

界面默认英文，内置完整简体中文。界面里的文字都走 i18n，两种语言由测试保证同步——中文用户读不
懂的句子算 bug。这现在也包括 Rust 侧返回的“操作不可执行”类结构化拒绝理由：无论是源不可写、连
不上，还是预览已过期、已经用过，传到前端的都是一个前端认得的 `{"kind": ...}` 小结构，显示成中
文，而不是那个错误自带的英文原文。真正没有被翻译的，是操作本身失败时套壳的包管理器自己打印的
内容——brew、npm 自己吐出的报错文本会原样显示，因为那是另一个程序自己的文字，没法翻译。

尚未支持：搜索与软件目录、安装新东西、macOS 以外的平台。按需刷新也还没有——只有刷新失败，或者
Homebrew 的索引过期了，才会出现“重试”按钮，不是随时可按的独立刷新控件。
