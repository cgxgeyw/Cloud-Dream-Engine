# OAG 本体设计与逻辑查询

> 状态：设计稿 v4（2026-11-26）。M0、M1 与 M2 的视角投影、逻辑查询内核已落地；统一只读模型工具 `oag_query` 已按现有工具机制接入导演与角色链路，条件访问策略与专用投影检查界面仍待实现。实施以 §3.5、§9、§11、§12 为准。
> 前置阅读：[记忆隔离代码审计](./02-isolation-audit.md)
> **本体归世界包定义**：对象类型、属性、关系及其约束由每个世界的 `ontology.json` 声明；应用只提供存储、校验、查询和知识隔离能力，不把某类题材的语义硬编码为所有世界的本体。`objects.json` 仍是可选增量扩展；没有本体文件时只使用明确标记的最小兼容底座，并从现有角色与地图节点生成最小 Canon。新制作并启用 OAG 的世界应提供本体文件。

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
| Schema | 无，LLM 自动抽 | 有，但通常固定 | 每个世界包定义自己的 object/link types |
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
- **跨世界统一本体**：禁止把某个世界的类型/关系强加给其他世界；只有平台保留的逻辑结构和存储约束跨世界一致。
- 实时多人协作。

## 2. 四层架构

```text
┌──────────────────────────────────────────────────────────────┐
│ 1. 世界包定义本体模式                                         │
│    对象类型 / 属性 / 关系 / 约束；应用不预设题材语义             │
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

### 3.1 对象类型

**对象类型由世界包声明，不由应用按题材预置。** `ontology.json` 是每个世界的类型字典；同一类型键可以出现在不同世界，但定义、属性和继承关系互不影响。每个世界至少声明一个对象类型；角色级 OAG 还要求至少一个类型显式标记 `actor=true`，地图导入要求一个类型显式标记 `platform_role='location'`。`actor` 只表示该类型的对象能作为发言者或知识主体，`platform_role` 只表示该类型承担平台地图地点，不限定名称、属性或关系。

平台只保留四类逻辑结构：`scene`、`event`、`memory`、`document`。它们分别复用 `oag_scenes`、`oag_events`、`memories`、`knowledge_documents`，不重复写入 `oag_objects`；作者自定义类型不能占用这四个保留 key。这四类表达的是运行记录，不是世界题材语义。

没有 `ontology.json` 的世界包使用**兼容模式**：生成明确标记的最小类型集合，仅用于迁移现有 `characters` 与 `map_nodes_json`。该集合不是世界作者的语义声明，不能据此臆造其他对象或关系；运行期无法归类的数据保留在 legacy 表中。新制作并启用 OAG 的世界应提供本体文件。

世界包中的类型声明示例：

```jsonc
// 世界包里的 ontology.object_types
[
  {
    "key": "suspect",
    "label": "嫌疑人",
    "parent": null,
    "actor": true,
    "platform_role": "",
    "attributes": [
      { "key": "alibi", "label": "不在场证明", "value_type": "string" }
    ]
  }
]
```

不同世界可以声明完全不同的类型，例如现代悬疑世界使用“嫌疑人、账户、证据、案件”，历史世界使用“人物、地点、组织、概念”。类型支持 `parent` 继承；查询包含子类型时按世界内的继承定义处理。

### 3.2 属性类型

属性结构由应用提供，**属性键、含义、取值和约束由世界包定义**。应用只规定值如何存储和比较：

- 值的基本存储类型：`string` / `number` / `boolean` / `enum` / `list` / `json`
- 可声明 `required`、`default`、`enum_options`
- 可声明 `functional`（同一对象同一时刻只有一个值）
- 可声明 `temporal`（保留历史，默认 true）

属性不是塞进 `attributes_json` 就完事：运行时属性写入 `oag_attributes` 表，带 `valid_from_turn` / `valid_to_turn`，这样才能回答“第 30 回合时他的年龄/身份是什么”。

### 3.3 关系类型

关系结构由应用提供，**关系名称、起点、终点、方向和约束由世界包定义**。下面是格式示例：

```jsonc
{
  "key": "knows",
  "label": "认识",
  "domain": ["suspect"],
  "range": ["suspect"],
  "functional": false,
  "temporal": true
}
```

以下只是世界包可以声明的示例，不是应用强制要求的题材语义。每个世界自行决定需要哪些关系；没有声明的关系不能写入该世界。

| 关系键 | 中文 | 起点类型 → 终点类型 | 语义 |
|---|---|---|---|
| `located_in` | 位于 | character/item → location | 当前所在地；functional |
| `part_of` | 属于 | location/item → location/item | 整体-部分；transitive |
| `owns` | 拥有 | character/organization → item | 所有权 |
| `holds` | 持有 | character → item | 当前持有（可和 owns 分离） |
| `member_of` | 成员 | character → organization | 组织成员 |
| `leader_of` | 首领 | character → organization | 组织首领；functional |
| `ally_of` | 盟友 | character/organization → 同类 | symmetric |
| `enemy_of` | 敌对 | character/organization → 同类 | symmetric |
| `mentor_of` | 师徒 | character → character | 时间关系 |
| `related_to` | 相关 | * → * | 可选兜底关系；只有世界包声明后才允许使用 |

| `related_to` | 相关 | * → * | 可选兜底关系；只有世界包声明后才允许使用 |

`knows` 这个名字如果表示角色之间的社会认识，可以由世界包声明为普通关系；它与平台用于记录“谁获知哪条信息”的知识边不是一回事。`participates_in` 由 `oag_event_objects` 表达；`caused_by` 留到后续版本。

`domain/range` 是写入校验，不是执行规则。违反世界包约束的作者数据必须拒绝；运行时候选关系若无法安全归类，保留原始文本并进入后续审核，不能静默套用未声明的兜底关系。查询按该世界自己的类型定义过滤。关系终点可以是对象稳定键，也可以是显式字面量 `{ "literal": ... }`；字面量关系的 `range` 使用 `string` / `number` / `boolean` / `json` 或 `literal`，普通字符串默认仍按对象稳定键解析。

### 3.4 约束与推理（只读）

- **类型约束**：属性值类型、关系的起点/终点类型；定义均来自世界包。
- **基数约束**：只执行世界包为具体类型/关系声明的 functional、对称、传递等约束；应用不预置“一个首领”“一个地点”等题材规则。
- **时间约束**：`valid_from <= valid_to`；同一功能关系在重叠时间不能有两个值。
- **推理**（物化，不做 OWL）：
  - 传递闭包只用于世界包明确声明为传递的关系；
  - 对称和反向行为只按世界包声明执行；
  - 推理结果标记 `inferred=1`、`confidence` 降低，查询可排除。
- 推理只在**可见子图**内做；不能用隐藏边推导可见结论。

### 3.5 M1 冻结方案（先做“可查的最小本体”）

M1 只交付一条闭环：**稳定对象 → 时间属性/关系 → 确定性事件 → actor 知识边 → 可见子图 → 基础查询**。它不新增 LLM 调用，也不改变角色 prompt；`memory_mode='oag'` 要到 M2 投影接入后才真正区别于 Embedding。

#### 类型与关系

世界包通过 `ontology.json` 声明本世界的对象类型、属性、关系、起点/终点类型、functional/symmetric/transitive 等约束；导入时按 world 隔离校验并 seed。`actor` 是平台绑定标记，表示该类型可作为发言者或知识主体；其余语义全部归世界包。应用只保留 `scene`、`event`、`memory`、`document` 四类逻辑结构，分别复用专用表，不重复写入 `oag_objects`；自定义类型不能占用这四个保留 key。

- 有本体文件：只允许使用该世界已声明的类型和关系；至少一个对象类型必须显式标记 `actor=true`，否则不能启用角色级 OAG 写入。`actor` 只标记“可作为发言者/知识主体”，不规定其他语义。
- 无本体文件：仅为现有角色和地图节点生成明确标记的 `character` / `location` 兼容类型，其中 `character` 标记 `actor=true`；不自动补物品、组织、概念或任何关系。新制作并启用 OAG 的世界应提供本体文件。
- 运行期遇到未声明类型：M1 不创建新类型；能按唯一 ID 复用既有对象时继续写入，否则拒绝该条结构化写入并记 trace。语义提取和新类型建议留到作者审核。
- 运行期遇到未声明关系：保留 `original_predicate` 与来源，只有世界显式声明 `related_to` 时才可降级写入；否则不写关系、不猜测。

平台不维护一份跨世界强制的“修仙关系表”。下表仅是可选声明示例：

| 世界包可声明的关系 | 世界自行规定的适用范围 | 可选约束 |
|---|---|---|
| `located_in` | character/item → location | functional，时间关系 |
| `part_of` | location/item → location/item | 时间关系；传递规则由世界声明 |
| `owns` | character/organization → item | 时间关系 |
| `holds` | character → item | functional，时间关系 |
| `member_of` | character → organization | 时间关系 |
| `leader_of` | character → organization | functional，时间关系 |
| `mentor_of` | character → character | 时间关系 |
| `ally_of` / `enemy_of` | character/organization → 同类 | symmetric；查询时在可见子图内补反向边 |
| `related_to` | * → * | 可选兜底，必须保存 `original_predicate` |

角色之间的社会认识可以由世界包声明为普通关系；平台的 `oag_knowledge` 只表达“谁在何时通过什么来源获知哪条信息”，不替世界定义社会关系。事件参与由 `oag_event_objects` 表达；`caused_by` 留到后续版本。

#### 稳定 ID 与实体解析

- Canon 角色：使用世界包中 `actor=true` 的角色类型；`canonical_key='<角色类型>:<source_character_id>'`，对象 ID 为 `obj:<world_id>:canon:<角色类型>:<canonical_key>`。其他世界使用自己声明的类型键，不再假定存在 `character`。
- Canon 地点：使用世界包声明的地点类型；`canonical_key='<地点类型>:<map_node_id>'`，对象 ID 同样采用 `obj:<world_id>:canon:<地点类型>:<canonical_key>`。地图节点没有 ID 时，使用递归导入时生成的世界内稳定节点编号，不能用显示名直接当 key。
- Session 对象：`obj:<world_id>:session:<session_id>:<object_type>:<uuid>`；`canonical_key='<object_type>:<uuid>'`，显示名可以重复。
- `canonical_key` 是导入/upsert 的稳定键；唯一约束为 `(world_id, session_id, canonical_key)`，**不是 name**。
- 解析顺序：调用方显式 ID → 同 world/session 内规范名精确匹配 → 别名精确匹配 → 唯一的归一化名匹配。同名或同别名命中多个对象时返回 `ambiguous`，禁止取第一个。
- 消息 metadata 新增 `speaker_character_id`、`witness_ids`、`recipient_ids`。旧 `witnesses` 名称只有在能唯一解析时用于迁移；歧义或缺失时失败关闭。

#### 事实与世界观

- Canon 对象/属性/关系：`source='author'`、`visibility='public'`、`truth_status='confirmed'`。
- 运行时关系/属性是 **claim**：保存 `asserted_by_id`、`source_event_id`、`confidence`、`truth_status='asserted'`。
- 同一说话人对同一 functional 属性/关系提出新值时，只 supersede 该说话人的旧 claim；不能因为一个角色说了谎，就全局作废另一个角色仍然相信的事实。
- 不同说话人的矛盾 claim 都保留，标记 `disputed` 并进入后续 review queue；Canon 不因 runtime claim 被静默覆盖。
- 时间窗口统一为半开区间：`valid_from_turn <= T < valid_to_turn`，`valid_to_turn IS NULL` 表示无穷。知识还要求 `learned_turn <= T`，避免把后来才知道的信息带到过去。

#### M1 查询边界

只实现 `resolve`、`entity`（对象卡 + 1 跳）、`find`（类型/属性/当前关系/as-of）和 `timeline`（可见事件）。多跳、path、aggregate、absence、`knows` 的完整语义留到 M2；在它们实现前，不向模型开放半成品工具。

`VisibleGraph` 必须先过滤对象存在性、属性/关系知识、事件知识和时间，再执行上述查询。导演/作者可以看全图，但全图结果不能自动进入公开旁白或角色 prompt。

## 4. ABox：实例图

### 4.1 对象

```sql
oag_objects(
  id, world_id, session_id,        -- session_id='' 表示世界 Canon
  object_type, canonical_key, name, name_normalized, aliases_json,
  description, source, visibility, asserted_by_id, source_event_id, truth_status,
  created_turn, updated_turn, retired_at, created_at, updated_at,
  UNIQUE(world_id, session_id, canonical_key)
)
```

- **Canon 对象**：世界包导入的角色、地图节点及 `objects.json` 显式对象，使用各世界声明的类型；`session_id=''`，所有存档共享。
- **Session 对象**：本局新生成的角色、临时地点、临时物品；`session_id=<id>`，只在当前存档可见。
- **ID 稳定**：角色优先用世界包标记为 actor 的类型与角色 ID；地点用世界包声明的地点类型与地图节点 ID；两者皆无声明时，兼容模式才使用 `character` / `location`。临时对象使用 `obj:<world_id>:session:<session_id>:<type>:<uuid>`。
- **稳定键不是显示名**：`canonical_key` 用于幂等 upsert；`name_normalized` 只用于解析。世界 ID 必须进入唯一约束，显示名不唯一。
- **对象也有知识边界**：Canon 默认 public；runtime 新对象默认 scene，只有见过/听过该对象的 actor 能在查询里看到它，防止通过实体解析泄露存在性。
- **退役不删除**：Canon 从世界包消失时写 `retired_at`；历史事件和旧关系仍可引用，查询默认过滤 retired 对象，`include_retired=true` 仅给导演/作者。

### 4.2 属性

```sql
oag_attributes(
  id, world_id, session_id, object_id,
  key, value_json, value_type,
  valid_from_turn, valid_to_turn,
  source, confidence, asserted_by_id, source_event_id,
  visibility, truth_status, supersedes_id, created_at
)
```

- `valid_to_turn IS NULL` = 当前有效；
- 属性变化：旧行写 `valid_to_turn=当前回合`，新行插入；runtime claim 只能 supersede 同一 `asserted_by_id` 的旧 claim；
- `asserted_by_id` 记录是谁说的/谁观测的，显示名仅用于渲染；
- 查询 `as_of_turn=30` 时取 `valid_from <= 30 AND (valid_to IS NULL OR valid_to > 30)`，并且 actor 在当时已经知道该属性。

### 4.3 关系

```sql
oag_links(
  id, world_id, session_id,
  from_object_id, link_type, to_object_id, to_value,
  attributes_json,
  valid_from_turn, valid_to_turn,
  original_predicate, attributes_json,
  source, confidence, asserted_by_id, source_event_id,
  visibility, truth_status, inferred, supersedes_id, created_at
)
```

- `to_object_id` 为空时用 `to_value` 表示字面量；普通对象关系必须解析出 object ID，不能靠显示名查询。
- 关系变化：旧边 `valid_to_turn=当前回合`，新边插入；不同 actor 的 claim 不互相静默作废。
- `visibility='public'`：所有 actor 可见；`scene`：需要知识边；`private`：仅 `asserted_by_id` 可见。要“只告诉某些人”应使用 scene + 仅给 recipients 的知识边，而不是 private。
- 默认值用 `scene`（最小可见），只有作者/Canon 写入才显式 `public`；不要把 `public` 当默认值；
- 未知谓词只有在世界包声明 `related_to` 时才写入该关系，并保存 `original_predicate`；否则不写关系并保留审核信息。`asserted_by_id` 与 `truth_status` 区分“谁说的”和“是否属实”。

### 4.4 场景

```sql
oag_scenes(
  id, world_id, session_id, runtime_scene_id,
  name, location_object_id, background_hint,
  valid_from_turn, valid_to_turn,
  UNIQUE(session_id, runtime_scene_id)
)
```

- 每次 `change_scene` 创建一个场景实例；`id` 是 OAG 稳定 ID，`runtime_scene_id` 对应当前 `SessionSnapshot.scene.scene_id`，两者不要混用。
- 同一地点多次进入必须是不同的场景实例，不能只用地点 slug，否则“上周在码头”和“这周在码头”的参与者会混在一起；
- 场景参与者用 `oag_scene_participants(id, scene_id, object_id, presence_mode, from_turn, to_turn)` 记录；同一角色离开再加入会产生新时间片，不允许一行覆盖历史；
- 当前场景 = `valid_to_turn IS NULL` 的那条；远程参与者记录 `presence_mode='remote'`，不冒充物理 present；
- 历史场景让“第 30 回合谁在场”可查，而不是只存当前名册。

### 4.5 事件

```sql
oag_events(
  id, world_id, session_id, turn_index, ordinal,
  scene_id, event_type, title, summary, details_json,
  source_message_id, dedupe_key, visibility, importance, truth_status, created_at,
  UNIQUE(session_id, dedupe_key)
)
oag_event_objects(
  event_id, object_id, role             -- actor/target/location/witness/mentioned
)
```

- 每回合至少一条确定性 `turn` 事件；每条 player/agent 消息再产生一个 `dialogue` 事件；属性/关系变化产生 `state_change` 事件。全部不依赖 LLM。
- `dialogue` 事件只保存 `source_message_id`、说话人和脱敏标题，**不在 `details_json` 复制正文**。正文仍以 `sessions.messages_json` 为唯一事实源，读取时按 viewer 的 recipients/witnesses 渲染。
- 耳语另建 `whisper_envelope` 事件给非 recipients：只含“X 对 Y 耳语”，不含正文；内容事件及 `told` 知识只给 speaker + recipients。
- `witness` 角色 = 事件观众，自动生成知识边；`dedupe_key` 由 session/turn/message/state-change 稳定推导，保证重试幂等。
- 事件记录“发生了什么/谁说了什么”，不把对话中的陈述自动升级为世界真理；摘要和记忆都是它的投影。

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

**角色类别必须由世界包显式声明**：世界通过类型上的 `actor` 标记指定哪些对象类型可以承担角色视角。`actor` 只表示“这种对象能作为发言者/知识主体”，不规定它必须叫人物，也不规定任何关系。现有角色资料只能导入到该类型；没有 `actor` 类型的世界不能启用角色级 OAG 写入。`character:<id>` / `player:<id>` 只是查询视角标识，不要求存在名为 `character` 的对象类型；`director` / `author` 仍是全图视角。玩家没有正式角色时只在该类型下创建合成对象。
- `director`：世界主控 / GM，可见全图；
- `author`：编辑器/调试，可见全图。

### 5.2 知识边

```sql
oag_knowledge(
  id, world_id, session_id,
  actor_id,
  target_kind,                  -- object/attribute/link/event/scene
  target_id,
  knowledge_type,               -- witnessed/told/inferred/private
  access_kind,                   -- content/envelope
  source_event_id, source_actor_id,
  learned_turn, confidence, created_at,
  UNIQUE(session_id, actor_id, target_kind, target_id, source_event_id, access_kind)
)
```

同一 actor 可以从不同事件知道同一目标，因此保留多条来源；查询“谁知道”按 actor 去重。`public` 目标不写知识边，`learned_turn` 是知识获得时间，不是目标生效时间。

语义：

| 类型 | 含义 | 例子 |
|---|---|---|
| `public` | 世界常识，由目标 `visibility='public'` 表达，不写知识边 | “临安是水运枢纽” |
| `witnessed` | 亲眼所见/亲耳所听 | Alice 在场看到老王逃走 |
| `told` | 被明确告知 | 玩家告诉 Alice 钥匙位置 |
| `inferred` | 从已知事实推导 | Alice 知道“老王在码头”+“码头失火”→ 推断老王有危险 |
| `private` | 只有自己知道 | 角色的内心判断、秘密 |

`public` 也可以直接由 `visibility='public'` 表达，二者等价；查询时统一处理：

```text
可见 = director/author 全图
     OR (
       target 在 as_of_turn 已生效
       AND (
         visibility='public'
         OR (visibility='private' AND actor_id=asserted_by_id)
         OR (visibility='scene' AND EXISTS(
           knowledge WHERE actor_id=? AND target_kind=? AND target_id=?
             AND learned_turn <= as_of_turn
         ))
       )
     )
