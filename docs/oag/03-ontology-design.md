# OAG 本体设计与逻辑查询

> 状态：设计稿 v2，替代上一版“世界书 + 事件账本 + 查询工具”的 RAG 设计。
> 前置阅读：[记忆隔离代码审计](./02-isolation-audit.md)
> 世界包处于开发阶段，**不做旧版本兼容**；格式可以改，旧包不保证可导入。

## 0. 什么才算 OAG

Palantir 的 OAG 不是“把文档切片塞进 prompt”，而是让模型在一个**类型化对象 + 显式关系 + 可查询逻辑**的世界模型上工作。它通常包含：

- **Object types**：人物、地点、物品、组织、事件等类型；
- **Properties**：类型化属性（数字、枚举、时间、引用）；
- **Links**：有方向、有基数、有语义的关系；
- **Actions / functions**：可执行动作和计算（本项目不需要）；
- **Query / traversal**：按类型、属性、关系、时间查询和遍历。

本项目**去掉 Actions / functions 和规则执行**，只保留 Object types、Properties、Links、事件实例，以及 Query / traversal。目标不是“让模型执行规则”，而是：

> 让智能体能够问出并回答“谁和谁是什么关系”“某个时间点事实是什么”“谁不知道这件事”“有多少人属于某个组织”“从 A 到 B 的关系链是什么”这类**有结构、可验证**的问题。

### 0.1 上一版为什么不算 OAG

| 上一版 | 本质 | 缺什么 |
|---|---|---|
| Lore 条目 + 关键词注入 | 文本检索（RAG） | 没有类型、属性、关系；不能查“谁住在哪” |
| Event Ledger | 追加日志 | 没有实体/关系/属性；不能做时间点查询、聚合、否定 |
| `oag_search` 工具 | 关键词搜索 | 没有图遍历、多跳、逻辑谓词、知识边界 |
| 记忆 + 事实 | 文本/三元组存储 | 事实没有 actor、没有 claim/truth、没有知识边 |

一句话：上一版是“更好的 RAG + 记忆”，不是本体增强生成。

### 0.2 和 GraphRAG / 普通知识图谱的区别

| | GraphRAG | 普通 KG | 本项目 OAG |
|---|---|---|---|
| Schema | 无，LLM 自动抽 | 有，但通常固定 | 每世界可定义 object/link types |
| 查询 | 相似度 + 社区摘要 | 图查询/SPARQL | 类型/属性/关系/时间/知识边界 |
| 写入 | LLM 抽取，质量不稳 | 人工/ETL | Canon 作者定，运行时 claim 抽取 |
| 推理 | 社区摘要 | 可选 | 只读物化（对称/传递/反向） |
| 隔离 | 弱 | 弱 | `oag_knowledge` 一等公民 |

LLM 在本项目里只负责**填充实例**（抽取事实/关系），不负责定义 schema，也不直接决定“谁应该知道”。

## 1. 目标与非目标

### 1.1 目标

1. **逻辑查询**：智能体能按类型、属性、关系、时间、知识边界查询，而不是只做相似度检索。
2. **稳定背景**：世界设定作为 Canon 对象/关系存在，作者可控；不因检索波动而丢失。
3. **长记忆**：每回合事件、属性变化、关系变化都进入实例图；支持 as-of 查询和分层摘要。
4. **记忆隔离**：每个 actor 只能看到自己可见的子图；记忆、事实、事件、知识库都受同一套边界约束。
5. **可解释**：每次查询/注入返回来源、回合、置信度、可见性；Debug 可复现。
6. **离线、可打包**：SQLite + Rust，不依赖外部图数据库/向量服务。

### 1.2 非目标

- OWL/SWRL 推理机、描述逻辑分类器。
- 可执行 Actions、规则引擎、自动改世界。
- 不决定角色行为、目标、性格、说话方式：这些由角色卡、系统提示和模型自己决定；OAG 只提供“这个角色视角下的事实/知识/记忆/关系”。
- 不把“他应该怎么做”写进事实层；事实与动机分离。
- 全自动本体建模（LLM 可以辅助抽取，但 Canon 由作者确认）。
- 跨世界统一本体（每个世界可以有自定义类型/关系）。
- 实时多人协作。

## 2. 四层架构

```text
┌──────────────────────────────────────────────────────────────┐
│ 1. TBox 本体模式                                             │
│    object_types / link_types / attribute_types / constraints │
│    作者定义：人物、地点、组织、物品、事件；关系域/值域/基数   │
└───────────────────────────┬──────────────────────────────────┘
                            │ 实例化
┌───────────────────────────▼──────────────────────────────────┐
│ 2. ABox 实例图                                               │
│    objects / attributes / links / scenes / events             │
│    Canon（世界级）+ Session（本局级）                         │
└───────────────────────────┬──────────────────────────────────┘
                            │ 谁看得见
┌───────────────────────────▼──────────────────────────────────┐
│ 3. Knowledge 知识边界                                        │
│    knowledge(actor, target, witnessed/told/private/public)    │
│    memories（角色主观记忆）                                   │
└───────────────────────────┬──────────────────────────────────┘
                            │ 查询
┌───────────────────────────▼──────────────────────────────────┐
│ 4. Query + Projection                                        │
│    逻辑查询：find/traverse/as-of/aggregate/negation/knows     │
│    上下文投影：scene frame / entity cards / facts / events    │
│    → oag_context（受 token 预算约束）                         │
└──────────────────────────────────────────────────────────────┘
```

关键点：**知识边界在查询之前生效**。先按 actor 裁剪可见子图，再做遍历/聚合；不能先查全图再过滤结果，否则多跳会通过隐藏边推断出秘密。

## 3. TBox：本体模式

### 3.1 Object types

内置类型（每个世界可扩展）：

| key | 中文 | 说明 | 典型属性 |
|---|---|---|---|
| `character` | 人物 | 玩家、NPC、历史人物 | name, aliases, role, age, status, tags |
| `location` | 地点 | 城市、建筑、房间、区域 | name, region, type, description |
| `scene` | 场景 | 一次具体的时空切片 | name, location, time, participants |
| `item` | 物品 | 武器、信物、钥匙、载具 | name, type, owner, condition |
| `organization` | 组织 | 门派、家族、国家、公司 | name, type, leader, motto |
| `concept` | 概念 | 规则、法术、信仰、身份 | name, category, description |
| `event` | 事件 | 发生过的事 | turn, scene, type, summary |
| `memory` | 记忆 | 角色主观记忆 | actor, content, turn, emotion |
| `document` | 资料 | 世界观文档、规则书 | title, audience, source |

