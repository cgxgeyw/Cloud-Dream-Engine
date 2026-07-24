# 记账助手世界包

这是一个单智能体、无模型调用的世界包。界面只使用宿主注册的 `ledger_book` 可信组件，不包含 `input_composer`，因此录入、编辑、删除、筛选和统计都不会进入模型回合。

## 打包与导入

在本目录内将 `manifest.json`、`world/` 和 `characters/` 放到 ZIP 根目录，生成的 ZIP 可从应用的“导入世界包”入口导入。仓库构建产物位于 `output/accounting-assistant-world.zip`。

该世界包要求客户端支持：

- 世界包格式 version 7
- UI runtime version 3
- `ledger_book` 注册组件
- `storage.collections.ledger.entries` 声明账单集合及字段约束
- `supports_world_storage` 通用宿主存储桥
- `supports_world_records` 仅保留为旧版 runtime 的兼容性能力标记

## 数据语义

- 每笔金额以整数“分”保存，避免浮点金额误差。
- 账单按导入后生成的 `world_id` 和 `ledger.entries` collection 隔离。
- 账单不写入世界包，也不随存档分支复制；删除单个游戏存档不会删除账本。
- 删除整个世界时，对应账单由宿主数据库外键级联清理。
- 导出世界包只导出定义和界面，不导出用户账单。

世界包作者不需要、也不应获得 SQLite、任意 SQL、文件系统或通用 Tauri 命令权限。世界包声明存储结构后，只能通过可信组件、通用存储 action 或沙箱逻辑 SDK 请求宿主执行类型化操作。