```

“当前场景的公共感知对象”不能绕过知识边：新发现的 runtime 对象也必须先写知识。属性/关系可见还要求其端点对象可见；事件正文还要求 `access_kind='content'`，`envelope` 只能渲染脱敏占位。

### 5.3 知情条件（M2 目标，M1 不实现）

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

- 事件：`witness` 角色获得 `witnessed`；公开事件可以 `public`。每条事件的知识边都指向具体 `source_event_id`。
- 对话：默认所有在场者获得 `witnessed`；如果消息有 `recipients`（耳语/密谈），只有 speaker + recipients 获得内容级 `told`，其他在场者只获得 `whisper_envelope` 事件级 `witnessed`。
- 事实/关系：`asserted_by_id` = 真实说话人；witness 获得 `witnessed`；如果说话人明确“只告诉 X”，只有 speaker + X 获得 `told`，其他人没有。模型自报的 speaker/visibility 不能覆盖运行时 provenance。
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

- 所有运行时关系/属性都是 **claim**，不是绝对真理；
- `asserted_by_id`：谁说的；`supersedes_id`：被同一说话人的哪条旧 claim 替代；
- `truth_status`：`asserted`（仅声称）/`confirmed`（作者 Canon 或系统接受的状态）/`disputed`（冲突）；
- 查询结果要能返回“据 X 所述”，而不是把 NPC 的谎话当事实；
- Canon 关系 `source='author'`、`truth_status='confirmed'`，运行时抽取默认 `asserted`；
- functional 冲突只在同一 claim 主体内自动 supersede；跨 actor 矛盾保留两边并标记 disputed，不能全局 invalidate。

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
    actor: OagActor,          // object/player/director/author
    as_of_turn: i32,          // 默认当前回合
    max_nodes: usize,         // 默认 200；多跳每一步的扩展上限
}
```

