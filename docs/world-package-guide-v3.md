# Cloud Dream Engine 世界包开发指南 v3

本文面向世界包设计者，说明如何为 Cloud Dream Engine 制作可导入、可导出、同时支持桌面与 Android 的世界包和游戏 UI。

如果目标是模型驱动的剧情、冒险或角色扮演世界，请先按[叙事 RPG 世界包制作手册](world-package-rpg-cookbook.md)完成玩法闭环，再回到本文查字段。可复制的工程模板位于 [`examples/world-packages/narrative-rpg-starter`](../examples/world-packages/narrative-rpg-starter/)。

v3 的核心原则是：世界包拥有游戏页面的结构和视觉设计，应用拥有可信能力与数据写入。世界包可以提供 JSONC、CSS、资源和可选的受限 Worker 逻辑，但不能在宿主页面执行 JavaScript。

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

每个游戏页面运行在 `sandbox="allow-scripts"` 的隔离 iframe 中。iframe 内执行引擎自带的可信渲染器；世界包的可选 `logic.js` 不注入页面，而是在按次创建的独立 Worker 中执行。

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
- 宿主页面 JavaScript、远程脚本、`eval`、网络请求或不受限的长驻脚本。

世界包可以选择提供 `sandbox-js-v1` 逻辑文件。该文件只在独立 Worker 中运行，通过受控 SDK 访问本世界存储和已授权的平台能力，不能访问 DOM、Tauri、任意文件系统和网络。图片选择、麦克风权限、录音、剪贴板、导航和游戏状态写入仍由父页面执行；文件等平台能力经「manifest 声明 + 玩家允许」后由宿主（安卓上经 Kotlin 中间件）执行。

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
    "supports_mic",
    "supports_world_storage"
  ],
  "platform_features": ["file.pick", "file.read", "file.write"],
  "storage": {
    "kv_namespaces": ["preferences"],
    "collections": {
      "journal.entries": {
        "schema": {
          "type": "object",
          "required": ["date", "content"],
          "properties": {
            "date": { "type": "string", "maxLength": 10 },
            "content": { "type": "string", "maxLength": 4000 }
          },
          "additionalProperties": false
        }
      }
    }
  },
  "logic": {
    "runtime": "sandbox-js-v1",
    "source": "<导入后保存的 logic.js 源码>",
    "timeout_ms": 1000
  },
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
| `supports_world_records` | 需要宿主提供按当前世界隔离的结构化记录存储；仅 runtime v3 可用 |
| `supports_world_storage` | 需要通用 `records` / `kv` 存储桥或沙箱逻辑运行时 |

声明不受支持的能力会导致 bundle 校验失败。运行时仍会根据设备实际能力提供 `capabilities` 数据，声明本身不会绕过系统权限。

### `platform_features`

用途：声明世界包要调用的平台能力（第 12 项）。当前目录：`file.pick`、`file.read`、`file.write`、`file.share`。声明了未知值会导致导入失败。

声明只是"申请资格"：玩家还要在「设置 → 世界权限」里逐项允许（默认全关），世界包的 `api.platform.invoke` 才能真正执行。详见第 10 节「平台能力」。

### `entries.desktop` 与 `entries.mobile`

用途：保存两套独立、完整的 UI 入口。

何时使用：始终同时提供。不要依赖一份桌面文档通过 CSS 缩放成手机 UI。

每个入口包含：

| 字段 | 用途 |
|---|---|
| `document` | schema 2 UI 文档，负责结构和组件 |
| `stylesheet` | v3 原始 CSS，负责该平台的完整视觉设计 |

## 5. 导出包目录结构

当前世界包格式为 `dream-world-package` version 7。应用导出的 ZIP 结构如下：

```text
manifest.json
world/
  world.json
  ui.desktop.jsonc
  ui.mobile.jsonc
  ui.desktop.css
  ui.mobile.css
  logic.js              # 可选
characters/
  <角色目录>/character.json
assets/
  <世界与角色资源>
```

空 stylesheet 可能不会写入 ZIP，但 manifest 中仍会保留入口路径。导入器兼容 version 5、6 世界包，并将旧的 `desktop_file` / `mobile_file` 归一化为双入口。

> **打包最常见的失败**：`manifest.json` 必须在 ZIP **根目录**。不要右键压缩外层文件夹——那会让所有条目多套一层 `<包名>/` 前缀，导入器按精确路径在根目录找 `manifest.json`，找不到就报 `Invalid manifest: specified file not found in archive`。正确做法是进入包目录、压缩里面的内容：
>
> ```powershell
> cd my-world-package
> tar -a -c -f ..\my-world-package.zip *   # 条目为 manifest.json、world/...、characters/...
> ```

manifest 中与 UI 有关的字段：

```json
{
  "format": "dream-world-package",
  "version": 7,
  "world_file": "world/world.json",
  "desktop_ui_file": "world/ui.desktop.jsonc",
  "mobile_ui_file": "world/ui.mobile.jsonc",
  "ui_runtime_version": 3,
  "desktop_ui_stylesheet_file": "world/ui.desktop.css",
  "mobile_ui_stylesheet_file": "world/ui.mobile.css",
  "logic_file": "world/logic.js"
}
```

