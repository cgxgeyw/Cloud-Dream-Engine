# Cloud Dream Engine（云朵梦境）

> An offline-first desktop & mobile engine for building LLM-driven narrative games and "world packs" — author worlds, characters, custom in-game UIs, and an AI director, then play them locally.

> [World package developer guide (runtime v3 / package format v7, Chinese)](docs/world-package-guide-v3.md)

<p align="center">
  <img src="https://github.com/user-attachments/assets/25ae1d57-5e2b-478c-902b-628dad463876" alt="Cloud Dream Engine — world editor and runtime" width="820">
</p>

<p align="center">
  <em>English</em> · <a href="#中文说明">中文</a>
</p>

---

## What it is

Cloud Dream Engine is a [Tauri](https://tauri.app/) + React + Rust/SQLite application for creating and playing **AI narrative games**. You design a *world* (setting, characters, rules, memory, attributes) and a *custom game UI*, and an LLM-powered **director** runs the game loop: it reads player input, updates scene/state/attributes/inventory/memory, and decides who speaks next.

World packs are portable, declarative content bundles rather than native plugins. Format version 7 can declare world-scoped `records` / `kv` storage and may include optional `sandbox-js-v1` logic that runs in an isolated Worker through a small, validated SDK.

> **Privacy & offline-first:** the app and all your worlds run locally. You bring your own LLM endpoint (any OpenAI-compatible API), so your data and keys stay on your machine.

## Why it's different

- **Worlds ship their own sandboxed UI.** A world's runtime interface is described by a UI document (`ui_theme_config`), not a fixed screen. It can only call registered components, validated actions, and declared capabilities.
- **Safe persistence and lightweight logic.** World packages can declare private structured storage and optional Worker logic without receiving SQLite, filesystem, network, raw Tauri, or host DOM access.
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
- World package version 7 with declared `records` / `kv` storage and optional `sandbox-js-v1` logic
- A ready-to-import, no-model [accounting assistant package](output/accounting-assistant-world.zip) with [editable source](examples/world-packages/accounting-assistant/)
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

## Status

Early and actively developed. Expect rapid changes. Issues and feedback are welcome.

## License

<!-- TODO: choose a license (e.g. MIT / Apache-2.0) and add a LICENSE file. -->
No license file yet — all rights reserved until one is added.

---

<a name="中文说明"></a>

## 中文说明

**云朵梦境（Cloud Dream Engine）** 是一个基于 Tauri + React + Rust/SQLite 的离线优先桌面/移动端引擎，用来创作和游玩**由大模型驱动的叙事游戏**。你设计一个*世界*（设定、角色、规则、记忆、属性）和一套*自定义游戏 UI*，由 LLM 驱动的**导演**运行游戏循环：解析玩家输入，写回场景/状态/属性/道具/记忆，并决定下一个发言者。

世界包是可分享的声明式内容包，而不是原生插件。version 7 支持声明按世界隔离的 `records` / `kv` 存储，也可以携带可选的 `sandbox-js-v1` 逻辑；逻辑仅通过受控 SDK 在独立 Worker 中运行。

> **隐私与离线优先：** 应用和你的所有世界都在本地运行。你接入自己的 LLM 端点（任意 OpenAI 兼容 API），数据与密钥不出本机。

> [世界包开发指南（runtime v3 / 世界包格式 v7）](docs/world-package-guide-v3.md)

### 特点

- **世界自带沙箱界面**：运行时 UI 由 UI 文档（`ui_theme_config`）描述，不是固定页面。世界只能调用已注册组件、校验后的动作和已声明能力。
- **安全持久化与轻量逻辑**：世界包可以声明私有结构化存储和可选 Worker 逻辑，但不会获得 SQLite、文件系统、网络、原始 Tauri 或宿主 DOM 权限。
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
- 世界包 version 7：声明式 `records` / `kv` 存储与可选 `sandbox-js-v1` 逻辑
- 可直接导入、无需模型的[记账助手世界包](output/accounting-assistant-world.zip)及其[可编辑源码](examples/world-packages/accounting-assistant/)
- 明暗模式、多种视觉风格、平台语言切换（中文 / English）

### 快速开始

**前置：** [Rust](https://www.rust-lang.org/tools/install)（stable）、[Node.js](https://nodejs.org/) 18+、以及对应系统的 [Tauri 2 环境](https://tauri.app/start/prerequisites/)。

```bash
npm run frontend:install   # 安装前端依赖
npm run tauri:dev          # 开发模式运行（Tauri + Vite）
```

随后在**设置**中添加一个 LLM 文本模型（OpenAI 兼容的 base URL + API key），它驱动 AI 辅助创建世界和游戏内导演。

### 状态

项目处于早期、活跃开发中，变化较快。欢迎 issue 与反馈。
