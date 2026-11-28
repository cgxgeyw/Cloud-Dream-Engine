# 记忆隔离代码审计（当前实现）

> 目的：在做 OAG 之前，先把“不同场景、不同人物为什么会有不同记忆”这件事从代码层面讲清楚。
> **2026-11-27 当前结论**：M0 隔离范围与 M2 的视角投影/查询已落地。逐消息记忆、耳语读过滤、事实 actor 可见性、知识库文档 audience、分支/回滚一致性、OAG 可见子图与紧凑模型注入均已修复或实现；独立事实抽取、远程发言策略和旧 audience 回填仍未实现。
>
> **2026-11-12 历史更新**：新消息已写入 `witnesses`（玩家消息用回合开始名册，NPC 消息用新场景名册，开场消息用开局名册）；`slice_character_history` / `collect_windowed_dialogue_messages` 已按 viewer 过滤；不在场角色不能进入 `planned_speakers`。当时仍缺：`build_turn_entries()` 按回合末名册写记忆；事实/知识库没有 actor；旧消息无 `witnesses` 时按可见处理；历史轮数默认 30，但不做 token 截断（按模型上下文能力）。下文 0～6 节保留 M0 前审计，便于对照；当前状态以本段和 §8.3 为准。
>
> 相关代码：`src-tauri/src/services/game_engine/memory/mod.rs`、`orchestrator/turn_context.rs`、`orchestrator/speaker_loop.rs`、`commands/sessions.rs`、`db/repositories/memory_repo.rs`。

## 0. M0 前结论摘要（历史审计）

| 层 | 现状 | 判断 |
|---|---|---|
| 记忆写入 | 每条记忆绑定 `character_id`，按“本回合参与者”逐角色写一份 | ✅ 基本成立 |
| 记忆召回 | SQL 按 `world_id + session_id + character_id` 过滤 | ✅ 成立 |
| 最近对话 | `session.messages` 全局切片，不按角色过滤 | ❌ 串场景 |
| 对话焦点 | 按命中回合取全局原始对话，不按在场者过滤 | ❌ 串场景 |
| 事实卡片 | `memory_relations` 只按 session 存，无 `character_id` | ❌ 无隔离 |
| 知识库 | 按世界/公共库检索，无角色 audience | ❌ 无隔离 |
| 属性 | 按 schema 的 `agent_self_read/agent_other_read`，不按场景 | ⚠️ 策略级隔离 |
| 场景名册 | 只持久化“当前”名册；`turn_journal.snapshot_created` 另有回合开始快照，但读路径不用 | ⚠️ 可回填，但无规范化观众 |
| 分支 | 复制 memories 和属性，不复制 entities/relations | ❌ 事实丢失 |

一句话：**“不同场景的人有不同记忆”目前只成立于“写记忆时他不在场”这一种情况；只要进入读路径，最近对话会把别的场景的内容带进来。**

## 1. 数据模型：记忆的主体是谁

记忆表在 `db/schema.rs:138-159`：

```sql
memories(
  id, world_id, session_id, character_id, layer, content, source,
  importance, created_at, turn_index, conversation_id, event_id,
  item_id, scene_id, memory_type, speaker, role, location,
  participants_json, keywords_json
)
```

关键点：

- `character_id` 是记忆的主体，不是“说话人”。同一句 NPC 台词，会给每个在场角色各写一条，内容相同、`character_id` 不同。
- `event_id`、`scene_id`、`participants_json` 已经在表里，但当前写入逻辑基本只填 `scene_id` 和 `participants`，`event_id` 为空。
- 没有 `witnesses`、`recipients`、`audience` 字段；也没有“谁听到了这句话”的独立记录。

`MemoryRepository::list()`（`db/repositories/memory_repo.rs:14-43`）的过滤条件是：

```text
world_id + session_id + character_id (+ layer)
ORDER BY created_at DESC
LIMIT n
```

所以召回阶段的隔离是硬过滤，不是靠排序“尽量不串”。测试 `recall_entries_for_character_are_isolated_per_character`（`memory/tests.rs:98`）验证了 Alice 的召回里不会出现 Bob 的记忆。

## 2. 写入路径：谁得到这一回合的记忆

