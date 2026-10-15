<div align="center">

# Banager

**把 Mac 上用命令行装的东西，管得明明白白。**

面向不想记命令的 Mac 用户，从 Homebrew 开始，查看已安装内容、处理更新、确认卸载。

[![CI](https://github.com/Brulek/Banager/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Brulek/Banager/actions/workflows/ci.yml)
![macOS 13.3+](https://img.shields.io/badge/macOS-13.3%2B-111827)
![Status: early development](https://img.shields.io/badge/status-early%20development-f59e0b)
[![MIT License](https://img.shields.io/badge/license-MIT-2563eb)](LICENSE)

[当前功能](#当前功能) · [快速开始](#快速开始) · [路线图](#路线图) · [参与贡献](#参与贡献)

</div>

> **项目状态：早期开发中。** 当前主分支已实现 Homebrew 管理与桌面界面；其他软件来源仍在规划中。截至 2026-10-15，仓库尚无公开 Release，体验方式为从源码运行。
>
> **名称说明：** GitHub 仓库名为 **Banager**，应用窗口、包名和部分文档目前仍使用 **Canager**。下文中的两者指同一项目。

## 为什么做 Banager

跟着教程装过一些工具，过了一段时间，却不记得装了什么、哪些需要更新、哪些能放心删除。Banager 希望把这些零散的维护工作放进一个桌面界面，让你先看懂将要发生什么，再决定是否执行。

当前版本通过本机已有的 Homebrew 完成管理：你可以浏览软件清单、筛选名称、选择更新，并在卸载前查看命令和依赖影响。长期目标是逐步接入更多命令行工具和本地模型来源。

## 当前功能

| 你想做什么 | 当前主分支已实现 |
| --- | --- |
| 看清装了什么 | 列出 Homebrew formula 和 cask，按 Homebrew 实例分组，支持按名称筛选 |
| 减少列表干扰 | 默认折叠依赖等非主动安装项目，需要时展开查看 |
| 处理软件更新 | 查看可用更新，选择一个或多个项目，预览后确认执行 |
| 暂时跳过更新 | 忽略指定项目的更新，并在设置中恢复显示 |
| 卸载前了解影响 | 展示实际命令、警告与检测到的依赖者；存在受影响项目时禁用确认按钮 |
| 跟踪执行过程 | 查看操作状态和实时日志、请求取消，并在执行后重新核实结果 |
| 按自己的习惯使用 | 简体中文、English 或跟随系统；可切换技术细节显示 |

**支持范围：** 当前接入 Homebrew 的命令行软件（formula）和桌面应用（cask）。npm、pipx、uv、pip、cargo、Ollama 尚未接入运行中的应用。

**界面范围：** 当前提供「已安装」「更新」「设置」三个页面。「搜索已安装项目」仅筛选本机清单；发现新软件与图形化安装入口尚未提供。

## 操作前后，都让你看得见

- **先预览，再执行。** 更新与卸载会先生成操作计划，展示将要运行的命令，再由你确认。
- **不盲目强制删除。** 卸载前查询依赖关系，不使用 `--ignore-dependencies` 绕过 Homebrew 的依赖保护。
- **逐项更新。** 多选更新也会按已确认的项目分别执行，不调用不带包名的 `brew upgrade`。
- **结果需要核实。** 执行后重新读取状态，区分成功、部分成功、结果未确认和需要处理的问题。
- **命令直接调用。** 核心使用可执行文件路径与参数数组运行命令，并校验包名，不把输入拼成 shell 命令。

取消操作不会自动回滚已经发生的更改，请以操作后的核实结果和日志为准。完整的 Homebrew 命令清单见 [What Canager Runs](docs/what-we-run.md)。

## 快速开始

### 准备环境

- **macOS 13.3 或更新版本**；当前项目面向 macOS。
- **Homebrew**：请先自行安装。应用会检测 `/opt/homebrew/bin/brew` 和 `/usr/local/bin/brew` 等预设路径。
- **Node.js 22.12+（22.x）**、**pnpm 12.4.2**（以 `package.json` 为准）。
- **Rust stable** 和 **Xcode Command Line Tools**，用于构建 Tauri 桌面应用。

### 从源码启动

```bash
git clone https://github.com/Brulek/Banager.git
cd Banager
pnpm install --frozen-lockfile
pnpm tauri dev
```

启动后显示的应用名称为 **Canager**。请使用普通用户运行，不要使用 `sudo` 启动应用。

`pnpm dev` 只启动前端开发服务器；完整的本机软件管理功能需要通过 `pnpm tauri dev` 打开桌面窗口。

### 第一次使用

1. 打开「已安装」，等待 Homebrew 检测与清单刷新完成。
2. 输入名称筛选工具；需要查看附带安装的组件时，展开依赖列表。
3. 在「更新」中选择要处理的项目，检查命令预览后确认；暂不处理的项目可以忽略。
4. 卸载时先查看影响提示；在底部操作区域查看进度和日志。
5. 在「设置」中切换语言、显示技术细节或恢复已忽略的更新。

### 当前使用边界

- **刷新可能联网。** 检查更新时会按缓存时效执行 `brew update`，刷新 Homebrew 本地索引；这不会自动升级已安装的软件。
- **管理员授权仍需打磨。** 部分 cask 操作需要密码，原生密码助手尚未完成集成与验证；遇到此类操作，请在终端中处理。
- **发行流程尚未完成。** 仓库已有 Universal 构建与草稿发布工作流，但签名、公证及应用自身更新配置仍需完善，不能据此视为已有可下载的正式版本。

## 路线图

以下是规划方向，不代表当前已经支持，也不承诺发布日期。

| 方向 | 状态 |
| --- | --- |
| Homebrew 适配器、任务执行与结果核实 | 已实现 |
| 已安装 / 更新 / 设置界面，中英双语 | 已实现 |
| npm 全局包、pipx、uv、cargo、Ollama；pip 只读清单 | 已有阶段 3 实施计划，待实现 |
| AI CLI 识别、发现页与安装入口 | 规划中 |
| 菜单栏、后台检查与通知 | 规划中 |
| 签名、公证、正式发行与应用自身更新 | 待完善与验证 |

设计目标见 [产品设计](docs/superpowers/specs/2026-09-17-canager-design.md)，后续接入方案见 [更多来源实施计划](docs/superpowers/plans/2026-09-20-phase-3-more-sources.md)，已知问题见 [Backlog](docs/superpowers/backlog.md)。历史设计文档包含尚未实现的能力，当前功能请以上文及主分支代码为准。

## 开发与验证

桌面外壳采用 **Tauri 2**，核心逻辑使用 **Rust**；界面使用 **React 19、TypeScript、Vite 和 Tailwind CSS**。软件来源适配、命令执行和操作结果核实放在独立的 `canager-core` 中。

```text
crates/canager-core/  软件来源适配、命令执行、任务调度与状态核实
src-tauri/           桌面应用、IPC 与设置存储接入
src/                React 页面、组件、状态与双语文案
adapters/           适配器元数据与真实命令输出样本
docs/               产品设计、实施计划、评审与命令说明
.github/workflows/  CI 与发布工作流
```

安装依赖后，可在仓库根目录执行以下检查：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm test
pnpm build
```

仅编译本机桌面应用、不生成安装包：

```bash
pnpm tauri build --no-bundle
```

CI 还配置了真实 Homebrew 冒烟测试，会安装并卸载测试软件 `hello`；它需要显式启用，不属于普通 `cargo test --workspace` 的执行范围。检查配置见 [CI 工作流](.github/workflows/ci.yml)，实际结果以 GitHub Actions 为准。

## 参与贡献

欢迎通过 [Issues](https://github.com/Brulek/Banager/issues) 反馈问题或讨论功能，通过 [Pull Requests](https://github.com/Brulek/Banager/pulls) 提交改进。

报告问题时，请附上 macOS 版本、芯片架构、Homebrew 版本、复现步骤和相关日志。提交日志前，请先移除令牌、私有源地址等敏感内容。

涉及功能变更时，请同步维护中英文界面文案与相关测试。适配器样本应来自真实命令输出；实现前建议先查看 [已知问题与待办](docs/superpowers/backlog.md)，了解现有约束。

## License

[MIT](LICENSE) © 2026 Brulek

---

**English overview:** Banager is an early-stage macOS desktop manager for software installed through Homebrew. It provides an installed inventory, selected updates, uninstall previews, operation logs, and English / Simplified Chinese UI. The app currently uses the name **Canager**. Support for npm, Python tools, cargo, Ollama models, and AI CLI discovery is planned; no public release is available as of October 15, 2026.
