# OAG 搭建与维护

> 边界：OAG 只提供**角色视角下的事实、知识、记忆、关系、感知**，以及查询这些内容的能力。
> 角色该做什么、性格、目标、说话方式，由角色卡/系统提示和模型自己决定，OAG 不参与。
> 关联：[OAG 本体设计](./03-ontology-design.md)、[记忆隔离审计](./02-isolation-audit.md)。
>
> **当前实现状态（2026-11-12）**：世界配置 `director_config.memory_mode`，默认 `embedding`（世界编辑器 → 基础权限 → 记忆模式）。两种模式都始终发送场景隔离的最近对话：`embedding` = 历史 + 现有向量/词法记忆召回；`oag` = 历史 + 结构化事实/关系层（尚未接入，暂按 Embedding 运行）。旧值 `history` 按 `embedding` 读取。角色与导演的历史轮数默认 30，不做 token 预算/截断；低上下文模型通过降低轮数解决。事实抽取另有 `director_config.fact_extraction_enabled`，默认 **false**；即使 OAG 模式也不会自动开启，且抽取必须是独立后置任务，不能并入角色扮演回复。

## 0. 先把职责钉死

OAG 的核心契约只有一句话：

```text
Perspective(actor, session, turn, scene) -> BeliefSnapshot
```

它回答：

- 这个角色此刻**感知到**什么？
- 他**知道/相信/怀疑/听说**什么？
- 这些事实/关系的**来源、时间、置信度**是什么？
- 他**不知道**什么（防止全知）？
- 他过去的**记忆**里，哪些与当前相关？
- 他能**查询**到什么（受同一个视角约束）？

它不回答：

- 他应该愤怒还是冷静；
- 他应该撒谎还是坦白；
- 他想要什么、怕什么；
- 他说话什么风格。

这些是角色卡和模型的事。OAG 把“事实”摆对，模型自己决定“怎么做”。

### 0.1 两个视角

| 视角 | 看到什么 | 用途 |
|---|---|---|
| `character:<id>` / `player:<id>` | 该角色见证/被告知/推断/记忆的事实；不含别人的私有信息 | NPC/玩家角色的 prompt、查询工具 |
| `director` / `author` | 全图：Canon + 所有 claim + 来源 + 冲突 | 导演编排、Debug、作者校对 |

同一个世界，不同 NPC 得到不同的 `BeliefSnapshot`；这是 OAG 和“全局记忆库”的根本区别。

### 0.2 记忆模式

两种模式共享同一个底座：对话历史始终发送最近 N 轮（按 `witnesses` 过滤）。切换只决定额外注入哪一层：

| 模式 | 额外注入 | 成本 | 现状 |
|---|---|---|---|
| `embedding` | 现有向量/词法记忆召回 | 中 | 已实现 |
| `oag` | 结构化事实/关系/知识边界 | 高 | 设计完成，未接入；暂按 Embedding 运行 |

- 场景隔离是共同底座：两种模式都必须按观众过滤历史，否则会串场景。
- Embedding 和 OAG 不是替代关系：Embedding 找“说过什么”，OAG 找“什么是真的/谁知道”。
- 旧配置 `memory_mode: "history"` 读取为 `embedding`，因为不再存在“只发历史、不注入”的模式。
- OAG 模式的事实抽取必须是独立后置任务，不能塞进角色回复。

### 0.3 固定桶记忆总结