类型支持 `parent`（如 `player` 继承 `character`，`city` 继承 `location`），用于查询时包含子类型。

> `event` / `memory` 是逻辑类型：物理上分别落在 `oag_events` / `memories`，查询层做对象适配，不再往 `oag_objects` 里重复存一行。

```jsonc
// 世界包里的 ontology.object_types
[
  {
    "key": "character",
    "label": "人物",
    "parent": null,
    "attributes": [
      { "key": "role", "label": "身份", "value_type": "string" },
      { "key": "age", "label": "年龄", "value_type": "number" },
      { "key": "status", "label": "状态", "value_type": "enum",
        "enum_options": ["alive", "dead", "missing", "unknown"] }
    ]
  }
]
```

### 3.2 Attribute types

属性值类型：

- `string` / `number` / `boolean` / `enum` / `list` / `json`
- 可声明 `required`、`default`、`enum_options`
- 可声明 `functional`（同一对象同一时刻只有一个值）
- 可声明 `temporal`（保留历史，默认 true）

属性不是塞进 `attributes_json` 就完事：运行时属性写入 `oag_attributes` 表，带 `valid_from_turn` / `valid_to_turn`，这样才能回答“第 30 回合时他的年龄/身份是什么”。

### 3.3 Link types

关系类型定义：

```jsonc
{
  "key": "located_in",
  "label": "位于",
  "domain": ["character", "item"],
  "range": ["location"],
  "functional": true,          // 同一时刻一个对象只能位于一个地点
  "temporal": true,
  "inverse": "contains"
}
```

内置关系：

| key | 中文 | domain → range | 语义 |
|---|---|---|---|
| `located_in` | 位于 | character/item → location | 当前所在地；functional |
| `part_of` | 属于 | location/item → location/item | 整体-部分；transitive |
| `owns` | 拥有 | character/organization → item | 所有权 |
| `holds` | 持有 | character → item | 当前持有（可和 owns 分离） |
| `member_of` | 成员 | character → organization | 组织成员 |
| `leader_of` | 首领 | character → organization | 组织首领；functional |
| `ally_of` | 盟友 | character/organization → 同类 | symmetric |
| `enemy_of` | 敌对 | character/organization → 同类 | symmetric |
| `related_to` | 相关 | * → * | 兜底关系；LLM 抽取未知谓词时使用 |
| `knows` | 知道 | character → fact/event/object | 知识边（见 §6） |
| `participates_in` | 参与 | character → event | 事件参与 |
| `caused_by` | 起因 | event → event | 事件因果 |

`domain/range` 是校验用的，不是执行规则：写入时校验，违反则降级为 `related_to` 并记录 warning；查询时按类型过滤。

### 3.4 约束与推理（只读）

- **类型约束**：属性值类型、link domain/range。
- **基数约束**：functional（同一时刻一个值）、`leader_of` 一人、`located_in` 一地。
- **时间约束**：`valid_from <= valid_to`；同一功能关系在重叠时间不能有两个值。
- **推理**（物化，不做 OWL）：
  - `part_of` 传递闭包；
  - `ally_of` 对称；
  - `inverse` 反向边；
  - 推理结果标记 `inferred=1`、`confidence` 降低，查询可排除。
- 推理只在**可见子图**内做；不能用隐藏边推导可见结论。

## 4. ABox：实例图

### 4.1 对象

```sql
oag_objects(
  id, world_id, session_id,        -- session_id='' 表示世界 Canon
  object_type, name, name_normalized, aliases_json,
  description, source,             -- author/world/llm/system
  created_turn, updated_turn, created_at, updated_at
)
```

- **Canon 对象**：世界包导入的角色、地点、组织、物品；`session_id=''`，所有存档共享。
- **Session 对象**：本局新生成的角色、临时地点、临时物品；`session_id=<id>`，只在当前存档可见。
- **ID 稳定**：角色用 `obj:<world_id>:character:<character_id>`；地点用 `obj:<world_id>:location:<slug>`；临时对象用 `obj:<world_id>:session:<session_id>:<uuid>`；避免跨世界同名冲突。
- **名称/别名**：`name` 是规范名，`aliases_json` 是别名；`name_normalized` 用于实体解析（复用 `normalize_memory_text`）。
- **同名**：按 ID 区分，不按 name 取第一个；显示名可以重复。

### 4.2 属性

```sql
oag_attributes(
  id, world_id, session_id, object_id,
  key, value_json, value_type,
  valid_from_turn, valid_to_turn,
  source, confidence, asserted_by,
  created_at
)
```

- `valid_to_turn IS NULL` = 当前有效；
- 属性变化：旧行写 `valid_to_turn=当前回合`，新行插入；
- `asserted_by` 记录是谁说的/谁观测的；
- 查询 `as_of_turn=30` 时取 `valid_from <= 30 AND (valid_to IS NULL OR valid_to > 30)`。

### 4.3 关系

```sql
oag_links(
  id, world_id, session_id,
  from_object_id, link_type, to_object_id, to_value,
  attributes_json,
  valid_from_turn, valid_to_turn,
  source, confidence, asserted_by, source_event_id,
  visibility,                     -- public/scene/private
  inferred, created_at
)
```

- `to_object_id` 为空时用 `to_value` 表示字面量（如“颜色=红”）；
- 关系变化：旧边 `valid_to_turn=当前回合`，新边插入；
- `visibility='public'`：所有 actor 可见；`scene`：需要知识边；`private`：仅指定 actor；
- 默认值用 `scene`（最小可见），只有作者/Canon 写入才显式 `public`；不要把 `public` 当默认值；
- `asserted_by` 与 `truth_status` 区分“谁说的”和“是否属实”。

### 4.4 场景

```sql
oag_scenes(
  id, world_id, session_id,
  name, location_object_id, background_hint,
  valid_from_turn, valid_to_turn
)
```

- 每次 `change_scene` 创建一个场景实例；
- 同一地点多次进入必须是不同的 `scene_id`（例如 `slug + 首次回合`），不能只用地点 slug，否则“上周在码头”和“这周在码头”的参与者会混在一起；
- 场景参与者用 `oag_scene_participants(scene_id, object_id, role, from_turn, to_turn)` 记录；
- 当前场景 = `valid_to_turn IS NULL` 的那条；
- 历史场景让“第 30 回合谁在场”可查，而不是只存当前名册。