`world/world.json` 保存世界设定、导演配置（含交互消息、提示词模块、生成参数）、资源配置、`ui_runtime_version`、`ui_capabilities`、`platform_features`、`storage` 和不含源码的 `logic` 配置。逻辑源码由 `manifest.logic_file` 指向；角色数据与资源路径通过 manifest 管理，导入时会重新映射为本机资源路径。

### manifest 完整字段

导入器对 manifest 做严格 schema 校验：**未知字段静默忽略，必填字段缺失即拒绝导入**。角色清单必须写在 `character_files` 里——写 `characters` 之类的自创字段会被忽略，然后报"缺少角色文件"。

```json
{
  "format": "dream-world-package",
  "version": 7,
  "world_file": "world/world.json",
  "desktop_ui_file": "world/ui.desktop.jsonc",
  "mobile_ui_file": "world/ui.mobile.jsonc",
  "ui_runtime_version": 3,
  "desktop_ui_stylesheet_file": "world/ui.desktop.css",
  "mobile_ui_stylesheet_file": "world/ui.mobile.css",
  "logic_file": "world/logic.js",
  "character_files": [
    {
      "source_character_id": "han-li",
      "character_name": "韩立",
      "file_path": "characters/han-li/character.json"
    }
  ],
  "assets": [
    {
      "source_path": "assets/bg-main.webp",
      "archive_path": "assets/bg-main.webp",
      "owner_type": null,
      "owner_id": null
    }
  ]
}
```

| 字段 | 必需 | 说明 |
|---|---|---|
| `format` | 是 | 必须为 `dream-world-package` |
| `version` | 是 | 当前为 `7`；兼容旧版 `5`、`6` |
| `world_file` | 是 | 世界数据 JSON 的路径 |
| `desktop_ui_file` / `mobile_ui_file` | 是 | 两份 UI 文档路径 |
| `ui_runtime_version` | 否 | `2` 或 `3`，缺省 `2`；新世界写 `3` |
| `desktop_ui_stylesheet_file` / `mobile_ui_stylesheet_file` | 否 | 样式表路径，空内容可不打包但保留入口 |
| `logic_file` | 否 | 沙箱逻辑源码路径（≤ 256 KB），无逻辑则省略 |
| `character_files` | 是 | **至少一个角色**。每条：`source_character_id`（包内角色 ID）、`character_name`（显示名）、`file_path`（指向 **character.json 文件**，不是目录） |
| `assets` | 否 | 资源条目：`source_path`（导出前的原始路径）、`archive_path`（ZIP 内路径）、`owner_type` / `owner_id`（可空） |

### world.json 必填字段

`world/world.json` 反序列化时以下字段**必须存在**（没有默认值，缺一个就报 `missing field`）；允许为**空值**（空字符串、空数组、空对象）但不能缺键：

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | string | 世界名 |
| `genre` | string | 题材，可空串 |
| `background_prompt` | string | 世界背景提示词，可空串 |
| `opening_scene` | string | 开场场景名 |
| `summary` | string | 简介，可空串 |
| `time_system` | string | 时间制度说明，可空串 |
| `map_nodes` | object | 地图拓扑，格式见下方「map_nodes 拓扑格式」 |
| `triggers` | string[] | 预留元数据，可空数组。**当前运行时不会执行它，也不能用它驱动剧情**；自动逻辑必须使用 `logic.events` |
| `time_config` | object | 时间配置，可空对象 |
| `director_config` | object | 世界主控配置（见第 13 节），可空对象 |
| `ui_assets_config` | object | 资源配置（见第 14 节），可空对象。**注意键名是 `ui_assets_config`，不是 `assets`** |
| `opening_messages` | object[] | 开场消息 `[{ "role", "content", "speaker"? }]`，可空数组 |
| `opening_character_names` | string[] | 开场在场角色名，可空数组 |
| `player_character_name` | string \| null | 玩家角色名 |
| `opening_character_source_ids` | string[] | 开场角色的 `source_character_id` 列表，可空数组 |
| `player_character_source_id` | string \| null | 玩家角色的 `source_character_id` |

以下字段有默认值，缺省即可：`attribute_schemas`（[]）、`initial_inventory_items`（[]）、`ui_runtime_version`（2）、`ui_capabilities`（[]）、`platform_features`（[]）、`storage`（{}）、`logic`（{}）。

### attribute_schemas 持久化属性

世界包在 `attribute_schemas` 声明世界主控可读写的权威状态。导入时宿主注册 schema，创建存档时把非空 `default_value` 初始化到当前会话或玩家角色；后续由主控通过 `session_attribute_updates` / `character_attribute_updates` 写回。不要再用 KV 复制一份气血、体力、装备或修炼状态。

```jsonc
{
  "scope": "session_character",
  "key": "my_world_stamina",
  "label": "体力",
  "value_type": "number",
  "description": "奔跑、攀爬和负重会消耗体力。",
  "default_value": 100,
  "display_policy": {
    "group": "状态",
    "presentation": "meter",
    "max": 100,
    "order": 20
  }
}
```

