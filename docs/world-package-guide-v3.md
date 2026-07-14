# Cloud Dream Engine 世界包开发指南 v3

本文面向世界包设计者，说明如何为 Cloud Dream Engine 制作可导入、可导出、同时支持桌面与 Android 的世界包和游戏 UI。

v3 的核心原则是：世界包拥有游戏页面的结构和视觉设计，应用拥有可信能力与数据写入。世界包可以提供 JSONC、CSS 和资源，但不能提供或执行 JavaScript。

## 1. 先理解两个版本号

世界 UI 同时存在两个不同层级的版本号：

| 字段 | 当前值 | 控制内容 |
|---|---:|---|
| `runtime_version` | `3` | iframe 隔离、桌面/移动双入口、原始 stylesheet、父页面能力桥接 |
| `schema_version` | `2` | 单份 UI 文档中的布局节点、组件、状态、绑定和动作 |

因此，v3 世界包中的 UI 文档仍应写成：

```jsonc
{
  "schema_version": 2,
  "layout": {
    "root": {
      "type": "stack",
      "children": []
    }
  }
}
```

不要把 UI 文档写成 `schema_version: 3`。当前客户端只支持 UI 文档 schema 2。

## 2. v3 运行边界

每个游戏页面运行在 `sandbox="allow-scripts"` 的隔离 iframe 中。iframe 内只执行引擎自带的可信渲染器，世界包不能附带脚本。

世界包可以控制：

- 桌面端和移动端各自完整的布局树。
- 页面内所有世界 UI 的颜色、字体、间距、尺寸、层级、动画和响应式规则。
- 注册组件的排列、显示条件、属性和插槽。
- 文本、图片、徽标、按钮、复选框、循环和条件节点。
- 通过已注册 action 请求发送消息、复制、导航、录音、选图、重试和分支等操作。

世界包不能直接控制：

- Tauri、Rust、SQLite、文件系统和模型 API。
- 应用首页、设置页、世界编辑器等游戏页之外的 UI。
- iframe 外的 DOM、CSS、剪贴板、系统权限和导航历史。
- 任意 JavaScript、远程脚本、`eval` 或网络请求。

图片选择、麦克风权限、录音、剪贴板、导航和游戏状态写入都由父页面执行。iframe 只接收可序列化快照并发送类型化动作。

## 3. 推荐开发流程

1. 在应用中创建或复制一个世界。
2. 打开“世界工坊 -> 界面风格”。
3. 分别编辑桌面端与移动端的 Raw JSONC。
4. 分别编辑桌面端与移动端的“v3 原始样式表”。
5. 使用内置桌面/移动预览检查布局。
6. 查看“界面治理”的编译、依赖和兼容性结果。
7. 在真实桌面会话和 Android 会话中测试交互。
8. 从应用导出 `.zip` 世界包，不建议手工拼装生产包。

本地开发应用：

```powershell
npm run tauri:dev
```

普通浏览器页面只能检查布局和 iframe 基础行为，无法完整测试 Tauri IPC、资源协议、文件、录音和原生权限。

## 4. v3 双入口配置

数据库中的 `ui_theme_config` 使用下面的逻辑结构。应用编辑器会自动生成它，世界包设计者通常不需要手写整段配置。

```jsonc
{
  "runtime_version": 3,
  "capabilities": [
    "supports_file_picker",
    "supports_mic"
  ],
  "assets": {
    "background_source_mode": "local-first",
    "portrait_source_mode": "local-first",
    "runtime_image_generation_enabled": false,
    "local_background_assets": [],
    "local_scene_backgrounds": {}
  },
  "entries": {
    "desktop": {
      "document": "<桌面 UI JSONC 字符串>",
      "stylesheet": "<桌面 CSS 字符串>"
    },
    "mobile": {
      "document": "<移动 UI JSONC 字符串>",
      "stylesheet": "<移动 CSS 字符串>"
    }
  }
}
```

### `runtime_version`

用途：选择世界 UI 运行架构。

何时使用：新世界统一使用 `3`。旧世界缺少该字段时按 v2 兼容模式归一化。

### `capabilities`

用途：声明世界 UI 需要的可信能力。

当前可声明值：

| 值 | 含义 |
|---|---|
| `supports_file_picker` | 需要父页面提供图片选择 |
| `supports_mic` | 需要父页面提供麦克风和录音 |
| `supports_hover` | UI 存在桌面 hover 交互 |

