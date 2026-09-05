# 叙事 RPG 起步包：甲子风云

这是一个用于复制和改写的结构参考，不是完整商业内容。故事从中平元年黄巾起义开始，玩家扮演原创乡民“阿宁”，避免把整部历史和所有名将一次性塞进开场。

示例重点是可执行闭环：

1. `director_interaction_kinds` 允许世界主控生成 `choice`。
2. 世界主控在路线未选择时输出三个固定 ID 的选项。
3. 宿主把玩家点击的选项标签写成真实玩家消息并自动启动下一回合。
4. 世界主控从 `current_state.runtime_attributes` 读取当前状态，用 `character_attribute_updates` 写入路线、体力、气血和物资变化。
5. `side_panel_tabs` 直接展示同一份权威属性和会话背包，不建立 KV 镜像。

包中没有虚构资源路径。正式世界应在 `assets/` 放入真实背景和立绘，并逐项写入 `manifest.assets`。

打包时进入本目录，将 `manifest.json`、`world/` 和 `characters/` 直接压到 ZIP 根目录。