- `scope`：世界包运行时属性使用 `session` 或 `session_character`；后者默认初始化给玩家角色。
- `key`：主控写回时使用的稳定 key。数据库当前要求同 scope 全局唯一，包作者必须加世界前缀。
- `value_type`：`text`、`number`、`boolean`、`list` 或 `json`。
- `default_value`：新存档初值，类型必须与 `value_type` 一致。
- `display_policy.group`：`side_panel_tabs` 中的标签名；同组属性自动聚合。
- `display_policy.presentation`：`meter` 显示数值条，`list` 显示条目集合，缺省显示普通值。
- `display_policy.max` / `order`：数值条上限和组内顺序。

世界主控请求的 `current_state.runtime_attributes` 会列出当前值和可写 key。行动真实造成消耗、恢复、受伤、身份、装备、功法或进度变化时，主控必须在同一回合返回属性更新，不能只在叙事中声称“体力下降”。地图位置、时间和在场人物分别继续使用 `next_location`、`next_time_label` 与 `scene_visible_characters`；背包使用主控的完整 `inventory_items` 写回。

`initial_inventory_items` 使用会话 `InventoryItem` 结构声明新存档的开局物品：`item_id`、`name`、`category`、`quantity`、`description`、`tags`、`owner_type`、`owner_id`、`visibility`、`disclosed_to`。它只负责初值，后续变化仍由主控返回完整 `inventory_items`；不要在属性或 KV 中再复制一份背包。

### map_nodes 拓扑格式

两种形态（选一种）；**连线只能写在顶层 `edges` 数组里**，节点内的 `links`、`neighbors` 之类自创字段一律被忽略——写了地图就只有孤点没有连线。

```jsonc
{
  "version": 1,
  // 形态 A：层级树（父子自动生成连线）
  "root": {
    "id": "tiannan",
    "label": "天南",
    "children": [
      { "id": "qixuanmen", "label": "七玄门" },
      { "id": "huangfenggu", "label": "黄枫谷" }
    ]
  },
  // 形态 A/B 通用：显式连线，source/target 填节点 label
  "edges": [
    { "source": "七玄门", "target": "黄枫谷" }
  ]
}
```

```jsonc
// 形态 B：平铺 nodes 数组 + 顶层 edges
{
  "version": 1,
  "nodes": [
    { "id": "qixuanmen", "label": "七玄门" },
    { "id": "huangfenggu", "label": "黄枫谷" }
  ],
  "edges": [
    { "source": "七玄门", "target": "黄枫谷" }
  ]
}
```

节点只识别 `id` 和 `label`（`label` 缺省时兼容读 `name`）；`edges` 的端点填节点 **label**，兼容键名 `from`/`to`。`type`、`region`、`desc` 等额外字段不参与地图渲染。地图在 UI 里由 `side_panel_tabs` 的地图页签展示（见第 8 节）。

### character.json 必填字段

每个角色文件的 schema（同样：必填字段必须存在，可以为空值；未知字段被忽略）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `source_character_id` | string | 包内角色 ID（与 manifest 条目一致） |
| `name` | string | 角色名 |
| `role` | string | 定位（如"主角""引导者"），可空串 |
| `background_prompt` | string | 角色背景提示词，可空串 |
| `model` | string | 指定模型引用（按 id / model_id / 名称匹配），空串 = 用默认模型 |
| `memory_strategy` | string | 记忆策略描述，空串 = 默认 |
| `recent_dialogue_rounds` | number | 发言时携带的近期对话轮数（如 `8`） |
| `attributes` | string[] | **字符串数组**（如 `["修为: 炼气三层", "灵石: 0"]`），不是对象 |
| `portrait_assets` | string[] | 立绘资源路径（导出时由应用填，手写可空数组） |
| `system_prompt_template` | string | 系统提示模板，可空串 |
| `response_contract_prompt` | string | 回复契约提示，可空串 |
| `narration_prompt` | string | 旁白风格提示，可空串 |
| `runtime_system_prompt` | string | 运行时追加提示，可空串 |

可选字段：`avatar_asset`（头像资源，缺省空串）。

玩家角色与 NPC 的区别由世界配置决定（`player_character_source_id`），角色文件里不需要 `is_player` 之类的字段。

### 角色提示词怎么写（避免 NPC 回复 JSON 解析失败）

NPC 发言时，宿主按以下顺序拼接系统提示：

1. `system_prompt_template` 的渲染结果（支持 `{{speaker}}`、`{{role}}`、`{{background_prompt}}` 占位符）；
2. 该角色的 `response_contract_prompt`（可空）；
3. **内置回复格式契约**（永远追加，创作者无法关闭）：说明 `speaker`/`content`/`narration` 三个必填字段，并附一个完整 JSON 示例和字符串转义规则；
4. 内置记忆提取契约（仅当世界开启事实提取时）。

各字段该写什么：

| 字段 | 该写 | 不该写 |
|---|---|---|
| `background_prompt` | 角色背景、经历、人物关系，纯散文 | 输出格式要求 |
| `system_prompt_template` | 人设、语气、行为禁忌、扮演要点 | 输出格式、JSON 字段定义 |
| `response_contract_prompt` | 默认留空；仅当需要角色**额外**输出结构化字段时，补一段该字段的说明 + 一个完整 JSON 示例 | 重复定义 `speaker`/`content`/`narration`；要求 Markdown 代码围栏 |
| `narration_prompt` | 旁白/动作描写的风格要求 | — |
| `runtime_system_prompt` | 需要运行时追加的简短指令 | — |