### 6.2 可见子图

`VisibleGraph` 包含：

- objects：Canon + session，过滤 `retired_at<>''`；`visibility='private'` 仅 `asserted_by_id`，runtime `scene` 对象必须有 actor 的 object 知识边；有 `access_policy` 的对象再按条件过滤；
- attributes：按 `oag_knowledge` + `access_policy` + 时间窗过滤；Canon 与 session claim 可同时存在，查询结果带 truth_status，不静默覆盖；
- links：按 `visibility` + `oag_knowledge` + `access_policy` + 时间窗过滤；只有同来源的系统状态变化才能 supersede 旧系统状态；
- events：按 witness/knowledge 过滤；耳语非接收者只拿到脱敏 envelope；
- knowledge：角色只加载自己的知识边，导演/作者可加载全图；目标与来源仍须属于当前可见子图；
- memories：只加载 actor 自己的；
- lore/docs：按 audience 过滤。

`access_policy` 是后续 M2 扩展；当前未实现条件策略，未知 visibility 失败关闭。

当前查询会加载全部可见对象、属性、关系和事件，`max_nodes` 只限制多跳每步扩展，不是物理加载硬上限；后续需实现加载预算、索引与缓存。

### 6.3 操作

| 操作 | 语义 | 例子 |
|---|---|---|
| `resolve` | 名称/别名 → 对象 | “老王” → character:lao-wang |
| `entity` | 对象卡片：属性 + 1 跳关系 + 相关事件 | “给我老王的所有信息” |
| `find` | 按类型/属性/关系过滤对象 | “当前可见记录中，谁住在码头” |
| `traverse` | 沿关系类型多跳，返回路径与置信度 | “韩立的师父的敌人” |
| `aggregate` | count/sum/avg/min/max/group by | “青云宗有多少弟子” |
| `timeline` | 按时间/参与者/地点/类型/正文提及查事件 | “第 30~40 回合发生了什么” |
| `knows` | 谁知道某条结构化信息 | “谁知道钥匙在 12 号柜” |
| `absence` | 当前可见子图中没有匹配属性/关系 | “当前可见记录中，谁没有去过码头” |

以上操作已在 Rust 查询内核与 Debug 面板实现。`path` 最短路径、独立 `get/events/knowledge` 别名尚未实现；统一模型工具 `oag_query` 已通过 `op` 按现有工具机制接入导演与角色链路，调用预算仍属于后续可选项。`find` 与聚合/否定共享“对象类型 + 至多一个属性条件 + 至多一个关系条件 + as_of_turn”的过滤契约。

### 6.4 语义细节

**时间**

- `as_of_turn` 默认当前回合；
- 关系/属性：`valid_from <= T AND (valid_to IS NULL OR valid_to > T)`；
- 事件：`turn_index <= T`；
- `include_history=true` 时返回已失效边，并标注 `invalid_at_turn`。

**方向与深度**

- `direction=out|in|both`；
- `steps` 固定为 1~3，超过或留空会拒绝；
- 每跳都应用类型过滤和可见性过滤。

**逻辑谓词**

