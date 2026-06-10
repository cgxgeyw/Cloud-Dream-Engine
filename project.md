# Cloud Dream Engine Project Notes

> 面向新对话和后续开发的快速理解文档。代码以本地项目 `E:\code\rustweb` 为准；对应仓库为 `cgxgeyw/Cloud-Dream-Engine`。

## 项目定位

Cloud Dream Engine 是一个 Tauri + React + Rust/SQLite 的叙事游戏/世界包引擎。前端负责编辑器、运行时游戏界面、预览和交互；后端负责世界、角色、存档、记忆、属性、规则、LLM 编排、世界包导入导出和 SQLite 持久化。

核心设计思路：

- 世界和角色是内容层，存储在 SQLite，也可以打包成世界包。
- 游戏运行时不是固定页面，而是由世界的 `ui_theme_config` 决定 UI 文档、资源和自定义 CSS。
- 前端只允许世界 UI 文档调用已注册的组件、动作和能力，避免世界包直接执行任意代码。
- 游戏循环由后端 orchestrator 驱动：导演解析玩家输入，应用场景/状态/属性/规则/道具/记忆写回，再由前端刷新会话状态。

## 关键目录

- `frontend/src/pages/`: 框架页面，如世界编辑器、角色编辑器、设置页、游戏页。
- `frontend/src/game/`: 游戏页面控制器、会话 hook、运行时工具。
- `frontend/src/game/shells/`: 桌面/移动游戏 shell，负责把会话状态桥接到世界 UI 渲染器。
- `frontend/src/components/GameUiRenderer.tsx`: 世界 UI 文档渲染器。
- `frontend/src/data/gameUi/`: 世界 UI 类型、默认文档、解析、样式生成。`frontend/src/data/gameUi.ts` 只是兼容 re-export。
- `frontend/src/gameUiRuntime/`: 世界 UI 可用组件、动作、能力、绑定、表达式。
- `src-tauri/src/models/`: Rust 数据模型。
- `src-tauri/src/db/`: SQLite schema、migration、repository、seed。
- `src-tauri/src/services/game_engine/`: 游戏引擎、导演、记忆、运行时效果、orchestrator。
- `src-tauri/src/services/game_ui.rs`: 世界 UI 文档校验/编译/兼容性检查。
- `src-tauri/src/services/world_package.rs`: 世界包导入导出。

## 世界包自定义前端 UI

### 存储形态

世界 UI 配置存放在 `WorldDefinition.ui_theme_config`，数据库列是 `worlds.ui_theme_config_json`。

典型结构：

```jsonc
{
  "assets": {
    "background_source_mode": "...",
    "portrait_source_mode": "...",
    "runtime_image_generation_enabled": false,
    "local_background_assets": [],
    "local_scene_backgrounds": []
  },
  "desktop_file": "{ schema_version: 2, ... }",
  "mobile_file": "{ schema_version: 2, ... }"
}
```

`desktop_file` 和 `mobile_file` 是 JSONC 字符串，不是外部文件路径。世界包导出时会把它们写入 zip 内的：

- `world/ui.desktop.jsonc`
- `world/ui.mobile.jsonc`

### 运行时数据流

游戏页链路：

```text
frontend/src/pages/GamePage.tsx
  -> frontend/src/game/GamePageController.tsx
  -> useGameSession()
  -> DesktopGameShell / MobileGameShell
  -> GameUiRenderer
```

`useGameSession()` 会：

- 从世界的 `ui_theme_config` 生成 `worldUiEnvelope`。
- 根据平台选择 `desktop_file` 或 `mobile_file`。
- 调 `parseGameUiDocument()` 得到文档。
- 调 `buildGameUiStylesheet()` 把文档内 `custom_css` scoped 到当前 game root。
- 解析世界/场景背景资源，写入 CSS 变量 `--game-runtime-bg-image`。
- 加载运行时属性，生成状态抽屉/侧栏里的属性 tab。

Shell 会：