写契约类提示词的要点（与内置契约同款写法）：

- 先列字段名和含义，再给**一个完整、可直接解析的 JSON 示例**。只描述字段不给示例时，模型很容易自造字段名（如把 `content` 写成 `utterance`）或写坏结构——这是 NPC 回复解析失败的最常见原因。
- 明确要求"只输出 JSON 本身"：不要用 ``` 代码围栏包裹，不要在 JSON 之外写解释或开场白。
- 提醒字符串转义：字符串值内的换行必须写成 `\n`，值内的英文双引号必须写成 `\"`，不要直接换行或直接写引号。
- 不要重命名或删除内置必填字段。额外字段会被宿主保留（`additionalProperties: true`），但未被宿主消费的字段写了也没用。

宿主侧容错（了解即可，不要依赖）：NPC 回复 JSON 写坏时，宿主会按字段名尽力打捞正文，实在打捞不到才显示占位文案；导演输出解析失败时会把错误反馈给模型自动重试修复。这些兜底不能替代写对提示词。

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

> 组件 / 动作 / 能力的唯一权威清单是仓库根的 `shared/game-ui/catalog.json`（前端注册表与后端校验都由此生成）。本节是面向作者的说明文字，如与本文件不一致，以 catalog.json 为准。

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

这是桌面和移动端统一的地图、背包与持久化属性入口。移动端文档必须包含它，否则玩家在手机上看不到地图和角色状态（校验器只给警告、不拦导入，作者需自查）。属性会按 `attribute_schemas[].display_policy.group` 自动分组；会话背包使用运行时 `inventory_items`，不需要世界包再建 KV 镜像。

Props：`show_map_tab`、`show_attribute_tabs`、`empty_text`、`drawer_label`。

移动端抽屉内顶部自带"收起"关闭按钮（`.game-status-drawer-close`），世界包 CSS 可以按需覆盖其样式；不要依赖抽屉把手来关闭抽屉，抽屉展开后会盖住把手。

支持 `content` slot，用于自定义当前标签内容。

### `floating_actions`

返回、调试和设置入口。

**请始终在布局里放它（至少 `show_back`）**：没有它玩家进入世界后无法退出到应用页面，只能杀进程。这是世界包 UI 最常漏的组件。

Props：`show_back`、`show_debug`、`show_settings`、`back_label`、`debug_label`、`settings_label`、`layout`。

`layout`：`row`、`column` 或 `wrap`。

### `ledger_book`

无模型记账工具。组件由应用提供可信实现，负责账单录入、编辑、删除、日/月/年明细和统计；世界包只控制布局、样式与受限 props。

Props：

| Prop | 用途 |
|---|---|
| `title` | 账本标题 |
| `collection` | 当前世界内的记录集合名，默认 `ledger.entries`；只允许 1-64 位 ASCII 字母、数字、点、短横线和下划线 |
| `currency` | 金额前缀，默认 `¥` |
| `default_view` | 首屏：`overview`、`transactions` 或 `stats` |
| `income_categories` | 收入分类字符串数组 |
| `expense_categories` | 支出分类字符串数组 |

使用要求：UI runtime 必须为 `3`，世界必须显式声明 `supports_world_records`；新包还应声明 `supports_world_storage` 和组件 `collection` 对应的 `storage.collections`。独立记账界面不需要 `input_composer`，所有操作都不会进入模型回合。

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
| `capabilities` | `platform`、`supports_mic`、`supports_file_picker`、`supports_hover`、`supports_world_records`、`supports_world_storage` |
| `ui_state` | 加载、提交、流式、分支、切换和重试状态 |
| `errors` | 当前 action 错误 |
| `side_tabs` | 可用侧栏标签 |
| `active_side_tab` | 当前侧栏标签 key |
| `active_attribute_content` | 当前属性标签内容 |
| `active_attribute_items` | 当前属性标签的结构化条目，可用于自定义展示 |
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
| `storage.records.list` | `collection` | 读取已声明集合 |
| `storage.records.create` | `collection`、`data` | 创建结构化记录 |
| `storage.records.update` | `collection`、`record_id`、`data` | 更新结构化记录 |
| `storage.records.delete` | `collection`、`record_id` | 删除结构化记录 |
| `storage.kv.list` | `namespace`、`scope?` | 列出命名空间条目 |
| `storage.kv.get` | `namespace`、`key`、`scope?` | 读取一个 KV 值 |
| `storage.kv.set` | `namespace`、`key`、`value`、`scope?` | 写入一个 KV 值 |
| `storage.kv.delete` | `namespace`、`key`、`scope?` | 删除一个 KV 值 |
| `logic.run` | `handler`、`input` | 在受限 Worker 中执行已注册逻辑 |

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

动作还支持三个宿主管理的 UI state 字段：

| 字段 | 用途 |
|---|---|
| `result_state` | 成功后把返回值写入 `$state.<名称>` |
| `error_state` | 失败后把错误文本写入 `$state.<名称>`；声明后错误不会成为未处理异常 |
| `pending_state` | 执行期间自动设为 `true`，结束后恢复 `false` |