### 2.1 主流程

```text
prepare_turn_context
  → director turn（导演决定场景、名册、说话人）
  → prepare_director_runtime（解析 visible_chars / next_scene）
  → run_speaker_turns（逐角色生成台词）
  → apply_director_runtime_effects（应用场景、属性、事实、记忆）
  → build_runtime_updated_session_snapshot（得到 updated）
  → commit_turn_memories(updated)
  → persist_turn_entries(...)
```

关键调用在 `commands/sessions.rs:464-474`：`commit_turn_memories` 拿到的是 **`updated`（运行时效果应用后的新快照）**，以及 `session`（回合开始前的旧快照）。

`build_turn_entries()`（`memory/mod.rs:386-502`）计算参与者：

```rust
let mut names = visible_characters.to_vec();          // updated.visible_characters（新名册）
names.push(player_character_name);
for message in messages where turn_index == current:
    if let Some(speaker) = message.speaker:
        names.push(speaker);
```

然后：

- 玩家消息：给每个 `participant_ids` 写一条 `player_action` 记忆；
- 每条 agent 消息：给每个 `participant_ids` 写一条 `speaker_response` 记忆。

所以“这一回合的观众” = **回合结束时的名册 + 玩家 + 本回合所有说话者**。

### 2.2 名册从哪里来

`prepare_director_runtime`（`orchestrator/run.rs:855-879`）：

- 默认：`visible_chars = session.visible_characters`，再把本回合新生成的角色加进去；
- 如果导演显式输出 `scene_visible_characters`：**完整替换** `visible_chars`；
- `switch_player_character` 提案里的 `scene_character_roster` 也会合并/替换（`run.rs:1132-1150`）。

最终名册写入：

- `session.visible_characters`（`writeback.rs:835`、`session_materialization.rs:90`）；
- `scene.present_characters` 由 `visible_chars + 玩家` 派生（`scene.rs:70-72`、`session_materialization.rs:28-31`）。

**没有专门的历史名册表**。但有一个容易被忽略的来源：`turn_journal` 的 `snapshot_created` 步骤在每回合开始前保存了 `session_snapshot`（含当时的 `visible_characters`，`run.rs:742-763`），旧数据可以用它回填/审计。它记录的是**回合开始前**的名册，不覆盖回合结束的新名册，也从未被读路径使用；`game_turn_states` 只有 phase/active_actor，也不存名册。

### 2.3 测试证明的边界

`persist_turn_entries_isolates_private_dialogue_by_present_participants`（`memory/tests.rs:313-391`）：

- 回合 7，只有 Alice 在场；
- 玩家说“告诉 Alice 口令是 moon glass”，Alice 回“记住了”；
- 写入后 Alice 有这条记忆，Bob 没有任何一条包含 `moon glass` 的记忆。

这个测试证明：**“不在场的人不会得到这一回合的记忆”在写入层是成立的。**

## 3. 召回路径：角色只能读自己的记忆

`speaker_loop.rs:143-161`：

```rust
let memory_pool = load_character_memory_pool(conn, world_id, session_id, speaker_character_id)?;
let recall_plan = memory_service.prepare_character_recall(
    conn, world, world_id, session_id,
    speaker_character_id,          // 必须非空，否则直接返回空
    player_input, location, scene_id, participants,
    recall_limit, memory_strategy,
)?;
```

`prepare_character_recall`（`memory/mod.rs:168-306`）第一件事就是：

```rust
let Some(character_id) = character_id ... else { return Ok(empty) };
```

然后 `MemoryRepository::list` 按 `character_id` 过滤。`finalize_character_recall` 只在这批候选里排序。因此：

- 语义/词法召回不会跨角色；
- `memory_pool`（用于 `event_timeline`）也是按角色取的；
- 角色策略 `memory_strategy=off/不记` 只会让召回为空，**不会停止写入**（记忆仍然落库，换角色或重新开启后还能召回）。

**这里的隔离是可信的，OAG 应保留，不要改成“全局记忆池”。**

## 4. 上下文的破口：隔离在读路径失效

### 4.1 `recent_dialogue`：全局最近对话

`speaker_loop.rs:162-168`：