```text
createGameUiRuntimeContext(bag, platform)
createGameUiRuntimeActions(bag, runtime, navigate)
createGameUiComponentRenderers(runtime, actions)
GameUiRenderer({ document, mounts, componentRenderers })
```

也就是说，世界 UI 文档控制“布局和样式”，但真正的数据、行为和安全边界由前端 runtime registry 决定。

### UI 文档 schema

`GameUiRenderer` 支持：

- `schema_version: 1`: 旧版 layout tree，以 `mount` 为主，保留兼容。
- `schema_version: 2`: 当前扩展方向，支持组件树。

v2 节点类型：

- `grid`
- `stack`
- `absolute`
- `component`
- `slot`
- `when`
- `for_each`
- `text`
- `image`
- `badge`
- `button`
- `checkbox`

`component` 节点通过 `component` 名字查 `frontend/src/gameUiRuntime/registry.tsx`。未知组件只会显示 missing placeholder，不会执行任意代码。

v2 还支持声明 `state`，用于世界 UI 的本地前端状态。它适合保存勾选项、临时选择、展开折叠等 UI 状态，不直接写数据库。`checkbox` 可以通过 `bind_checked_list` 写入本地数组；`button` 可以通过受控 action 调用已有能力，例如：

```jsonc
{
  "state": { "checkedTodos": [] },
  "layout": {
    "root": {
      "type": "stack",
      "children": [
        {
          "type": "checkbox",
          "label": "订酒店",
          "value": "订酒店",
          "bind_checked_list": "checkedTodos"
        },
        {
          "type": "button",
          "label": "确认完成",
          "disabled_when_empty_state": "checkedTodos",
          "action": {
            "id": "@submit_message",
            "content_template": "我已完成：{{state.checkedTodos}}"
          }
        }
      ]
    }
  }
}
```

这些基础 DSL 节点只提供声明式 UI、局部状态和模板化受控动作，不允许世界包执行任意 JavaScript。
`button.disabled_when_empty_state` 可用于在某个本地 state 数组为空时禁用按钮；`side_panel_tabs` 支持 `content` slot，可让世界 UI 在侧栏/移动状态抽屉里声明自定义内容。

当前注册组件包括：

- `scene_header`
- `scene_focus`
- `character_bar`
- `narration_card`
- `message_list`
- `input_composer`
- `side_panel_tabs`
- `floating_actions`

旧版 mount id 包括：

- `header`
- `scene`
- `scene_focus`
- `character_bar`
- `narration`
- `message_list`
- `side_panel`
- `input_area`
- `floating_actions`

### 动作和能力

动作定义在 `frontend/src/gameUiRuntime/actions.ts`，后端支持列表在 `src-tauri/src/services/game_ui.rs` 里同步维护。常见动作：

- `submit_message`
- `edit_turn_start`
- `edit_turn_cancel`
- `branch_from_current`
- `retry_turn`
- `copy_text`
- `switch_side_tab`
- `navigate_settings`
- `navigate_debug`
- `pick_image`
- `start_recording`

能力定义在 `frontend/src/gameUiRuntime/capabilities.ts`，后端兼容性校验目前识别：

- `supports_file_picker`
- `supports_hover`
- `supports_mic`

开发新 UI 能力时要同时改：

1. 前端组件/动作/能力实现。
2. `registry.tsx` 或 `actions.ts` 注册。
3. `src-tauri/src/services/game_ui.rs` 的组件 props、动作、能力校验。
4. 默认/seed UI 文档。

### 校验和编辑器

世界编辑器在 `frontend/src/pages/WorldEditorPage.tsx`，负责编辑 `ui_theme_config`、预览和调用后端校验。

后端命令在 `src-tauri/src/commands/game_ui.rs`：

- `validate_world_ui_document`
- `validate_world_ui_bundle`
- `compile_world_ui_document`
- `verify_world_package_ui_compatibility`

服务实现是 `src-tauri/src/services/game_ui.rs`，主要做：

- JSONC 解析。
- schema version 检查。
- v1 mount / v2 component tree 检查。
- props、slot、binding、action、capability 依赖收集。
- 移动端规则警告，例如不要用固定 `100vh/100dvh` 卡住键盘布局。
- 当前客户端兼容性报告。