- `eq/ne/gt/gte/lt/lte/in/contains`；`exists/not_exists` 尚未实现；
- 属性比较按 `value_type` 解析（数字/字符串/枚举/列表）；
- 否定查询（absence）必须给出对象类型和至少一个属性或关系条件，只返回“当前视角可见子图中没有匹配”的对象；结果附带限制说明，不宣称“从未发生”或“不存在隐藏信息”。

**聚合**

- `count`、`count_distinct`、`sum`、`avg`、`min`、`max`；
- `group_by`：对象类型、属性、关系目标；
- 聚合只统计可见子图。

**推理**

- 多跳只沿已存在的可见边前进；对称和 inverse 仅在对应世界包作出声明时执行，传递闭包与自动推断写入尚未实现；
- 推理不能跨越不可见边。

### 6.5 查询示例

以下类型和关系仅为说明查询语义，全部以某个示例世界已声明的本体为准；其他世界必须替换成自己的类型/关系，未声明的类型或关系会被拒绝。

#### 例 1：谁住在码头？

```json
{
  "op": "find",
  "object_type": "character",
  "link": { "link_type": "located_in", "to": { "name": "码头", "object_type": "location" } },
  "as_of_turn": 42
}
```

SQL/图遍历：`character -[located_in]-> location(码头)`，过滤当前有效边。

#### 例 2：韩立的师父的敌人是谁？

```json
{
  "op": "traverse",
  "entity": { "name": "韩立", "object_type": "character" },
  "steps": [
    { "link_type": "mentor_of", "direction": "out" },
    { "link_type": "enemy_of", "direction": "out" }
  ],
  "as_of_turn": 42
}
```

两跳；如果 `enemy_of` 是 symmetric，反向边只在可见子图内补齐。

#### 例 3：第 30 回合时银钥匙在谁手里？

```json
{
  "op": "find",
  "object_type": "character",
  "link": { "link_type": "holds", "to": { "name": "银钥匙", "object_type": "item" } },
  "as_of_turn": 30,
  "include_history": false
}
```

关键在于 `as_of_turn` 用关系的时间窗，而不是“当前状态”。

#### 例 4：谁知道钥匙在 12 号柜？

```json
{
  "op": "knows",
  "target": { "kind": "link", "id": "link-key-locker" },
  "knowledge_actor": { "id": "character:alice" }
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
  "link": { "link_type": "member_of", "to": { "name": "青云宗", "object_type": "organization" } },
  "as_of_turn": 42
}
```

#### 例 6：当前可见记录中，谁没有去过码头？

```json
{
  "op": "absence",
  "object_type": "character",
  "link": { "link_type": "located_in", "to": { "name": "码头" } },
  "as_of_turn": 42
}
```

只基于可见子图；如果某人的行踪未知，结果只能表述为“当前可见记录中没有找到匹配”，不能断言“从没去过”。

#### 例 7：阿禾最近一次提到父亲是什么时候？

```json
{
  "op": "timeline",
  "participant": { "name": "阿禾" },
  "mentioned": "父亲",
  "from_turn": 1,
  "to_turn": 42,
  "limit": 5
}
```

事件的 `title` / `summary` / `details` 做文本匹配，且只搜索正文可见事件；结构化关系优先使用该世界已声明的关系，不假定存在 `related_to` / `family_of`。

### 6.6 实现策略

- SQLite 负责存储和直接索引查询；
- 多跳/否定/聚合在 Rust 内存图里做；图规模当前可控，后续需继续做物理加载上限与缓存；
- 每次查询先加载 `VisibleGraph`，避免在 SQL 里写复杂的可见性 JOIN；
- 每次查询写入 `oag_query_traces`，失败也记录错误；当前没有实现同回合结果缓存复用；
- 限制：`steps` 长度 1~3、`max_nodes<=200`、`limit<=20`，防止图爆炸；
- 所有排序确定性（分数 + id），避免同一输入不同结果。

## 7. 给智能体的查询工具

工具是 OAG 的“结构化查询接口”，不是关键词搜索。当前只交付一个统一只读工具 `oag_query`，内部复用 §6 的查询内核，不再拆成实体、多跳、时间线等多个模型工具。

### 7.1 接入范围与固定上下文

- 世界启用 OAG 模式时，导演与普通角色都通过现有工具能力机制获得此工具，不区分单智能体和多角色链路。
- 角色开场提示词预览与实际请求共用同一套工具能力生成，不额外隐藏或放宽工具。
- 自动注入的 `oag_context` 仍是主要上下文；工具只补充投影没有展开的关系、体系与全局格局，不要求模型重复查询已有信息。
- 世界、存档、查询视角和当前回合由后端固定。模型不能指定 actor、世界、会话或查询回合。
- 模型即使传入 `as_of_turn` 也会被清除；`include_retired` 强制为 false。模型不能借工具读取退休记录或绕过知识、时间、场景和耳语隔离。
- 工具只读，不修改 Canon、运行时事实、关系或知识；失败以工具结果回传，不让整个回合直接中断。

### 7.2 操作与参数

必填参数只有 `op`，可选值如下。

| `op` | 用途 | 主要参数 |
|---|---|---|
| `resolve` | 按名称、别名或 ID 解析实体 | `entity` |
| `entity` | 查询实体详情、一跳关系和事件 | `entity`、`include_history` |
| `find` | 按类型、属性和关系查找实体 | `object_type`、`attribute`、`link` |
| `traverse` | 沿已声明关系做 1~3 跳追踪 | `entity`、`steps`、`object_type` |
| `aggregate` | 统计或分组统计 | `metric`、`metric_attribute`、`group_by`、`group_attribute`，以及 `find` 的过滤参数 |
| `absence` | 查询当前可见记录中不存在匹配属性或关系的实体 | 与 `find` 相同 |
| `knows` | 查询某条对象、属性、关系、事件或记忆的知情记录 | `target` |
| `timeline` | 按回合、事件类型、参与者、地点或正文提及查事件 | `from_turn`、`to_turn`、`event_type`、`participant`、`location`、`mentioned` |

实体引用可使用 `id`、`name` 或 `object_type`。属性条件支持 `eq/ne/gt/gte/lt/lte/in/contains`；关系条件与多跳步骤必须使用当前世界本体声明的关系类型，方向支持 `out/in/both`。公共参数 `limit` 限制为 1~20，默认 20。`include_history` 只影响已失效事实或关系是否出现在可见结果中，不会扩大知识边界。

### 7.3 调用示例

解析实体：

```json
{ "op": "resolve", "entity": { "name": "韩立" } }
```

条件查找：

```json
{
  "op": "find",
  "object_type": "character",
  "attribute": { "key": "status", "op": "eq", "value": "alive" },
  "link": { "link_type": "member_of", "to": { "name": "青云宗" }, "direction": "out" },
  "limit": 10
}
```

多跳追踪：

```json
{
  "op": "traverse",
  "entity": { "name": "韩立" },
  "steps": [
    { "link_type": "member_of", "direction": "out" },
    { "link_type": "enemy_of", "direction": "both" }
  ],
  "object_type": "character"
}
```

聚合、否定、知情和时间线：

```json
{ "op": "aggregate", "object_type": "character", "metric": "count", "group_by": "link_target" }
{ "op": "absence", "object_type": "character", "attribute": { "key": "visited_dock", "op": "eq", "value": true } }
{ "op": "knows", "target": { "kind": "link", "id": "link-key-locker" } }
{
  "op": "timeline",
  "participant": { "name": "韩立" },
  "location": { "name": "码头" },
  "event_type": "combat",
  "from_turn": 1,
  "limit": 10
}
```

### 7.4 结果与调用策略

成功结果沿用查询内核格式，包含 `ok`、`op`、`count`、`truncated`、`results`、`trace_id` 和 `hint`。实体、事实、关系、事件与知识结果保留来源、回合和置信度等已有字段；私聊来源继续按视角脱敏。参数错误或数据库暂不可用时，返回 `ok:false` 和明确错误文本，由模型决定是否改用已有上下文回答。

当前沿用导演与角色现有的工具循环，不新增调度器、递归调用、同回合缓存或调用预算。`VisibleGraph` 固定最多加载 200 个节点，单次结果最多 20 条，多跳最多 3 步。空结果只表示当前可见子图没有匹配，不能据此推断隐藏信息或从未发生。

文本兜底仍由现有历史对话、摘要、Embedding 与关键词召回承担；`oag_query` 不替代这些通道，也不提供写操作。