### 4.5 事件

```sql
oag_events(
  id, world_id, session_id, turn_index, ordinal,
  scene_id, event_type, title, summary, details_json,
  visibility, importance, created_at
)
oag_event_objects(
  event_id, object_id, role             -- actor/target/location/witness/mentioned
)
```

- 每回合至少一条确定性事件；不依赖 LLM；
- `witness` 角色 = 事件观众，自动生成知识边；
- `details_json` 保留玩家输入、关键台词、属性/关系变化，供查询展开；
- 事件是“发生过什么”的真相源，摘要和记忆都是它的投影。

### 4.6 记忆

保留现有 `memories` 表（按角色、按回合、主观），增加关联：

```sql
oag_memory_objects(memory_id, object_id, role)
```

- 记忆是**角色主观**的：只属于 `memories.character_id`；
- 记忆可以关联事件、对象、关系；
- 召回仍然按 `character_id` 过滤；
- OAG 查询“我记得什么”只查自己的记忆，不查别人的。

### 4.7 文本条目

```sql
oag_lore_entries(
  id, world_id, title, keys_json, content,
  object_ids_json,                 -- 关联的本体对象
  constant, sort_order, audience, enabled, source
)
```

- Lore 是**文本投影**，不是真相源；它负责把本体对象/关系渲染成作者风格的散文；
- 关键词激活仍然保留，但激活后应尽量关联到对象，查询时可以用对象补充结构化事实；
- `audience`：public/director/character。

知识库文档同理：

```sql
oag_documents(document_id, audience, object_ids_json, tags_json)
```

正文仍用现有 `knowledge_documents`，OAG 只加可见性和对象关联。

### 4.8 摘要

```sql
oag_summaries(
  id, world_id, session_id, level,
  from_turn, to_turn, title, text,
  event_count, source, created_at
)
```

- level 1 = arc（默认 12 回合），level 2 = chapter（5 个 arc），level 3 = synopsis；
- 摘要关联事件范围，不替代事件账本；
- 查询“第 1~100 回合发生了什么”优先用摘要，细节再下钻事件。

## 5. 认知围栏：谁在什么时候知道什么

这是 OAG 和“全局记忆库”的根本区别，也是上一版最缺的一层。

### 5.0 术语：三道围栏

你说的“地理围栏”不是空间问题，而是“这个角色具不具备知道的条件”。建议拆成三道围栏：

| 围栏 | 回答的问题 | 例子 |
|---|---|---|
| **场景围栏**（Scene Fence） | 在不在场？ | Alice 在码头仓库；电话那头的 Bob 算远程在场 |
| **知情围栏**（Need-to-Know Fence） | 具不具备知道的条件？ | 只有长老知道；只有戴戒指的人看得见；只有去过地牢的人知道密道 |
| **表达围栏**（Render Fence） | 知道之后能显示多少？ | 耳语只显示给 recipients；旁人只看到“他们低声说了什么” |

**正式名称建议：知情围栏**（强调“是否具备知情条件”）；如果希望更有叙事感，可以用 **认知围栏**（Epistemic Fence）。代码里用 `KnowledgeBoundary` / `VisibilityPolicy` / `OagScope`，不要把 “fence” 写进 API。

不建议叫“地理围栏”：同一地点的人也可能不知道，电话/传音/信件也不受地理约束；叫“条件围栏”又太泛，像 if-else 配置。

### 5.1 Actor

- `character:<id>`：NPC / 角色；
- `player:<id>`：玩家当前扮演的角色（本质上也是 character，但查询权限不同：玩家可以看自己的角色卡，不看其他 NPC 私有记忆）；
- `director`：世界主控 / GM，可见全图；
- `author`：编辑器/调试，可见全图。

### 5.2 知识边

```sql
oag_knowledge(
  id, world_id, session_id,
  actor_id,
  target_kind,                  -- link/attribute/event/object
  target_id,
  knowledge_type,               -- public/witnessed/told/inferred/private
  source_event_id, source_actor_id,
  learned_turn, confidence, created_at
)
```

语义：

| 类型 | 含义 | 例子 |
|---|---|---|
| `public` | 世界常识，所有 actor 可见 | “临安是水运枢纽” |
| `witnessed` | 亲眼所见/亲耳所听 | Alice 在场看到老王逃走 |
| `told` | 被明确告知 | 玩家告诉 Alice 钥匙位置 |
| `inferred` | 从已知事实推导 | Alice 知道“老王在码头”+“码头失火”→ 推断老王有危险 |
| `private` | 只有自己知道 | 角色的内心判断、秘密 |

`public` 也可以直接由 `visibility='public'` 表达，二者等价；查询时统一处理：

```text
可见 = visibility='public'
     OR EXISTS(knowledge WHERE actor_id=? AND target_kind=? AND target_id=?)
     OR target 是当前场景的公共感知对象
```

### 5.3 知情条件

“在场”只是最低门槛。很多信息还要求角色具备条件：

- 知道另一个秘密（`knowledge`）；
- 身份/等级/感知/语言达到阈值（`attribute`）；
- 属于某个组织（`membership`）；
- 持有某件物品（`item`）；
- 与目标有某种关系（`relationship`）；
- 身处某个场景/区域（`scene`）；
- 时间未到/已过（`time`）；
- 显式白名单（`actor`）。

条件用声明式 JSON 表达，不由 LLM 临时判断：

```jsonc
{
  "mode": "all",
  "conditions": [
    { "type": "knowledge", "target": "event:evt-dungeon-visit" },
    { "type": "attribute", "key": "rank", "op": "gte", "value": 3 },
    { "type": "membership", "organization": "青云宗" }
  ]
}
```

| type | 参数 | 语义 |
|---|---|---|
| `knowledge` | `target` | actor 知道某个对象/关系/事件 |
| `attribute` | `key`, `op`, `value` | actor 的属性比较（eq/ne/gt/gte/lt/lte/in/contains） |
| `membership` | `organization` | actor 通过 `member_of` 属于某组织 |
| `item` | `item` | actor `holds`/`owns` 某物品 |
| `relationship` | `link`, `target` | actor 与目标存在某关系（如 `trusts`） |
| `scene` | `scene_id` 或 `location` | 当前场景/地点匹配 |
| `time` | `after_turn`/`before_turn` | 在某个回合之后/之前才可见 |
| `actor` | `actor_id` | 显式白名单 |
| `not` | `condition` | 条件取反 |
| `any`/`all` | `conditions` | 组合 |