### 世界包导入导出

`src-tauri/src/services/world_package.rs` 使用 zip 打包，当前格式：

- `format = "dream-world-package"`
- `version = 5`
- `world/world.json`
- `world/ui.desktop.jsonc`
- `world/ui.mobile.jsonc`
- `characters/.../character.json`
- `assets/...`

导出时：

- 从 `world.ui_theme_config.desktop_file/mobile_file` 写 UI 文件。
- 从 manifest 收集 assets，把本地 assets 写入 zip。

导入时：

- 读取 manifest 和 world 数据。
- 读取 UI JSONC 文件为 `desktop_ui_source/mobile_ui_source`。
- 把 zip 内 assets 落盘到本地 assets 目录。
- 构建 `asset_map`，再用 `remap_world_ui_theme_assets()` 替换 UI/世界配置里的资源路径。

如果世界包导入后背景或 UI 资源丢失，优先查：

- `src-tauri/src/services/world_package.rs`
- `src-tauri/src/commands/uploads.rs`
- `worlds.ui_theme_config_json`

## 永久记忆和隔离记忆

### 数据模型

记忆表在 `src-tauri/src/db/schema.rs`：

```text
memories(
  id,
  world_id,
  session_id,
  character_id,
  layer,
  content,
  source,
  importance,
  created_at,
  turn_index,
  conversation_id,
  event_id,
  item_id,
  scene_id,
  memory_type,
  speaker,
  role,
  location,
  participants_json,
  keywords_json
)
```

另有 `memory_embeddings(memory_id, model_key, vector_json, updated_at)` 用于语义召回缓存。

目前没有单独叫“永久记忆”的表。长期/永久效果主要由 `layer = "archive"` 承担，另有 `canonical_event` 可作为规范事件层参与上下文窗口。普通每轮对话会写入多个层：

- `working`
- `short_term`
- `archive`

### 写入机制

核心在 `src-tauri/src/services/game_engine/memory.rs`：

- `build_turn_entries()` 根据当前 turn 的 player/agent 消息生成记忆。
- `persist_turn_entries()` 将规则/导演额外记忆和自动生成的 turn 记忆一起入库。
- `commit_turn_memories()` 在运行时效果和属性提交后写记忆，并在 turn recovery journal 中记录 `memory_committed`，避免恢复/重试重复提交。

写入时会保存：

- `world_id/session_id/character_id`
- `layer`
- `source`: 如 `player_action`、`speaker_response`
- `memory_type`: 如 `dialogue`、`event`
- `speaker/role/location/scene_id`
- `participants`
- `keywords`

### 隔离机制

隔离不是靠一张“私有记忆表”，而是多层约束：

1. 每条记忆都绑定 `character_id`。
2. 召回时 `MemoryRepository.list()` 用 `world_id + session_id + character_id` 查询。
3. `MemoryService.recall_entries_for_character()` 如果没有明确角色 id，直接返回空。
4. `build_turn_entries()` 只给当前可见/在场参与者对应的角色写入本 turn 记忆。
5. 测试覆盖了 Alice/Bob 私有信息不互相泄露的场景。

这意味着同一场会话里，不在场角色不会收到该 turn 的对话记忆；已写入某角色的记忆也只在该角色召回时进入 prompt。

### 召回机制

召回入口：

```text
speaker_loop
  -> load_character_memory_pool()
  -> recall_character_memories()
  -> MemoryService.recall_entries_for_character()
```

排序策略：

- 默认候选上限来自 `world.director_config.character_memory_candidate_limit`，默认 200，限制 20..600。
- 默认召回模式是 `hybrid`，也支持 `lexical_only` 和 `semantic_only`。
- 语义权重来自 `world.director_config.character_memory_semantic_weight`，默认 0.65。
- 语义模型来自 settings/model config；内置模型是 `BAAI/bge-small-zh-v1.5`，可走本地或远端 embedding。
- lexical 分数会考虑内容、speaker、location、participants、keywords、scene_id、conversation_id、importance、layer bonus、recency bonus。
- 排序后做层配额平衡，优先保留若干 `working`、`short_term`、`archive`、`canonical_event`，确保 archive 长期记忆不会被近期噪声完全挤掉。