声明不受支持的能力会导致 bundle 校验失败。运行时仍会根据设备实际能力提供 `capabilities` 数据，声明本身不会绕过系统权限。

### `entries.desktop` 与 `entries.mobile`

用途：保存两套独立、完整的 UI 入口。

何时使用：始终同时提供。不要依赖一份桌面文档通过 CSS 缩放成手机 UI。

每个入口包含：

| 字段 | 用途 |
|---|---|
| `document` | schema 2 UI 文档，负责结构和组件 |
| `stylesheet` | v3 原始 CSS，负责该平台的完整视觉设计 |

## 5. 导出包目录结构

当前世界包格式为 `dream-world-package` version 6。应用导出的 ZIP 结构如下：

```text
manifest.json
world/
  world.json
  ui.desktop.jsonc
  ui.mobile.jsonc
  ui.desktop.css
  ui.mobile.css
characters/
  <角色目录>/character.json
assets/
  <世界与角色资源>
```

空 stylesheet 可能不会写入 ZIP，但 manifest 中仍会保留入口路径。导入器兼容 version 5 世界包，并将旧的 `desktop_file` / `mobile_file` 归一化为双入口。

manifest 中与 UI 有关的字段：

```json
{
  "format": "dream-world-package",
  "version": 6,
  "world_file": "world/world.json",
  "desktop_ui_file": "world/ui.desktop.jsonc",
  "mobile_ui_file": "world/ui.mobile.jsonc",
  "ui_runtime_version": 3,
  "desktop_ui_stylesheet_file": "world/ui.desktop.css",
  "mobile_ui_stylesheet_file": "world/ui.mobile.css"
}
```

`world/world.json` 保存世界设定、导演配置、资源配置、`ui_runtime_version` 和 `ui_capabilities`。角色数据与资源路径通过 manifest 管理，导入时会重新映射为本机资源路径。

## 6. UI 文档顶层字段

```jsonc
{
  "schema_version": 2,
  "meta": {
    "name": "My desktop UI"
  },
  "tokens": {
    "color-accent": "#2563eb",
    "radius-md": "8px"
  },
  "components": {},
  "state": {
    "selected_items": []
  },
  "layout": {
    "root": {
      "type": "stack",
      "children": []
    }
  },
  "custom_css": ""
}
```

| 字段 | 必需 | 用途 |
|---|---|---|
| `schema_version` | 是 | 当前必须为 `2` |
| `layout.root` | 是 | 页面布局树根节点 |
| `state` | 否 | 文档本地交互状态，例如复选框选择列表 |
| `tokens` | 否 | 生成 `--game-ui-token-*` CSS 变量 |
| `components` | 否 | 注册组件的 base / variant 样式定义 |
| `meta` | 否 | 作者、名称、说明等元数据 |
| `custom_css` | 否 | v2 兼容 CSS；新 v3 世界优先使用入口 stylesheet |
| `mounts` | 否 | 旧 mount 兼容字段，新文档不应依赖它扩展功能 |

## 7. 布局节点

所有节点都可使用以下通用字段：

| 字段 | 用途 |
|---|---|
| `id` | 节点标识 |
| `visible` | 设为 `false` 时不渲染 |
| `class_name` | 添加作者自定义 class |
| `area` | 指定 CSS Grid area |
| `width` / `height` | 尺寸 |
| `min_width` / `min_height` | 最小尺寸 |
| `max_width` / `max_height` | 最大尺寸 |
| `padding` / `margin` | 内外边距 |
| `align` / `justify` | 对齐方式 |
| `style` | React inline style 格式的键值对象 |

### `grid`

```jsonc
{
  "type": "grid",
  "columns": ["minmax(0, 1fr)", "320px"],
  "rows": ["auto", "minmax(0, 1fr)", "auto"],
  "areas": [
    ["header", "header"],
    ["chat", "side"],
    ["input", "side"]
  ],
  "gap": "12px",
  "children": []
}
```

用途：桌面多栏布局、固定区域布局。

### `stack`

```jsonc
{
  "type": "stack",
  "direction": "vertical",
  "gap": "10px",
  "wrap": false,
  "children": []
}
```

用途：普通文档流、移动端纵向布局、工具栏横向布局。

### `absolute`

```jsonc
{
  "type": "absolute",
  "children": []
}
```

用途：浮动控制、覆盖层和装饰层。该容器默认不接收指针事件，带 `anchor` 的组件会恢复指针事件。

### `component`