存储：

```sql
oag_access_policies(
  id, world_id, session_id,
  target_kind,                 -- object/attribute/link/event
  target_id,
  policy_json, priority, enabled,
  created_at,
  UNIQUE(target_kind, target_id)
)
```

`visibility` 负责常见情况（public/scene/private），`access_policy` 负责更细的条件；两者同时存在时取交集。

评估规则：

- 在构建 `VisibleGraph` 时评估，**先过滤再遍历**；
- `knowledge` 条件只查 actor 的 `oag_knowledge`/public，不递归查 VisibleGraph，避免循环；
- `attribute` 缺失时按“不知道”处理（fail closed），不能把未知当满足；
- `membership`/`item`/`relationship` 只查 actor 自己可见的低敏边；
- 失败时对角色返回“不存在”，不要返回“无权限”，避免泄露存在性；
- 导演/作者/调试可以看到 `filtered_by_policy` 和原因；
- 条件最多 8 个、嵌套最多 3 层，禁止任意脚本；
- LLM 可以建议条件，但最终由作者/引擎确认。

例子：

| 信息 | 条件 |
|---|---|
| 只有青云宗长老知道掌门闭关 | `membership=青云宗` + `attribute.rank >= 3` |
| 只有戴窥视之戒的人能看见幽灵 | `item=窥视之戒` |
| 只有去过地牢的人知道密道 | `knowledge=event:dungeon-visit` |
| 只有懂精灵语的人听得懂这句话 | `attribute.languages contains elvish` |
| 第 30 回合后才公开 | `time.after_turn=30` |
| 只有 Alice 知道 | `actor=Alice` |

### 5.4 写入规则

- 事件：`witness` 角色获得 `witnessed`；公开事件可以 `public`。
- 对话：默认所有在场者获得 `witnessed`；如果消息有 `recipients`（耳语/密谈），只有 recipients 获得内容级 `told`，其他在场者只获得“发生了一次耳语”的事件级 `witnessed`。
- 事实/关系：`asserted_by` = 说话人；`witness` 获得 `witnessed`；如果说话人明确“只告诉 X”，则 X 获得 `told`，其他人没有。
- 知识库：文档有 `audience`；角色读取文档不等于世界内知道，但查询工具应按 audience 过滤，避免剧透。
- 记忆：`memories.character_id` 是唯一主体；记忆本身不再额外写知识边（记忆存在即证明该角色知道）。

### 5.5 查询时先裁剪，再遍历

错误做法：

```text
1. 查全图最短路径 A→B
2. 过滤掉 actor 不知道的边
3. 返回结果
```

这样会泄露“存在一条经过隐藏边的路径”（路径长度、可达性本身就是信息）。

正确做法：

```text
1. 按 actor 加载可见对象/属性/关系/事件
2. 在可见子图上做遍历/聚合/否定
3. 返回结果 + 可见性说明
```

实现上，`OagQueryContext` 先构建 `VisibleGraph`，后续所有操作只操作这个图。

### 5.6 事实与声称

- 所有关系/属性都是 **claim**，不是绝对真理；
- `asserted_by`：谁说的；
- `truth_status`：`asserted`（仅声称）/`confirmed`（多方确认/作者 Canon）/`disputed`（冲突）；
- 查询结果要能返回“据 X 所述”，而不是把 NPC 的谎话当事实；
- Canon 关系 `source='author'`、`truth_status='confirmed'`，LLM 抽取默认 `asserted`。

### 5.7 场景围栏怎么用

- 本项目没有连续坐标/寻路，只有 `location` / `scene`。所以场景围栏用**成员关系**，不是多边形。
- **说话资格 = 在当前 witnesses 内**：不在场角色不能进入 `planned_speakers`；电话/远程角色必须先加入本回合 `witnesses`（`presence_mode=remote`）才允许发言。
- **场景就是围栏**：`scene_id` + `scene_participants`；同一场景默认互相可见/可听。
- **电话/远程 = `presence_mode=remote`**：加入本回合的 `participants` / `witnesses`，但不加入物理 `present_characters`；知识上按在场处理。用户已确认“电话等于在场”，默认给完整在场权限；如果以后要“只听见声音、看不见画面”，再加 `perception_modality`。
- 严格来说，电话是连接两个场景的**通信事件**：两边各自有自己的物理场景，只有通话内容共享。按“电话=在场”简化，就是把远程角色加入本回合 witnesses，代价是两边场景的感知被合并。若要严格模拟，需要 `oag_communications(from_scene, to_scene, participants)`，不建议现在做。
- **场景围栏只用于写时观众/场景边界，不用来硬过滤记忆召回**。否则角色一离开地点就“失忆”；当前位置只应作为召回的相关性加成，不是硬条件。
- **子区域/隔音/潜行**再引入 `oag_zones` 图：location 下的 zone + audibility/visibility 边 + 角色 `current_zone`；不要一上来做坐标几何。
- 为什么不用坐标围栏：
  - 文本叙事没有稳定的位置数据，作者要维护多边形；
  - “码头”内部仍然需要耳语、隔墙、楼上楼下规则；
  - 电话、传音、神识、监控会直接打破纯几何；
  - 坐标围栏会让作者把精力花在几何上，而不是剧情。

结论：**场景围栏能覆盖大部分需求，耳语补例外，zone 图留到需要潜行/窃听时。**

### 5.8 耳语/私聊

不必新建“私聊记忆表”；在消息上带 `recipients` 即可。

一条耳语同时产生两个东西：

1. **内容**：只有 recipients 获得 `told` 知识 + 记忆；
2. **事件**：所有在场者知道“X 对 Y 耳语”这件事，但不知道内容（`visibility='scene'` 的事件，summary 不含正文）。

读路径：

- recipients：`recent_dialogue` / 记忆显示正文；
- 非 recipients：显示 `[X 对 Y 低声说了什么]`，不显示正文；
- 非在场者：连“发生过耳语”都不知道。

谁决定 recipients：

- 优先让说话角色在返回契约里给 `recipients`（默认空 = 所有在场者）；
- 玩家侧后续可以加显式 UI 动作；
- 校验：`recipients ⊆ 当前 witnesses`，不含说话人自己，不能是空字符串。

