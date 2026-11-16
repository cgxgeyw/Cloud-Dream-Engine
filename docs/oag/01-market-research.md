# OAG 市场调研：酒馆、RikkaHub、AI Dungeon 与 Agent Memory 框架

> 调研日期：2026-10-31
> 目的：在动手做 OAG 前，先确认“背景信息注入 / 历史对话召回 / 信息查询”这三件事，成熟产品分别是怎么做的、哪些经验值得抄、哪些坑要避开。
>
> 本文只做调研和结论，不涉及代码改动。落地方案见 [02-design.md](./02-design.md)。

## 0. 一句话结论

成熟方案几乎都不是“把历史全塞进上下文”，而是 **三层结构**：

1. **常驻层**：小而稳定的世界/角色/规则信息，每回合都在。
2. **压缩层**：把长历史压成摘要、事件、事实，按时间或主题组织。
3. **检索层**：每回合只取与当前输入最相关的少量片段，并且有明确 token 预算。

酒馆把第 1 层做到极致（世界书 + 插入位置 + 预算），AI Dungeon 把第 2 层做到极致（记忆摘要 + 故事摘要），RikkaHub 把“抽取 → 存储 → 注入”做成了轻量产品闭环，Letta/Mem0/Zep 则分别代表“分层记忆 + 工具”“抽取 + 合并”“时序知识图谱”三条工程路线。

对本项目最重要的三条：

- **背景注入要可控、可解释**：关键词/常驻/向量混合 + 预算 + 插入位置，比“把设定文档整篇塞进 system prompt”稳定得多。
- **长记忆要靠事件账本 + 摘要 + 检索**：只靠 `memories` 表按时间倒序取最新 N 条，历史一长必然断片。
- **事实要带时间与可见性**：谁在什么时候知道什么，比“存了一条事实”更重要；否则 NPC 会集体开天眼。

## 1. 调研范围

| 类别 | 方案 | 重点关注 |
|---|---|---|
| 角色扮演前端 | SillyTavern（酒馆）World Info / Lorebook | 背景注入、关键词触发、预算、插入位置 |
| 移动端 AI 聊天 | RikkaHub | 记忆抽取、全局/私有作用域、提示词注入 |
| 叙事游戏 | AI Dungeon Memory System | 摘要节奏、Memory Bank、上下文预算分配 |
| Agent 记忆框架 | Letta / Mem0 / Zep+Graphiti | 分层记忆、合并去重、时序图谱 |
| 企业检索范式 | Palantir OAG / OG-RAG | 本体、实体关系、多跳查询 |
| 图检索研究 | GraphRAG / HippoRAG / A-MEM | 图扩展、社区摘要、记忆演化 |

## 2. SillyTavern World Info（酒馆世界书）

酒馆的世界书是目前“作者可控背景注入”最成熟的民用实现。核心不是向量，而是 **条目 + 触发 + 预算 + 位置** 四件套。

### 2.1 条目模型

| 字段 | 作用 | 对本项目的启示 |
|---|---|---|
| `key` / `keysecondary` | 主关键词 / 次级关键词；支持正则 | 关键词要支持中文子串，不能照搬英文全词匹配 |
| `content` | 命中后注入的正文 | 条目应自包含，不依赖标题 |
| `constant` | 无需关键词，始终注入 | 适合“世界通则/当前时代”这类常驻信息 |
| `order` | 插入排序；越大越靠后（影响力越强） | 需要“重要性/顺序”而不是只按相关度 |
| `position` | 角色定义前/后、AN 上/下、@depth、outlet | 注入位置本身是效果的一部分 |
| `depth` / `role` | @depth 时距对话末尾几条、以什么角色注入 | 高影响信息应靠近当前输入 |
| `probability` | 命中后仍有一定概率不注入 | 可用于随机事件，但会损害可复现性 |
| `group` / `groupWeight` / `prioritize` | 同组只选一个，加权随机或按 order 确定性选 | 适合“同一场景的多种写法”互斥 |
| `scanDepth` | 条目级扫描最近多少条消息 | 全局默认 + 条目覆盖，灵活 |
| `sticky` / `cooldown` / `delay` | 触发后保持 N 条 / 冷却 N 条 / 至少 N 条才可触发 | 适合“战斗状态”“刚发生的事” |
| `vectorized` | 允许向量相似度激活 | 关键词的补充，不是替代 |

### 2.2 激活与预算