```jsonc
{
  "type": "component",
  "component": "message_list",
  "class_name": "world-chat",
  "props": {
    "auto_scroll": true,
    "mobile_simple": false
  }
}
```

用途：调用引擎注册组件。不能填写任意 React 组件名。

### `text`

```jsonc
{
  "type": "text",
  "text": "当前位置：{{ session.location }}",
  "variant": "caption"
}
```

### `image`

```jsonc
{
  "type": "image",
  "src": "$scene_focus.portrait_path",
  "alt": "{{ scene_focus.speaker }}",
  "fit": "cover"
}
```

`fit` 可用值：`cover`、`contain`、`fill`、`none`、`scale-down`。

### `badge`

```jsonc
{
  "type": "badge",
  "text": "{{ session.time_label }}",
  "variant": "info"
}
```

### `button`

```jsonc
{
  "type": "button",
  "label": "发送调查指令",
  "variant": "primary",
  "action": {
    "id": "submit_message",
    "mode": "submit",
    "content_template": "调查 {{ session.location }}"
  }
}
```

`disabled_when_empty_state` 可指向 `state` 中的数组字段，数组为空时禁用按钮。

### `checkbox`

```jsonc
{
  "type": "checkbox",
  "label": "{{$item.name}}",
  "value": "$item.id",
  "bind_checked_list": "selected_items"
}
```

用途：维护文档本地字符串数组状态。

### `when`

```jsonc
{
  "type": "when",
  "expr": "capabilities.supports_hover == true && attributes.energy > 0",
  "child": {
    "type": "text",
    "text": "桌面悬停提示可用"
  }
}
```

支持：`==`、`!=`、`>`、`>=`、`<`、`<=`、`&&`、`||`、括号、字符串、数字、布尔值、`null` 和点路径。

不支持：函数调用、数组下标、模板字符串、对象字面量、赋值和任意 JavaScript。

### `for_each`

```jsonc
{
  "type": "for_each",
  "source": "visible_characters",
  "item_as": "character",
  "index_as": "index",
  "empty": {
    "type": "text",
    "text": "当前无人"
  },
  "child": {
    "type": "badge",
    "text": "{{ index }}. {{ character }}"
  }
}
```

## 8. 注册组件

### `scene_header`

场景标题、世界、地点、时间、玩家和在场角色。

Props：`show_world_name`、`show_location`、`show_time_label`、`show_player_identity`、`show_visible_characters`、`show_copy_button`、`player_identity_format`、`title_mode`。

`player_identity_format`：`label` 或 `action_phrase`。`title_mode`：`desktop` 或 `mobile`。

### `scene_focus`

当前发言角色头像和焦点台词。

Props：`show_avatar`、`show_line`、`avatar_variant`。

### `character_bar`

在场角色列表。

Props：`empty_text`、`max_items`、`show_player`。

### `narration_card`

最新旁白区域。

Props：`title`、`show_copy_button`、`empty_text`。

### `message_list`

完整聊天、流式状态、思维链、消息动作、失败重试和角色切换提议。

Props：

| Prop | 用途 |
|---|---|
| `auto_scroll` | 新消息时自动滚动 |
| `show_pending_state` | 显示待处理消息 |
| `show_agent_reasoning` | 显示导演/NPC 思维链 |
| `show_typing_indicator` | 显示等待输入指示 |
| `mobile_simple` | 移动端精简消息流 |

### `input_composer`

输入、编辑、图片、录音和发送区域。文件选择和麦克风由父页面执行。

Props：`placeholder`、`submit_label`、`editing_submit_label`、`show_image_button`、`show_audio_button`、`show_session_meta`、`enter_to_submit`。

### `side_panel_tabs`

地图和自定义属性标签。移动端会作为状态抽屉呈现。

Props：`show_map_tab`、`show_attribute_tabs`、`empty_text`、`drawer_label`。

支持 `content` slot，用于自定义当前标签内容。

### `floating_actions`

返回、调试和设置入口。

Props：`show_back`、`show_debug`、`show_settings`、`back_label`、`debug_label`、`settings_label`、`layout`。

`layout`：`row`、`column` 或 `wrap`。

## 9. 运行时数据与绑定

直接绑定使用 `$路径`，内嵌文本使用 `{{ 路径 }}`。

```jsonc
{
  "type": "text",
  "text": "{{ world.name }} / {{ session.location }}"
}
```

```jsonc
{
  "type": "component",
  "component": "character_bar",
  "props": {
    "show_player": "$state.show_player"
  }
}
```

