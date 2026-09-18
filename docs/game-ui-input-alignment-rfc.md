# 世界包可样式化（Stylability）— 核心契约与整改方案

状态：草案（RFC，框架核心思想）  
范围：全部 `gameUiRuntime` 注册组件、`theme.css`、`buildGameUiStylesheet` 生成基线、世界编辑器校验  
前置：`docs/world-package-guide-v3.md` 已写「世界包拥有游戏页面的结构和视觉设计」——本 RFC 把该句落成**可验收的技术契约**。

---

## 0. 核心思想（不变式）

> **世界包作者可以修改游戏页内一切视觉表现。**  
> 宿主只保证：结构可用、交互可用、安全边界不破；**不得锁死颜色、尺寸、间距、圆角、阴影、字体、动效、层级等可主题化属性。**

这是 v3「世界包拥有视觉」的工程含义，与下列能力同级：

| 能力 | 谁拥有 |
|---|---|
| 页面结构 / 布局树 | 世界包 JSONC |
| 视觉设计 | 世界包 stylesheet / tokens / custom_css |
| 可信能力与数据写入 | 宿主应用 |
| 安全沙箱 | 宿主 |

**违反可样式化的典型症状（均算框架 bug，不是作者问题）：**

1. 写了 CSS 不生效，或必须 `!important` + 深层选择器才生效  
2. 必须读源码 / 逆向 DOM 才能改某处外观  
3. `theme.css` 或生成基线用更高特异性压住作者规则  
4. 尺寸/颜色写死在宿主或生成 CSS，无变量、无锚点  
5. 文档未列出可覆盖选择器  

---

## 1. 目标 / 非目标

### 目标

- G1：**一切可主题化视觉**都有稳定锚点（class / `data-*`）和（若需要）CSS 变量  
- G2：作者 stylesheet 与 `custom_css` 在**同等或更合理特异性**下可覆盖宿主基线  
- G3：`theme.css` 不再用「触控/平台」选择器强制主题色与装饰尺寸  
- G4：生成基线（`buildGameUiStylesheet`）只提供**可被单 class 覆盖**的默认值  
- G5：指南 + catalog 提供**锚点表、变量表、覆盖示例**；校验器对无效选择器 warning  
- G6：改样式在会话内可感知（重进会话即可；编辑器预览一致）

### 非目标

- 不开放宿主应用壳（首页、设置、世界编辑器）的样式  
- 不允许世界包注入任意 JS 改 DOM  
- 不保证跨 major 版本内部类名永不变化（变更须写迁移说明）

---

## 2. 可样式化等级（组件契约）

每个注册组件标注等级，**新组件至少 S2**，聊天类核心组件目标 **S3**。

| 等级 | 含义 | 例子 |
|---|---|---|
| **S0 结构** | 布局由 JSONC 控制，装饰色可有可无 | 纯 `stack`/`grid` |
| **S1 Token** | 颜色/圆角/间距走 `--game-ui-token-*` 或 `--game-ui-*` | badge、chip |
| **S2 锚点** | 稳定 `class`/`data-component`，作者可写 CSS 改任意属性 | 多数组件 |
| **S3 内部契约** | 关键内部节点类名文档化；默认不锁 `max-width`/背景 | `input_composer`、`message_list`、`side_panel_tabs` |
| **S4 变量完备** | 所有写死 px/颜色均 `var()` + 默认值 | 目标态 |

**禁止：**

- 在 `theme.css` 或生成 CSS 中对 S2+ 组件使用 `!important` 锁定视觉（error/disabled 可读性除外，且须用变量）  
- 对 S2+ 组件使用「无锚点的深层 DOM 选择器」作为唯一修改入口  
- 在 React 组件内写死主题色/大尺寸（应走 class 或 CSS 变量；交互态可 inline **只读**布局如拖拽 `top`）

---

## 3. 横切规则

### 3.1 特异性阶梯

| 来源 | 期望特异性 | 说明 |
|---|---|---|
| `theme.css` | 低（单 class / 属性） | 只设默认与 a11y 最小尺寸 |
| 生成基线 | `[data-game-ui-scope] .game-*` | 可被作者同 scope 或更高一级覆盖 |
| 世界 `custom_css` | scope 前缀 + 作者类 | 鼓励 `& .my-x` |
| 世界 v3 stylesheet | **无 scope 前缀**，最后注入 | 同特异性时最后写赢 |

规则：**平台/触控 media 不得提高视觉强制力**；只允许调整 hit-area 最小值（并用变量）。

### 3.2 颜色与装饰

- 主题色：`--game-ui-token-*` 或 `--game-ui-*-bg/fg/border`  
- `background: transparent` 等默认可被变量替换，禁止写死「永远透明」  
- 渐变、阴影、圆角：提供默认，但作者可覆盖；默认值不得依赖「必须改内部伪元素才可见」

