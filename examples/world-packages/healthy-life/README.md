# 健康生活世界包

单智能体健康管理世界：与「健康小助手」对话，在底部三个 Tab 中切换「对话 / 成长 / 我的」。

## 功能

- **对话**：饮食、训练、睡眠、习惯建议；教练在此维护身体与目标属性。
- **成长**：只读展示目标、净能量、摄入/消耗；无触发模型的按钮。
- **我的**：只读展示身高体重与 BMI/BMR；无触发模型的按钮。

## 界面

- UI runtime 3，schema 2
- 冷灰纸面轻拟物：白面板浮起，橙 / 绿 / 紫仅作数据点缀，发送键蓝色
- 底部三 Tab 通过 `logic.js` 的 `ui.setTab` + `result_state` 切换
- 桌面：主舞台 + 右侧属性栏；移动：全宽面板 + 底部 Tab + 状态抽屉

## 打包与导入

在本目录内将 `manifest.json`、`world/`、`characters/` 放到 ZIP 根目录：

```powershell
cd examples/world-packages/healthy-life
tar -a -c -f ..\..\..\output\healthy-life-world.zip *
```

从应用「导入世界包」导入即可。

## 依赖

- 世界包格式 version 8
- UI runtime version 3
- `supports_world_storage` + `sandbox-js-v1`（Tab 切换）
- `agent_chat` 单智能体模式

## 数据说明

- 身体与目标数据使用 `health_*` session 属性，由教练通过 `session_attribute_updates` 写回。
- 不提供医疗诊断；伤病、极端饮食等情况会引导就医。
- 导出世界包不包含用户数据。
