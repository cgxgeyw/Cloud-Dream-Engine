# OAG 文档

OAG（Ontology-Augmented Generation）系统的调研、隔离审计与本体设计。

| 文档 | 内容 |
|---|---|
| [01-market-research.md](./01-market-research.md) | 酒馆 / RikkaHub / AI Dungeon / Letta / Mem0 / Zep / Palantir OAG 的做法与取舍 |
| [02-isolation-audit.md](./02-isolation-audit.md) | 当前记忆隔离的代码级审计：哪些成立、哪些在读路径失效 |
| [03-ontology-design.md](./03-ontology-design.md) | 对象/属性/关系/知识边界/逻辑查询的 OAG 本体设计 |
| [04-npc-prompt-blueprint.md](./04-npc-prompt-blueprint.md) | NPC 提示词蓝图：真实感需要哪些层、现有 prompt 缺什么、示例与评测 |
| [05-oag-build-and-maintain.md](./05-oag-build-and-maintain.md) | OAG 怎么搭建、怎么维护：模块、阶段、写入/读取、Canon/运行时/结构/运维 |

## 三句话版本

1. **OAG 不是“世界书 + 事件日志 + 搜索”**：它是类型化对象 + 属性 + 关系 + 事件 + 知识边界 + 逻辑查询。
2. **隔离不是过滤结果，而是先裁剪可见子图**：每个 actor 只能在自己的“场景围栏 + 知情围栏”内做遍历、聚合、否定。
3. **记忆是主观投影，事件是客观账本**：角色的 `memories` 只属于自己；事实是带 `asserted_by` 的 claim，不是绝对真理。

## 当前状态（2026-11-12）

- 记忆模式：世界编辑器 → 基础权限 → **记忆模式**，默认 `历史 + Embedding`；可选 `历史 + OAG（开发中）`。
- 两种模式都会发送按消息 `witnesses` 过滤的最近 N 轮对话；切换只改变额外注入层。
- Embedding：历史 + 现有向量/词法记忆召回；召回对象是固定桶总结，不再直接插入原始台词。
- 固定桶总结：每 `memory_summary_interval_rounds` 回合（默认 10）一个桶（1-10、11-20……），桶内按场景拆条；只召回已经离开原文窗口、且所在桶已有总结的桶。原文窗口按桶对齐，避免边界重复或空洞。
- 总结提示词：角色用 `character_memory_summary_prompt`，主控全局总结用 `director_memory_summary_prompt`；世界编辑器可编辑，留空用内置默认，支持 `{{character_name}}`、`{{transcript}}` 等占位符。
- OAG：历史 + 结构化事实/关系；结构化层尚未接入，暂按 Embedding 运行；事实抽取默认关闭，且必须独立于角色回复。
- 角色与导演的历史轮数默认 30；不设 token 预算/截断，低上下文模型通过降低轮数解决；已存在角色保留原值。

## 当前代码里最要紧的四件事

1. **记忆候选不再截断**：`prepare_character_recall` 和 `memory_pool` 读取本存档该角色的全部 `memories`；生产召回只取 `memory_type='summary'`，未总结的桶用旧对话兜底。
2. **`memory_context` 重复注入已修**：`chat_history` 现在只发最近对话；原始对话仍写入 `memories` 作为总结来源和迁移兜底，但生产召回只取 `memory_type='summary'`。`build_turn_entries()` 仍按回合末名册写记忆，场景切换回合的归属还没修。
3. **最近对话已按 `witnesses` 过滤**：新消息有观众；旧消息没有 `witnesses` 时仍按可见处理。
4. **事实和知识库没有 actor**：`memory_relations` 按 session 存；`search_knowledge` 按世界查；角色会开天眼。

## 建议的下一步

先做 **M0 隔离基础**（消息级 witnesses、每回合观众、读路径过滤、事实带 speaker），再做 **M1 本体核心 + 查询**。

原因：没有知识边界，OAG 的多跳查询只会更高效地泄露秘密。

验收标准见 [02-isolation-audit.md §8](./02-isolation-audit.md) 和 [03-ontology-design.md §12](./03-ontology-design.md)。