### 3.3 尺寸与布局

- 内容容器默认：`width:100%; max-width:none`（上限交给作者）  
- 装饰尺寸（滚动钮、地图节点、头像）一律 `var(--game-ui-*, 默认px)`  
- 节点 JSONC 的 `style` inline 优先于 CSS（作者自己声明的，合法）

### 3.4 动效与状态

- `transition` / `animation` 可被 `animation: none` 或自定义覆盖  
- hover/active 不得锁死唯一视觉；可用 `:where()` 降低特异性  
- disabled：保持可读，颜色可用变量，不禁止作者改透明度以外的装饰

### 3.5 滚动与溢出

- 滚动容器 `overflow` 默认合理，作者可改 `auto/hidden/visible`  
- 不得在 wrapper 上锁 `overflow: hidden` 导致作者子元素裁切且无法解除（除非文档写明并给类）

---

## 4. 审计清单（当前实现）

### 4.1 高优先级（直接违反「作者可改」）

| ID | 组件/规则 | 违反点 | 整改 |
|---|---|---|---|
| A1 | `input_composer` | 内层宽度/padding 无单一入口；改 wrapper 常无效 | S3：内层 `width:100%`；锚点 `[data-component]` / `.game-textarea`；变量 `--game-ui-input-max-width` |
| A2 | 消息操作按钮 `theme.css` | 触控规则强制 `background:transparent`、`min-height:24px`，高特异性 | 改 `var(--game-ui-action-btn-*)`；降选择器层级 |
| A3 | `game-chat-scroll-btn` | 写死 36/34px | `--game-ui-scroll-btn-size` |
| A4 | 生成 `.game-typing-bubble` | 写死 `max-width:80px` + scope 特异性 | 变量化；允许单 class 覆盖 |
| A5 | 生成 `.game-avatar` | 写死 256×320 | 变量 `--game-ui-avatar-w/h` |
| A6 | `message_list` 气泡 | 无宿主统一 max-width 变量，各包手写 | `--game-ui-message-max-width` |

### 4.2 中优先级

| ID | 项 | 整改 |
|---|---|---|
| B1 | 地图 canvas `min-height:260px`、node `width:150px` | 变量化 |
| B2 | `game-status-handle` inline 拖拽 | 尺寸/颜色仍走 class；拖动后 inline 接管 top、停靠边与圆角（`--left` 变体 + localStorage 记忆），文档已同步 |
| B3 | `floating_actions` 按钮尺寸散落 | `--game-ui-fab-size` |
| B4 | `game-mobile-error-retry` 写死红底 | `--game-ui-error-bg/fg` |
| B5 | `game-session-diagnostic` 280px | 变量（低影响） |
| B6 | 三套 composer DOM | 抽共享结构，避免预览/会话不一致 |

### 4.3 文档与工具缺口

| ID | 缺口 | 整改 |
|---|---|---|
| C1 | 无完整锚点/变量总表 | 指南 + 本 RFC 附录同步维护 |
| C2 | 无效选择器无提示 | bundle/stylesheet lint |
| C3 | 预览 computed width 不可见 | 世界工坊调试层 |
| C4 | 改 stylesheet 需重进会话 | 文档醒目提示；后续热注入 |

---

## 5. 组件覆盖模板（新增组件 checklist）

实现任何新注册组件时必须：

1. [ ] 根节点：`game-ui-component` + `data-component="<id>"` + 作者 `class_name`  
2. [ ] 关键内部节点：稳定类名（写入指南「锚点表」）  
3. [ ] 所有主题色/尺寸：`var(--game-ui-*, fallback)` 或可覆盖 class  
4. [ ] 内容容器：`max-width: none`，不用写死像素上限  
5. [ ] `theme.css` 仅 a11y 最小尺寸 + 默认 token  
6. [ ] 不写视觉 `!important`  
7. [ ] 预览器与 shell / iframe DOM 一致  
8. [ ] 示例：一段能改颜色/宽度的 CSS 通过视觉验收  

---

## 6. 变量命名（扩展）

```text
--game-ui-chat-gutter-x / -y
--game-ui-message-max-width
--game-ui-input-max-width
--game-ui-textarea-min-height
--game-ui-action-btn-min-h / -bg / -fg
--game-ui-scroll-btn-size
--game-ui-typing-max-width
--game-ui-avatar-w / -h
--game-ui-map-min-height / --game-ui-map-node-w
--game-ui-fab-size
--game-ui-error-bg / -fg
--game-ui-status-handle-size
```

同时继续使用文档 `tokens` → `--game-ui-token-*`（颜色、圆角、字体）。  
约定：`--game-ui-*` 管宿主组件几何/状态；`--game-ui-token-*` 管作者主题。