难度：

- 存储/查询：约 1 天（复用消息 metadata + 知识边）；
- 契约/解析/校验/渲染/测试：约 2~3 天；
- 真正的难点不是表，而是**同一条消息对不同 viewer 渲染不同内容**，以及“知道有耳语但不知道内容”的事件建模。

如果只做“玩家私聊 NPC”、不做 NPC 主动耳语，可以更简单：玩家消息带 recipients，NPC 默认全员；但这样 NPC 不能主动密谈。

工作量（基于当前代码，不含 M0 隔离）：

| 范围 | 内容 | 预估 |
|---|---|---|
| 最小 | 玩家消息带 recipients；NPC 回复仍全员；只做记忆/知识过滤 | 1~2 天 |
| v1（推荐） | NPC 返回 `recipients`；recipients 拿正文，其他在场者只看到脱敏占位；旁白不随耳语下发 | 4~6 天 |
| 完整 | v1 + 玩家侧收件人 UI + 旁白分视角 + 分支/回滚 + 测试 | 约 1 周 |

注意：耳语依赖 M0 的消息级 `witnesses`。没有 M0，先做耳语一定会在 `recent_dialogue` / `dialogue_focus` 漏正文。

### 5.9 记忆隔离不变量

1. 角色查询永远不能返回其他角色的 `memories`；
2. 角色查询不能返回未见证/未被告知的关系、属性、事件；
3. 角色查询不能通过路径长度、数量、聚合间接推断隐藏事实；
4. 导演/作者可以看全图，但结果要标记 `private_to`，避免旁白泄露；
5. 分支/回滚后，知识边与事件必须一起回滚，不能出现“记忆回到过去、知识还在未来”；
6. 同一条消息对不同 viewer 可以渲染不同内容；非 recipients 只能看到脱敏事件，不能看到正文。

## 6. 查询引擎

### 6.1 查询上下文

```rust
struct OagQueryContext {
    world_id: String,
    session_id: String,
    actor: OagActor,          // character/player/director
    as_of_turn: i32,          // 默认当前回合
    max_depth: u8,            // 默认 3
    max_nodes: usize,         // 默认 200
}
```

### 6.2 可见子图

`VisibleGraph` 包含：

- objects：Canon + session，过滤掉 `visibility='private'` 且 actor 不可见的对象；有 `access_policy` 的对象再按条件过滤；
- attributes：按 `oag_knowledge` + `access_policy` 过滤；同一 object+key 同时有 Canon 和 session 行时，session 行优先（更具体）；
- links：按 `visibility` + `oag_knowledge` + `access_policy` 过滤；session 级关系覆盖同类型的 Canon 关系；
- events：按 witness/knowledge + `access_policy` 过滤；
- memories：只加载 actor 自己的；
- lore/docs：按 audience + `access_policy` 过滤。

`access_policy` 的评估结果按 `(actor, target)` 缓存；条件失败时该节点不进图，后续遍历/聚合自然看不到它。

图规模控制在几百到几千节点；每次查询在内存里做 BFS/过滤，SQL 只负责加载和索引。

### 6.3 操作

| 操作 | 语义 | 例子 |
|---|---|---|
| `resolve` | 名称/别名 → 对象 | “老王” → character:lao-wang |
| `get` | 对象卡片：属性 + 1 跳关系 + 相关事件 | “给我老王的所有信息” |
| `find` | 按类型/属性/关系过滤对象 | “所有住在码头的女性角色” |
| `traverse` | 沿关系类型多跳 | “韩立的师父的敌人” |
| `path` | 两对象之间的最短路径 | “老王和城主怎么认识的” |
| `aggregate` | count/sum/avg/group by | “青云宗有多少弟子” |
| `events` | 按时间/参与者/地点/类型查事件 | “第 30~40 回合发生了什么” |
| `knowledge` | 谁知识什么 / 某人知道什么 | “谁知道钥匙在 12 号柜” |
| `absence` | 不存在某关系/属性 | “谁从没去过码头” |

### 6.4 语义细节

**时间**

- `as_of_turn` 默认当前回合；
- 关系/属性：`valid_from <= T AND (valid_to IS NULL OR valid_to > T)`；
- 事件：`turn_index <= T`；
- `include_history=true` 时返回已失效边，并标注 `invalid_at_turn`。

**方向与深度**

- `direction=out|in|both`；
- `depth` 1~3（可配置），超过则截断并提示；
- 每跳都应用类型过滤和可见性过滤。

**逻辑谓词**

- `eq/ne/gt/gte/lt/lte/in/contains/exists/not_exists`；
- 属性比较按 `value_type` 解析（数字/字符串/枚举/列表）；
- 否定查询（absence）只返回“在可见子图里确实没有”的结果，并注明“基于当前已知信息”。

**聚合**

- `count`、`count_distinct`、`sum`、`avg`、`min`、`max`；
- `group_by`：对象类型、属性、关系目标；
- 聚合只统计可见子图。

**推理**

- `part_of` 传递、`ally_of` 对称、`inverse` 反向；
- 推理边 `inferred=1`，默认包含但可排除；
- 推理不能跨越不可见边。

### 6.5 查询示例

#### 例 1：谁住在码头？

```json
{
  "op": "find",
  "object_type": "character",
  "link": { "type": "located_in", "to": { "name": "码头", "type": "location" } },
  "as_of_turn": 42
}
```

SQL/图遍历：`character -[located_in]-> location(码头)`，过滤当前有效边。

#### 例 2：韩立的师父的敌人是谁？

```json
{
  "op": "traverse",
  "from": { "name": "韩立", "type": "character" },
  "steps": [
    { "link": "member_of", "direction": "out" },
    { "link": "enemy_of", "direction": "out" }
  ],
  "as_of_turn": 42
}
```

两跳；如果 `enemy_of` 是 symmetric，反向边自动物化。

#### 例 3：第 30 回合时银钥匙在谁手里？

```json
{
  "op": "find",
  "object_type": "character",
  "link": { "type": "holds", "to": { "name": "银钥匙", "type": "item" } },
  "as_of_turn": 30,
  "include_history": false
}
```

关键在于 `as_of_turn` 用关系的时间窗，而不是“当前状态”。

#### 例 4：谁知道钥匙在 12 号柜？

