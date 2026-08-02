# 叙事 RPG 世界包制作手册

本文不是字段表，而是一条可执行的制作流程。目标是让作者或制作智能体交付一个“能形成游戏循环”的世界，而不是只包含背景资料、角色人设和一套 CSS 的聊天包。字段定义仍以[世界包开发指南 v3](world-package-guide-v3.md)为准。

可直接复制的工程位于 [`examples/world-packages/narrative-rpg-starter`](../examples/world-packages/narrative-rpg-starter/)。它以黄巾起义为开场，演示：

```text
角色生成 choice
  -> 玩家点击
  -> 宿主把选项标签写成玩家消息
  -> 自动启动下一回合
  -> 主控更新地点、在场人物、背包和声明过的角色属性
  -> 主控与角色按玩家选择继续剧情
```

## 1. 先写玩法闭环

开始写 JSON 前，先回答以下问题：

| 问题 | 必须落实到 |
|---|---|
| 玩家每回合做什么 | `input_composer`、世界主控交互或可信组件 |
| 谁决定剧情推进 | 世界主控、角色模型或确定性 logic.js |
| 哪些状态会改变 | 核心玩法用引擎属性/背包/场景，独立工具数据才用 records/KV |
| 状态如何影响下一回合 | `current_state.runtime_attributes`、背包与场景上下文；辅助变量才用 `{{var:key}}` |
| 玩家在哪里看到结果 | 消息、地图、属性标签或注册组件 |

任何一个“系统”都必须能画出完整链路。例如“门派声望”不能只有 storage schema，也不能只有角色属性字符串：必须有写入入口、持久化位置、Prompt 消费点和可见反馈。

## 2. 分清三种状态

1. **引擎权威状态**：场景、位置、在场角色、角色属性、背包、规则和记忆。由主控结构化写回，适合叙事游戏的核心状态。世界包用 `attribute_schemas` 声明属性和默认值，用 `initial_inventory_items` 声明开局背包。
2. **session KV**：单个存档内的路线、开关、计分和轻量聚合值。声明 `variables` namespace 后，可用 `{{var:key}}` 注入 Prompt。
3. **records**：任务日志、事件历史、交易、图鉴等多条同构记录。需要主动通过 UI action 或 logic.js 写入。

不要在 records 中另存一份“修为”，同时又期待角色属性自动更新。两套状态不会自行同步；必须选定唯一真相来源，或明确编写同步逻辑。

## 3. 地图只描述空间，不负责剧情

`map_nodes` 的 `nodes/root` 和顶层 `edges` 负责地图展示。边的 `source/target` 填节点 `label`。地图不会自动解锁角色、触发事件或推进章节。

`triggers` 当前只是预留元数据，没有执行语义。自动逻辑使用：

- `session_start`：初始化存档变量。
- `turn_completed`：回合完成后记日志、累计数值。
- `interaction_answered`：可选的额外本地事件；正常剧情不依赖它，回答会自动成为玩家消息并继续下一回合。

## 4. Prompt 模块的正确分工

`scope` 只允许 `director`、`character`、`both`：

```json
{
  "name": "历史边界",
  "content": "时间为中平元年，不得提前出现尚未发生的事件。",
  "position": "system_prefix",
  "scope": "both"
}
```

常驻模块不填写 `keywords`。条件模块通过 `keywords` 控制是否注入，不能写 `scope: "keyword"`。世界背景写稳定设定；Prompt 模块写运行规则、条件知识和会话变量，不要重复堆砌同一段世界观。

## 5. 交互必须进入世界主控回复 JSON

先在 `director_config` 声明：

```json
"director_interaction_kinds": ["choice"]
```

然后在世界主控提示词中说明何时出题，例如：

```text
当路线仍为“未选择”且剧情来到岔路时，必须用 choice 让玩家从 join、defend、flee 中选择；不要把三个选项只写成正文列表。
```

写主控提示词的通用原则（同样适用于角色提示词）：每要求一个输出字段，都要附一个完整、可直接解析的 JSON 示例，并明确要求"只输出 JSON 本身、不用代码围栏、字符串内换行写 `\n`"。只描述字段不给示例是模型输出无法解析的最常见原因。详见《world-package-guide-v3.md》"角色提示词怎么写"一节。

宿主会把允许类型和精确格式加入世界主控回复契约。模型应输出：

```json
"interaction": {
  "kind": "choice",
  "prompt": "你准备如何应对？",
  "config": {
    "options": [
      { "id": "join", "label": "投奔义军" },
      { "id": "defend", "label": "守护乡里" },
      { "id": "flee", "label": "避乱南下" }
    ]
  }
}
```