### 进入角色 prompt 的方式

`src-tauri/src/services/game_engine/orchestrator/turn_context.rs` 构造角色 turn payload：

- `matched_memories`: 召回命中的记忆。
- `hit_turns`: 命中记忆对应 turn。
- `event_timeline`: 命中 turn 附近窗口内的事件记忆。
- `dialogue_focus`: 命中 turn 附近窗口内的原始对话。

窗口配置来自 `world.director_config`：

- `character_memory_hit_turns`
- `character_memory_event_window_rounds`
- `character_memory_dialogue_window_rounds`

## 自定义属性项

### 当前设计

属性由两层组成：

1. `attribute_schemas`: 属性定义。
2. `attribute_values`: 某个 owner 上的属性值。

schema 字段：

- `scope`: `world`、`character`、`session`、`session_character`
- `key`
- `label`
- `value_type`: `text`、`number`、`boolean`、`list`、`json`
- `description`
- `default_value`
- `enum_options`
- `display_policy`
- `access_policy`
- `mutation_policy`
- `influence_policy`
- `projection_policy`

value 字段：

- `schema_id`
- `owner_type`
- `owner_id`
- `value`
- `source`

`attribute_values` 对 `(schema_id, owner_type, owner_id)` 唯一，`upsert_value()` 用 `INSERT OR REPLACE`。

### 编辑入口

前端组件是 `frontend/src/components/AttributePanel.tsx`。

当前它用于：

- `WorldEditorPage`: 编辑世界级、角色共享类自定义属性。
- `CharacterEditorPage`: 编辑角色属性。

注意：该组件里有不少已有 mojibake 文案。不要做全局编码修复，除非明确进行编码清理任务。

`AttributePanel` 创建 schema 时会填默认策略：

- `display_policy`: editor/debug 可见，game 默认不可见。
- `access_policy`: creator/director/plugin 可读；角色 scope 默认 agent_self_read。
- `mutation_policy`: creator 可写，只允许 set。
- `influence_policy`: 默认投射到 director prompt 和 UI status panel。
- `projection_policy`: 默认继承到 session；world -> `session`，character -> `session_character`。

### 运行时投影和写回

属性命令在 `src-tauri/src/commands/attributes.rs`：

- `list_attribute_schemas`
- `create_attribute_schema`
- `update_attribute_schema`
- `list_attribute_values`
- `upsert_attribute_value`

运行时写回在 `src-tauri/src/services/game_engine/runtime_effects.rs`：

- 解析导演输出的 `session_attribute_updates` 或旧名 `attribute_updates`。
- 解析 `character_attribute_updates`。
- 触发器和规则也可以追加属性更新。
- session 属性写为 `owner_type = "session", owner_id = session.id`。
- 角色会话属性写为 `owner_type = "session_character", owner_id = "{session_id}:{character_id}"`。

前端展示：

- `get_session_runtime_attributes()` 在 `orchestrator/run.rs` 聚合 session 和 session_character 属性。
- `useGameSession()` 把运行时属性转成 `attributeSideTabs`。
- `side_panel_tabs` 组件把 `attribute:*` tab 显示到侧栏/移动状态抽屉。

角色 prompt 可见性：

- `load_character_visible_attribute_lines()` 读取 session 和 session_character 值。
- session 属性目前要求 `access_policy.agent_self_read = true` 才对角色可见。
- session_character 属性根据 owner 是否是当前角色，分别看 `agent_self_read` 或 `agent_other_read`。
- 可见属性会进入 `visibility_context.visible_attribute_lines` 和 `scene_state.visible_attributes`。

### 未优化/需小心点

这块还没有完全成熟，开发时不要误判为完整策略系统：