```rust
let recent_messages = slice_character_history(
    &messages,                                  // 整个 session.messages + 当前玩家输入
    speaker_char.map(|c| c.recent_dialogue_rounds).unwrap_or(2),
    Some(session.player_character_name.as_str()),
);
```

`slice_character_history`（`orchestrator/turn_context.rs:534-566`）只做三件事：

1. 过滤 `player` / `agent` 角色；
2. 按“玩家消息”计数，往前取 N 轮；
3. 反转回时间顺序。

**没有 viewer 参数，没有检查消息的在场者/场景。**

所以：Alice 和 Bob 在码头私聊（C 不在场），C 在第 10 回合回到酒馆说话，只要 `recent_dialogue_rounds >= 3`，C 的 prompt 里就会出现第 7 回合 Alice/Bob 的私聊原文。存储层没给 C 写记忆，但最近对话直接绕过了记忆。

开场消息也一样：`session_lifecycle.rs:71-82` 创建的 `opening_messages` 没有 `turn_index`、没有 audience，`slice_character_history` 会无条件把它们带给任何后来生成的 NPC。

### 4.2 `dialogue_focus`：按命中回合取全局原文

`build_character_memory_context()`（`turn_context.rs:191-225`）用角色自己的 `recalled_memories` 得到 `hit_turns`，然后：

```rust
let dialogue_focus = collect_windowed_dialogue_messages(
    &session.messages,      // 全局
    &hit_turns,
    dialogue_window_rounds,
);
```

`collect_windowed_dialogue_messages`（`turn_context.rs:268-288`）只按 `turn_index` 是否落在 `hit_turn ± window` 里过滤，不检查这条消息的观众。

举例：C 在第 5 回合在场，第 6~7 回合离开；第 5 回合的记忆命中后，`window=2` 会把第 6、7 回合 Alice/Bob 的私聊也带进 C 的 `dialogue_focus`。**命中回合本身是 C 在场的，但窗口扩到了 C 不在场的回合。**

### 4.3 `present_characters` / 场景状态：所有说话人看到同一份当前场景

`collect_memory_participants(session)`（`turn_context.rs:409-422`）直接返回：

```text
session.player_character_name + session.visible_characters
```

`build_character_scene_state_payload` 把它作为 `present_characters` 发给当前说话人。对于正常在场角色没问题；但如果导演安排了一个不在 `visible_characters` 里的角色发言，这个角色会看到“当前场景的在场名单”，而名单里没有他自己。

更麻烦的是，同一份 prompt 里有两个名册来源：

- `scene_state.present_characters` 来自 `build_character_turn_payload(session, ...)` 使用的**回合开始前 session**（旧名册，`character_prompt.rs:111-124`、`turn_context.rs:339-369`）；
- `visible_characters` section 和 `public_scene_state_lines` 里的一部分来自 `visible_chars` / `runtime_preparation.visible_chars`（新名册）。

场景切换的回合，旧名册和新名册会同时出现在 prompt 里，且可能互相矛盾。这不是“显示问题”，而是写记忆和读上下文用了不同名册的同一个根因。

### 4.4 事实：完全按 session 存

`persist_fact_updates()`（`memory/mod.rs:541-634`）写入 `memory_entities` / `memory_relations`：

- 实体：`UNIQUE(session_id, name_normalized)`；
- 关系：`session_id + subject + predicate + object + valid_from/invalid_at`；
- **没有 `character_id`，没有 `known_by`，没有 `witnesses`。**

多说话人回合的 facts 合并也有信息丢失：`merge_speaker_fact_extractions()`（`commands/sessions.rs:913-941`）只是把每个 speaker payload 里的 `fact_extractions` 数组拼起来，**不带说话人**。进入 `parse_fact_extractions` 时，事实已经不知道是谁说的。

结果：

- Alice 私下告诉玩家“钥匙在 12 号柜”，这条事实会变成整个 session 的事实；
- 一旦 OAG 把事实注入上下文或提供给查询工具，Bob 也会知道；
- NPC 说谎也会被当成事实（`source=llm_extraction`，没有 `truth_status`）。

### 4.5 知识库：世界级，不是角色级