- 总结按固定桶生成：`1-10`、`11-20`、`21-30`……不滚动、不累积，不会出现 `1-20` 或 `1-30`。
- 一个桶内按场景拆条：桶内换场景就拆成多条；场景跨桶时每个桶各一条，不跨桶合并。
- 每条总结只用自己的桶内、该角色 `witnesses` 可见的消息生成，不引用前一条总结。
- 总结存为 `memories(memory_type='summary')`，桶/场景/回合范围存 `memory_summary_ranges`；embedding 复用现有 `memory_embeddings`。
- 召回只取 `memory_type='summary'`，且桶结束回合必须严格小于 `current_turn - 原文轮数`。
- 原文窗口按桶对齐：如果窗口起点落在桶内，比较“补回桶首”和“整桶让给总结”，选更接近用户设定轮数的方案；完全没有总结时保持原窗口，已有部分总结但缺旧桶时补回缺失桶首（有上限，避免老存档拉回全部历史）。
- 迁移兜底：老存档还没有 summary 的桶继续用旧对话记忆，已有 summary 的桶只走总结。
- 角色总结必须有明确 `witnesses`；旧存档里没有观众名单的消息不生成角色总结（继续原文兜底），避免把所有人的经历写进每个角色记忆。主控全局总结不按角色隔离，仍可使用这些消息。
- 总结生成是独立异步任务，不并入角色扮演回复；失败不阻塞回合，下次桶边界会重试。
- 总结默认用应用默认文本模型；可在 `director_config.memory_summary_model` 指定更便宜的模型。
- 角色总结提示词：`director_config.character_memory_summary_prompt`；主控总结提示词：`director_config.director_memory_summary_prompt`。留空用内置默认，支持 `{{character_name}}`、`{{scene_name}}`、`{{bucket_start}}`、`{{bucket_end}}`、`{{turn_start}}`、`{{turn_end}}`、`{{transcript}}`；模板漏写 `{{transcript}}` 时材料会自动附在末尾。
- 主控总结是全局的：按固定桶汇总全部 player/agent 消息，不按角色隔离，存为 `character_id='director'` 的 summary；主控原文窗口按同一套桶对齐，离开原文窗口后注入主控 prompt 的 `oag_context`。
- 老存档升级时回填最近 12 个桶（默认 120 回合）；更早的桶继续走原文兜底，不会失忆。

## 1. 总体架构

```text
┌────────────────────────────────────────────────────────────────────┐
│ 作者层                                                             │
│  world package / ontology.json / objects.json / lore.json          │
│  character cards（性格、目标、声音：OAG 之外）                      │
└───────────────────────────┬────────────────────────────────────────┘
                            │ ingest / diff
┌───────────────────────────▼────────────────────────────────────────┐
│ Canon 层（世界真相）                                               │
│  object_types / link_types / objects / attributes / links           │
│  scenes / events / documents                                       │
└───────────────────────────┬────────────────────────────────────────┘
                            │ 每回合写入
┌───────────────────────────▼────────────────────────────────────────┐
│ Runtime + Epistemic 层                                             │
│  events / event_objects / claims / knowledge / memories             │
│  turn_audience / conflicts / review_queue                          │
└───────────────────────────┬────────────────────────────────────────┘
                            │ perspective 过滤
┌───────────────────────────▼────────────────────────────────────────┐
│ Query + Projection 层                                              │
│  VisibleGraph / find / traverse / as-of / aggregate / knows         │
│  BeliefSnapshot -> oag_context                                     │
└───────────────────────────┬────────────────────────────────────────┘
                            │
                  ┌─────────┴─────────┐
                  ▼                   ▼
            NPC prompt          查询工具 / Debug
```

### 1.1 组件职责

| 组件 | 职责 | 不做什么 |
|---|---|---|
| `OagIngest` | 世界包 → Canon 对象/关系；更新 diff | 不写运行时事实 |
| `OagTurnWriter` | 回合 → 事件、claim、知识边、记忆 | 不决定角色行为 |
| `OagResolver` | 名称/别名 → 对象；谓词归一化 | 不做全自动本体 |
| `OagGraph` | 按 perspective 加载可见子图 | 不加载全图再过滤 |
| `OagQuery` | find/traverse/as-of/aggregate/knows | 不生成文本答案 |
| `OagProjection` | BeliefSnapshot → `oag_context` | 不写“该怎么做” |
| `OagMaintenance` | 完整性检查、重建索引、迁移、去重 | 不自动改剧情 |
| `OagTrace` | 每次查询/投影的来源与选择 | 不记录隐私正文到外部 |

### 1.2 存储分层

| 层 | 表 | 说明 |
|---|---|---|
| TBox | `oag_object_types` / `oag_link_types` | 每个世界可扩展 |
| ABox | `oag_objects` / `oag_attributes` / `oag_links` / `oag_scenes` / `oag_events` / `oag_event_objects` | Canon + Session |
| 认知 | `oag_knowledge` / `oag_access_policies` / `oag_turn_audience` | 谁在何时知道什么 |
| 记忆 | `memories` / `oag_memory_objects` | 角色主观记忆 |
| 文本 | `oag_lore_entries` / `knowledge_documents` / `oag_summaries` | 文本投影与压缩 |
| 运维 | `oag_query_traces` / `oag_conflicts` / `oag_review_queue` | 调试、冲突、待审 |

## 2. 搭建步骤

