# 叙事 RPG 起步包：甲子风云

这是一个用于复制和改写的结构参考，不是完整商业内容。故事从中平元年黄巾起义开始，玩家扮演原创乡民“阿宁”，避免把整部历史和所有名将一次性塞进开场。

示例重点是可执行闭环：

1. `message_interaction_kinds` 允许角色生成 `choice`。
2. 角色在路线未选择时要求模型输出三个固定 ID 的选项。
3. `interaction_answered` 调用 `starter.interactionAnswered`。
4. logic.js 将回答写入 session 级 `variables.route`。
5. Prompt 模块通过 `{{var:route}}` 读取路线，约束后续主控和角色回复。

包中没有虚构资源路径。正式世界应在 `assets/` 放入真实背景和立绘，并逐项写入 `manifest.assets`。

打包时进入本目录，将 `manifest.json`、`world/` 和 `characters/` 直接压到 ZIP 根目录。