M0 前，`commands/knowledge.rs` 的 `search()` 只按 `world_id` + `knowledge_base.enabled` + 公共库绑定过滤，工具能力只看旧的 `knowledge_access` 布尔字段，没有 actor 参数和文档 audience。任何在场或不在场角色调用 `search_knowledge` 都可能读到整个世界资料。

M0 后，工具授权统一读取 `director_config.allowed_mcp_tool_ids` 中的 `mcp-tool-search-knowledge`；检索按消息观众、角色知识范围和文档 audience 过滤。旧布尔字段不再参与判断。

### 4.6 属性：策略级，不是场景级

`load_character_visible_attribute_lines()`（`turn_context.rs:452-532`）：

- `session` 属性：只看 schema 的 `access_policy.agent_self_read`；
- `session_character` 属性：owner 是自己看 `agent_self_read`，是别人看 `agent_other_read`。

没有“这个角色是否在当前场景”“这个值是否被隐藏/伪装”的判断。属性可见性是 schema 级，不是运行时知识级。

## 5. 场景与名册的生命周期

| 时点 | 行为 | 代码 |
|---|---|---|
| 新建存档 | `visible_characters` = 开局角色（不含玩家）；opening messages 无 audience | `session_lifecycle.rs:58-82` |
| 回合开始 | 旧名册 + 新生成角色，或导演显式 `scene_visible_characters` 完整替换 | `run.rs:855-879` |
| 回合中 | 场景切换在说话人之前决定；说话人使用新场景名/新名册 | `run.rs:1035-1209`、`speaker_loop.rs:190-216` |
| 回合结束 | `session.visible_characters` 更新为新名册；`scene.present_characters` 派生 | `writeback.rs:811-835`、`session_materialization.rs:90` |
| 记忆写入 | 用**回合结束的新名册**给本回合所有消息分配观众 | `memory/mod.rs:675-685` |
| 分支 | 复制 session（含当前名册）+ memories；不复制 facts/entities | `save_repo.rs:159`、`:205-206`、`:257-363` |

### 5.1 一个具体的串场景回合

假设：

- 回合 7 开始时，场景 A 的在场者是 Alice、Bob；
- 玩家在 A 对 Alice 说：“钥匙在 12 号柜，别告诉 Bob。”
- 导演输出 `change_scene` 到场景 B，`scene_visible_characters = ["Carol"]`；
- 本回合说话人只有 Carol（新场景）；
- 回合结束 `updated.visible_characters = ["Carol"]`。

实际结果：

- `build_turn_entries` 的 `participant_names` = `["Carol"] + 玩家 + 本回合说话者`；
- 玩家在场景 A 说的秘密，被写进 Carol 的记忆；
- Alice、Bob 没有拿到这条记忆（因为他们不在新名册里）；
- 但 `recent_dialogue` 是全局的，下一回合 Alice 说话时仍能看到“玩家对 Alice 说秘密 + Carol 回复”的原文；
- `dialogue_focus` 也可能把这段带回来。

这就是“新名册覆盖旧名册”导致的**回合级观众错配**。

### 5.2 不在场角色发言

`structured_output.rs:242-281` 的 `allowed_visible` 是：

```rust
visible_characters
  .chain(world_character_roster)       // 全世界所有角色
  .chain(generated_characters)
```

也就是说，`planned_speakers` 校验允许“世界角色表里的任何角色”，不只是当前场景名册。一个不在场的角色可以被安排发言：

- 他的 prompt 里 `present_characters` 是当前场景（没有他）；
- `recent_dialogue` 是全局；
- 他的台词会被写给当前场景的所有人；
- 他自己也会拿到本回合记忆。

如果这是“远程通话/切镜头”的设计，需要显式的远程场景；当前没有，所以这属于隔离漏洞。

## 6. 边界情况清单

