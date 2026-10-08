# 健康生活世界包

单智能体健康管理世界：与「健康小助手」对话，四个页签「对话 / 记录 / 计划 / 我的」。

## 功能

- **对话**：饮食、训练、睡眠；教练用 `session_attribute_updates` 维护今日摄入/消耗与身体档案。
- **记录**：今日能量环 + 摄入/消耗；「同步统计」把今日写入 `health.energy_days`；日/周/月柱状图与每日明细。
- **计划**：今日重点 + 今日行动清单（确认完成会 `submit_message` 给教练）。
- **我的**：只读身高体重与 BMI/BMR。

## 数据

- 今日实时数字：`health_*` session 属性（模型写回）。
- 历史统计：世界存储集合 `health.energy_days`（`storage.records` + `logic.js` 的 `health.syncStats` / `health.loadStats` / `health.selectDay`）。
- 页签切换：`set_state`（`active_tab` = chat / records / plan / profile）。
- 柱状图：周=近 7 日，月=本月每一天，日=每日明细列表。

## 依赖

- 世界包 format 8，UI runtime 3
- `supports_world_records` + `supports_world_storage`
- `sandbox-js-v1` logic（`timeout_ms` 2000）：`health.syncStats` / `health.loadStats` / `health.selectDay` 全在包内 `logic.js`
- 宿主仅内置通用 `set_state` 与 `ui.setTab`（chat/records/plan/profile；兼容 growth/mine 别名）

## 打包

```powershell
cd E:\code\rustweb
python scripts/pack_healthy_life.py
```

产物：`output/healthy-life-world.zip`

浏览器预览（静态示意，非游戏运行时）：打开仓库根目录 `index.html`。