玩家点击后，宿主会把选项 `label` 作为一条真实玩家回复并自动启动下一回合。主控会像处理普通输入一样处理这次选择，并用 `next_location`、`scene_visible_characters`、`inventory_items` 和属性更新写回结果，不需要 logic.js 中转。

## 6. 用属性把结果接回世界

先在 `world.json` 声明稳定状态：

```jsonc
"attribute_schemas": [
  {
    "scope": "session_character",
    "key": "starter_stamina",
    "label": "体力",
    "value_type": "number",
    "default_value": 100,
    "display_policy": { "group": "状态", "presentation": "meter", "max": 100 }
  },
  {
    "scope": "session_character",
    "key": "starter_route",
    "label": "当前路线",
    "value_type": "text",
    "default_value": "未选择",
    "display_policy": { "group": "阅历" }
  }
]
```

再明确要求主控在行动产生事实结果时写回：

```text
current_state.runtime_attributes 是权威状态。玩家选择路线后，用
character_attribute_updates 更新 starter_route；奔跑、攀爬或负重时同时更新
starter_stamina。不能只在叙事中描述变化。
```

主控返回示例：

```json
{
  "planned_speakers": ["乡勇首领"],
  "next_location": "村东土垒",
  "scene_visible_characters": ["乡勇首领"],
  "character_attribute_updates": [
    { "character_name": "玩家角色名", "key": "starter_route", "value": "守护乡里" },
    { "character_name": "玩家角色名", "key": "starter_stamina", "value": 92 }
  ]
}
```

这条链路只有一份真相来源：schema 定义初值，主控读取当前值并写回，`side_panel_tabs` 直接展示结果。只有确定性计分、独立工具记录或复杂本地计算才需要额外使用 `interaction_answered` 和 logic.js。

## 7. UI 节点不要猜字段

组件参数必须全部放进 `props`：

```jsonc
{
  "type": "component",
  "component": "input_composer",
  "class_name": "rpg-input",
  "props": {
    "placeholder": "说出你的决定……",
    "show_image_button": true
  }
}
```

`class_name`、`area`、尺寸和 `style` 属于节点；`placeholder`、`show_back`、`show_map_tab` 等属于组件 `props`。导入器会拒绝放错层级的已知 prop。

桌面和移动端必须分别提供完整 UI。移动端至少包含 `side_panel_tabs` 和可返回的 `floating_actions`，并为状态栏、右侧抽屉把手、软键盘和底部手势区留空间。

## 8. 资源必须真实存在

不要为了“看起来完整”在 `world.json` 中虚构 `assets/scenes/a.webp`。每个以 `assets/` 开头的引用必须：

1. 在 ZIP 中存在真实文件。
2. 在 `manifest.assets` 中声明 `source_path` 与 `archive_path`。
3. 被 `ui_assets_config`、角色 `portrait_assets` 或 `avatar_asset` 引用。

没有资源时保持数组为空；这比引用不存在的图片更可诊断。正式发布的叙事世界应补齐开场场景、主要地点背景和开场角色立绘，并在桌面与移动端分别检查裁切。

## 9. 角色按剧情阶段投放

只把开场真正出现的角色放进 `opening_character_names`。不要因为角色在整个作品中重要，就让跨越数十年剧情的角色同时出现在开场。

玩家角色可以是原作人物，也可以是原创身份。若希望玩家拥有选择空间，优先使用原创角色，把历史人物作为 NPC；角色属性写当前可见状态，不要提前泄露后期境界、关系和秘密。

## 10. 制作智能体交付检查

制作智能体在生成 ZIP 前必须逐项回答“是”：

- 每个组件 prop 都在 `props` 中。
- 每个 Prompt scope 都是 `director`、`character` 或 `both`。
- 每个存在的 logic handler 都能从 `logic.events` 或 UI `logic.run` 到达。
- 每个交互回答都会作为玩家消息进入下一回合；需要持久化的结果由主控写入权威状态。
- 每个持久化值都有写入者和消费方。
- 核心角色状态没有同时复制到 KV、records 或自定义面板状态。
- 没有把 `triggers` 当作事件系统。
- 没有引用 ZIP 中不存在的资源。
- 地图、角色和设定只暴露当前剧情阶段需要的内容。
- 桌面和移动 UI 都包含聊天、输入、状态入口和退出路径。
- ZIP 根目录直接包含 `manifest.json`，不是再套一层文件夹。

通过上述检查后，再使用主指南的发布前检查表完成真机验证。