不要一次做完。按“视角正确 → 本体够用 → 查询可用 → 自动化 → 打磨”的顺序。

### Phase 0：视角与隔离基础（必须先做）

**目标**：任何上下文/查询都只使用该角色视角内的内容。

任务：

- 消息 metadata 加 `scene_id`、`witnesses`、`presence_mode`、`recipients`、`speaker_character_id`；
- 写入 `oag_turn_audience`；
- `build_turn_entries` 按每条消息的 witnesses 写记忆；
- `slice_character_history` / `dialogue_focus` 按 viewer 过滤；
- `FactUpdate` 带 `asserted_by` / `visibility`；
- `planned_speakers ⊆ witnesses`；电话/远程先加入 witnesses；
- 测试：私聊不出现在第三方上下文、场景切换不串、耳语只给 recipients、电话参与者有记忆。

验收：

- 第三方 NPC prompt 里不出现它没见证的对话；
- 场景切换回合，玩家输入和 NPC 回复各自只进入对应观众；
- 分支/回滚不串知识。

### Phase 1：本体核心

**目标**：把世界从“文本”变成可查询的对象/关系/事件。

任务：

- TBox：内置 8~10 个 object type、10~12 个 link type；世界可扩展；
- ABox：`oag_objects` / `attributes` / `links` / `scenes` / `events`；
- Canon 导入：characters → character objects；map nodes → location objects；可选 `objects.json`；
- 事件：每回合确定性写 `oag_events` + `oag_event_objects`；
- 知识：从 witnesses 写 `oag_knowledge`；
- 实体解析：规范名 + 别名 + 最长匹配；同名按 id；
- 时间：`valid_from_turn` / `valid_to_turn` / `learned_turn`。

验收：

- 能回答“第 30 回合谁在码头”“老王现在在哪”“谁持有铜钥匙”；
- 角色查询只返回自己知道的；
- 事件按回合可分页。

### Phase 2：Perspective 投影

**目标**：NPC prompt 里的“事实/关系/记忆”来自 OAG，而不是全局 JSON。

任务：

- `Perspective { actor, session, turn, scene }`；
- `VisibleGraph`：先裁剪再遍历；
- 投影查询：scene frame、entity cards、fact digest、event digest、memory digest、lore text、summary；
- 去重：对象/关系/事件 id；记忆与事实同一件事只留一个；
- 预算：默认角色 2400 / 导演 1800，按来源配额；
- 渲染：`oag_context` 单一 section；`chat_history` 只放最近对话；
- trace：本回合投影了什么、为什么、多少 token。

验收：

- 同一场景两个 NPC 的 `oag_context` 不同（知识边界不同）；
- 投影不超预算；
- 不再有 `memory_context` 双发；
- Debug 能看到“为什么这条事实进来了”。

### Phase 3：逻辑查询工具

**目标**：模型能查结构化信息，而不是只读投影。

任务：

- `oag_entity`：实体卡；
- `oag_query`：类型/属性/关系过滤；
- `oag_traverse`：多跳；
- `oag_timeline`：事件；
- `oag_knows`：谁知识什么；
- 所有工具带 actor 视角，结果带来源/回合/置信度；
- 每回合调用次数/结果长度限制。

验收：

- “韩立的师父的敌人”“第 30 回合钥匙在谁手里”“谁知道钥匙在 12 号柜”可回答；
- 角色调用不会泄露未见证事实；
- 空结果有明确提示。

### Phase 4：自动抽取与压缩

**目标**：减少人工维护，让 OAG 跟得上长篇。

任务：

- 独立后置抽取任务从回合记录生成 claim（带 speaker/witnesses/confidence）；**不要**让角色扮演模型在回复 JSON 里附带 `fact_extractions`，那会破坏入戏；
- 可选回合后快速模型抽取/合并；
- 谓词归一化：内置关系 + 同义词 + 未知 → `related_to` + review queue；
- 事件摘要、arc/chapter 摘要；
- 矛盾检测：functional link 冲突 → invalidate + conflict；
- 嵌入/索引：事件、lore、文档 chunk；增量更新。

验收：

- 100 回合后，旧事件仍可查；
- 矛盾不会静默覆盖；
- 低置信 claim 进入 review queue，不直接污染 Canon。

### Phase 5：作者与运维界面

**目标**：让作者能维护本体，让开发者能排查。

任务：