直接 `$binding` 会保留布尔、数字、数组和对象类型；`{{ }}` 模板始终输出字符串。

主要数据路径：

| 路径 | 内容 |
|---|---|
| `session` | `id`、`world_name`、`location`、`time_label`、`player_character_name`、`visible_characters` |
| `world` | 当前世界 `id`、`name` |
| `player` | 当前玩家角色 `id`、`name` |
| `attributes` | 会话属性和当前玩家角色属性的扁平视图 |
| `attributes_by_owner` | 按 owner type / owner id 分组的完整属性 |
| `attribute_items` | 属性条目数组 |
| `messages` | 当前渲染消息数组 |
| `visible_characters` | 在场角色名称数组 |
| `capabilities` | `platform`、`supports_mic`、`supports_file_picker`、`supports_hover` |
| `ui_state` | 加载、提交、流式、分支、切换和重试状态 |
| `errors` | 当前 action 错误 |
| `side_tabs` | 可用侧栏标签 |
| `active_side_tab` | 当前侧栏标签 key |
| `active_attribute_content` | 当前属性标签内容 |
| `scene_focus` | 当前焦点发言者、内容和头像路径 |
| `latest_narration` | 最新旁白 |
| `draft_input` | 草稿文本、附件、录音状态和麦克风错误 |
| `viewport` | 宽高、键盘高度、偏移和 safe area |
| `state` | 当前 UI 文档本地状态 |

## 10. 动作

动作只能请求父页面执行，世界包不能绕过参数校验或直接访问系统 API。

| Action | 参数 | 用途 |
|---|---|---|
| `submit_message` | `mode?`、`content?`、`turn_index?` | 发送、编辑或重发输入 |
| `edit_turn_start` | `content`、`turn_index` | 开始编辑玩家回合 |
| `edit_turn_cancel` | 无 | 取消编辑 |
| `branch_from_current` | 无 | 从当前状态创建分支 |
| `retry_turn` | `retry_token` | 重试失败模型步骤 |
| `accept_switch_proposal` | `proposal_key` | 接受角色切换提议 |
| `dismiss_switch_proposal` | `proposal_key` | 忽略角色切换提议 |
| `dismiss_retry_card` | `card_key` | 关闭重试卡片 |
| `copy_text` | `text` | 请求父页面复制文本 |
| `switch_side_tab` | `tab_key` | 切换地图/属性标签 |
| `navigate_back` | 无 | 返回上一应用页面 |
| `navigate_home` | 无 | 返回首页 |
| `navigate_settings` | 无 | 打开设置 |
| `navigate_debug` | 无 | 打开当前会话调试页 |
| `pick_image` | 无 | 打开父页面图片选择器 |
| `remove_image` | `index` | 移除草稿图片 |
| `start_recording` | 无 | 请求录音 |
| `stop_recording` | 无 | 停止录音并附加文件 |
| `remove_audio` | `index` | 移除草稿录音 |

动作参数支持 `$binding` 和 `{{ }}` 模板：

```jsonc
{
  "type": "button",
  "label": "复制地点",
  "action": {
    "id": "copy_text",
    "args": {
      "text": "$session.location"
    }
  }
}
```

## 11. v3 原始 CSS

v3 stylesheet 在世界 iframe 内原样注入，不做 selector 前缀改写。它可以重排、覆盖或隐藏世界页面内的任何元素，但不能影响 iframe 外的应用。

推荐以稳定选择器为入口：

```css
.game-root[data-world-frame-runtime="3"] {
  color: #e8ecf3;
  background: #0c111b;
}

[data-component="message_list"] {
  min-height: 0;
  overflow: hidden;
}

[data-component="input_composer"] {
  align-self: end;
}
```

可用根 class：

- `.game-root`
- `.game-root--desktop-session`
- `.game-root--mobile-session`
- `.game-ui-layout`
- `.game-ui-node`
- `.game-ui-component`
- `.game-ui-component--<组件名转短横线>`
- `[data-component="<组件名>"]`
- `[data-variant="<variant>"]`

世界文档中的 `class_name` 是最稳定的作者自定义 CSS 锚点。复杂主题应优先给关键节点添加自己的 class，而不是依赖很深的内部 DOM 层级。

### Token

文档 `tokens` 会转换为 CSS 变量：

```jsonc
{
  "tokens": {
    "color-accent": "#5eead4",
    "radius-md": "6px"
  }
}
```