| 情况 | 当前行为 | 风险 |
|---|---|---|
| 场景切换回合 | 玩家输入（旧场景）和 NPC 回复（新场景）统一记到新名册 | 新场景角色知道旧场景的秘密 |
| 角色中途加入 | 没有旧记忆，但能读全局最近对话/开场消息 | 新角色“天生知道”之前所有事 |
| 同场景私聊/耳语 | 所有在场者都拿到同一条记忆 | 在场 ≠ 听到；缺少 recipients/perception |
| 不在场角色发言 | 校验允许世界角色表，发言写入当前场景 | 缺席角色参与场景并共享记忆 |
| 玩家切换角色 | 记忆按 character_id 跟随；最近对话仍是全局 | 新玩家角色看到旧角色私聊 |
| 分支存档 | 复制 memories + 属性；facts/entities 丢失 | 分支后事实卡片消失 |
| 开场消息 | 无 turn_index/audience，永久进入全局历史 | 后来生成的 NPC 看到开局所有内容 |
| 同名角色 | 名册按 name 解析，`characters.find(name)` 取第一个 | 记忆可能写到错误角色 |
| NPC 说谎 | facts 无 `truth_status`，说话人丢失 | 谎言变事实，污染查询 |
| 删除角色 | 名册移除；记忆/事实仍在库 | 残留记忆可能被同名新角色召回 |
| agent_chat 单角色 | `visible_chars = [agent]`，无多角色问题 | 不适用 |

## 7. 目标模型：把“在场”升级成“知识边界”

OAG 不能只查对象和关系，还必须知道**这个 actor 能不能看到这条边**。建议的目标模型：

### 7.1 消息级观众

给 `ChatMessage.metadata` 增加（不改表结构）：

```jsonc
{
  "turn_index": 7,
  "scene_id": "dock-warehouse",
  "speaker_character_id": "char-alice",
  "witnesses": ["char-player", "char-alice", "char-bob"],
  "presence_mode": "physical",         // physical | remote（电话）
  "recipients": ["char-alice"],        // 可选：耳语/密谈
  "perception": "public"               // public | private | internal
}
```

- 玩家消息：用**回合开始时**的场景名册；
- NPC 消息：用**生成时的**场景名册（新场景）；
- 电话：`presence_mode=remote`，加入 `witnesses`，不进物理 `present_characters`；
- 系统/旁白：不进对话历史，单独处理；
- 开场消息：补 `scene_id` + 开局名册。

### 7.2 每回合观众快照

新增 `oag_turn_audience(session_id, turn_index, scene_id, witnesses_json, recipients_json)`：

- 写入时和消息一起落；
- 旧数据优先从 `turn_journal.snapshot_created.session_snapshot.visible_characters` 取“回合开始名册”；缺失时从 `memories` 反推：某回合有 memory 的 `character_id` 集合 ≈ 该回合观众；
- 读路径优先用消息 metadata，缺失时回退到 turn audience。

### 7.3 知识边

新增 `oag_knowledge(actor_id, target_kind, target_id, knowledge_type, source_event_id, learned_turn)`：

- `witnessed`：事件参与者/见证者；
- `told`：对话中被明确告知的人（recipients）；
- `public`：世界通则；
- `private`：只有自己知道（内心、秘密）。

查询时先按 actor 过滤可见子图，再做遍历。**不能先遍历再过滤结果**，否则多跳会通过隐藏边推断出秘密。

更细的“知情条件”（组织、等级、物品、语言、时间）用 `oag_access_policies` 表达，在 `VisibleGraph` 构建时评估；不满足的节点直接不进入图，查询结果里也不应出现“无权限/存在但不可见”的提示。

### 7.4 事实带说话人

`FactUpdate` 增加 `asserted_by` / `source_character_id` / `visibility`；`merge_speaker_fact_extractions` 保留 speaker；`persist_fact_updates` 写知识边：

- 本回合观众都获得 `witnessed`；
- 如果事实只在某个角色的台词中出现且不是公开宣告，标记 `asserted`，不要自动变成所有人知道；
- `truth_status` 区分 `asserted/confirmed/disputed`。

### 7.5 读路径过滤

- `slice_character_history(messages, viewer, rounds)`：只保留 `witnesses` 含 viewer 的消息；无 metadata 时查 turn audience；
- `collect_windowed_dialogue_messages(messages, viewer, hit_turns, window)`：同上；
- `present_characters`：只给“当前场景 + 当前说话人确实在场”的上下文；不在场发言要么禁止，要么显式远程场景；
- `search_knowledge` / OAG 查询：带 actor，按文档 audience 过滤。

## 8. 修复顺序与测试矩阵

### 8.1 顺序