- 世界编辑器：object/link 最小编辑器、lore 关联、ontology 校验；
- Debug：Perspective Inspector（“Alice 在第 42 回合知道什么”）、事件时间线、实体页、冲突/待审队列；
- 维护命令：重建索引、重算知识、去重实体、修复引用；
- 健康检查：启动/加载存档时跑完整性检查。

验收：

- 作者能在不写 SQL 的情况下修一条错误关系；
- 开发者能在 3 分钟内定位“NPC 为什么知道/不知道”。

## 3. 写入管线（每回合）

```text
turn finish
  → build_turn_event（确定性，不调 LLM）
  → extract_claims（独立后置抽取；角色回复不再附带 fact_extractions）
  → resolve_entities（名称/别名/类型）
  → write_links_attributes（时间窗、冲突失效）
  → write_knowledge（witnessed/told/public/private）
  → write_memories（按消息级 witnesses）
  → journal oag_committed（幂等）
  → async: summaries / embeddings / index / conflict scan
```

原则：

- **事件先于摘要**：事件是真相源，摘要可重建；
- **claim 不是真理**：`asserted_by` + `source` + `confidence` + `truth_status`；
- **知识边不是可选项**：没有 `oag_knowledge` 的关系默认只对导演可见；
- **幂等**：`UNIQUE(session_id, turn_index, ordinal)` + journal step；
- **失败隔离**：OAG 写失败只记 trace，不阻断回合；后台重试；
- **不在 DB 锁内调 LLM/embedding**：沿用现有 prepare/embed/finalize 三段式。

### 3.1 事件

每回合至少一条：

```json
{
  "turn_index": 42,
  "scene_id": "scene-dock-42",
  "event_type": "conflict",
  "title": "仓库冲突",
  "summary": "玩家逼问老王 3 号仓的事，老王否认，门外出现脚步声。",
  "details": {"player_input": "...", "speaker_lines": ["..."], "changes": ["..."]},
  "visibility": "scene"
}
```

`witnesses` → `oag_event_objects(role='witness')` + `oag_knowledge(witnessed)`。

### 3.2 Claim/关系

```json
{
  "subject": "铜钥匙",
  "predicate": "held_by",
  "object": "阿禾",
  "asserted_by": "老王",
  "witnesses": ["玩家", "老王", "阿禾"],
  "confidence": 0.8
}
```

- 主语/宾语先解析成对象；未知则创建 session object；
- 谓词归一化到 link type；未知 → `related_to` + 原文谓词；
- `valid_from_turn=当前`；同一 functional link 的旧边 `valid_to_turn=当前`；
- 写入 `oag_knowledge`：witnesses 得 `witnessed`；recipients 得 `told`；
- 只有说话人/作者确认的 claim 才 `truth_status=confirmed`，其余 `asserted`。

### 3.3 记忆

- 保留现有 `memories`，按角色、按回合、主观；
- 每条消息的 audience = witnesses - 非 recipients（耳语）；
- `memory_entries` 仍然可以指定 `character_names`，但必须 ⊆ audience；
- 记忆关联事件/对象，便于投影和查询。

### 3.4 摘要与索引

- 每 N 回合（默认 12）生成 arc 摘要；每 5 个 arc 生成 chapter；
- 摘要输入是事件（不是全文），输出带事件范围；
- 摘要失败 → 事件拼接兜底；
- 嵌入/索引异步，content hash 去重；
- 分支/回滚后按 journal 重建。

## 4. 读取管线（每个说话人）

```text
speaker turn
  → Perspective { actor, session, turn, scene }
  → load VisibleGraph（Canon + Session，按 knowledge 过滤）
  → projection queries
  → rank / dedupe / budget
  → render oag_context
  → trace
  → prompt assembly（oag_context + 最近对话 + 角色卡）
```

原则：

- **先裁剪，再查询**：不能在结果上过滤；
- **只取相关**：当前场景、玩家提到的实体、在场者、最近事件、相关记忆；
- **不重复**：事实/记忆/最近对话同一事件只出现一次；
- **带来源**：亲眼/被告知/听说/怀疑/不知道；
- **不生成行为建议**：只给事实，模型自己决定；
- **缓存**：同一回合同一 actor 的 `BeliefSnapshot` 可缓存；
- **trace**：记录每个投影的候选、选中、token、耗时。

### 4.1 BeliefSnapshot