```json
{
  "op": "knowledge",
  "target": { "kind": "link", "id": "link-key-locker" },
  "actor": "character:alice"
}
```

- 角色查询只返回自己知道/不知道；
- 导演查询返回所有知道的人 + 知识来源（witnessed/told/inferred）。

#### 例 5：青云宗有多少弟子？

```json
{
  "op": "aggregate",
  "metric": "count",
  "object_type": "character",
  "link": { "type": "member_of", "to": { "name": "青云宗", "type": "organization" } },
  "as_of_turn": 42
}
```

#### 例 6：哪些人从没去过码头？

```json
{
  "op": "absence",
  "object_type": "character",
  "link": { "type": "located_in", "to": { "name": "码头" } },
  "as_of_turn": 42
}
```

只基于可见子图；如果某人的行踪未知，结果要标注“未知”，不能断言“没去过”。

#### 例 7：阿禾最近一次提到父亲是什么时候？

```json
{
  "op": "events",
  "participant": "阿禾",
  "mentioned": "父亲",
  "from_turn": 1,
  "to_turn": 42,
  "limit": 5
}
```

事件 `details_json` / `summary` 做文本匹配；结构化关系优先（`related_to`/`family_of`）。

### 6.6 实现策略

- SQLite 负责存储和直接索引查询；
- 多跳/路径/否定/聚合在 Rust 内存图里做（BFS/DFS），图规模小、可控；
- 每次查询先加载 `VisibleGraph`，避免在 SQL 里写复杂的可见性 JOIN；
- 查询结果缓存到本轮 `oag_query_traces`，同一回合重复查询可复用；
- 限制：`max_depth<=3`、`max_nodes<=200`、`max_results<=20`，防止图爆炸；
- 所有排序确定性（分数 + id），避免同一输入不同结果。

## 7. 给智能体的查询工具

工具是 OAG 的“查询接口”，不是关键词搜索。建议 5 个：

### 7.1 `oag_entity`

```json
{
  "name": "韩立",
  "as_of_turn": 42,
  "include_history": false
}
```

返回：对象卡片（类型、属性、别名）、1 跳关系、最近事件、相关记忆（仅自己的）、相关 lore。

### 7.2 `oag_query`

```json
{
  "object_type": "character",
  "name": "韩立",
  "attributes": [{ "key": "status", "op": "eq", "value": "alive" }],
  "link": { "type": "member_of", "to": "青云宗", "direction": "out" },
  "as_of_turn": 42,
  "limit": 10
}
```

用于 find/过滤；支持属性比较、关系存在、类型过滤。

### 7.3 `oag_traverse`

```json
{
  "from": "韩立",
  "steps": [
    { "link": "member_of", "direction": "out" },
    { "link": "enemy_of", "direction": "both" }
  ],
  "target_type": "character",
  "as_of_turn": 42
}
```

用于多跳关系链；返回路径、节点、边、置信度。

### 7.4 `oag_timeline`

```json
{
  "participant": "韩立",
  "location": "码头",
  "event_type": "combat",
  "from_turn": 1,
  "to_turn": 42,
  "limit": 10
}
```

用于事件查询；返回事件摘要、参与者、来源、回合。

### 7.5 `oag_knows`

```json
{
  "actor": "Alice",
  "target": "银钥匙在12号柜"
}
```

- 角色调用：只回答“我知道/不知道”，以及自己知道的原因（witnessed/told/inferred）；
- 导演调用：返回所有知道的人、来源、时间；
- 这是隔离的显式查询入口，不允许绕过。

### 7.6 文本兜底 `oag_search`

结构化查询查不到时，再查 lore 文本/知识库文档/记忆原文；结果按 actor audience 过滤。它不再是主入口。

### 7.7 结果格式

```json
{
  "ok": true,
  "op": "traverse",
  "count": 2,
  "truncated": false,
  "results": [
    {
      "kind": "character",
      "id": "obj:character:alice",
      "name": "Alice",
      "path": [
        { "from": "韩立", "link": "member_of", "to": "青云宗" },
        { "from": "青云宗", "link": "enemy_of", "to": "魔教" }
      ],
      "confidence": 0.9,
      "citation": "第 42 回合 / 作者设定"
    }
  ],
  "hint": "只依据以上结构化结果回答；没有查到就说不知道。"
}
```

### 7.8 工具策略

- 工具结果里的对象/关系/事件都必须带 `source`、`turn`、`confidence`；
- 角色工具结果先过 `VisibleGraph`；
- 导演工具可以看全图，但结果标 `private_to`；
- 单次结果不超过 2000 字符、每回合不超过 3 次调用；
- 空结果返回 `count=0` + `hint`，不要编造。

## 8. 上下文投影：从本体生成 `oag_context`

有了本体，上下文不再是“检索几个文本块”，而是**一组投影查询**。投影只负责“这个角色此刻知道/相信什么”，不负责“他应该怎么做”；目标、性格、说话方式由角色卡和系统提示提供，由模型自己决定行为。

所有投影都经过 `Perspective`（actor + session + turn + scene）过滤：同一个世界，不同 NPC 得到不同的 belief snapshot；导演拿 truth。

| 投影 | 查询 | 预算 |
|---|---|---|
| `scene_frame` | 当前场景、地点、时间、在场者、公开属性 | 250 tokens |
| `entity_cards` | 玩家输入提到的实体 + 场景参与者 + 当前活跃实体，各取属性 + 1 跳关系 | 600 |
| `fact_digest` | 与场景/实体相关的当前有效关系/属性，按重要度 | 400 |
| `event_digest` | 最近 N 条可见事件 + 相关历史事件 | 500 |
| `memory_digest` | 角色自己的相关记忆（复用现有召回） | 400 |
| `lore_text` | 被选中对象关联的 lore 文本 + 关键词命中 | 500 |
| `summary` | 最新 arc/chapter 摘要 | 300 |

总预算默认角色 2400、导演 1800，按来源配额裁剪；去重按对象/关系/事件 id，而不是文本相似度。

### 8.1 渲染