- `display_policy/mutation_policy/influence_policy/projection_policy` 已入库，但并非所有策略字段都被完整执行。
- 编辑器默认只处理 `world` 和 `character` scope，运行时实际大量使用 `session` 和 `session_character` owner。
- `owner_label` 对 session_character 当前从 `owner_id` 截取 character id，不一定显示角色名。
- UI status panel 目前把属性 tab 内容做成文本摘要，不是结构化表格。
- `AttributePanel` 文案存在 mojibake，改这个文件要特别注意编码。

## 移动世界 UI 规则

移动端世界 UI seed 和 runtime component 要特别注意：

- 顶部需要 safe area，不能贴状态栏。
- 右侧状态抽屉 handle 不能遮住标题/地点文字，标题要省略。
- 移动端自定义属性 tab 应在侧边/状态抽屉里，不要塞进聊天列。
- 移动聊天流只放叙事、角色/玩家消息；调试、导演 trace、retry 管理卡不要进入普通移动聊天流。
- 移动输入框使用两行布局：textarea 一整行，图片/语音/发送按钮下一行。
- 图片/语音按钮用图标按钮；发送可用短中文文案。

## 运行时特殊机制

### 回合恢复和幂等写回

后端游戏回合不是一次性直接写完所有状态，而是带 recovery journal 的分阶段写回。常见 journal step 包括：

- `attributes_committed`
- `memory_committed`

相关位置：

- `src-tauri/src/services/game_engine/memory.rs`
- `src-tauri/src/services/game_engine/orchestrator/writeback.rs`
- `src-tauri/src/services/game_engine/orchestrator/run.rs`

意义：

- LLM 调用、运行时效果、属性提交、记忆提交之间任何一步失败，都可以通过 journal 判断哪些步骤已经完成。
- 重试/恢复时不能重复写属性或重复插入同一轮记忆。
- 调试“某轮为什么没有写入/为什么重复写入”时，要先看 recovery journal，而不是只看最终 session。

开发注意：

- 新增会产生持久化副作用的回合步骤时，应考虑是否需要 journal step。
- 写入属性、记忆、存档快照这类副作用时，要保证重试不会造成重复提交或状态漂移。

### 存档分支复制

分支存档不是只复制 `sessions` 一行。`src-tauri/src/db/repositories/save_repo.rs` 会复制：

- session 快照。
- save 元信息和 branch 信息。
- 当前 session 下的 memories。
- runtime 属性值，包括 `owner_type = "session"` 和 `owner_type = "session_character"`。

`session_character` 属性 owner id 形如：

```text
{session_id}:{character_id}
```

分支时需要把旧 session id 替换成新 session id，否则新分支会读到旧分支角色属性，或者丢失角色运行时属性。

调试分支问题时优先查：

- `copy_branch_memories()`
- attribute value 复制逻辑。
- `restore_runtime_attribute_values()`
- `collect_runtime_attribute_values()`

### 导演输出契约

导演/LLM 输出不是简单文本。后端会把结构化输出解析成 runtime application，再分别应用到 session、场景、属性、记忆、道具、规则和 UI 可见状态。

关键位置：

- `src-tauri/src/services/game_engine/director.rs`
- `src-tauri/src/services/game_engine/structured_output.rs`
- `src-tauri/src/services/game_engine/runtime_effects.rs`
- `src-tauri/src/services/game_engine/orchestrator/writeback.rs`

常见结构化字段：

- `world_phase` / `state_phase`
- `next_location`
- `next_scene_name`
- `next_scene_background_hint`
- `scene_visible_characters`
- `session_attribute_updates`，旧兼容名 `attribute_updates`
- `character_attribute_updates`
- `memory_entries`
- `inventory_items`
- `generated_characters`
- `tool_calls` / MCP 工具相关结果

改 prompt、改结构化输出、改 runtime 写回时要同时检查：

- 字段是否在 structured output schema 中允许。
- `runtime_effects.rs` 是否解析并应用。
- `writeback.rs` 是否保存到 session/messages/system_log。
- Debug 页面和 prompt trace 是否还能解释这次回合。