```rust
struct BeliefSnapshot {
    actor: ActorId,
    turn: i32,
    scene: SceneFrame,              // 时间/地点/在场/可见物品
    entities: Vec<EntityCard>,      // 属性 + 1 跳关系
    facts: Vec<FactView>,           // 关系/属性，带来源/置信度
    events: Vec<EventView>,         // 最近 + 相关
    memories: Vec<MemoryView>,      // 角色自己的
    unknowns: Vec<UnknownView>,     // 可选：明确“没有信息”
}
```

- 对角色：`FactView` 是 belief（“你以为/你听说”），不是世界真理；
- 对导演：`FactView` 带 truth_status/conflict/source；
- `unknowns` 只在被查询/被提及时生成，不要主动列“你不知道全世界”。

## 5. 数据表与模块

### 5.1 新表

按 [OAG 本体设计 §9](./03-ontology-design.md) 的表：

- TBox：`oag_object_types`、`oag_link_types`
- ABox：`oag_objects`、`oag_attributes`、`oag_links`、`oag_scenes`、`oag_scene_participants`、`oag_events`、`oag_event_objects`
- 认知：`oag_knowledge`、`oag_access_policies`、`oag_turn_audience`
- 记忆：`oag_memory_objects`
- 文本：`oag_lore_entries`、`oag_summaries`
- 运维：`oag_query_traces`、`oag_conflicts`、`oag_review_queue`

### 5.2 Rust 模块

```text
src-tauri/src/services/game_engine/oag/
  mod.rs            OagService facade
  types.rs          Perspective / Actor / BeliefSnapshot / views
  schema.rs         TBox 读取与校验
  store.rs          repositories 组合
  ingest.rs         世界包 -> Canon
  turn.rs           回合写入（事件/claim/knowledge）
  resolve.rs        实体解析 / 谓词归一化
  graph.rs          VisibleGraph
  query.rs          find/traverse/as-of/aggregate/knows
  projection.rs     BeliefSnapshot -> oag_context
  tools.rs          模型工具适配
  maintenance.rs    完整性检查/重建/迁移
  tests.rs
```

repository：

```text
db/repositories/oag_object_repo.rs
db/repositories/oag_link_repo.rs
db/repositories/oag_event_repo.rs
db/repositories/oag_knowledge_repo.rs
db/repositories/oag_scene_repo.rs
db/repositories/oag_lore_repo.rs
db/repositories/oag_trace_repo.rs
```

命令：

```text
commands/oag.rs
  list_objects / get_object / upsert_object
  list_links / upsert_link
  query_oag / get_perspective / get_oag_trace
  list_conflicts / resolve_conflict
  list_review_queue / review_claim
  rebuild_oag_index / recompute_knowledge / dedupe_entities / repair_refs
```

## 6. 怎么维护

OAG 的维护分“Canon 维护、运行时维护、结构维护、运维”四类。麻烦的不是建表，是**长期不漂移**。

### 6.1 Canon 维护

- **稳定 key**：对象用 `obj:<world>:<type>:<normalized-key>`；不要用显示名当主键；
- **导入 diff**：世界包更新时按 key upsert；消失的 Canon 标记 `retired_at`，不物理删除（避免引用断裂）；
- **不覆盖运行时**：导入只改 `session_id=''` 的 Canon，不碰 session 对象/关系；
- **schema 校验**：object/link type 必须存在；domain/range 不匹配 → 拒收或降级 `related_to` + warning；
- **lore 关联**：lore 文本必须指向存在的对象；孤儿 lore 进 lint；
- **版本**：`oag_schema_version` + 世界包版本；开发阶段可拒绝旧包，但导出要写版本。

#### 作者工作流

```text
改 world package / objects.json / lore.json
  → 导入预览（新增/修改/退役对象与关系 diff）
  → schema 校验 + domain/range 检查
  → 应用到 Canon（不碰运行时）
  → 重建相关索引
  → 抽样检查：随机 5 个角色视角，看“知道/不知道”是否符合预期
```

- Canon 修改必须走预览，不能直接覆盖数据库；
- 运行时 claim 由作者/导演在 Debug 的 review/conflict 队列处理；
- 世界包发布前跑一次 `ontology lint` + `perspective smoke test`。

### 6.2 运行时维护

#### 抽取质量