```text
【场景】临安城 · 码头仓库 · 第 42 回合
在场：你、老王、阿禾

【老王】
身份：码头管事 / 状态：受伤
关系：位于 码头仓库；持有 铜钥匙；与 阿禾 敌对
最近：第 40 回合与你在仓库冲突

【当前事实】
- 铜钥匙 在 阿禾 手中（第 41 回合，你亲眼所见）
- 老王 欠 沈老板 一笔钱（第 38 回合，据老王所述）

【相关背景】
- [码头仓库] 临安城最大的私货中转点，夜禁后仍有搬运工进出……

【历史事件】
- 第 37 回合 · 仓库冲突：你逼问老王，老王承认见过半张地图。
- 第 41 回合 · 交钥匙：阿禾把铜钥匙交给你，要求你不再追问她父亲。

【使用说明】
以上是结构化设定与检索结果，不是当前对话。只使用与当前场景相关的部分；
与最近对话冲突时，以最近对话为准；没有提到的信息不要编造。
```

### 8.2 注入点

- 只保留一个 `oag_context` section；
- `chat_history` 只放**按 witnesses 过滤后的**最近对话；
- `oag_context` 放在静态段之后、`chat_history` 之前，避免每回合变化破坏前缀缓存；
- 空则跳过。

## 9. 存储设计（DDL 草案）

```sql
-- TBox
CREATE TABLE oag_object_types (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, key TEXT NOT NULL,
  label TEXT NOT NULL, parent_key TEXT,
  attributes_json TEXT NOT NULL DEFAULT '[]',
  builtin INTEGER NOT NULL DEFAULT 0,
  UNIQUE(world_id, key)
);

CREATE TABLE oag_link_types (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, key TEXT NOT NULL,
  label TEXT NOT NULL,
  domain_json TEXT NOT NULL DEFAULT '[]',
  range_json TEXT NOT NULL DEFAULT '[]',
  functional INTEGER NOT NULL DEFAULT 0,
  symmetric INTEGER NOT NULL DEFAULT 0,
  transitive INTEGER NOT NULL DEFAULT 0,
  temporal INTEGER NOT NULL DEFAULT 1,
  inverse TEXT,
  builtin INTEGER NOT NULL DEFAULT 0,
  UNIQUE(world_id, key)
);

-- ABox
CREATE TABLE oag_objects (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',        -- '' = Canon
  object_type TEXT NOT NULL,
  name TEXT NOT NULL, name_normalized TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  description TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'world',
  created_turn INTEGER NOT NULL DEFAULT 0,
  updated_turn INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, object_type, name_normalized)
);

CREATE TABLE oag_attributes (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  object_id TEXT NOT NULL, key TEXT NOT NULL,
  value_json TEXT NOT NULL DEFAULT 'null',
  value_type TEXT NOT NULL DEFAULT 'text',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'world',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  created_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE oag_links (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  from_object_id TEXT NOT NULL, link_type TEXT NOT NULL,
  to_object_id TEXT, to_value TEXT NOT NULL DEFAULT '',
  attributes_json TEXT NOT NULL DEFAULT '{}',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'world',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by TEXT, source_event_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  inferred INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE oag_scenes (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
  name TEXT NOT NULL, location_object_id TEXT,
  background_hint TEXT NOT NULL DEFAULT '',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER
);

CREATE TABLE oag_scene_participants (
  scene_id TEXT NOT NULL, object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'present',
  from_turn INTEGER NOT NULL DEFAULT 0,
  to_turn INTEGER,
  PRIMARY KEY(scene_id, object_id)
);

CREATE TABLE oag_events (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0,
  scene_id TEXT, event_type TEXT NOT NULL DEFAULT 'turn',
  title TEXT NOT NULL DEFAULT '', summary TEXT NOT NULL DEFAULT '',
  details_json TEXT NOT NULL DEFAULT '{}',
  visibility TEXT NOT NULL DEFAULT 'scene',
  importance REAL NOT NULL DEFAULT 0.5,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, turn_index, ordinal)
);

CREATE TABLE oag_event_objects (
  event_id TEXT NOT NULL, object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'participant',
  PRIMARY KEY(event_id, object_id, role)
);

-- Knowledge
CREATE TABLE oag_knowledge (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  target_kind TEXT NOT NULL, target_id TEXT NOT NULL,
  knowledge_type TEXT NOT NULL DEFAULT 'witnessed',
  source_event_id TEXT, source_actor_id TEXT,
  learned_turn INTEGER NOT NULL DEFAULT 0,
  confidence REAL NOT NULL DEFAULT 1.0,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(actor_id, target_kind, target_id)
);

-- 知情条件（可选的细粒度围栏；visibility 负责常见情况）
CREATE TABLE oag_access_policies (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  policy_json TEXT NOT NULL DEFAULT '{}',
  priority INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(target_kind, target_id)
);

-- Text projections
CREATE TABLE oag_lore_entries (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  keys_json TEXT NOT NULL DEFAULT '[]',
  content TEXT NOT NULL DEFAULT '',
  object_ids_json TEXT NOT NULL DEFAULT '[]',
  constant INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 100,
  audience TEXT NOT NULL DEFAULT 'public',
  enabled INTEGER NOT NULL DEFAULT 1,
  source TEXT NOT NULL DEFAULT 'author',
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE oag_summaries (
  id TEXT PRIMARY KEY, world_id TEXT NOT NULL, session_id TEXT NOT NULL,
  level INTEGER NOT NULL,
  from_turn INTEGER NOT NULL, to_turn INTEGER NOT NULL,
  title TEXT NOT NULL DEFAULT '', text TEXT NOT NULL DEFAULT '',
  event_count INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL DEFAULT 'llm',
  created_at TEXT NOT NULL DEFAULT ''
);

-- Trace
CREATE TABLE oag_query_traces (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL, actor_id TEXT,
  query_json TEXT NOT NULL DEFAULT '{}',
  visible_nodes INTEGER NOT NULL DEFAULT 0,
  result_json TEXT NOT NULL DEFAULT '{}',
  latency_ms INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT ''
);
```

索引：

```sql
CREATE INDEX idx_oag_objects_name ON oag_objects(world_id, name_normalized);
CREATE INDEX idx_oag_objects_session ON oag_objects(session_id, object_type);
CREATE INDEX idx_oag_links_from ON oag_links(from_object_id, link_type, valid_to_turn);
CREATE INDEX idx_oag_links_to ON oag_links(to_object_id, link_type, valid_to_turn);
CREATE INDEX idx_oag_attr_object ON oag_attributes(object_id, key, valid_to_turn);
CREATE INDEX idx_oag_events_turn ON oag_events(session_id, turn_index);
CREATE INDEX idx_oag_event_objects_object ON oag_event_objects(object_id, event_id);
CREATE INDEX idx_oag_knowledge_actor ON oag_knowledge(actor_id, target_kind, target_id);
```

