# 工具包（MCP Tool Packs）

工具包是可在「MCP 工具」页面导入/导出的 JSON 文件，用来分发工具定义。
格式：`{"format": "dream-mcp-tools", "version": 1, "tools": [...]}`。

## 使用

1. 打开应用的「MCP 工具」页面，点「导入工具」，选择本目录下的 JSON 文件。
2. 在需要用到这些工具的世界里，把工具 id 加进世界 `director_config.allowed_mcp_tool_ids`
   （世界编辑器 → 主控配置）。未授权的工具不会出现在智能体的工具列表里，也无法被调用。
3. 「导出工具」可把你当前配置的工具（引擎内置工具除外）导出成同样的格式分享或备份。

## stock-market-tools.json

A 股数据工具包，全部走「本地 HTTP」实现（`builtin_http`），无需外部 MCP server，
桌面端与 Android 均可用：

| 工具 id | tool_name | 数据来源 | 用途 |
|---|---|---|---|
| mcp-tool-stock-quote | stock_quote | 腾讯财经 qt.gtimg.cn | 个股实时行情 |
| mcp-tool-stock-kline-daily | stock_kline_daily | 东方财富 push2his | 日 K 线历史 |
| mcp-tool-stock-kline-daily-sina | stock_kline_daily_sina | 新浪财经 quotes.sina.cn | 日 K 线备用源（东财失败时用） |
| mcp-tool-stock-announcements | stock_announcements | 东方财富 datacenter | 公司公告列表 |
| mcp-tool-market-news | market_news | 新浪财经滚动新闻 | 财经快讯 |

数据均为公开免费接口，仅供参考，不构成投资建议。接口字段与可用性可能随上游调整变化。

## 本地 HTTP 工具配置（impl_config）

`builtin_http` 工具由应用核心直接执行，两种模式：

- `{"mode": "generic"}`：模型调用时自由传 `url/method/headers/params/body`；
  可用 `"allowed_hosts": ["example.com"]` 限制可访问域名。
- `{"mode": "template", ...}`：请求在配置里写死，入参只做占位替换：
  - `method` / `url` / `query` / `headers`：字符串中的 `{{参数名}}` 会被工具入参替换（URL 编码）。
  - `charset`：响应编码，`gbk` 或缺省 UTF-8。
  - `extractor`：
    - `{"type": "text"}` 返回原文（默认）；
    - `{"type": "json", "path": "data.klines"}` 解析 JSON 并按点分路径取值；
      可加 `"tail": 60`（或 `"{{参数名}}"`）只保留数组末尾 N 条（最大 320）——
      时间升序接口被结果大小上限截断时会丢掉最新数据，截尾可兜底；
    - `{"type": "split_map", "pattern": "...", "delimiter": "~", "fields": {"1": "名称"}}`
      正则抓一段后按分隔符切开，把指定下标映射成字段名。