1. **消息级 witnesses + turn audience**（不改表，先改 metadata 和读过滤）。
2. **per-message 记忆写入**：`build_turn_entries` 按每条消息的 witnesses 写，而不是回合结束名册。
3. **事实说话人 + 知识边**：`FactUpdate` 带 speaker，写 `oag_knowledge`。
4. **不在场发言策略**：禁止；`planned_speakers ⊆ 场景名册 ∪ 新生成 ∪ 远程参与者`；电话用 `presence_mode=remote`。
5. **知识库 audience**：文档加可见范围。
6. **分支/回滚**：复制/回滚 audience、events、knowledge。

### 8.2 测试

| 测试 | 断言 |
|---|---|
| 私聊不在场 | C 的 `recent_dialogue` 不含 A/B 私聊；C 的 `dialogue_focus` 不含 |
| 场景切换回合 | 玩家输入只写给旧名册；NPC 回复只写给新名册；互不串 |
| 新角色 | 新角色的最近对话从它入场那回合开始 |
| 耳语 | `recipients=[Alice]` 时 Bob 不获得记忆/知识 |
| 缺席发言 | 默认被拒绝；显式远程场景才允许 |
| 事实隔离 | Alice 的私密事实不出现在 Bob 的查询/上下文 |
| 分支 | 分支保留 events/knowledge/facts；不读到旧分支 |
| 同名角色 | 按 id 而不是 name 解析 |
| 旧数据回填 | 从 memories 反推 turn audience 后，旧存档不串 |

### 8.3 M0 实施状态（2026-11-26）

| 范围 | 当前实现 | 状态 |
|---|---|---|
| 逐消息记忆 | 每条 player/agent 消息按自己的 witnesses/recipients/scene 写入；旧消息回退到兼容观众 | ✅ 已实现 |
| 最近对话与 dialogue_focus | 按 viewer 的 witnesses 过滤；recipients 非空时只保留说话人和收件人 | ✅ 已实现 |
| 事实来源 | speaker payload 覆盖 asserted_by/id 和 witnesses，模型不能自报 confirmed | ✅ 已实现 |
| 事实可见性 | public/private/scene；actor 查询先过滤，矛盾失效限定在同一 actor | ✅ 已实现 |
| 知识库 audience | public/director/character；FTS、fallback、分块、嵌入、排序前过滤 | ✅ 已实现 |
| 分支 | memories + embeddings + summary_ranges + entities + relations，全部 ID 重映射并保留 provenance | ✅ 已实现 |
| 回滚 | 快照包含 summary_ranges；清除快照后的旧桶异步总结，还原实体/事实/来源/可见性 | ✅ 已实现 |
| 世界包 | 知识库 audience 随导入导出保留；旧资料默认 public | ✅ 已实现 |
| 不在场发言 | 世界角色表兜底与显式远程场景策略尚未收敛 | ⏳ 未完成 |
| audience 回填 | 尚无 oag_turn_audience 表或旧消息批量回填 | ⏳ 未完成 |
| 耳语占位事件 | 非收件人目前不取得正文，但还没有“发生耳语”的脱敏事件 | ⏳ 未完成 |
| OAG 投影/查询 | 已实现视角投影、八种逻辑查询与统一只读模型工具；模型只接收裁剪后的紧凑文本，完整 trace 留在 Debug | ✅ M2 已交付 |

新增回归测试覆盖：场景切换、耳语、事实来源合并、actor 私有事实与失效隔离、知识库 FTS/fallback audience、分支图 ID 重映射、回滚旧桶总结与 provenance 恢复。

## 9. 和 OAG 的关系

OAG 的查询必须建立在可信的隔离之上：

- `oag_query` 的 actor 上下文不是“装饰参数”，而是**可见子图的边界**；
- 知识边是本体的一部分（`Character -[knows]-> Fact/Event`），不是事后过滤；
- 记忆是角色的主观投影，事实/事件是客观层，二者用 `memory -> event -> knowledge` 连接；
- 没有这层，OAG 就是一个“更聪明的泄密机”：它能多跳查到 NPC 不该知道的事。

因此实现顺序上，**隔离先于查询**：M0 修隔离，M1 建本体，M2 上逻辑查询。