- 低置信 claim（`confidence < 0.6`）进 `oag_review_queue`，不直接进 Canon/检索；
- 未知谓词统一进 `related_to`，同时记录原文；作者可提升为正式 link type；
- 实体解析优先精确名/别名，其次最长匹配；自动合并只对高置信、同类型、无冲突的实体；
- 角色说谎/误判：存 `asserted`，不存 `confirmed`；查询结果标“据 X 所述”。

#### 矛盾

- functional link（`located_in`/`leader_of`）冲突：旧边 `valid_to_turn=当前`，新边生效，记 `oag_conflicts`；
- 非 functional 冲突：两条都保留，`truth_status=disputed`，进冲突队列；
- Canon vs runtime 冲突：runtime 视为剧情变化，不静默改 Canon；导演/Debug 可见；
- 冲突不阻断回合，只进 trace/队列。

#### 知识传播

- 默认：witnesses 得 `witnessed`；
- 耳语：recipients 得 `told`，其他在场者只知道“发生了一次耳语”；
- 电话/远程：先加入 witnesses，再按在场处理；
- 二手信息：A 告诉 B，B 得 `told`，来源记 A/事件；不要自动扩散给所有在场者；
- 推断：只在可见子图内做；`inferred` 标低置信，可关闭。

#### 时间

- `valid_from_turn <= valid_to_turn`；`learned_turn <= 当前回合`；
- as-of 查询用时间窗，不用“当前快照覆盖”；
- 分支/回滚按 turn 截断，不把未来知识带回过去。

#### 摘要

- `summary_cursor` 记录已摘要到哪回合；
- 事件变更/回滚 → 标记相关摘要 stale，后台重建；
- 摘要只压缩，不删除事件；
- 旧摘要保留，按 level/to_turn 查询。

#### 索引/嵌入

- content hash 判断是否需要重算；
- 增量索引：新事件/新 lore/新 chunk；
- stale 检测：索引版本/模型版本/内容 hash；
- 重建命令：`rebuild_oag_index`（可按 session/world）。

### 6.3 结构维护

#### 迁移

- `PRAGMA user_version` 或 `oag_schema_version`；
- 迁移脚本只做结构/数据搬运，不夹带业务逻辑；
- 新表用 `CREATE TABLE IF NOT EXISTS`，旧表逐步退役；
- 开发阶段可清库，但测试档要有一次性迁移路径。

#### 完整性检查

启动/加载存档/导入世界时跑：

```sql
-- 例子
links 指向不存在的 object
knowledge 指向不存在的 target
events 没有 witness（且 visibility != public）
attributes 时间窗非法
scene_participants 指向不存在的 object
同一 session 下 normalized name 重复但未合并
knowledge.learned_turn > event.turn_index
```

修复命令：

```text
repair_refs          删除/隔离孤儿引用
recompute_knowledge  从 turn audience/events 重算知识边
dedupe_entities      合并重复对象（需人工确认）
rebuild_summaries    重建摘要
rebuild_oag_index    重建索引
```

维护命令统一要求：`--dry-run` 预览、`--world`/`--session` 限定范围、输出变更计数；破坏性修复必须二次确认。

#### 分支/回滚/导出

- 分支复制：events（<= 分支回合）、links/attributes、knowledge、scenes、summaries、memory_objects；
- 回滚：按 journal snapshot/turn 截断，不能让知识边跑到未来；
- 世界包：导出 Canon objects/links/lore；不导出 session 数据；旧包拒绝（开发阶段）；
- 删除：session/world/character 级联清理；保留 `retired` 引用检查。

### 6.4 运维与观测

指标：

- `projection_empty_rate`：投影为空的比例；
- `knowledge_leak_rate`：隔离测试/线上抽检；
- `contradiction_rate`：冲突/总 claim；
- `entity_duplicate_rate`：重复实体；
- `p95_projection_latency` / `p95_query_latency`；
- `avg_oag_tokens`；
- `review_queue_size`；
- `stale_summary_count`。

Trace：

- 每次投影：perspective、候选数、选中数、token、耗时、过滤原因；
- 每次查询：query、visible nodes、结果、耗时；
- 每次写入：事件/claim/知识边、invalidate、失败原因。

Debug 三问：

1. “Alice 为什么知道 X？” → 查 `oag_knowledge` + source_event + turn audience；
2. “Alice 为什么不知道 X？” → 查是否有 knowledge、是否被 policy 过滤、是否在视野外；
3. “这条事实从哪来？” → 查 link source/asserted_by/event。

### 6.5 运维节奏