---

## 7. 实施阶段

### Phase 0 — 契约写入（已完成）

- 指南「核心原则」增加可样式化不变式  
- 指南「注册组件」增加等级与锚点列（可后续自动从 catalog 生成）  
- 禁止在 PR 中对 S2+ 组件增加视觉 `!important`

### Phase A — 聊天链路可改（进行中 / 首批落地）

已改（`theme.css` + `parser.ts`）：

- `.game-root` 注入 `--game-ui-*` 默认变量  
- `[data-component="input_composer"]` 内层默认 `width:100%; max-width` 走变量  
- 消息气泡 `--game-ui-message-max-width`  
- 消息操作按钮：`--game-ui-action-btn-bg/fg/min-h`，触控块不再写死透明底  
- 滚动钮 `--game-ui-scroll-btn-size`  
- typing / avatar 生成基线改 `var(--game-ui-typing-max-width)` / `--game-ui-avatar-w/h`  

健康生活包已用变量 + `[data-component]` 演示覆盖。

验收见 §8.1（需 `npm run tauri:dev` 或重建前端后重进会话）。

### Phase B — Token 化生成基线与 theme（已完成）

- 地图 `min-height` / 节点宽：`--game-ui-map-min-height` / `--game-ui-map-node-w`  
- 错误条/重试钮：`--game-ui-error-accent/bg/fg/btn-*`  
- 诊断条：`--game-ui-diagnostic-width` / `-min-h`  
- floating_actions：`--game-ui-fab-size`  
- 侧栏把手：`--game-ui-status-handle-w/h`  

### Phase C — 工具（已完成首批）

- bundle 校验：stylesheet 引用未知 `game-*` 类 → `unknown_game_class` warning（`game_ui/mod.rs`）  
- 世界工坊预览：`GameUiPreview` 增加 `showStyleDebug`，展示关键变量与 textarea 计算宽度  
- 编辑器预览默认开启 style debug  

后续可选：dev 热更新 stylesheet、catalog 自动生成锚点表。

---

## 8. 验收标准

### 8.1 聊天链路（Phase A）

- [x] `[data-component="input_composer"] .game-textarea { max-width: 400px }` 输入框变窄（内层默认可撑开）  
- [x] `--game-ui-message-max-width: 520px` 气泡变窄  
- [x] `--game-ui-action-btn-bg: #b436f2` 分支/复制按钮变紫（含触控 media）  
- [x] `--game-ui-scroll-btn-size: 28px` 滚动钮变小  
- [x] `--game-ui-typing-max-width: 120px` typing 气泡变宽  
- [x] 仅改 `custom_css`、不改源码即可完成以上  

### 8.2 回归

- [ ] stock-analyst / narrative-rpg / healthy-life 视觉无破坏性回归（需真机/会话目视）  
- [x] 未设变量时默认与现状一致（fallback 与旧字面量相同）  

### 8.3 文档与工具

- [x] 指南含变量表 + 覆盖示例  
- [x] RFC 与 catalog comment 同步  
- [x] 未知 `game-*` 类 warning（`unknown_game_class`）  
- [x] 预览 style debug（变量 + textarea 计算宽）  

---

## 9. 决策记录

| 问题 | 决策 |
|---|---|
| 是否强制输入框=聊天区宽 | **否**。只提供变量/默认，不锁死 |
| 是否允许 theme.css 用 !important | **禁止**用于可主题化视觉 |
| 内部类名是否公开 | **是**，S3 组件必须写进指南 |
| 改不了时算谁的问题 | **算框架**，除非作者选择器明显错误 |
| 是否允许 inline style | 仅限运行时布局（拖拽位置）；主题视觉用 class |

---

## 10. 附录 A — 快速覆盖示例（目标态）

```css
/* 输入区改宽 */
[data-component="input_composer"] .game-textarea {
  max-width: 480px;
}

/* 消息按钮主题色 */
.game-root {
  --game-ui-action-btn-bg: #b436f2;
  --game-ui-action-btn-fg: #fff;
  --game-ui-action-btn-min-h: 32px;
}

/* 滚动钮 */
.game-root { --game-ui-scroll-btn-size: 28px; }

/* 头像 */
.game-root { --game-ui-avatar-w: 200px; --game-ui-avatar-h: 280px; }
```

## 11. 附录 B — 与仓库现状的差距（摘要）

见 §4。优先序：A1 输入区 → A2 按钮 theme → A3/A4/A5/A6 → B* → C*。

---

**一句话：**  
可样式化不是「美化技巧」，而是 v3 **世界包拥有视觉** 的可执行标准；任何「作者改不了」都按框架缺陷处理。
