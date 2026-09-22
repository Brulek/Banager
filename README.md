# Canager

**A Mac app for everything you installed from the terminal and then forgot about.**

If you've followed a few tutorials, you probably have Homebrew formulae, a couple of global npm
packages, some Python tools, a Rust binary and an Ollama model or two scattered across your Mac.
Each was installed with a different command. Updating them needs a different command again.
Removing them needs a third. Most people never do either, and the tools quietly rot.

Canager puts all of it in one window: what you have, what has an update, and a button for each.

> **Status: pre-release.** The core and the UI work and are covered by ~310 Rust and ~210
> front-end tests, but there is no downloadable build yet — v0.1 is being prepared. Nothing here
> is ready to rely on.

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
- **You see the exact command before it runs.** Every install, update and uninstall shows its
  real argv, what it will affect, and whether it needs your password.
- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones,
  in your language.

## What it deliberately does not do yet

Being honest about this is part of the point:

- **No search and no catalogue.** You can manage what you already have; you cannot yet discover
  new things through Canager.
- **No refresh button.** It checks at launch and after each operation. This is a known gap.
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

`cargo test --workspace` has one `#[ignore]`d test that really installs and removes a Homebrew
formula. CI runs it; run it yourself with:

```bash
CANAGER_LIVE=1 cargo test -p canager-core --test brew_live -- --ignored
```

## Language

English by default, with a full Simplified Chinese translation. Every user-facing string goes
through i18n and both locales are kept in step by a test — a string a Chinese user cannot read
is treated as a bug, including error text coming back from Rust.

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

**目前处于发布前阶段**，核心与界面已经可用、有约 310 个 Rust 测试和约 210 个前端测试，但还没有
可下载的版本，v0.1 正在准备。现在还不适合依赖它。

界面默认英文，内置完整简体中文。所有面向用户的文字都走 i18n，两种语言由测试保证同步——
中文用户读不懂的句子算 bug，包括 Rust 侧返回的错误文本。

尚未支持：搜索与软件目录、刷新按钮、macOS 以外的平台。