- 扫描缓冲区 = 最近 `scanDepth` 条消息（可带角色名前缀）。
- **递归扫描**：已激活条目的正文会进入下一轮扫描缓冲区，从而由 A 的关键词带出 B；可限制递归步数、禁止某条被递归、延迟到递归阶段才生效。
- **预算**：世界书总 token 占上下文比例（默认 25%）或绝对上限；预算耗尽后即使命中也不再插入。
- **优先级**：常驻条目先插入，然后 order 大的；直接关键词命中优先于递归命中。
- **向量匹配**：用最近 N 条消息做 query，相似度阈值约 0.25，最多 5 条；仍然受预算、概率、分组约束；官方明确说“想要确定性就用关键词”。

### 2.3 值得抄的

1. **作者侧条目化**：把“世界背景”拆成可复用、可触发的条目，而不是一篇长文。
2. **常驻 + 条件 + 向量三种激活方式并存**，各自有明确语义。
3. **预算是一等公民**：没有预算的世界书一定会挤爆上下文。
4. **插入位置可配置**：靠近当前输入的信息影响力更大。
5. **递归激活**：用条目之间的关键词引用，低成本实现“提到 A 就带出 A 的关系网”。

### 2.4 要避开的

- 英文全词匹配对中文有破坏性；中文必须用子串/二元组匹配。
- 大量常驻条目会饿死条件条目。
- 向量匹配不可预测，不能作为唯一激活方式。
- 世界书只管“注入”，不管“历史事实”，长对话仍然要靠摘要/记忆扩展。

