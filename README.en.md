<div align="center">

# Banager

**One place to manage the software you installed from the command line on your Mac.**

[简体中文](README.md) | English

Banager lists, updates and uninstalls software installed through Homebrew, npm, pipx, uv, pip, Cargo / rustup, Ollama, and several standalone command-line installers. You review each step and confirm it before Banager runs anything.

[![CI](https://github.com/Brulek/Banager/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Brulek/Banager/actions/workflows/ci.yml)
![macOS 13.3+](https://img.shields.io/badge/macOS-13.3%2B-111827)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-f59e0b)
[![MIT License](https://img.shields.io/badge/license-MIT-2563eb)](LICENSE)

</div>

> **Status: pre-release v0.1: downloadable from GitHub Releases (unsigned).** You can also run Banager from source. Banager cannot update itself yet. The project was formerly named Canager and was renamed to Banager on 2026-09-30.

## Installation

1. Open the [GitHub Releases page](https://github.com/Brulek/Banager/releases) and download the `.dmg` file.
2. Open the `.dmg` and drag Banager into the Applications folder.
3. The first time you open Banager, macOS will say that the developer cannot be verified. Open System Settings > Privacy & Security, click "Open Anyway" next to Banager, and then confirm in the dialog that follows.

This version is not yet notarised by Apple. The source code is public, so you can also build Banager yourself as described in "Running from Source" below.

## Features

- **One inventory**: the Installed page groups software by source, covering Homebrew, npm global packages, pipx, uv tools, pip, cargo, Ollama models, and several standalone command-line installers.
- **Selective updates**: on the Updates page, select one or more items, confirm, and Banager runs each one in turn.
- **Clear impact before uninstalling**: the uninstall preview lists the commands that will run, what will be deleted, what settings and data are kept, and any other software that depends on the item.
- **Verified results**: after each operation, Banager reads the state again and distinguishes success, partial success, and unconfirmed results.
- **Background checks and notifications**: optional daily or weekly checks notify you when updates are found or an operation finishes.
- **Read-only other programs**: the Other Programs page lists programs that these sources did not install. Banager does not run, move, or delete any of them.
- **Recent activity**: updates and uninstalls from the last 30 days are kept, including failures and results that need attention.
- **Menu bar shortcuts**: Command-1 to Command-4 switch pages, Command-R checks again, Command-F searches, and Command-comma opens Settings.

For the full feature list and the support scope of each source, see the [full reference (English)](docs/reference.en.md). For the commands each source runs, see [What Banager Runs](docs/what-we-run.md).

## Getting Started

### Requirements

- macOS 13.3 or later.
- Homebrew, installed separately.
- Node.js 22 (CI uses 22) and pnpm 12.4.2 (the version in `package.json` is authoritative).
- Rust stable and the Xcode Command Line Tools, required to build the Tauri desktop app.

### Running from Source

```bash
git clone https://github.com/Brulek/Banager.git
cd Banager
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run Banager as a regular user. Do not start it with `sudo`. `pnpm dev` starts only the frontend development server; the full feature set requires `pnpm tauri dev`.

To look at the interface without touching real software, run `pnpm tauri:mock`. It opens a window that uses fake data and does not run Homebrew or any other source. You can also run `pnpm dev:mock` and open <http://localhost:1430/> in a browser.

## More Documentation

- [Full reference (English)](docs/reference.en.md): feature details, safety design, known limits, and build notes.
- [What Banager Runs](docs/what-we-run.md): the commands each source runs, the network hosts it contacts, and the files it reads and writes.
- [UI preview](docs/ui-preview.md): how to preview each page with fake data.
- [Product design](docs/superpowers/specs/2026-09-17-banager-design.md): design goals and trade-offs.

## Roadmap

The items below are planned directions. They are not yet supported, and no release date is promised.

| Direction | Status |
| --- | --- |
| Reading, updating, and uninstalling for Homebrew, npm, pipx, uv, cargo, Ollama, and other sources | Implemented |
| Installed, Updates, Overview, Other Programs, and Settings pages, in three languages | Implemented |
| Menu bar, background checks and notifications, recent activity, AI tool filter | Implemented |
| Discover page and install entry point | Partly done: Discover installs seven common tools with Homebrew and a starter model with Ollama, and walks you through installing Homebrew in Terminal; search is planned |
| Signing, notarization, formal release, and self-update | Pending |

## Contributing

Report problems or discuss features through [Issues](https://github.com/Brulek/Banager/issues), and submit improvements through [Pull Requests](https://github.com/Brulek/Banager/pulls). When a change affects behavior, update the Chinese and English text and the related tests together.

## License

[MIT](LICENSE) © 2026 Brulek. The logos of tools and sources are not covered by the MIT License; see the [full reference (English)](docs/reference.en.md#license) for details.