## 10. 世界包格式（开发阶段，不做兼容）

世界包版本直接升到新格式，不保留 v10 导入：

```text
world/world.json          # 基础信息
world/ontology.json       # 可选：自定义 object_types / link_types
world/objects.json        # 可选：Canon 对象与关系（作者显式建模）
world/lore.json           # 可选：文本条目
characters/*/character.json
assets/*
knowledge_bases 仍在 world.json 或独立文件
```

- `characters` 导入时自动生成 `character` Canon 对象（name/role/background/attributes）；
- `world.map_nodes_json` 导入时生成 `location` 对象；
- `world/objects.json` 允许作者显式声明组织、物品、人物关系；
- `ontology.json` 没写就用内置类型/关系；
- 导出时从 `oag_objects`（`session_id=''`）生成 `objects.json`，从 `oag_lore_entries` 生成 `lore.json`；
- 旧包直接拒绝，开发阶段不写兼容分支。

## 11. 从现有数据迁移

开发阶段可以“清库重来”，但为了不丢测试档，建议做一次性迁移。

不复用 `memory_entities` / `memory_relations` 作为本体表：它们只有 session 级、没有 Canon 层、没有类型/基数、没有可见性/属性历史。迁移完成后这两张表只作为 legacy 只读数据或直接删除。

| 旧 | 新 |
|---|---|
| `memory_entities` | `oag_objects`（type 用 `entity_type`，未知用 `concept`） |
| `memory_relations` | `oag_links`（predicate → link_type；未知 → `related_to`） |
| `fact_extractions` | `oag_links` + `oag_knowledge` |
| `memories` | 保留；补 `event_id`/`oag_memory_objects` |
| `visible_characters` + memories | 反推每回合 audience，写 `oag_knowledge`；优先用 `turn_journal.snapshot_created.session_snapshot.visible_characters` 还原回合开始名册，用下一回合快照/当前 session 还原回合结束名册 |
| `attribute_values` | 桥接为 `oag_attributes`（session/session_character） |
| `knowledge_documents` | `oag_documents` audience 默认 `public` |

迁移只跑一次，由 `schema_version`/`settings` 标记；失败可回滚（事务）。

## 12. 实现顺序

### M0 隔离基础（必须先做）

- **禁止不在场角色发言**：`planned_speakers` 只允许“场景名册 + 本回合新生成角色 + 远程参与者”；从 `allowed_visible` 里移除 `world_character_roster`。
- **电话 = 远程在场**：远程角色以 `presence_mode=remote` 加入本回合 `witnesses`，不进物理 `present_characters`；知识/记忆按在场处理。
- 消息 metadata 加 `scene_id/witnesses/presence_mode/recipients`；
- 写入 turn audience；
- `build_turn_entries` 按每条消息的 witnesses 写；
- `slice_character_history` / `dialogue_focus` 按 viewer 过滤；
- `FactUpdate` 带 speaker/visibility；
- 测试：私聊不出现在第三方上下文、场景切换回合不串、电话参与者拿到记忆、缺席角色发言被拒绝。

耳语（`recipients`）可以放在 M0.5：内容只给 recipients，其他在场者只看到“发生了一次耳语”的脱敏事件。若 M0 时间紧，先保证“在场=听到”，但 metadata 预留 `recipients`。

### M1 本体核心

- TBox/ABox 表 + repository；
- 世界包导入 characters/locations → objects；
- `oag_events` 确定性写入 + event_objects；
- `oag_knowledge` 从 witnesses 生成；
- `oag_access_policies` + 条件评估（public/scene/private/conditional）；
- 实体解析（name/alias/type）；
- `OagQueryContext` + `VisibleGraph`；
- `oag_entity` / `oag_query` 基础查询。

### M2 逻辑查询与投影

- `oag_traverse` / `oag_timeline` / `oag_knows` / aggregate / absence；
- 扩展条件类型：attribute / membership / item / relationship / time / knowledge；
- `oag_context` 投影（scene frame/entity cards/facts/events/memories/lore）；
- Debug 查询 trace；
- 世界编辑器本体/关系编辑最小 UI。

### M3 抽取与压缩

- LLM 事实/关系抽取（带 speaker/visibility）；
- 事件摘要、arc/chapter 摘要；
- 知识传播（told）；
- 知识库文档 audience + chunks。

### M4 打磨

- 矛盾检测、claim truth_status、验证约束；
- 查询 UI（实体页/时间线/关系图）；
- 评测（逻辑查询集、隔离测试、长会话）；
- 性能（索引、缓存、分页）。

## 13. 风险与开放问题

1. **抽取质量**：LLM 把“老王说钥匙在 12 号柜”抽成事实，而不是 claim；需要 `asserted_by` + `truth_status`，查询时渲染“据老王所述”。
2. **谓词归一化**：自由谓词会导致 `related_to` 泛滥；需要内置关系表 + 同义词映射 + 作者审核。
3. **实体解析**：同名、代称、翻译名；先做规范名 + 别名 + 最长匹配，不追求完美。
4. **知识传播**：默认“在场=听到”；耳语/密谈需要 recipients；远程通信需要显式场景。
5. **性能**：VisibleGraph 每次加载可能几百到几千节点；需要索引、缓存、深度限制。
6. **提示词复杂度**：工具 schema 不能太复杂；用 `oag_entity`/`oag_query`/`oag_traverse` 三个主入口 + 文本兜底。
7. **作者负担**：要求作者建对象/关系会劝退；默认从角色/地点自动生成，lore 仍可只写文本。
8. **是否要规则校验**：只做只读约束（functional/domain/range），不做执行；冲突时记录 warning，不阻断回合。

## 14. 和 Palantir OAG 的对应关系

| Palantir OAG | 本项目 |
|---|---|
| Object types | `oag_object_types` |
| Objects / properties | `oag_objects` / `oag_attributes` |
| Links | `oag_links` |
| Actions | 不做 |
| Functions / logic | 查询操作 + 只读约束 |
| Ontology-backed retrieval | `VisibleGraph` + `oag_query` |
| Governance / permissions | `oag_knowledge` + actor |

差别是：我们不追求企业级本体建模和动作闭环，只追求**叙事世界里的结构化查询 + 记忆隔离**。这也是“轻量 OAG”的边界。