```css
.world-send-button {
  color: var(--game-ui-token-color-accent);
  border-radius: var(--game-ui-token-radius-md);
}
```

### Safe area 与键盘

父页面向移动 iframe 提供：

```css
--game-visual-viewport-height
--world-safe-area-top
--world-safe-area-right
--world-safe-area-bottom
--world-safe-area-left
```

移动端建议：

```css
.game-root--mobile-session {
  height: var(--game-visual-viewport-height, 100dvh);
  padding-top: max(var(--world-safe-area-top, 0px), env(safe-area-inset-top, 0px));
  padding-right: max(var(--world-safe-area-right, 0px), env(safe-area-inset-right, 0px));
  padding-bottom: max(var(--world-safe-area-bottom, 0px), env(safe-area-inset-bottom, 0px));
  overflow: hidden;
}
```

### 资源背景

世界背景由资源配置和引擎解析，运行时通过 `--game-runtime-bg-image` 提供：

```css
.game-root {
  background-image: var(--game-runtime-bg-image, none);
  background-size: cover;
  background-position: center;
}
```

不要在 CSS 中写本机绝对路径。上传资源后使用世界资源配置，导出器会收集文件，导入器会重映射路径。

## 12. 桌面与 Android 设计要求

### 桌面端

- 可使用多栏 grid、hover、较高信息密度和固定侧栏。
- 主聊天列必须使用 `minmax(0, 1fr)`，避免长内容撑破布局。
- 消息区域和侧栏需要明确 `min-height: 0` 与滚动所有权。

### 移动端

- 使用独立 mobile document 和 stylesheet。
- 顶部必须预留 safe area，标题文字应截断，不得进入右侧状态/抽屉把手区域。
- 自定义属性放入状态抽屉，不要挤在聊天列顶部。
- 输入区采用两行：textarea 独占一行，图片、录音和发送按钮位于下一行。
- 聊天流聚焦叙事、角色/玩家发言和折叠思维链。
- 角色消息下使用复制/分支，玩家消息下使用编辑/重发。
- 不要用桌面 `transform: scale()` 模拟手机 UI。

## 13. 资源配置

```jsonc
{
  "assets": {
    "background_source_mode": "local-first",
    "portrait_source_mode": "local-first",
    "runtime_image_generation_enabled": false,
    "local_background_assets": [
      "worlds/my-world/backgrounds/main.webp"
    ],
    "local_scene_backgrounds": {
      "庭院": ["worlds/my-world/backgrounds/courtyard.webp"],
      "书房": ["worlds/my-world/backgrounds/study.webp"]
    }
  }
}
```

| 字段 | 用途 |
|---|---|
| `background_source_mode` | 场景背景来源策略 |
| `portrait_source_mode` | 角色立绘来源策略 |
| `runtime_image_generation_enabled` | 是否允许运行时生成图片 |
| `local_background_assets` | 通用背景候选 |
| `local_scene_backgrounds` | 按场景名称分组的背景候选 |

推荐通过世界编辑器上传资源，避免手工构造内部路径。

## 14. 校验与调试

世界编辑器会同时运行：

- 单文档 schema 编译。
- 桌面/移动 bundle 校验。
- 组件、action、capability 依赖收集。
- 当前客户端兼容性检查。

常见错误：

| 错误 | 原因 | 修复 |
|---|---|---|
| `unsupported_schema_version` | UI 文档不是 schema 2 | 改为 `schema_version: 2` |
| `unknown_component` | 使用未注册组件 | 使用本文组件表中的名称 |
| `unknown_component_prop` | prop 名称不受支持 | 检查组件 props 表 |
| `unknown_action` | action ID 不存在 | 检查动作表 |
| `invalid_binding` | binding 不是简单点路径 | 使用 `$session.location` 形式 |
| `unsafe_expression` | `when` 中出现函数、下标或脚本语法 | 改用安全表达式子集 |
| `stylesheet_too_large` | 单份 stylesheet 超过 1 MiB | 拆减 CSS 和内嵌数据 |
| `unsupported_declared_capability` | 声明未知能力 | 使用当前三种 capability |
| 一直显示“正在启动隔离界面” | 可信 frame bundle 未启动 | 查看 Tauri DevTools Console；世界 CSS 通常不是该错误来源 |

开发时至少检查：

```powershell
cd frontend
npx tsc --noEmit --pretty false
npm test
npm run build

cd ..\src-tauri
cargo check --tests
cargo test --lib
```