## 8. 上下文投影：从本体生成 `oag_context`（M2）

> **2026-11-27 当前实现**：角色与导演都从 `VisibleGraph` 已裁剪的视角生成投影。完整投影保留在 Debug 与请求追踪中，模型只读取其中的 `model_context` 紧凑文本；预算、选择与省略 trace 不进入模型。OAG 模式同时保留历史对话、摘要和 Embedding/关键词召回，两类记忆合并到一个 section 并跨层去重。多跳/聚合/否定/知识查询内核与 Debug 面板已交付；统一只读工具 `oag_query` 已按现有工具机制接入导演与角色链路，条件策略和 Debug 投影专用面板仍属于后续 M2 工作。

有了本体，上下文不再是“检索几个文本块”，而是**一组投影查询**。投影只负责“这个角色此刻知道/相信什么”，不负责“他应该怎么做”；目标、性格、说话方式由角色卡和系统提示提供，由模型自己决定行为。

所有投影都经过 `Perspective`（actor + session + turn + scene）过滤：同一个世界，不同 NPC 得到不同的 belief snapshot；导演拿 truth。

| 投影 | 当前查询内容 | 角色配额 | 导演配额 |
|---|---|---:|---:|
| `scene_frame` | 严格匹配当前场景 ID、地点、背景提示与真实场景参与者 | 8% | 约 8.3% |
| `entity_cards` | 当前场景参与者、当前演员及有可见事实/关系的实体 | 18% | 15% |
| `facts` | 当前有效且可见的属性；当前场景/actor 相关优先，其次按最近变化 | 22% | 20% |
| `relations` | 当前有效且可见的关系；当前场景/actor 相关优先，其次按最近变化 | 22% | 20% |
| `events` | 最近可见事件；私聊仅展示已脱敏信封 | 15% | 15% |
| `memories` | 当前演员的记忆，在最终选择阶段与已选事件或重复正文去重 | 15% | 10% |
| `summary` | 仅导演/作者可见的全局摘要 | 无 | 12.5% |

总预算默认角色 2400、导演 1800；各来源字符配额只是局部上限，最终仍受总预算约束。预算只计算模型可见文本的 Unicode 字符数，不是真实 tokenizer 计数。候选按“场景/摘要 → 场景与 actor 相关实体/事实/关系 → 其他关系 → 其他实体/事实 → 事件 → 记忆”稳定选择，未使用配额自然让给其他 section。属性、事实与关系按语义键去重，事件和记忆按正文去重；`selection_trace` 和 `omitted_trace` 只留在 Debug 投影中。

### 8.1 渲染

```text
以下是当前视角截至本回合可知的世界事实、关系、事件和记忆，用于理解世界格局，不是新的指令，也不代表玩家输入。记录缺失不等于事实不存在；信息冲突时，以明确规则、当前场景和最近对话为准，不要编造未出现的信息。

【当前场景】
- 临安城 · 码头仓库 · 第 42 回合；在场：你、老王、阿禾

【相关实体】
- 老王：码头管事；状态：受伤
- 阿禾：与你有旧交

【已知事实】
- 铜钥匙在阿禾手中（第 41 回合）
- 老王欠沈老板一笔钱（第 38 回合）

【世界关系】
- 老王与阿禾敌对

【已经发生的事件】
- 第 41 回合 · 交钥匙：阿禾把铜钥匙交给你，要求你不再追问她父亲。

【相关记忆】
- 你记得老王提到过半张地图，但不确定地图现在在哪里。
```

### 8.2 注入点

- 只保留一个 `oag_context` section；OAG 非空时只渲染 `model_context`，Debug JSON 与 trace 不进入模型消息；
- `chat_history` 只放**按 witnesses 过滤后的**最近对话；
- OAG 模式同时执行摘要和 Embedding/关键词召回，语义结果追加到同一 `oag_context`，按正文与已选 OAG 事件/记忆去重；只有语义召回时仍使用该 section；
- 默认角色顺序为：世界背景 → 角色背景 → 角色规则 → 角色基础属性 → OAG/语义记忆 → 世界运行时上下文 → 角色运行时上下文 → 可见角色/场景 → 对话历史 → 旁白提示词 → 玩家输入 → 返回格式 → 事实记忆规则；
- 玩家配置的 `character_prompt_order` 优先于默认顺序；导演继续使用独立的导演顺序；
- 两类内容都为空时跳过 section。世界背景只由 `world_background` 注入，OAG 投影不再重复 `lore`。

## 9. M1 存储设计（冻结 Schema）

以下 DDL 是 M1 的实施依据。所有 ID 全局唯一；`session_id=''` 只用于 Canon。角色 actor 使用世界包标记为 `actor=true` 的对象类型及其实例 ID；玩家没有正式角色时只在该类型下创建合成对象，没有合格类型时 OAG 写入失败关闭并记 trace，不偷偷新增类型。`director` / `author` 是查询上下文，不写知识边。