参考：[SillyTavern World Info 文档](https://docs.sillytavern.app/usage/core-concepts/worldinfo/)、[世界书参考手册（TavernWeave）](https://github.com/liarmttt/tavernweave/blob/d134ddd69f941e2320160101315d760a631bfa65/skills/consult-tavernweave-library/references/st-guides/A3_%E4%B8%96%E7%95%8C%E4%B9%A6%E4%BC%98%E5%8C%96.md)。

## 3. RikkaHub

RikkaHub 是 Android 上的多模型聊天客户端，它的记忆和注入设计很“产品化”，适合作为轻量方案的参照。

### 3.1 记忆

- 用 **快速模型** 监控对话，抽取“值得记住的信息”（名字、偏好、过往决定），存成独立记忆条目。
- 每次新对话前，把相关条目注入上下文。
- **全局记忆 vs 助手私有记忆**：默认每个助手私有，开启全局后共享；避免角色之间串信息。
- **最近对话引用**：除离散记忆外，还能引用最近对话的摘要，提供更连续的上下文。
- **时间间隔提醒**：告诉模型“距上次互动过了多久”。
- 记忆可查看、手动新增、编辑、删除。

### 3.2 提示词注入 / 知识库

- **模式注入**：手动开关，启用后始终注入，适合写作风格、推理格式。
- **知识库**：一组带关键词的条目；关键词出现在最近消息中时自动注入。
- 条目支持：关键词、正则、大小写、扫描深度、始终激活。
- 注入位置：系统提示词前/后、对话顶部、对话底部、指定深度；同位置按优先级合并。
- 一个很实用的工程细节：**不在用户消息和紧随其后的工具调用 assistant 消息之间插入内容**，否则部分服务商（如 DeepSeek）推理会坏。

### 3.3 值得抄的

- 抽取用便宜模型、异步做，不阻塞主回复。
- 记忆作用域（全局/私有）必须是显式设置。
- 注入位置、优先级、扫描深度这些“小参数”决定了稳定性。
- 手动可编辑：用户/作者能纠正记忆，比纯自动更可信。

### 3.4 不足

- 记忆是扁平条目，没有时间线、实体关系、事实失效。
- 没有“事件账本”，无法回答“第 37 回合到底发生了什么”。
- 知识库仍以关键词为主，规模大了会退化。

参考：[RikkaHub 助手记忆](https://docs.rikka-ai.com/zh/assistants/memory)、[提示词注入与知识库](https://docs.rikka-ai.com/zh/assistants/prompt-injection)。

## 4. AI Dungeon Memory System

AI Dungeon 是“长叙事 + 有限上下文”的经典工程样本，它的做法非常值得直接映射到本项目的回合制叙事。

### 4.1 两层记忆

- **Memory**：每 6 个 action（玩家输入 + AI 回复）总结成一条信息密度更高的记忆；第一次在第 12 个 action 时总结最早的 6 个，之后每 6 个 action 一条。
- **Auto Summarization / Story Summary**：每 15 个 action 更新一次全局故事摘要；摘要过长会自动再压缩；摘要覆盖不到的部分由 Memories 填补。
- **Memory Bank**：对 Memories 做 embedding，检索 query 是“最近一个 action”，按相关度取若干条注入。

### 4.2 上下文预算

AI Dungeon 把上下文元素分成 Required 和 Dynamic：

- Required：Instructions、Plot Essentials、Story Summary、Author's Note、Front Memory、Last Action。总和超过上下文 70% 时按优先级裁剪。
- Dynamic：Story Cards、Memory Bank、Story History。剩余空间大约按 25% / 25% / 50% 分配（Memory Bank 关闭时 History 可到 75%）。
- 组装顺序：Instructions → Plot Essentials → Story Cards → Story Summary → Memory Bank → History → Author's Note → Last Action → Front Memory。

### 4.3 值得抄的

1. **压缩 + 检索两条腿**：摘要保全局，Memory Bank 保细节。
2. **固定节奏**：每 N 回合总结一次，简单可靠；不用每回合都调用 LLM。
3. **显式预算比例**：哪些东西必须留、哪些可裁，一开始就定好。
4. **Story Cards 按“最近 + 频率”排序**：不是纯语义相关。
5. **组装顺序固定**，便于调试和缓存。

### 4.4 不足

- 固定节奏会产生不必要的调用；对本项目应按回合数 + 重要性触发。
- 摘要会丢细节，必须保留原始事件账本作为“真相源”。
- 记忆是文本摘要，没有实体/关系/可见性。

参考：[AI Dungeon Memory System](https://help.aidungeon.com/faq/the-memory-system)、[Context 组成](https://help.aidungeon.com/faq/what-goes-into-the-context-sent-to-the-ai)。

## 5. Agent Memory 框架

### 5.1 Letta / MemGPT：分层 + 工具

- **Core memory**：常驻上下文、可被 Agent 自己编辑的记忆块（人设、用户偏好、当前目标）。
- **Recall memory**：对话历史，可搜索。
- **Archival memory**：外部向量存储，通过工具按需检索。
- Agent 通过工具读写记忆，形成“自我管理记忆”的闭环。

启示：给智能体 **显式的记忆工具**（查事件、查实体、查时间线），比每回合硬塞更灵活；核心记忆块要小而稳定。

### 5.2 Mem0：抽取 + 合并

- 写入分两步：LLM 从对话中抽取候选记忆；再与已有相似记忆比较，执行 `ADD / UPDATE / DELETE / NOOP`。
- 存储支持向量 + 图；检索支持元数据过滤、多租户。
- 重点不是“存”，而是 **合并、去重、失效**。

启示：长会话不能只 append；事实变更要作废旧事实，摘要要合并，重复记忆要折叠。

### 5.3 Zep / Graphiti：时序知识图谱

- **Episode**：原始输入（消息、文档、事件），作为 provenance。
- **Entity / Fact**：从 episode 抽取的实体和关系；关系边带 `valid_from` / `invalid_at`，新事实会失效旧事实而不是删除。
- **Community**：实体社区摘要，用于全局问题。
- 检索：语义 + BM25 + 图遍历混合，返回带来源的 context block。

启示：这正是本项目 `memory_entities` / `memory_relations` 应该走的方向；“曾经怎样/现在怎样”都能回答，前提是保留失效历史。

### 5.4 Palantir OAG / OG-RAG：本体增强

- OAG（Ontology-Augmented Generation）把 LLM 的上下文从“文档片段”换成“类型化对象 + 显式关系 + 可调用逻辑”。
- 典型能力：实体链接、同义词/等价类、property chain 多跳、规则校验。
- OG-RAG 论文表明，把文档组织成本体超图再检索，能提升事实性问题的准确率。

启示：对叙事世界，不需要 OWL 推理机，但需要 **轻量本体**：实体类型（人物/地点/物品/组织/概念）、规范名与别名、关系谓词、事件-实体关联。这样“提到老王 → 找到他的所有关系 → 找到相关事件”就是确定性的图查询，而不是向量碰运气。

### 5.5 GraphRAG / HippoRAG / A-MEM（简）

- GraphRAG：LLM 自动抽实体/关系/社区摘要，擅长全局性问题，但索引贵、抽取不稳定。
- HippoRAG：用知识图谱 + 个性化 PageRank 做多跳记忆检索。
- A-MEM：把记忆做成可链接的 Zettelkasten 笔记，支持记忆演化。

启示：图扩展和社区摘要可以作为 P2；先把“事件账本 + 实体关系 + 混合检索”做扎实。

参考：[Zep Concepts](https://help.getzep.com/concepts)、[Mem0 How it works](https://docs.mem0.ai/core-concepts/how-it-works)、[Letta Memory Blocks](https://docs.letta.com/v1-sdk/memory/memory-blocks/)、[Palantir RAG/OAG](https://blog.palantir.com/building-with-palantir-aip-logic-tools-for-rag-oag-fdaf8938d02e)、[OG-RAG (arXiv:2412.15235)](https://arxiv.org/abs/2412.15235)。

## 6. 横向对比

| 方案 | 背景注入 | 历史召回 | 信息查询 | 时间/事实 | 作用域隔离 | 预算控制 | 作者可控性 | 工程复杂度 |
|---|---|---|---|---|---|---|---|---|
| SillyTavern | 世界书：关键词/常驻/向量/递归 | 聊天历史 + 向量扩展 | 无原生查询工具 | 弱（靠条目） | 角色/聊天/Persona 绑定 | 强（比例/上限） | 极强 | 中 |
| RikkaHub | 模式注入 + 知识库 | 记忆条目 + 最近对话摘要 | 无 | 弱 | 全局/助手私有 | 弱 | 强 | 低 |
| AI Dungeon | Story Cards | Memories + Story Summary + Memory Bank | 无 | 弱 | 单局 | 强（显式比例） | 中 | 中 |
| Letta | Core memory blocks | Recall + archival | 记忆工具 | 中 | Agent 级 | 中 | 中 | 高 |
| Mem0 | 无（通用记忆） | 向量/图检索 | API | 中（更新/删除） | 多租户 | 弱 | 低 | 中 |
| Zep/Graphiti | 无（通用上下文） | 时序图 + 混合检索 | API | 强（双时间） | 图/用户 | 中 | 低 | 高 |
| OAG（企业） | 本体对象/关系 | 图遍历 + 向量 | 逻辑工具 | 强 | 强 | 强 | 高（需建模） | 很高 |

## 7. 对本项目的 10 条结论

1. **检索候选不能设天花板**：只取最新 200 条记忆，历史一长就永远看不到旧事；这是“无限记忆”的第一道硬伤。（本项目 2026-11-12 已改为读取本存档该角色的全部记忆。）
2. **建事件账本**：每回合至少落一条 canonical event，作为“发生过什么”的真相源；摘要和记忆都是它的投影。
3. **摘要分层**：回合事件 → 章节摘要 → 全局梗概；按回合数 + 重要性触发，不要每回合都调用 LLM。
4. **世界书条目化**：作者可编辑的 lore entry（关键词/常驻/顺序/深度/分组），比一整篇 `background_prompt` 稳定。
5. **统一注入点**：只保留一个 `oag_context`，`chat_history` 只放最近对话；现在同一份记忆发两遍，既费 token 又干扰模型。
6. **事实要带可见性**：`known_by` / witness；否则一个 NPC 的私密信息会进入所有角色上下文。
7. **查询工具必须有**：检索再强也覆盖不了“第 37 回合发生了什么”“老王现在住哪”；给模型 `search / entity / timeline` 三类工具。
8. **混合检索 + 预算**：关键词/BM25 + 向量 + 图邻接，RRF 融合；按来源配额和 token 预算裁剪，避免单一来源霸屏。
9. **不要上重型本体**：不引入 OWL/推理机/外部图数据库；用 SQLite 表 + 轻量实体关系即可，保持离线、可打包、可分支。
10. **可观测性先行**：每回合记录检索候选、选中项、token 数、耗时；否则无法调参，也无法解释“为什么它忘了”。

## 8. 参考链接

- SillyTavern World Info：https://docs.sillytavern.app/usage/core-concepts/worldinfo/
- 世界书参考手册（TavernWeave）：https://github.com/liarmttt/tavernweave
- RikkaHub 助手记忆：https://docs.rikka-ai.com/zh/assistants/memory
- RikkaHub 提示词注入：https://docs.rikka-ai.com/zh/assistants/prompt-injection
- AI Dungeon Memory System：https://help.aidungeon.com/faq/the-memory-system
- AI Dungeon Context：https://help.aidungeon.com/faq/what-goes-into-the-context-sent-to-the-ai
- Zep Concepts：https://help.getzep.com/concepts
- Mem0 How it works：https://docs.mem0.ai/core-concepts/how-it-works
- Letta Memory Blocks：https://docs.letta.com/v1-sdk/memory/memory-blocks/
- Palantir RAG/OAG：https://blog.palantir.com/building-with-palantir-aip-logic-tools-for-rag-oag-fdaf8938d02e
- OG-RAG：https://arxiv.org/abs/2412.15235
- OAG 中文实践（腾讯云）：https://cloud.tencent.cn/developer/article/2740300