### 世界 UI 安全边界

世界 UI 是“声明式可定制”，不是“世界包可执行代码”。

世界 UI 文档允许：

- 声明 layout tree。
- 使用已注册组件。
- 写 props、slot、when、for_each。
- 使用简单 binding，例如 `$session.location`。
- 使用受限 safe expression。
- 写被 scoped 的 `custom_css`。

世界 UI 文档不应允许：

- 任意 JavaScript。
- 动态 import。
- 直接访问 Tauri command。
- 绕过 `gameUiRuntime/actions.ts` 执行业务行为。

安全边界在这些位置共同维护：

- `frontend/src/components/GameUiRenderer.tsx`
- `frontend/src/gameUiRuntime/registry.tsx`
- `frontend/src/gameUiRuntime/actions.ts`
- `frontend/src/gameUiRuntime/binding.ts`
- `frontend/src/gameUiRuntime/expression.ts`
- `src-tauri/src/services/game_ui.rs`

如果新增 UI 能力，需要通过 registry/action/schema 显式暴露，而不是让 JSONC 文档获得任意执行能力。

### 资源路径重映射

世界包、上传资源、运行时背景和角色立绘共用一套路径重映射思路。世界包导入时：

1. 读取 zip 内 manifest。
2. 把 assets 写入本地 assets root。
3. 构建 `asset_map`。
4. 调 `remap_world_ui_theme_assets()` 替换世界 UI 和资源配置里的旧路径。

相关位置：

- `src-tauri/src/services/world_package.rs`
- `src-tauri/src/commands/uploads.rs`
- `frontend/src/game/useGameSession.ts`
- `frontend/src/data/gameUi/parser.ts`

容易出问题的资源：

- `ui_theme_config.assets.local_background_assets`
- `ui_theme_config.assets.local_scene_backgrounds`
- UI `custom_css` 间接依赖的背景变量。
- 角色 `avatar_asset` 和 `portrait_assets`
- 运行时生成的场景背景/图片。

世界包导入后如果 UI 样式还在但图没了，优先检查 asset_map 是否覆盖了 UI 文档和角色资源两条链路。

角色/智能体头像使用单独的 `avatar_asset` 字段保存单张资源路径，和 `portrait_assets` 立绘列表分开。角色编辑器、角色卡片、模板导入导出、世界包导入导出、世界复制和资源清理都要同时维护这个字段；没有头像时前端列表可以回退显示第一张立绘，但不要把头像写回立绘数组。

### 移动端可视高度和键盘

移动端游戏 UI 不能简单用 `100vh` 或 `100dvh` 锁死根容器。原因是手机键盘弹起、浏览器/系统状态栏、安全区都会改变真实可用高度。

相关约束：

- 使用 runtime 可视高度变量，例如 `--game-visual-viewport-height`。
- 顶部要考虑 `env(safe-area-inset-top, 0px)` 加真实 fallback。
- 底部输入区要考虑 `env(safe-area-inset-bottom, 0px)`。
- 聊天消息区滚动，输入框固定/贴底，不能让整个页面被键盘顶坏。
- 状态抽屉 handle 要避开右侧安全区和标题区域。

后端 `game_ui.rs` 对移动 UI 文档有部分治理警告，会提示固定 viewport height、缺少 `input_composer`、缺少 `side_panel_tabs` 等问题。

### 调试入口

复杂回合问题不要只看最终 UI。优先看 debug commands 和 Debug 页面聚合出的中间产物。

相关位置：

- `frontend/src/pages/DebugPage.tsx`
- `frontend/src/components/PromptTraceView.tsx`
- `src-tauri/src/commands/debug/`

Debug 信息通常能看到：

- prompt trace。
- director/runtime payload。
- recovery journal。
- grouped memories。
- runtime session/character attributes。
- 事件链和回合写回结果。
- LLM retry/error 信息。

排查顺序建议：