```sql
CREATE TABLE oag_schema_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  schema_version INTEGER NOT NULL,
  applied_at TEXT NOT NULL DEFAULT ''
);

-- TBox：每个世界一份，只 seed 该世界包声明的类型；无本体文件时兼容类型单独标记。
CREATE TABLE oag_object_types (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  key TEXT NOT NULL,
  label TEXT NOT NULL,
  parent_key TEXT,
  actor INTEGER NOT NULL DEFAULT 0,
  platform_role TEXT NOT NULL DEFAULT '',
  attributes_json TEXT NOT NULL DEFAULT '[]',
  builtin INTEGER NOT NULL DEFAULT 0,
  UNIQUE(world_id, key)
);

CREATE TABLE oag_link_types (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  key TEXT NOT NULL,
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

-- ABox：Canon + Session 共用对象表，用 session_id 分层。
CREATE TABLE oag_objects (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  object_type TEXT NOT NULL,
  canonical_key TEXT NOT NULL,
  name TEXT NOT NULL,
  name_normalized TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  description TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'system',
  visibility TEXT NOT NULL DEFAULT 'scene',
  asserted_by_id TEXT,
  source_event_id TEXT,
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  created_turn INTEGER NOT NULL DEFAULT 0,
  updated_turn INTEGER NOT NULL DEFAULT 0,
  retired_at TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT '',
  UNIQUE(world_id, session_id, canonical_key),
  FOREIGN KEY(asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY(source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL
);

CREATE TABLE oag_attributes (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  object_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL DEFAULT 'null',
  value_type TEXT NOT NULL DEFAULT 'string',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'system',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by_id TEXT,
  source_event_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  supersedes_id TEXT,
  created_at TEXT NOT NULL DEFAULT '',
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY (source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL,
  FOREIGN KEY (supersedes_id) REFERENCES oag_attributes(id) ON DELETE SET NULL
);

CREATE TABLE oag_links (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  from_object_id TEXT NOT NULL,
  link_type TEXT NOT NULL,
  to_object_id TEXT,
  to_value TEXT NOT NULL DEFAULT '',
  original_predicate TEXT NOT NULL DEFAULT '',
  attributes_json TEXT NOT NULL DEFAULT '{}',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  source TEXT NOT NULL DEFAULT 'system',
  confidence REAL NOT NULL DEFAULT 1.0,
  asserted_by_id TEXT,
  source_event_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'scene',
  truth_status TEXT NOT NULL DEFAULT 'asserted',
  inferred INTEGER NOT NULL DEFAULT 0,
  supersedes_id TEXT,
  created_at TEXT NOT NULL DEFAULT '',
  CHECK (
    (to_object_id IS NOT NULL AND to_value = '')
    OR (to_object_id IS NULL AND to_value <> '')
  ),
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (from_object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (to_object_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY (asserted_by_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY (source_event_id) REFERENCES oag_events(id) ON DELETE SET NULL,
  FOREIGN KEY (supersedes_id) REFERENCES oag_links(id) ON DELETE SET NULL
);

CREATE TABLE oag_scenes (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  runtime_scene_id TEXT NOT NULL,
  name TEXT NOT NULL,
  location_object_id TEXT,
  background_hint TEXT NOT NULL DEFAULT '',
  valid_from_turn INTEGER NOT NULL DEFAULT 0,
  valid_to_turn INTEGER,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, runtime_scene_id),
  CHECK (valid_to_turn IS NULL OR valid_to_turn > valid_from_turn),
  FOREIGN KEY (location_object_id) REFERENCES oag_objects(id) ON DELETE SET NULL
);

CREATE TABLE oag_scene_participants (
  id TEXT PRIMARY KEY,
  scene_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'participant',
  presence_mode TEXT NOT NULL DEFAULT 'present',
  from_turn INTEGER NOT NULL DEFAULT 0,
  to_turn INTEGER,
  UNIQUE(scene_id, object_id, from_turn, presence_mode),
  CHECK (to_turn IS NULL OR to_turn > from_turn),
  FOREIGN KEY (scene_id) REFERENCES oag_scenes(id) ON DELETE CASCADE,
  FOREIGN KEY (object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE oag_events (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  ordinal INTEGER NOT NULL DEFAULT 0,
  scene_id TEXT,
  event_type TEXT NOT NULL DEFAULT 'turn',
  title TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  details_json TEXT NOT NULL DEFAULT '{}',
  source_message_id TEXT,
  dedupe_key TEXT NOT NULL,
  visibility TEXT NOT NULL DEFAULT 'scene',
  importance REAL NOT NULL DEFAULT 0.5,
  truth_status TEXT NOT NULL DEFAULT 'confirmed',
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, dedupe_key),
  FOREIGN KEY (scene_id) REFERENCES oag_scenes(id) ON DELETE SET NULL
);

CREATE TABLE oag_event_objects (
  event_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'participant',
  PRIMARY KEY(event_id, object_id, role),
  FOREIGN KEY(event_id) REFERENCES oag_events(id) ON DELETE CASCADE,
  FOREIGN KEY(object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

-- 一条消息在不同场景/收件人下的实际观众，粒度为 message × actor × access_kind。
CREATE TABLE oag_turn_audience (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  message_id TEXT NOT NULL,
  scene_id TEXT,
  actor_id TEXT NOT NULL,
  access_kind TEXT NOT NULL DEFAULT 'content',
  presence_mode TEXT NOT NULL DEFAULT 'present',
  learned_turn INTEGER NOT NULL,
  UNIQUE(session_id, message_id, actor_id, access_kind),
  FOREIGN KEY(scene_id) REFERENCES oag_scenes(id) ON DELETE SET NULL,
  FOREIGN KEY(actor_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

CREATE TABLE oag_knowledge (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  knowledge_type TEXT NOT NULL DEFAULT 'witnessed',
  access_kind TEXT NOT NULL DEFAULT 'content',
  source_event_id TEXT,
  source_actor_id TEXT,
  learned_turn INTEGER NOT NULL,
  confidence REAL NOT NULL DEFAULT 1.0,
  created_at TEXT NOT NULL DEFAULT '',
  UNIQUE(session_id, actor_id, target_kind, target_id, source_event_id, access_kind),
  FOREIGN KEY(actor_id) REFERENCES oag_objects(id) ON DELETE CASCADE,
  FOREIGN KEY(source_actor_id) REFERENCES oag_objects(id) ON DELETE SET NULL,
  FOREIGN KEY(source_event_id) REFERENCES oag_events(id) ON DELETE CASCADE
);

CREATE TABLE oag_memory_objects (
  memory_id TEXT NOT NULL,
  object_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'mentioned',
  PRIMARY KEY(memory_id, object_id, role),
  FOREIGN KEY(memory_id) REFERENCES memories(id) ON DELETE CASCADE,
  FOREIGN KEY(object_id) REFERENCES oag_objects(id) ON DELETE CASCADE
);

-- 一次性迁移的显式映射；后续分支复制也复用同一思路重映射 session ID。
CREATE TABLE oag_legacy_id_map (
  source_table TEXT NOT NULL,
  source_id TEXT NOT NULL,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  target_table TEXT NOT NULL,
  target_id TEXT NOT NULL,
  PRIMARY KEY(source_table, source_id)
);

CREATE TABLE oag_query_traces (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  actor_id TEXT,
  as_of_turn INTEGER NOT NULL,
  query_json TEXT NOT NULL DEFAULT '{}',
  visible_nodes INTEGER NOT NULL DEFAULT 0,
  filtered_count INTEGER NOT NULL DEFAULT 0,
  result_json TEXT NOT NULL DEFAULT '{}',
  latency_ms INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE oag_write_traces (
  id TEXT PRIMARY KEY,
  world_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_index INTEGER NOT NULL,
  step TEXT NOT NULL,
  status TEXT NOT NULL,
  detail_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT ''
);
```

索引：

```sql
CREATE INDEX idx_oag_objects_scope_name
  ON oag_objects(world_id, session_id, object_type, name_normalized);
CREATE INDEX idx_oag_objects_canonical_key
  ON oag_objects(world_id, session_id, canonical_key);
CREATE INDEX idx_oag_attributes_object
  ON oag_attributes(object_id, key, valid_from_turn, valid_to_turn);
CREATE INDEX idx_oag_links_from
  ON oag_links(from_object_id, link_type, valid_from_turn, valid_to_turn);
CREATE INDEX idx_oag_links_to
  ON oag_links(to_object_id, link_type, valid_from_turn, valid_to_turn);
CREATE INDEX idx_oag_scenes_session
  ON oag_scenes(session_id, valid_from_turn, valid_to_turn);
CREATE INDEX idx_oag_scene_participants_object
  ON oag_scene_participants(object_id, from_turn, to_turn);
CREATE INDEX idx_oag_events_turn
  ON oag_events(session_id, turn_index, ordinal);
CREATE INDEX idx_oag_event_objects_object
  ON oag_event_objects(object_id, event_id);
CREATE INDEX idx_oag_turn_audience_turn
  ON oag_turn_audience(session_id, turn_index, actor_id);
CREATE INDEX idx_oag_knowledge_actor_target
  ON oag_knowledge(session_id, actor_id, target_kind, target_id, learned_turn);
CREATE INDEX idx_oag_knowledge_target_actor
  ON oag_knowledge(session_id, target_kind, target_id, actor_id, learned_turn);
CREATE INDEX idx_oag_memory_objects_object
  ON oag_memory_objects(object_id, memory_id);
CREATE INDEX idx_oag_query_traces_turn
  ON oag_query_traces(session_id, turn_index, actor_id);
```

M1 不新建 `oag_access_policies`、`oag_lore_entries`、`oag_summaries`、`oag_documents`：lore、知识文档和固定桶总结继续使用现有表， audience 已有实现，避免第二套正文源。后续需要条件策略或独立摘要层时再迁移。

## 10. 世界包格式（M1 增量扩展）

M1 保持现有包可导入。有 `ontology.json` 时以世界包声明为准；没有时使用明确标记的 `character` / `location` 兼容类型，只迁移现有角色与地图节点，不补造其他类型或关系。新制作并启用 OAG 的世界应提供本体文件。

```text
world/world.json          # 基础信息
world/ontology.json       # 本世界权威定义：object_types / link_types / attribute specs
world/objects.json        # 可选：Canon 对象、属性、关系
world/lore.json           # 现有文本条目，M1 不迁移成第二份正文
characters/*/character.json
assets/*
knowledge_bases 仍在 world.json 或独立文件
```

`objects.json` 使用世界内稳定键，不直接写运行时 UUID。若一个世界有多个 `actor=true` 或多个地点类型，`ontology.json` 必须用 `character_object_type` / `location_object_type` 明确指定角色和地图使用哪个类型；省略时只在对应角色或地点类型唯一（或键名恰好为 `character` / `location`）时自动绑定，歧义则拒绝导入。下例只有在世界包已声明 `organization`、`item`、`character` 类型及 `member_of`、`holds` 关系时才合法；应用不会替世界补上这些声明。

```jsonc
{
  "objects": [
    {
      "object_type": "organization",
      "canonical_key": "qingyun-sect",
      "name": "青云宗",
      "aliases": ["青云"],
      "attributes": { "type": "sect" }
    },
    {
      "object_type": "item",
      "canonical_key": "bronze-key",
      "name": "铜钥匙",
      "attributes": { "condition": "old" }
    }
  ],
  "links": [
    {
      "from": "character:char-hanli",
      "link_type": "member_of",
      "to": "qingyun-sect"
    },
    {
      "from": "character:char-hanli",
      "link_type": "holds",
      "to": "bronze-key"
    }
  ]
}
```

