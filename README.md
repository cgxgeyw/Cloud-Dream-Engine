# Cloud Dream Engine（云朵梦境）

> An offline-first desktop & mobile engine for building LLM-driven narrative games and "world packs" — author worlds, characters, custom in-game UIs, and an AI director, then play them locally.

> [World package developer guide v3 (Chinese)](docs/world-package-guide-v3.md)

<p align="center">
  <img src="https://github.com/user-attachments/assets/25ae1d57-5e2b-478c-902b-628dad463876" alt="Cloud Dream Engine — world editor and runtime" width="820">
</p>

<p align="center">
  <em>English</em> · <a href="#中文说明">中文</a>
</p>

---

## What it is

Cloud Dream Engine is a [Tauri](https://tauri.app/) + React + Rust/SQLite application for creating and playing **AI narrative games**. You design a *world* (setting, characters, rules, memory, attributes) and a *custom game UI*, and an LLM-powered **director** runs the game loop: it reads player input, updates scene/state/attributes/inventory/memory, and decides who speaks next.

Worlds are content, not code. They live in SQLite and can be exported as portable **world packs** (`.zip`) and shared.

> **Privacy & offline-first:** the app and all your worlds run locally. You bring your own LLM endpoint (any OpenAI-compatible API), so your data and keys stay on your machine.

## Why it's different

- **Worlds ship their own UI.** A world's runtime interface is described by a sandboxed UI document (`ui_theme_config`), not a fixed screen. Worlds can only call registered components, actions, and capabilities — they never execute arbitrary code.
- **Backend-driven game loop.** A Rust orchestrator drives the turn: director parses input → applies scene/state/attribute/rule/inventory/memory writebacks → frontend refreshes session state.
- **Director + character model.** A world director coordinates the scene; characters respond with their own prompts, memory, and attributes.
- **Reusable prompt presets.** Attach scoped, ordered prompt fragments to a world (director / character / both) without editing core prompts.
- **Independent desktop + mobile world UIs.** Runtime v3 gives each platform its own complete UI document and raw stylesheet inside an isolated iframe.

## Features

- World & character editor (background, attributes, memory strategy, per-character prompts)
- AI-assisted world creation — describe a concept, get a draft world + characters (single- or multi-agent)
- Custom in-game UI documents (`schema_version: 2` component tree: grid/stack/absolute, components, slots, conditionals, loops, text/image/badge/button/checkbox …)
- Per-platform raw CSS inside an isolated world iframe, with scoped v2 CSS compatibility
- Memory, attributes, inventory, rules, scene/state writeback
- Prompt trace viewer — inspect exactly what was sent to the model and how the response was processed
- World pack import/export
- Light/dark mode, multiple visual styles, and a platform language toggle (中文 / English)

## Tech stack

| Layer | Stack |
|---|---|
| Shell | Tauri 2 (Windows desktop, Android) |
| Frontend | React 19 + TypeScript + Vite |
| Backend | Rust |
| Storage | SQLite (schema, migrations, repositories, seeds) |
| LLM | Any OpenAI-compatible chat endpoint (bring your own key) |

## Architecture at a glance

```
GamePage → GamePageController → useGameSession()
  → GameUiSandboxRuntime (trusted parent capability bridge)
  → sandboxed WorldFrame → GameUiRenderer

Rust orchestrator (per turn):
  director parses input
  → scene / state / attribute / rule / inventory / memory writeback
  → session state returned to the frontend
```

Key directories:

- `frontend/src/pages/` — editor, settings, game pages
- `frontend/src/gameUiRuntime/` — registered components, actions, capabilities for world UIs
- `frontend/src/components/GameUiRenderer.tsx` — world UI document renderer
- `src-tauri/src/services/game_engine/` — engine, director, memory, orchestrator
- `src-tauri/src/services/game_ui.rs` — world UI validation/compilation
- `src-tauri/src/services/world_package.rs` — world pack import/export
- `src-tauri/src/db/` — SQLite schema, migrations, repositories, seeds

## Quick start

**Prerequisites:** [Rust](https://www.rust-lang.org/tools/install) (stable), [Node.js](https://nodejs.org/) 18+, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
# 1. Install frontend dependencies
npm run frontend:install

# 2. Run the app in development (Tauri + Vite)
npm run tauri:dev
```

Then open **Settings** and add an LLM text model (OpenAI-compatible base URL + API key). That model powers AI-assisted world creation and the in-game director.

### Build

```bash
# Windows installer (.msi)
npm run build:windows-msi

# Android (.apk)
npm run build:android-apk
```

## Status

Early and actively developed. Expect rapid changes. Issues and feedback are welcome.

## License

<!-- TODO: choose a license (e.g. MIT / Apache-2.0) and add a LICENSE file. -->
No license file yet — all rights reserved until one is added.

---

<a name="中文说明"></a>

## 中文说明

**云朵梦境（Cloud Dream Engine）** 是一个基于 Tauri + React + Rust/SQLite 的离线优先桌面/移动端引擎，用来创作和游玩**由大模型驱动的叙事游戏**。你设计一个*世界*（设定、角色、规则、记忆、属性）和一套*自定义游戏 UI*，由 LLM 驱动的**导演**运行游戏循环：解析玩家输入，写回场景/状态/属性/道具/记忆，并决定下一个发言者。

世界是内容而非代码：它们存在 SQLite 里，可导出为可分享的**世界包**（`.zip`）。

> **隐私与离线优先：** 应用和你的所有世界都在本地运行。你接入自己的 LLM 端点（任意 OpenAI 兼容 API），数据与密钥不出本机。

> [世界包开发指南 v3](docs/world-package-guide-v3.md)

### 特点

- **世界自带界面**：运行时 UI 由沙箱化的 UI 文档（`ui_theme_config`）描述，不是固定页面。世界只能调用已注册的组件、动作和能力，无法执行任意代码。
- **后端驱动游戏循环**：Rust orchestrator 驱动每个回合——导演解析输入 → 写回场景/状态/属性/规则/道具/记忆 → 前端刷新会话状态。
- **导演 + 角色模型**：世界导演统筹场景，角色用各自的提示词、记忆和属性回应。
- **可复用提示词预设**：按作用域（导演/角色/两者）和顺序给世界挂载提示词片段，无需改动核心提示词。
- **桌面 + 移动双入口**：runtime v3 为两个平台分别提供完整 UI 文档和原始 stylesheet，并在隔离 iframe 中渲染。

### 功能

- 世界与角色编辑器（背景、属性、记忆策略、各角色提示词）
- AI 辅助创建世界：输入一个构想，生成草稿世界与角色（单/多智能体）
- 自定义游戏内 UI 文档（`schema_version: 2` 组件树）
- 桌面/移动分别使用 iframe 内原始 CSS，并兼容 v2 作用域化 CSS
- 记忆、属性、道具、规则、场景/状态写回
- 提示词追踪：查看实际发给模型的内容及返回处理过程
- 世界包导入/导出
- 明暗模式、多种视觉风格、平台语言切换（中文 / English）

### 快速开始

**前置：** [Rust](https://www.rust-lang.org/tools/install)（stable）、[Node.js](https://nodejs.org/) 18+、以及对应系统的 [Tauri 2 环境](https://tauri.app/start/prerequisites/)。

```bash
npm run frontend:install   # 安装前端依赖
npm run tauri:dev          # 开发模式运行（Tauri + Vite）
```

随后在**设置**中添加一个 LLM 文本模型（OpenAI 兼容的 base URL + API key），它驱动 AI 辅助创建世界和游戏内导演。

### 构建

```bash
npm run build:windows-msi   # Windows 安装包
npm run build:android-apk   # 安卓 APK
```

### 状态

项目处于早期、活跃开发中，变化较快。欢迎 issue 与反馈。