```jsonc
{
  "type": "button",
  "label": "读取日记",
  "action": {
    "id": "storage.records.list",
    "args": { "collection": "journal.entries" },
    "result_state": "entries",
    "error_state": "storage_error",
    "pending_state": "loading_entries"
  }
}
```

### 通用世界存储

世界包作者不获得 SQLite、任意 SQL、文件系统或通用 Tauri command 权限。低风险的世界私有存储不触发系统权限弹窗，但必须在 `world/world.json` 中预先声明。

```json
{
  "storage": {
    "kv_namespaces": ["preferences"],
    "collections": {
      "journal.entries": {
        "schema": {
          "type": "object",
          "required": ["date", "content"],
          "properties": {
            "date": { "type": "string", "maxLength": 10 },
            "content": { "type": "string", "maxLength": 4000 },
            "favorite": { "type": "boolean" }
          },
          "additionalProperties": false
        },
        "indexes": ["date", "favorite"]
      }
    }
  }
}
```

当前 schema 子集支持 `type`、`required`、`properties`、`additionalProperties`、`enum`、`minLength`、`maxLength`、`minimum` 和 `maximum`。`indexes` 是为后续宿主查询优化保留的提示；当前 `api.records.query` 读取集合后在 Worker 内筛选，不会创建物理数据库索引。

- `records`：用于账单、任务、日记、商品等多条同构记录，按 `world_id + collection` 隔离。
- `kv`：用于设置、偏好和少量聚合状态，分三级作用域：**world**（跨存档共享，缺省）/ **session**（单个世界存档）/ **character**（存档内角色）。UI action 可带 `{ "scope": "session" }` 或 `{ "scope": "character", "character_id": "角色ID" }`；沙箱 `api.kv.*` 的末参使用 `{ scope: "session" }` 或 `{ scope: "character", characterId: "角色ID" }`。作用域会从 Worker SDK 原样传入宿主，session_id 由宿主注入，世界包无法伪造或读写其它存档。删除存档连带清 session 级和角色级变量，删除世界清 world 级。
- UI 文档自己的 `state` 只存在于当前页面，不属于持久化存储。
- 单条值、JSON 深度、字段数、条目数、集合容量和世界总容量均有限额。
- 更新和删除记录必须匹配宿主生成的 UUID；iframe 和逻辑脚本不能提交其他 `world_id`。
- 删除整个世界会级联删除记录和 KV。导出世界包不会包含用户数据。
- `supports_world_records` 为旧版 `ledger_book` 兼容标记；新通用存储同时声明 `supports_world_storage` 和具体 `storage` 结构。

### 沙箱 JavaScript

需要计算、字段转换或多步存储操作时，可以在包中增加 `world/logic.js`：

```js
world.register("journal.save", async (input, api) => {
  const content = String(input.content || "").trim();
  if (!content) {
    throw new Error("内容不能为空");
  }

  return api.records.create("journal.entries", {
    date: String(input.date),
    content,
    favorite: Boolean(input.favorite)
  });
});

world.register("journal.monthSummary", async (input, api) => {
  const rows = await api.records.query("journal.entries", {
    where: { date: { gte: input.month + "-01", lte: input.month + "-31" } },
    orderBy: ["date", "desc"],
    limit: 500
  });
  return { count: rows.length, favorites: rows.filter(row => row.data.favorite).length };
});
```

`world.register(name, handler)` 注册动作。handler 的第二个参数提供：

| API | 返回值 |
|---|---|
| `api.records.list(collection)` | 集合记录数组 |
| `api.records.query(collection, options)` | 支持 `where`、`orderBy`、`offset`、`limit` 的内存查询结果 |
| `api.records.create(collection, data)` | 新记录 |
| `api.records.update(collection, recordId, data)` | 更新后的记录 |
| `api.records.remove(collection, recordId)` | 无 |
| `api.kv.list(namespace, options?)` | KV 条目数组 |
| `api.kv.get(namespace, key, fallback?, options?)` | 保存的值或 fallback |
| `api.kv.set(namespace, key, value, options?)` | 更新后的 KV 条目 |
| `api.kv.remove(namespace, key, options?)` | 无 |
| `api.platform.invoke(feature, params)` | 平台能力调用的结构化结果（见「平台能力」节） |

`options` 即作用域参数：`{ scope: "world" | "session" | "character", characterId?: string }`，缺省 world；character 作用域需带 `characterId`。

### 世界事件（logic.events）

世界包可以让 logic.js 在特定时刻自动运行，而不必等玩家点按钮。在 `world/world.json` 的 `logic` 配置里声明「事件 → 处理函数名」：

```json
{
  "logic": {
    "runtime": "sandbox-js-v1",
    "timeout_ms": 1000,
    "events": {
      "session_start": "onSessionStart",
      "turn_completed": "onTurn",
      "interaction_answered": "onAnswered"
    }
  }
}
```

| 事件 | 触发时机 | 负载 |
|---|---|---|
| `session_start` | 每个会话一次 | `session_id` |
| `turn_completed` | 回合成功提交（含重发/编辑/重新生成） | `session_id`、`turn_index`、该回合新增消息 |
| `interaction_answered` | 玩家首次回答一条交互（重复提交不触发） | `session_id`、`message_id`、`interaction_id`、`answer` |

