# 游戏 UI 架构

当前实现已经从“shell 直接拼 mount JSX”演进到“世界包文档 + runtime 组件注册 + action registry + 治理接口”四层结构。

## 相关文件

- `frontend/src/game/useGameSession.ts`
- `frontend/src/game/shells/DesktopGameShell.tsx`
- `frontend/src/game/shells/MobileGameShell.tsx`
- `frontend/src/gameUiRuntime/runtimeContext.ts`
- `frontend/src/gameUiRuntime/actions.ts`
- `frontend/src/gameUiRuntime/actionSchemas.ts`
- `frontend/src/gameUiRuntime/registry.tsx`
- `frontend/src/gameUiRuntime/components/`
- `frontend/src/components/GameUiRenderer.tsx`
- `frontend/src/data/gameUi/parser.ts`
- `frontend/src/pages/WorldEditorPage.tsx`

## 当前分层

框架层负责：

- 拉取和维护 `session`
- 暴露 `GameUiRuntimeContext`
- 暴露 action registry
- 注入桌面 / 移动平台 capability
- 提供 v1 mount 兼容层
- 提供 v2 `componentRenderers`

世界包 UI 文档负责：

- layout tree
- v2 component tree
- component props
- tokens / components / custom_css
- 平台差异表达

后端治理层负责：

- UI 文档校验
- bundle 校验
- UI 编译
- 兼容性检查

## 已抽离的运行时组件

以下游戏内 UI 块已经不再由 desktop/mobile shell 手写完整 JSX：

- `message_list`
- `input_composer`
- `side_panel_tabs`
- `floating_actions`
- `scene_header`
- `scene_focus`
- `character_bar`
- `narration_card`

这些组件都通过 `frontend/src/gameUiRuntime/registry.tsx` 注册。

## 当前 action registry

当前显式 action id 包括：

- `submit_message`
- `edit_turn_start`
- `edit_turn_cancel`
- `branch_from_current`
- `retry_turn`
- `accept_switch_proposal`
- `dismiss_switch_proposal`
- `dismiss_retry_card`
- `copy_text`
- `switch_side_tab`
- `navigate_home`
- `navigate_settings`
- `navigate_debug`
- `pick_image`
- `remove_image`
- `start_recording`
- `stop_recording`
- `remove_audio`

其中附件与录音相关 action 仍然是前端浏览器能力适配，不依赖新增后端接口。

## v1 / v2 状态

- `schema_version: 1` 只保留兼容层，不再作为长期扩展方向
- `schema_version: 2` 支持 `grid / stack / absolute / component / slot / when / for_each`
- `GameUiRenderer` 按 schema version 分发到 v1 或 v2 路径

## 编辑器与治理现状

当前世界编辑器已经支持：

- raw JSONC 编辑
- v1 / v2 schema 切换
- 桌面 / 移动预览切换
- 结构化 v2 组件树编辑
- 后端 validate / compile / compatibility 反馈展示

## 还要怎么理解“世界包控制 UI”

更准确的说法是：

- 世界包已经可以在 v2 文档里声明页面由哪些注册组件组成
- 框架层负责提供稳定组件、runtime 数据、action 和平台能力
- 新功能优先通过“新增组件 / 新增 action / 新增治理规则”进入系统，而不是回到 shell 里继续拼私有 JSX

## 进一步状态审计

更完整的实施进度与证据审计见：

- `docs/world-package-game-ui-progress-audit-2026-06-01.md`
- `docs/world-package-game-ui-status-2026-06-01.md`