## 15. 从 v2 迁移到 v3

v2 数据不会被删除。当前迁移层会：

1. 读取旧 `desktop_file` 和 `mobile_file`。
2. 分别放入 `entries.desktop.document` 和 `entries.mobile.document`。
3. 保留 UI 文档的 `schema_version: 2`。
4. 保留文档内 `custom_css` 的 v2 scoped 行为。
5. 为两个 v3 stylesheet 初始化空字符串。

建议后续手工迁移：

1. 复制世界，保留原世界作为回退。
2. 将桌面和移动 UI 文档分别确认可解析。
3. 把需要完全自由控制的 CSS 移到对应 v3 stylesheet。
4. 移除旧 CSS 中依赖父页面 selector 的规则。
5. 为关键节点补充作者自己的 `class_name`。
6. 在桌面和 Android 真机分别检查 safe area、键盘、滚动和消息动作。

仓库保留两套迁移基线：

### 飞花令夜宴

- `src-tauri/src/db/seeds/assets/poetry-desktop-ui.jsonc`
- `src-tauri/src/db/seeds/assets/poetry-mobile-ui.jsonc`
- `src-tauri/src/db/seeds/feihualing_world.rs`

### 日程助手

- `src-tauri/src/db/seeds/assets/schedule-assistant-desktop-ui.jsonc`
- `src-tauri/src/db/seeds/assets/schedule-assistant-mobile-ui.jsonc`
- `src-tauri/src/db/seeds/schedule_assistant_world.rs`

`frontend/src/data/gameUi/migration.test.ts` 会逐字验证四份文档在迁移后未改变，并验证桌面与移动入口保持独立。Rust bundle 测试也会校验两套示例在 runtime v3 下仍受支持。

## 16. 最小完整示例

桌面文档：

```jsonc
{
  "schema_version": 2,
  "meta": { "name": "Minimal desktop" },
  "layout": {
    "root": {
      "type": "grid",
      "columns": ["minmax(0, 1fr)", "300px"],
      "rows": ["auto", "minmax(0, 1fr)", "auto"],
      "areas": [
        ["header", "header"],
        ["chat", "side"],
        ["input", "side"]
      ],
      "gap": "12px",
      "children": [
        { "type": "component", "component": "scene_header", "area": "header" },
        { "type": "component", "component": "message_list", "area": "chat" },
        { "type": "component", "component": "input_composer", "area": "input" },
        { "type": "component", "component": "side_panel_tabs", "area": "side" }
      ]
    }
  }
}
```

移动文档：

```jsonc
{
  "schema_version": 2,
  "meta": { "name": "Minimal mobile" },
  "layout": {
    "root": {
      "type": "stack",
      "class_name": "mobile-shell",
      "children": [
        {
          "type": "component",
          "component": "scene_header",
          "props": {
            "title_mode": "mobile",
            "show_visible_characters": false
          }
        },
        {
          "type": "component",
          "component": "message_list",
          "class_name": "mobile-messages",
          "props": {
            "mobile_simple": true,
            "show_agent_reasoning": true
          }
        },
        {
          "type": "component",
          "component": "input_composer",
          "class_name": "mobile-input"
        },
        {
          "type": "component",
          "component": "side_panel_tabs",
          "class_name": "mobile-status",
          "props": {
            "drawer_label": "状态"
          }
        }
      ]
    }
  }
}
```

移动 stylesheet：

```css
.mobile-shell {
  height: var(--game-visual-viewport-height, 100dvh);
  min-height: 0;
  padding-top: max(var(--world-safe-area-top, 0px), env(safe-area-inset-top, 0px));
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  overflow: hidden;
}

.mobile-messages {
  min-height: 0;
  overflow: hidden;
}

.mobile-input {
  min-width: 0;
}
```

## 17. 发布前检查表

- `runtime_version` 为 `3`。
- 两份 UI 文档都声明 `schema_version: 2`。
- desktop 和 mobile 都有独立完整入口。
- 没有 JavaScript、远程脚本或本机绝对路径。
- 所有组件、props、actions 和 capabilities 均通过治理校验。
- 桌面窗口缩放后没有横向溢出。
- Android 状态栏、右侧把手和底部手势区没有遮挡内容。
- 软键盘打开时消息区和输入区仍可用。
- 图片、录音、复制、编辑、重发、分支和重试经过真实会话测试。
- 导出后的 ZIP 可在另一份本地数据环境中重新导入。