handler 里可正常使用 `api.kv` / `api.records` / `api.platform`。handler 失败只记日志，不打断游戏；同一事件不会因界面刷新重复触发。

```js
world.register("onAnswered", async (input, api) => {
  const score = (await api.kv.get("game", "score", 0, { scope: "session" })) || 0;
  await api.kv.set("game", "score", score + 10, { scope: "session" });
});
```

### 平台能力（第 12 项，第一批：文件）

世界包可以调用目录内的平台 action。统一模式：**manifest 声明 → 玩家在「设置 → 世界权限」里允许 → 调用**。任一环不满足都会得到明确报错，不会静默失败。

在 world.json（世界包 `world/world.json`）里声明：

```json
{
  "platform_features": ["file.pick", "file.read", "file.write"]
}
```

调用：

```js
world.register("import.ledger", async (input, api) => {
  const files = await api.platform.invoke("file.pick", { multiple: false, extensions: ["json", "csv"] });
  // files: [{ name, size, data_base64 }]
  await api.platform.invoke("file.write", { path: "imports/last.json", data_base64: files[0].data_base64 });
  const saved = await api.platform.invoke("file.read", { path: "imports/last.json" });
  return { name: saved.name, size: saved.size };
});
```

| action | 参数 | 结果 | 桌面 | 安卓 |
|---|---|---|---|---|
| `file.pick` | `multiple?`、`extensions?` | `[{ name, size, data_base64 }]` | ✅ 系统文件选择器 | `unsupported`（SAF 链路后续批次） |
| `file.read` | `path` | `{ name, path, size, data_base64 }` | ✅ | ✅ |
| `file.write` | `path` + `text` 或 `data_base64` | `{ path, size }` | ✅ | ✅ |
| `file.share` | `path` | `{ shared: true }` | `unsupported`（桌面无系统分享面板） | ✅ 系统分享面板 |

- `path` 是**该世界专属目录**内的相对路径（拒绝绝对路径与 `..` 穿越）；不同世界的文件互不可见。`file.pick` 选的是用户显式指定的文件，不受目录限制。
- 单文件大小上限 10 MB。
- 错误统一为 `code: 中文说明` 前缀字符串，可用前缀程序化判断：`unsupported`（当前平台不支持）、`not_declared`（包未声明）、`not_granted`（玩家未允许）、`invalid_params`、`io`、`cancelled`（用户取消选择）。

`logic` 配置示例：

```json
{
  "logic": {
    "runtime": "sandbox-js-v1",
    "timeout_ms": 1000
  }
}
```

每次 `logic.run` 都创建独立 Worker，结束或超时后销毁。超时范围为 100-5000 ms，默认 1000 ms；输入、输出和逻辑源码都有大小限制。Worker CSP 禁止网络，且没有 DOM、父页面、Tauri、Node、文件系统、模型 API 和数据库对象。脚本只能通过上表 SDK 请求宿主操作，宿主和 Rust 后端都会重新检查 collection/namespace 声明。

调用示例：

```jsonc
{
  "type": "button",
  "label": "保存",
  "action": {
    "id": "logic.run",
    "args": {
      "handler": "journal.save",
      "input": {
        "date": "$state.date",
        "content": "$state.content",
        "favorite": "$state.favorite"
      }
    },
    "result_state": "saved_entry",
    "error_state": "save_error",
    "pending_state": "saving"
  }
}
```

### 记账组件的数据语义

- `ledger_book` 把金额保存为整数“分”，日期保存为 ISO `YYYY-MM-DD`，统计在可信组件内计算。
- 账单属于世界，不属于某个游戏存档；存档分支不会复制账单，删除单个存档不会删除账单。
- 示例包声明 `ledger.entries` schema；账单仍由可信 `ledger_book` 组件渲染，不需要调用模型或沙箱 JS。

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
- **必须提供地图/属性入口**：使用 `side_panel_tabs`；没有它时玩家在手机上看不到地图、背包和角色状态。
- **必须包含 `floating_actions`（至少 `show_back`）**：玩家需要能退出世界返回应用。
- 输入区采用两行：textarea 独占一行，图片、录音和发送按钮位于下一行。
- 聊天流聚焦叙事、角色/玩家发言和折叠思维链。
- 角色消息下使用复制/分支，玩家消息下使用编辑/重发。
- 不要用桌面 `transform: scale()` 模拟手机 UI。

## 13. 世界主控配置（director_config）

以下字段都在 `world/world.json` 的 `director_config` 中，随世界包导出导入。

### 行动选项（director_interaction_kinds）

行动选项只由世界主控生成，角色回复不提供选项。宿主负责在输入框旁渲染控件，玩家作答结果会成为下一条真实玩家消息。世界包必须显式声明允许的类型，未声明的类型一律丢弃：

```json
{
  "director_config": {
    "director_interaction_kinds": ["choice", "slider"]
  }
}
```

所有交互统一放在世界主控回复 JSON 顶层的 `interaction` 字段中，不能嵌在其它业务字段里。交互使用 `{ "kind", "prompt", "config" }`：