1. 看本轮 prompt trace 和导演结构化输出是否符合预期。
2. 看 runtime payload 是否被解析成属性/记忆/场景更新。
3. 看 journal 是否停在某个 step。
4. 看 session/messages/system_log 是否写回。
5. 最后再看前端 UI 文档和组件渲染。

## 开发检查

项目级约定和架构变化要同步记录在 `project.md`。如果修改了世界包格式、UI DSL、运行时安全边界、提示词/记忆/属性管线、导入导出规则、提交或忽略规则，改代码时也要更新本文件。

角色提示词和世界主控提示词允许保存模板变量。当前支持 `{{current_time}}` / `{{当前时间}}`，发送给模型前展开为本机当前时间，格式为 `YYYY-MM-DD HH:mm:ss`；数据库和编辑器里保留原始模板文本。

源码目录不能被生成数据忽略规则遮住。根目录运行时数据使用 `/data/`，前端本地运行库数据使用 `/frontend/data/`；不要用裸 `data/` 规则误伤 `frontend/src/data/`。

常用检查：

```powershell
cd frontend
npx tsc --noEmit --pretty false
npm run build

cd ../src-tauri
cargo check
```

如果出现大量 `unknown prefix`、raw string 未闭合、JSX closing tag 爆炸、JSONC 后段莫名解析失败，优先怀疑前面某个中文/字符串边界被编码损坏。

本项目有已存在 mojibake 文件。编辑 TSX/Rust/JSONC/seed 时：

- 优先 `apply_patch`。
- 不要用 PowerShell `Set-Content` 改含中文的源码。
- 不要做大范围 search/replace。
- 如果必须脚本改，用 Node `fs.readFileSync(path, "utf8")` / `fs.writeFileSync(path, text, "utf8")`，范围要窄。
- 不要全局修复 mojibake，除非任务就是编码清理。

## 快速定位

- 游戏运行时布局不对：`world.ui_theme_config -> parseGameUiDocument -> GameUiRenderer`。
- 某组件不显示：UI 文档 `component` 名 -> `gameUiRuntime/registry.tsx`。
- 某动作无效：UI 文档动作 -> `gameUiRuntime/actions.ts` -> `services/game_ui.rs` 支持列表。
- 世界包 UI 丢失：`world_package.rs -> manifest -> ui_theme_config_json -> asset_map`。
- 角色记忆串线：查 `memories.character_id`、`visible_characters`、`build_turn_entries()`、`recall_entries_for_character()`。
- 长期记忆召回不到：查 `layer = archive` 是否写入、候选上限、retrieval mode、embedding 设置、层配额排序。
- 属性不进 prompt：查 `attribute_values.owner_type/owner_id`、schema `access_policy.agent_self_read/agent_other_read`、`load_character_visible_attribute_lines()`。
- 属性不在 UI 里显示：查 `get_session_runtime_attributes()`、`buildAttributeSideTabsFromRuntimeAttributes()`、`side_panel_tabs.show_attribute_tabs`。

## 运行时上下文提示词

世界编辑器有独立的“运行时上下文”Tab，内容保存到 `world.director_config.runtime_context_prompt`。角色编辑器也有独立的“运行时上下文”Tab，内容保存到 `character.runtime_system_prompt`。两者都是可编辑内容数据；世界级字段随 `director_config_json` 保存，角色级字段使用已有的 `characters.runtime_system_prompt` 列保存，并随角色模板、世界包导入导出流转。

发送给模型前，后端会对这些字段执行模板变量替换；当前支持 `{{current_time}}` / `{{当前时间}}`，格式为本机当前时间 `YYYY-MM-DD HH:mm:ss`。世界级运行时上下文会作为独立 `system` 消息注入世界主控和角色请求；角色级运行时上下文会作为独立 `system` 消息注入该角色请求。字段为空时不发送。Prompt 预览中世界级模块显示为 `runtime_context`，角色级模块显示为 `character_runtime_context`。

内置行程助手通过 seed 世界配置填写 `runtime_context_prompt`，而不是在代码里做行程助手特判。