- 导入顺序：校验世界包 TBox → upsert 角色/地图对象 → upsert 显式对象 → 解析并写入 links；任一步失败整体回滚。
- `characters` 自动生成角色对象，使用世界包声明的角色类型；没有声明时兼容模式才使用 `character`。name/role/background 的可公开部分进入属性，角色卡正文仍是角色 prompt 的来源，不复制进 OAG 描述。
- `world.map_nodes_json` 递归导入地点对象，使用世界包声明的地点类型；没有声明时兼容模式才使用 `location`。`edges` 只在同一 world 内解析，未知端点拒绝写入。
- `ontology.json` 没写才启用上述兼容类型；自定义类型必须通过 key、parent、domain/range、value_type 校验。应用不得把示例类型或关系隐式加入其他世界。
- 导出只从 `oag_objects` / `oag_attributes` / `oag_links` 的 `session_id=''` 行生成 `ontology.json` / `objects.json`；不导出 session claim、知识边、事件或记忆。
- 导入预览必须列出新增/修改/退役 diff；消失的 Canon 对象写 `retired_at`，不物理删除。重新导入同一 `canonical_key` 时恢复该对象并清空 retired 标记。

## 11. 从现有数据迁移

开发阶段不清库。迁移在一个 SQLite 事务内完成，`oag_schema_meta.schema_version=1` 是唯一完成标记；中途失败全部回滚，下次启动重跑。建表使用 `CREATE TABLE IF NOT EXISTS`，数据迁移使用确定性 ID + upsert，保证可重入。

### 11.1 迁移顺序

1. 创建 §9 的表与索引；每个 world 只 seed 其 `ontology.json` 声明的 TBox。没有声明时，seed 明确标记且 `actor=true` 的 `character` / `location` 兼容类型，不 seed 任何默认关系。
2. 从 `characters` 生成 Canon 角色对象，从 `map_nodes_json` 递归生成 Canon 地点对象，类型取世界包声明；只有世界显式声明 `part_of` 且地图边语义符合其 domain/range 时才生成关系，否则地图边保留 legacy 只读数据。
3. 从 `memory_entities` 生成 session 对象，并写 `oag_legacy_id_map`。只迁移能归入该世界已声明类型的实体；无法归类时保留 legacy，不自动创建 `concept`。能按**唯一 ID** 对上 Canon 角色/地点时复用 Canon ID；只按同名猜中的情况不合并。
4. 从 `memory_relations` 生成 `oag_links`，保留时间窗、confidence、asserted_by_id、visibility、truth_status；只接受该世界已声明的关系。未声明谓词只有在兼容世界显式声明 `related_to` 时才写入该兜底关系并保存 `original_predicate`，否则保留 legacy 只读数据，不猜测类型。
5. 从旧事实的 speaker/witnesses 生成知识边：names 必须能唯一解析成 object ID；private 只给 speaker，scene 给 speaker + witnesses，public 不写边。缺失或歧义 audience 的事实不生成知识边，actor 查询失败关闭。
6. 从 `attribute_values` 的**当前快照**生成 `oag_attributes`，`valid_from_turn` 设为迁移时的当前回合，`source='legacy'`；不伪造历史变化。`value_type='text'` 归一为 `string`，显示策略无法映射的属性默认 scene 且不公开。
7. 保留 `memories` / `memory_summary_ranges` / `knowledge_documents` 原表；只补 `oag_memory_objects` 的可靠关联，不复制正文。
8. 不根据当前 `visible_characters` 反推历史观众。只有消息 metadata 同时具备可解析 scene、speaker ID、witness IDs 时，才为该消息补 `dialogue` 事件与知识边；其余旧消息继续走现有历史/Embedding 兜底。

### 11.2 映射规则

| 旧 | 新 | 关键约束 |
|---|---|---|
| `characters` | Canon 角色对象 | 类型取世界包声明；ID 派生稳定，显示名不唯一 |
| `map_nodes_json` | Canon 地点对象 + 可选 `part_of` | 类型取世界包声明；只有关系已声明且语义匹配才写边 |
| `memory_entities` | session `oag_objects` | 只迁移已声明类型；无法归类则保留 legacy ID 映射 |
| `memory_relations` | session `oag_links` | 只迁移世界已声明的关系；未声明谓词不猜测 |
| 旧事实 witnesses | `oag_knowledge` | 只接受稳定 ID 或唯一名称；缺失失败关闭 |
| `attribute_values` | 当前 `oag_attributes` | 只迁移当前值，不虚构历史 |
| `memories` | 原表 + `oag_memory_objects` | 不迁移正文，不跨 actor |
| `knowledge_documents` | 原表 | 保留现有 audience；M1 不新建文档副本 |
| `turn_journal` | `oag_write_traces` + `oag_committed` step | 只记录 OAG 写入结果，不复制对话正文 |

### 11.3 与分支/回滚的一致性

- Canon 对象与 TBox 不随分支复制；所有 `session_id<>''` 的 OAG 表必须纳入 branch snapshot。
- 分支时重映射 session 对象、场景、属性、关系、事件、观众、知识边和 legacy map 的 ID；Canon ID 保持不变。
- 回滚按 `turn_index` 删除快照之后产生的 session OAG 行，并恢复被 supersede 的属性/关系；不能只删 events 留下 knowledge。
- `oag_committed` 写入 turn journal，payload 记录事件数、知识边数和 object/link ID；重试时按 `dedupe_key` 幂等。
- OAG 写入使用外层回合事务内的 `SAVEPOINT oag_turn`：成功释放并记 journal；失败回滚到 savepoint、写 `oag_write_traces`，不阻断已经完成的剧情回合。

### 11.4 迁移后的读写切换

`memory_entities` / `memory_relations` 保留为 legacy 只读数据，不再作为 OAG 查询源，也不继续接收新的事实写入。M1 开启后，新的事实/事件/知识只写 OAG 表；旧调试命令在迁移前读 legacy，迁移后读 OAG，避免双写漂移。


## 12. 实现顺序

### M0 隔离基础（必须先做）

