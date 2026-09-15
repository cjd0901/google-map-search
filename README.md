# Google Maps 商家信息采集工具

![应用图标](./assets/branding/map-leads-icon-source.png)

基于 Tauri 2、React 和 Rust 的桌面工具，用于按关键词与地区检索商家、整理公开商家资料，并从商家官网发现公开联系邮箱。

当前已实现第一个可运行 MVP：

- Playwright 驱动本机 Edge/Chrome 搜索 Google Maps。
- 采集名称、分类、地址、电话、官网、评分和 Maps 链接。
- Rust 官网采集器发现公开邮箱、业务摘要，并保留官网中的 Facebook 主页链接（只保存链接，不访问 Facebook 抓邮箱）。
- SQLite 持久化、任务暂停/续跑和实时进度。
- 可选择多个历史任务，合并、去重并导出 UTF-8 CSV；重复记录会合并邮箱与来源。
- 可上传导出的 CSV 批量补抓官网公开邮箱，并生成新的结果 CSV。
- 官网请求包含私网地址拦截、robots 检查、限速和页面大小限制。

## 开发运行

```bash
pnpm install
pnpm tauri dev
```

开发环境需要安装 Node.js，以及 Microsoft Edge 或 Google Chrome。采集任务固定使用简体中文并默认在后台运行。

授权服务地址默认是 `https://wa.sililand.com:39128/gs`，实际请求会访问 `https://wa.sililand.com:39128/gs/api/v1/...`。如需连接其他服务地址，可在构建前设置 `VITE_SERVER_URL`。

正式安装包已内置 Node.js 运行时和 `playwright-core`，最终用户无需安装任何开发语言或数据库；电脑只需装有 Microsoft Edge 或 Google Chrome。

## 代码结构

- `src/components`：无数据访问职责的界面组件。
- `src/hooks/useLeadCollection.ts`：任务列表、当前任务、实时事件及用户操作的状态编排。
- `src/services/searchService.ts`：前端访问 Tauri 命令与事件的唯一入口。
- `src/domain`：前端领域类型和任务状态规则。
- `src-tauri/src/commands.rs`：桌面命令入口与任务生命周期编排。
- `src-tauri/src/export.rs`：CSV 生成、跨任务合并与商家去重。
- `src-tauri/src/state.rs`：应用共享运行状态。
- `src-tauri/src/db.rs`、`crawler.rs`、`website.rs`：分别负责持久化、地图采集进程和官网信息采集。

详细设计与迭代范围见 [PROJECT_PLAN.md](./PROJECT_PLAN.md)。