| 类型 | 说明 | `config` 关键字段 |
|---|---|---|
| `choice` | 单选 | `options: [{ "id", "label" }]` |
| `multi_choice` | 多选 | `options`、`min`、`max` |
| `form` | 表单 | `fields: [{ "id", "label", "input": "text" | "number" | "textarea", "required" }]` |
| `confirm` | 确认 | `confirm_label`、`cancel_label` |
| `slider` | 滑条 | `min`、`max`、`step`、`default` |

单选示例：

```json
{
  "interaction": {
    "kind": "choice",
    "prompt": "你准备走哪条路？",
    "config": {
      "options": [
        { "id": "mountain", "label": "翻山绕行" },
        { "id": "river", "label": "沿河疾行" }
      ]
    }
  }
}
```

在世界主控提示词里只需说明**何时**出题和选项语义；宿主会按世界声明自动补充精确 JSON 格式。玩家提交后，宿主先把交互标记为已回答，再把选项标签、表单内容、确认文案或滑条数值格式化成一条真实玩家消息，并自动启动下一模型回合，因此后续剧情会像普通玩家输入一样继续。回答**幂等**：重复提交返回首次结果，不会重复创建玩家消息。`interaction_answered` 事件只用于世界包确实需要的额外本地逻辑，不能代替正常回合提交。

候选解析器也兼容把 `options`、`fields`、`min/max` 等字段直接放在 `interaction` 顶层，但这是宽容输入，不是作者应要求模型生成的标准格式。宿主校验后会统一持久化为世界主控消息的 `metadata.interaction.config`，前端只渲染这个归一化结果。选项标识必须使用 `id`，不能使用 `value`；表单字段必须使用 `id` 与 `input`，不能使用 `key` 与 `type`。

### 玩家角色切换

`allow_player_character_switch` 控制世界主控能否提议玩家切换到其他角色，默认 `true`。关闭后，主控返回的 `switch_character_proposal` 会被运行时丢弃；玩家仍可由应用的独立切换功能手动切换。

### 提示词模块（prompt_presets）

世界包可以向指定位置注入设定/规则文本，支持关键词触发和占位符：

```jsonc
{
  "director_config": {
    "prompt_presets": [
      {
        "name": "背景设定",
        "content": "本世界的背景是……",
        "position": "system_prefix",
        "scope": "both"
      },
      {
        "name": "炼金术规则",
        "content": "当玩家提到炼金时……{{var:alchemy_level}}",
        "keywords": ["炼金", "药剂"],
        "keyword_scan_depth": 10
      }
    ]
  }
}
```

| 字段 | 说明 |
|---|---|
| `content` | 注入文本（单模块渲染后上限 4000 字符） |
| `position` | `system_prefix`（核心系统提示之前）/ `system_suffix`（默认）/ `depth:N`（历史第 N 层） |
| `scope` | `director` / `character` / `both` |
| `keywords` | 命中近期消息才注入；不填则总是注入 |
| `keyword_scan_depth` | 关键词扫描的近期消息条数（默认 10） |
| `enabled`、`order` | 开关与注入顺序 |

占位符（纯文本替换，最多 3 轮展开，未知占位符原样保留，无法注入代码）：`{{var:key}}`（session 级 KV）、`{{world:name|genre|summary|opening_scene}}`、`{{session:location|time}}`、`{{date}}`、`{{random:a,b,c}}`。

每个模块是否实际注入及原因（总是注入 / 命中关键词 / 未命中 / 渲染后为空）都可在调试页的 Prompt 追踪里看到。

### 生成参数（generation_params）

世界层可以为采样参数给一组默认值（每个字段都可留空 = 本层不覆盖）：

```json
{
  "director_config": {
    "generation_params": { "temperature": 0.9, "max_tokens": 800 }
  }
}
```

可用字段：`temperature`、`top_p`、`top_k`、`max_tokens`、`stop`、`presence_penalty`、`frequency_penalty`、`seed`。实际生效值按「角色内置默认 → 应用设置 → 世界 → 会话存档」逐字段取最内层；模型/provider 不支持的参数会被过滤并在调试页留痕（`dropped` 带原因），越界值夹到合法区间。

注意：玩家能否发图片/语音附件，取决于所用模型在「设置 → 模型」里是否开启了对应**输入模态**开关；未开启的模型收到附件会在提交时明确报错，不会静默丢弃。

## 14. 资源配置

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

手工制作 ZIP 时，`ui_assets_config`、`portrait_assets` 和 `avatar_asset` 中每个以 `assets/` 开头的路径都必须同时出现在 `manifest.assets` 的 `source_path` 或 `archive_path` 中，并且 ZIP 内必须存在对应文件。导入器会拒绝只有引用、没有文件的“假资源”。

## 15. 校验与调试

世界编辑器会同时运行：

- 单文档 schema 编译。
- 桌面/移动 bundle 校验。
- 组件、action、capability 依赖收集。
- 当前客户端兼容性检查。

常见错误：