> **2026-11-26 实施状态**：下列消息级写入/读取、耳语 recipients、事实 speaker/visibility 已完成；知识库文档 audience 与分支/回滚一致性也已补齐。禁止不在场发言、远程 presence、turn audience 表、旧数据回填与独立事实抽取仍未完成。当前实现细节见 [02-isolation-audit.md §8.3](./02-isolation-audit.md#83-m0-实施状态2026-11-26)。

- **禁止不在场角色发言**：`planned_speakers` 只允许“场景名册 + 本回合新生成角色 + 远程参与者”；从 `allowed_visible` 里移除 `world_character_roster`。
- **电话 = 远程在场**：远程角色以 `presence_mode=remote` 加入本回合 `witnesses`，不进物理 `present_characters`；知识/记忆按在场处理。
- 消息 metadata 加 `scene_id/witnesses/presence_mode/recipients`；
- 写入 turn audience；
- `build_turn_entries` 按每条消息的 witnesses 写；
- `slice_character_history` / `dialogue_focus` 按 viewer 过滤；
- `FactUpdate` 带 speaker/visibility；
- 测试：私聊不出现在第三方上下文、场景切换回合不串、电话参与者拿到记忆、缺席角色发言被拒绝。

耳语（`recipients`）可以放在 M0.5：内容只给 recipients，其他在场者只看到“发生了一次耳语”的脱敏事件。若 M0 时间紧，先保证“在场=听到”，但 metadata 预留 `recipients`。

### M1 本体核心（本次设计对应范围）

交付顺序必须按“schema → Canon → resolver → turn writer → visible query → 分支回滚”推进，不能先接 prompt。

1. **Schema 与迁移**
   - 新增 §9 的表、索引、`oag_schema_meta` 与 repository。
   - 实现 §11 的一次性、可重入迁移；旧表保留只读，不删除用户存档。
   - 所有新 session 表同步加入 cleanup、branch、rollback 的清单。

2. **TBox 与 Canon ingest**
   - 读取每个世界的 `ontology.json`，按 world 隔离校验并 seed 对象类型、属性和关系；应用不注入跨世界固定清单。
   - 从 characters、map nodes、可选 `objects.json` 做稳定键 upsert；有本体文件时类型必须来自该世界声明，无本体文件时才使用明确标记的兼容类型。
   - 校验 object/link type、domain/range、属性值类型与引用；作者数据不满足约束时拒绝写入，不静默降级或补造类型。

3. **实体解析**
   - 实现显式 ID、规范名、别名、归一化名解析；歧义返回错误。
   - 为消息 metadata 增加稳定 audience ID；旧名称只在唯一命中时转换。
   - 运行期未知实体只有能归入该世界已声明类型时才创建 session 对象；无法归类则保留原文并拒绝该条结构化写入。未知谓词不能新增关系类型，只有世界声明 `related_to` 时才可写入该兜底关系并保存 `original_predicate`。

4. **确定性回合写入**
   - 新增 `OagTurnWriter`：turn/dialogue/whisper_envelope/state_change 事件、event_objects、turn_audience、knowledge。
   - 对话事件引用 `source_message_id`，不复制正文；scene 使用稳定 OAG ID 与 runtime_scene_id 映射。
   - 已接受的 runtime attribute 变化在同一 savepoint 内镜像为时间属性；`attribute_values` 仍是现有 UI 的当前值投影。属性可见性先映射现有 `display_policy`（self/other read），无法映射的策略失败关闭，再只给符合条件的 actor 写知识边。
   - M1 **不消费角色 JSON 的 `fact_extractions`**，不新增 LLM 调用；独立抽取留到 M3。

5. **可见子图与基础查询**
   - 实现 `Perspective`、`OagQueryContext`、`VisibleGraph` 和 `resolve/entity/find/timeline`。
   - actor 查询先过滤对象存在性、知识时间窗和端点可见性；导演/作者使用独立的全图上下文。
   - Debug 暴露对象、关系、事件、knowledge source 与 query trace；不向模型开放查询工具。

6. **运行开关与兼容**
   - Canon ingest 总是执行；`memory_mode='oag'` 才启动 runtime OAG writer。
   - 从 embedding 切换到 oag 时，只回填带可靠稳定 audience 的消息事件，不调 LLM；缺失 audience 的历史继续走 Embedding。
   - 角色 prompt 与 `memory_mode` 的实际召回分叉仍留到 M2；M1 完成后不改变当前 embedding 召回行为，也不以“已实现 OAG”描述运行时效果。

验收测试：

- 同名角色/地点不会串 ID；别名歧义不会自动选第一个。
- 两个 world 可以定义完全不同的类型与关系；相同名称的 Canon 对象也不会跨 world 混用。
- 世界未声明的类型/关系不会被隐式补造；无本体文件只启用最小兼容类型，新 OAG 世界必须提供本体文件。
- 重复导入不产生重复行。
- 第 30 回合查不到第 40 回合的属性/关系/知识。
- 私聊正文只给 speaker + recipients；第三方只能看到脱敏 envelope。
- 场景切换回合的事件、audience、knowledge 不串。
- 一个 actor 的 functional claim 只 supersede 自己的旧 claim；另一个 actor 的矛盾 claim 仍可见。
- 分支复制全部 session OAG 行并重映射 ID；回滚不残留未来 knowledge/event。
- 迁移重跑结果一致；缺失/歧义 audience 的旧事实对 actor 不可见。
- `resolve/entity/find/timeline` 的空结果、截断、as-of 和来源字段符合 §6/§7。
- OAG savepoint 失败不阻断回合，trace 可定位失败步骤。

> **2026-11-26 实施状态**：M1 已交付。Schema/迁移、世界包本体与 Canon、实体解析、确定性回合写入、可见子图与 Debug 基础查询、分支/回滚/清理、世界包导入差异预览均已实现并通过测试。写入器只在 `memory_mode='oag'` 时运行；失败会回滚本回合 OAG 保存点并记录 trace/journal，不阻断剧情；已完成的 `oag_turn_written` 会跳过重复写入。角色属性按角色稳定 ID 镜像；现有 session 级属性更新投影到玩家角色对象，因此也必须在本体中于该对象类型声明同名属性，不能把未声明的会话字段猜成角色事实。Debug 已提供 `resolve/entity/find/timeline`、导演/作者/角色/玩家视角、历史回合、来源与查询追踪。世界包预检按稳定身份只读比较本体模式、对象类型、关系类型、Canon 对象和 Canon 关系，作者确认前不写数据库。M2 第一阶段的结构化提示词投影现已接入；角色与导演使用同一套已裁剪视角生成 `oag_context`，OAG 模式同时保留 Embedding 与关键词召回。

### M2 逻辑查询与投影

**已交付（2026-11-26）**：

- `oag_context` 投影（scene frame/entity cards/facts/relations/events/memories/summary）、角色 2400 与导演 1800 的模型文本字符预算。模型只接收 `model_context`，完整投影与选择 trace 留在 Debug/请求追踪中。
- 投影只读取 `VisibleGraph` 已裁剪数据；当前场景严格匹配场景 ID，参与者读取场景成员表；场景/actor 相关内容与最近变化优先，事件和记忆在最终选择阶段去重。覆盖私密事实、未来事实、延迟知识、私聊事件信封、正文去重与预算截断测试。
- 角色与导演的实际请求、模块和 prompt trace 共用同一投影；OAG 模式同时保留 Embedding/关键词召回，并合并到同一记忆 section 跨层去重。默认角色顺序可被玩家的 `character_prompt_order` 覆盖。
- 逻辑查询内核支持 `traverse`（1~3 跳、声明的对称/反向关系、路径置信度、环路控制）、`aggregate`（六种指标、三种分组）、`absence`（仅基于当前可见子图）、`knows`（角色只能查自己，来源事件按正文/envelope 脱敏）；`timeline` 增加正文提及过滤。
- 知识边纳入 `VisibleGraph` 的时间、actor、目标与来源裁剪；Debug 面板可构造全部查询并展示结果与追踪。统一只读模型工具 `oag_query` 已按现有工具机制接入 OAG 模式的导演与角色链路；每回合调用预算尚未接入。

**后续待办**：

- 条件策略先实现 `actor` 白名单与 `knowledge` 条件，再逐步开放 attribute/membership/item/relationship/time；不引入任意脚本。
- Debug perspective/projection inspector、世界编辑器本体/关系最小 UI。
- 查询缓存、`VisibleGraph` 物理加载上限与调用预算验收。

### M3 独立抽取与压缩

- 独立后置任务抽取 claim，带真实 speaker、source_event、audience、confidence；不再读取角色回复里的 `fact_extractions`。
- 低置信进入 review queue，未知谓词不污染正式 link type。
-  told/inferred 知识传播、事件摘要、arc/chapter 摘要与 stale 重建。
- 知识库文档 audience + chunks 继续复用现有实现，不迁移正文。

### M4 打磨

- 跨 actor 矛盾检测、claim truth_status 审核、作者确认与回滚。
- 查询 UI（实体页/时间线/关系图）、评测集、长会话性能与缓存。

## 13. 风险与开放问题

1. **抽取质量**：LLM 把“老王说钥匙在 12 号柜”抽成事实，而不是 claim；需要 `asserted_by_id` + `source_event_id` + `truth_status`，查询时渲染“据老王所述”。
2. **谓词归一化**：自由谓词会导致关系泛滥；关系字典必须来自世界包，运行时不能私自扩表。未声明谓词保留原文并进入后续作者审核，只有世界显式提供兜底关系时才允许降级。
3. **实体解析**：同名、代称、翻译名；先做显式 ID + 规范名/别名/唯一归一化名，歧义失败关闭，不靠“取第一个”或最长匹配猜身份。
4. **知识传播**：默认“在场=听到”；耳语/密谈需要 recipients；远程通信需要显式场景。
5. **性能**：VisibleGraph 每次加载可能几百到几千节点；需要索引、缓存、深度限制。
6. **提示词复杂度**：工具 schema 不能太复杂；只保留统一 `oag_query` 入口，用 `op` 区分实体、条件查找、多跳、聚合、否定、知情和时间线，文本兜底继续复用现有召回。
7. **作者负担**：要求作者建对象/关系会劝退；默认从角色/地点自动生成，lore 仍可只写文本。
8. **是否要规则校验**：只做只读约束（functional/domain/range），不做执行；冲突时记录 warning，不阻断回合。

## 14. 和 Palantir OAG 的对应关系

| Palantir OAG | 本项目 |
|---|---|
| 对象类型 | 每个世界的 `oag_object_types`；由世界包声明，不由应用预置题材语义 |
| Objects / properties | `oag_objects` / `oag_attributes` |
| Links | `oag_links` |
| Actions | 不做 |
| Functions / logic | 查询操作 + 只读约束 |
| Ontology-backed retrieval | `VisibleGraph` + `oag_query` |
| Governance / permissions | `oag_knowledge` + actor |

差别是：我们不追求企业级本体建模和动作闭环，只追求**叙事世界里的结构化查询 + 记忆隔离**。这也是“轻量 OAG”的边界。