角色回复解析会读取 JSON 里的 `content` / `response` / `message` / `text` 作为聊天正文；如果模型只返回 `session_attribute_updates` 这类状态更新 JSON，解析器要保留 raw payload 供写回，但聊天气泡使用自然语言兜底文案，不直接显示裸 JSON。行程助手 seed 提示词必须要求结构化回复同时包含 `response` 和 `session_attribute_updates`。

运行时 UI 展示的 session/character 属性不只依赖重新进入页面加载。`useGameSession()` 在会话快照变化时会刷新运行时属性；玩家动作、重试动作完成后也会主动短窗口刷新一次运行时属性，覆盖 agent_chat 等流程先写属性、后续快照不携带属性明细的时序。世界包里的 checkbox/list/badge 等组件应读取同一份运行时属性，不要为某个内置世界写专属前端刷新逻辑。

行程助手内置 desktop/mobile UI 使用世界包 `custom_css` 美化待办和已完成 checkbox 列表：两个列表分别有独立边框容器，确认完成按钮是渐变背景的圆角矩形主按钮。这类视觉调整应优先落在 seed UI 文档，避免写成运行时组件特判。

`scene_header` 支持 `show_copy_button` 布尔属性控制标题区复制对话按钮，默认保持显示；行程助手 desktop/mobile UI 将其设为 `false`，避免标题下出现复制按钮。

安卓通知权限由原生 Android 入口请求：`src-tauri/gen/android/app/src/main/AndroidManifest.xml` 声明 `POST_NOTIFICATIONS`，`MainActivity` 在 Android 13+ 启动时通过 `ActivityResultContracts.RequestPermission()` 请求通知权限；这两个文件在 `.gitignore` 中被显式 unignore，必须随 Android 通知逻辑一起提交。`useGameSession()` 在发送可能创建提醒/定时事项的消息前只检查通知权限，不再通过 JS 插件调用 `requestPermission()`。麦克风权限保持在语音按钮点击后通过 `getUserMedia({ audio: true })` 按需触发。后端 `ensure_notification_permission()` 在移动端只检查权限状态，不再从 Rust 工具调用路径直接触发 `request_permission()`，避免 Tauri 通知插件的 `requestPermissionsLauncher` 未初始化错误污染行程助手回复。

`schedule_notification` 工具的 `time` 参数描述会传给模型，但工具端不能假设模型一定严格输出首选格式。解析器接受带时区 RFC3339、相对时间，以及本地时间 `YYYY-MM-DD HH:MM[:SS]` / `YYYY-MM-DDTHH:MM[:SS]`；无时区格式按本机本地时间解释后转 UTC 存储。

MCP 工具定义包含可编辑的 `input_schema` JSON Schema。工具管理页保存 `input_schema`，后端持久化到 `mcp_tools.input_schema_json`；世界主控构建 `available_tools` 时，会把世界已授权、已启用、暴露策略不是 `disabled` 的自定义工具连同 `arguments_schema` 发给模型。当前通用 MCP 执行器尚未实现，模型调用非内置工具时后端会返回明确的未实现工具错误，避免静默吞掉调用。

Android APK 打包脚本 `scripts/build_android_apk.ps1` 会在调用 Tauri build 前清理 Android 构建目录中复制出来的 `embedding-models` 资源副本，并做一次临时复制预检，用来提前暴露 Windows `os error 1224` 这类 safetensors 文件被 mmap/进程占用的问题。Tauri build 失败后不要再用 Gradle `assembleUniversalRelease` 作为兜底重试；生成的 Android Studio Gradle task 依赖 Tauri CLI 的 live context，脱离 Tauri build 直接运行会触发 `android-studio-script` WebSocket 连接失败，不能修复资源复制错误。

移动端消息操作按钮需要位于气泡外侧，不要放在气泡同一背景里。复制成功通过全局 toast 提示“已复制”；复制/分支等小图标按钮有按压缩放反馈；编辑/重发文字按钮在移动端使用更小字号。行程助手移动 UI 也在 seed `custom_css` 中补了同样的按钮尺寸和按压状态，避免世界包样式覆盖通用规则。
