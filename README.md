# 云朵梦境 Cloud Dream Engine

云朵梦境是一套以“世界包”为核心的 AI 叙事游戏创作与运行引擎。它把世界设定、角色、地图、提示词、状态字段、游戏内 UI、素材资源和存档调试能力放在同一个工程里，让创作者可以从“写设定”一路走到“可安装、可游玩、可迁移”的桌面与移动端体验。

当前项目以 Tauri 2 + Rust + React/Vite 为主体，面向 Windows 桌面和 Android 设备构建。

## 项目特点

- **世界包驱动**：每个世界可以独立配置背景设定、开场内容、地图拓扑、角色池、提示词、状态页签、桌面 UI、移动 UI 与依赖素材。
- **AI 叙事会话**：玩家在世界中发言或行动，由模型推进剧情、生成旁白、角色对话和世界状态变化。
- **多角色与视角切换**：支持世界中的角色配置、玩家身份、在场人物、角色记忆与剧情中的切换建议。
- **存档与分支**：支持从已有存档继续游玩，也可以从当前剧情节点分支出新的路线。
- **可配置游戏 UI**：世界包可声明桌面端与移动端 UI 文档，运行时通过组件注册、action registry 和能力适配来渲染。
- **调试视图完整**：内置回合链路、Prompt、记忆、工具调用、异常与重试信息，方便内容迭代和问题排查。
- **本地数据优先**：Rust/Tauri 后端使用本地应用数据目录和 SQLite 存储世界、角色、会话、存档、模型配置与上传素材。
- **模型配置与测试**：支持配置文本模型、图像模型、默认模型、模型发现与连接测试。
- **跨端交付**：项目已有 Windows 安装包和 Android APK 构建脚本，适合桌面创作、移动体验与私有发布。

## 适合谁

- 想制作多角色、多场景、持续推进的 AI 剧情世界的创作者。
- 需要把世界设定、角色资产、界面和运行规则一起打包交付的内容团队。
- 需要保留完整调试链路、验证叙事逻辑和快速迭代提示词的开发/测试团队。

## 技术栈

- **桌面/移动壳**：Tauri 2
- **后端**：Rust、rusqlite、serde、reqwest、tokio
- **前端**：React 19、Vite、TypeScript、React Router、Tailwind CSS、HeroUI、Framer Motion
- **世界 UI 文档**：JSONC、运行时组件注册、受限表达式、action registry
- **构建目标**：Windows MSI/EXE、Android APK

## 目录结构

```text
.
├── frontend/              # React/Vite 前端应用
│   └── src/
│       ├── pages/         # 首页、世界、角色、存档、设置、调试等页面
│       ├── game/          # 游戏会话控制器与桌面/移动 shell
│       ├── gameUiRuntime/ # 世界包 UI 运行时、组件注册与 action
│       └── data/          # Tauri API、设置、数据类型与适配层
├── src-tauri/             # Tauri/Rust 后端
│   └── src/
│       ├── commands/      # 前端可调用命令
│       ├── db/            # SQLite schema、migration、repository、seed
│       ├── models/        # Rust 数据模型
│       └── services/      # LLM、世界包、游戏 UI、地图拓扑等服务
├── docs/                  # 世界包 UI schema、架构与进度文档
├── scripts/               # 开发、构建、打包脚本
├── artifacts/             # 本地构建产物与发布材料
└── data/                  # 设计稿、素材和实验数据
```

## 核心功能模块

- **世界管理**：创建、编辑、复制、删除、导入、导出世界包。
- **角色管理**：为世界创建角色，配置角色提示词、身份、素材、模板导入导出。
- **新游戏与会话**：选择世界、创建会话、提交玩家行动、恢复未完成回合。
- **存档管理**：列出存档、删除存档、从存档创建分支。
- **游戏 UI 运行时**：支持 `schema_version: 2` 的组件树、布局节点、props 绑定和 action 引用。
- **调试工具**：查看时间线、Prompt、记忆、错误、失败步骤和重试卡片。
- **模型与设置**：配置模型、测试模型、设置默认模型、管理导出目录建议。
- **素材上传**：上传图片/资源并通过 Tauri asset protocol 在应用内访问。

## 开发环境

建议准备：

- Node.js 与 npm
- Rust stable
- Tauri 2 所需系统依赖
- Android 构建环境（仅构建 APK 时需要）

安装前端依赖：

```powershell
npm run frontend:install
```

启动 Tauri 开发环境：

```powershell
npm run tauri:dev
```

仅启动前端：

```powershell
cd frontend
npm run dev
```

## 构建

Windows 安装包：

```powershell
npm run build:windows-msi
```

Android APK：

```powershell
npm run build:android-apk
```

前端构建：

```powershell
cd frontend
npm run build
```

Rust 检查：

```powershell
cd src-tauri
cargo check
```

## 世界包 UI

项目正在把游戏内界面从固定 shell 逐步抽象为“世界包 UI 文档 + 运行时组件”的结构。当前 v2 UI 文档支持：

- `grid`、`stack`、`absolute`、`component`、`slot`、`when`、`for_each` 等布局节点。
- 桌面端和移动端分别声明入口文档。
- 通过注册组件渲染消息列表、输入框、侧栏页签、浮动按钮、场景头部、角色条、旁白卡等模块。
- 通过 action registry 触发提交消息、编辑、重试、分支、复制、切换页签、导航、选择图片和录音等动作。

相关文档位于 `docs/`，尤其是世界包 UI schema 与架构说明。

## 当前状态

这是一个面向私有发布与持续迭代的应用工程，已经包含可运行应用、世界包能力、调试工具和跨端构建脚本。仓库中可能保留历史实验、构建日志和本地产物目录；正式发布时建议只打包必要源码、文档、脚本和安装产物。

## 许可证

见 [LICENSE](LICENSE)。
