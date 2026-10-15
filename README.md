<div align="center">

# Banager

**统一管理你在 Mac 上用命令行安装的软件。**

简体中文 | [English](README.en.md)

Banager 把 Homebrew、npm、pipx、uv、pip、Cargo / rustup、Ollama 及几款命令行工具的独立安装器装过的东西放进一个桌面界面：统一查看，选择更新，确认后再卸载。

[![CI](https://github.com/Brulek/Banager/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Brulek/Banager/actions/workflows/ci.yml)
![macOS 13.3+](https://img.shields.io/badge/macOS-13.3%2B-111827)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-f59e0b)
[![MIT License](https://img.shields.io/badge/license-MIT-2563eb)](LICENSE)

</div>

> **项目状态：发布前。** 截至 2026-10-15，仓库尚无可下载的正式版本，请从源码运行。Banager 暂时还不能自行更新。项目原名 Canager，2026-09-30 改名为 Banager。

## 主要功能

- **统一清单**：「已安装」页按来源分组，列出 Homebrew、npm 全局包、pipx、uv 工具、pip、cargo、Ollama 模型，以及独立安装的几款命令行工具。
- **选择性更新**：「更新」页可以勾选一个或多个项目，确认后逐项执行。
- **卸载前看清影响**：卸载预览列出将运行的命令、会删除的内容、保留的设置与数据，以及依赖它的其他软件。
- **结果需要核实**：每次操作后重新读取状态，区分成功、部分成功和结果未确认。
- **后台检查与通知**：可选每天或每周自动检查，发现更新或操作完成时通知。
- **其他程序只读**：列出不是上述来源安装的程序，但不运行、不移动、不删除其中的任何东西。
- **最近更新记录**：保留 30 天内的更新与卸载记录，包括失败和需要处理的结果。
- **菜单栏快捷键**：⌘1 至 ⌘4 切换页面，⌘R 重新检查，⌘F 搜索，⌘, 打开设置。

完整功能与各来源的支持范围，见 [完整说明](docs/reference.zh-CN.md)。各来源会运行的命令，见 [Banager 会运行什么](docs/what-we-run.md)。

## 快速开始

### 环境要求

- macOS 13.3 或更新版本。
- Homebrew，请先自行安装。
- Node.js 22（CI 使用 22）、pnpm 12.4.2（以 `package.json` 为准）。
- Rust stable 和 Xcode Command Line Tools，用于构建 Tauri 桌面应用。

### 从源码启动

```bash
git clone https://github.com/Brulek/Banager.git
cd Banager
pnpm install --frozen-lockfile
pnpm tauri dev
```

请使用普通用户运行，不要用 `sudo` 启动应用。`pnpm dev` 只启动前端开发服务器，完整功能需要 `pnpm tauri dev`。

想先看界面而不接触真实软件，可以运行 `pnpm tauri:mock`，它打开使用假数据的窗口，不会运行真实的 Homebrew 或其他来源。也可以运行 `pnpm dev:mock`，在浏览器中打开 <http://localhost:1430/> 预览。

## 更多文档

- [完整说明（简体中文）](docs/reference.zh-CN.md)：功能细节、安全设计、已知限制与构建说明。
- [Banager 会运行什么](docs/what-we-run.md)：每个来源执行的命令、访问的网络地址与读写的文件。
- [界面预览](docs/ui-preview.md)：如何用假数据预览各个页面。
- [产品设计](docs/superpowers/specs/2026-09-17-banager-design.md)：设计目标与取舍。

## 路线图

以下是规划方向，不代表已经支持，也不承诺发布日期。

| 方向 | 状态 |
| --- | --- |
| Homebrew、npm、pipx、uv、cargo、Ollama 等来源的读取、更新与卸载 | 已实现 |
| 已安装、更新、概览、其他程序、设置界面，支持三种语言 | 已实现 |
| 菜单栏、后台检查与通知、最近更新记录、AI 工具筛选 | 已实现 |
| 发现页与安装入口 | 规划中 |
| 签名、公证、正式发行与应用自身更新 | 待完善与验证 |

## 参与贡献

欢迎通过 [Issues](https://github.com/Brulek/Banager/issues) 反馈问题、讨论功能，也欢迎通过 [Pull Requests](https://github.com/Brulek/Banager/pulls) 提交改进。涉及功能变更时，请同步维护中英文文案与相关测试。

## 许可证

[MIT](LICENSE) © 2026 Brulek。工具和来源的标志不在 MIT 许可之内，详见 [完整说明](docs/reference.zh-CN.md#许可证)。