| 时机 | 动作 |
|---|---|
| 每回合 | 事件/claim/知识/记忆写入；trace |
| 每 N 回合 | 摘要；冲突扫描；索引增量 |
| 每次加载存档 | 完整性检查；stale 摘要/索引修复 |
| 每次导入世界 | Canon diff；schema 校验；重建索引 |
| 每次 schema 变更 | 迁移；reindex；黄金会话回归 |
| 每周/每月 | trace 清理；embedding 重建；评测集回归；性能报告 |

## 7. 质量门禁

必须有的测试：

1. **隔离**：第三方看不到私聊；场景切换不串；耳语只给 recipients；电话参与者有记忆。
2. **时间**：第 30 回合查不到第 40 回合的事实；as-of 正确；invalidate 后历史仍可查。
3. **知识边界**：角色查不到未见证事实；导演能查全图；`oag_knows` 分视角。
4. **查询语义**：find/traverse/aggregate/absence 的边界条件；空结果不编造。
5. **分支/回滚**：知识边/事件/摘要与存档回合一致。
6. **导入/导出**：Canon round-trip；旧包拒绝。
7. **性能**：1000 回合会话的投影/查询 P95；token 预算。
8. **抽取质量**：低置信进队列；未知谓词不污染；说谎不变成 confirmed。

评测集：

- 合成 200~1000 回合会话；
- 在第 5/50/150 回合埋事实、关系、秘密、耳语、电话；
- 固定问题集：位置、持有、关系、时间、谁知道、谁不知道、聚合、否定；
- 每次改动跑回归，记录 recall/leak/contradiction。

## 8. MVP：先做哪一块

不要一开始做全量本体。建议第一个竖切（约 2~3 周）：

1. **Phase 0 隔离**：消息 witnesses、turn audience、读过滤、fact speaker。
2. **最小本体**：character / location / item / organization；`located_in` / `holds` / `owns` / `member_of` / `ally_of` / `enemy_of`。
3. **事件 + 知识**：每回合事件、witnesses、`oag_knowledge`。
4. **Perspective 投影**：scene frame + entity cards + known facts + events + memories。
5. **两个查询**：`oag_entity` + `oag_query`（find/1 跳）；Debug perspective inspector。

先不做：

- 全量 lore/文档 chunk；
- 复杂 access policy；
- 自动摘要；
- 图推理/社区；
- 全自动 ontology 编辑 UI。

原因：先把“视角正确 + 能查基本事实”跑通，再谈压缩和自动化。

## 9. 常见坑

1. **先做查询、后做隔离**：多跳会放大泄密；隔离必须 Phase 0。
2. **把 claim 当真理**：NPC 说谎/误判会污染 Canon；必须 `asserted_by` + `truth_status`。
3. **在结果上过滤**：路径长度/聚合会泄露隐藏事实；必须 VisibleGraph 先裁剪。
4. **用 name 当主键**：同名/改名/翻译会串；用稳定 id + aliases。
5. **全量导入 lore 当事实**：文本是投影，不是真相源；结构化对象/关系才是。
6. **每回合都 LLM 抽取**：成本高、噪声大；优先确定性事件 + 独立后置抽取，且不要把抽取塞进角色扮演回复。
7. **摘要替代事件**：摘要会丢细节；事件必须保留。
8. **没有 review queue**：低置信 claim 直接进图，越跑越脏。
9. **没有 trace**：出问题只能猜；投影/查询都要 trace。
10. **分支/回滚漏表**：知识边/事件/摘要必须一起截断。

## 10. 一页检查表

搭建：

- [ ] Perspective 契约确定：actor/session/turn/scene
- [ ] 消息级 witnesses/recipients/presence_mode
- [ ] Canon 导入 + 稳定 ID + alias
- [ ] 事件/claim/knowledge 写入 + journal
- [ ] VisibleGraph 先裁剪再查询
- [ ] 投影 + 预算 + 去重
- [ ] 查询工具 + trace
- [ ] Debug perspective inspector

维护：

- [ ] 世界导入 diff / retired
- [ ] claim review queue
- [ ] 冲突队列 + invalidate
- [ ] 知识传播规则（witnessed/told/phone/whisper）
- [ ] 摘要 watermark + stale 重建
- [ ] 索引 content hash + 重建命令
- [ ] 完整性检查 + repair
- [ ] 分支/回滚/导出
- [ ] 指标 + 黄金会话
- [ ] 迁移 + schema version