| 错误 | 原因 | 修复 |
|---|---|---|
| 导入报 `Invalid manifest: specified file not found in archive` | 压缩了外层文件夹，条目多套一层前缀 | 进入包目录压缩**内容**，让 `manifest.json` 在 ZIP 根（第 5 节） |
| 导入报 `Invalid world data: missing field ...` | world.json 缺必填字段或键名不对（如把 `ui_assets_config` 写成 `assets`） | 对照第 5 节「world.json 必填字段」补齐 |
| 导入报 `World package is missing character files` | manifest 用了 `characters` 等自创字段，或 `character_files` 为空 | 按第 5 节写 `character_files`，`file_path` 指向 character.json 文件 |
| `unsupported_schema_version` | UI 文档不是 schema 2 | 改为 `schema_version: 2` |
| `unknown_component` | 使用未注册组件 | 使用本文组件表中的名称 |
| `unknown_component_prop` | prop 名称不受支持 | 检查组件 props 表 |
| `misplaced_component_prop` | 把组件 prop 写在节点顶层 | 将字段移入同一节点的 `props` 对象 |
| `unknown_action` | action ID 不存在 | 检查动作表 |
| `invalid_binding` | binding 不是简单点路径 | 使用 `$session.location` 形式 |
| `unsafe_expression` | `when` 中出现函数、下标或脚本语法 | 改用安全表达式子集 |
| `stylesheet_too_large` | 单份 stylesheet 超过 1 MiB | 拆减 CSS 和内嵌数据 |
| `unsupported_declared_capability` | 声明未知能力 | 使用当前五种 capability |
| `invalid_world_storage_config` | `storage` 不是合法对象或名称不合规 | 检查 collection/namespace 名称和结构 |
| `invalid_world_logic_config` | logic runtime、源码或大小不合规 | 使用 `disabled` 或 `sandbox-js-v1` 并提供 `logic_file` |
| `World logic has no reachable entry point` | logic.js 注册了处理器，但 UI 和事件都不会调用 | 增加 `logic.events`，或由 UI action 调用 `logic.run` |
| `Invalid prompt scope` | 使用了 `always`、`keyword` 等不存在的 scope | scope 只填 `director`、`character`、`both`；常驻模块不填 keywords |
| `references assets that are not declared` | world/character 引用了 ZIP 中未声明的资源 | 把真实文件加入 ZIP，并在 `manifest.assets` 中逐项声明 |
| 导入报 `Unknown platform feature` | `platform_features` 含目录外的 action | 只用 `file.pick` / `file.read` / `file.write` / `file.share` |
| 运行时 `not_declared:` / `not_granted:` / `unsupported:` 前缀错误 | 平台能力未声明 / 玩家未允许 / 当前平台不支持 | 见第 10 节「平台能力」的可用性表 |
| `missing_world_storage_capability` | 使用通用存储或沙箱逻辑但未声明能力 | 在 `ui_capabilities` 中加入 `supports_world_storage` |
| `world_records_require_runtime_v3` | 记录组件运行在 v2 | 将 `runtime_version` 改为 `3` |
| `missing_world_records_capability` | 使用 `ledger_book` 但世界未声明存储能力 | 在 `ui_capabilities` 中加入 `supports_world_records` |
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

## 16. 从 v2 迁移到 v3

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

另有一个无模型、受控持久化的完整世界包示例：

- [可编辑源码](../examples/world-packages/accounting-assistant/)
- [可直接导入的 ZIP](../output/accounting-assistant-world.zip)

模型驱动叙事世界的结构化起步包：

- [制作手册](world-package-rpg-cookbook.md)
- [可编辑起步包](../examples/world-packages/narrative-rpg-starter/)
- 演示链路：角色选择题 → `interaction_answered` → session KV → `{{var:route}}` 注入后续 Prompt

`frontend/src/data/gameUi/migration.test.ts` 会逐字验证四份文档在迁移后未改变，并验证桌面与移动入口保持独立。Rust bundle 测试也会校验两套示例在 runtime v3 下仍受支持。

## 17. 最小完整示例

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
        { "type": "component", "component": "side_panel_tabs", "area": "side" },
        { "type": "component", "component": "floating_actions" }
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

## 18. 发布前检查表

- `runtime_version` 为 `3`。
- 两份 UI 文档都声明 `schema_version: 2`。
- desktop 和 mobile 都有独立完整入口。
- 没有 JavaScript、远程脚本或本机绝对路径。
- 所有组件、props、actions 和 capabilities 均通过治理校验。
- 桌面窗口缩放后没有横向溢出。
- Android 状态栏、右侧把手和底部手势区没有遮挡内容。
- 软键盘打开时消息区和输入区仍可用。
- 图片、录音、复制、编辑、重发、分支和重试经过真实会话测试。
- 两份文档都包含 `floating_actions`（返回）和 `side_panel_tabs`（移动端地图/属性入口）。
- 地图有连线：`map_nodes` 的连接写在顶层 `edges`，不是节点内的自创字段。
- 声明的 `platform_features` 在真机上逐项允许后可用，未允许时得到 `not_granted:` 报错。
- 交互消息（如使用）在真机上出题、作答、重复点击不重复计分。
- 每个 `logic.js` 处理器至少能从 `logic.events` 或 UI `logic.run` 到达，事件中的 handler 名与 `world.register` 完全一致。
- `triggers` 只作为元数据；没有把它误当作剧情事件系统。
- 关键词触发的提示词模块（如使用）在调试页可见命中与否。
- 导出后的 ZIP 可在另一份本地数据环境中重新导入。
