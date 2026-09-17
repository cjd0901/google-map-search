# 迎风数据（yingfeng-data）

![应用图标](./assets/branding/map-leads-icon-source.png)

迎风数据是一款基于 Tauri 2、React 和 Rust 的桌面工具，用于按关键词与地区检索商家、整理公开商家资料，并从商家官网发现公开联系邮箱。

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

开发环境需要安装 Node.js。macOS 首次构建时会下载并内置专用的无头浏览器，使后台采集不在程序坞显示 Chrome 图标；若该浏览器不可用，仍会回退到本机的 Microsoft Edge 或 Google Chrome。

授权服务地址默认是 `https://wa.sililand.com:39128/gs`，实际请求会访问 `https://wa.sililand.com:39128/gs/api/v1/...`。如需连接其他服务地址，可在构建前设置 `VITE_SERVER_URL`。

应用不会明文保存激活卡密，而是将服务端授权所绑定的设备标识持久化到应用数据数据库。覆盖安装应用会保留授权；手动删除应用数据目录会同时清除任务数据和设备标识。

正式安装包已内置 Node.js 运行时和 `playwright-core`。macOS 安装包还内置专用的无头浏览器，最终用户无需安装任何开发语言、数据库或额外浏览器；其他平台需要 Microsoft Edge 或 Google Chrome。

## 自动打包

GitHub Actions 支持手动运行，也会在推送 `v*` 版本标签时自动构建：

- Windows x64：NSIS 安装程序和 MSI 安装包。
- macOS Apple Silicon：ARM64 应用和 DMG。
- macOS Intel：x64 应用和 DMG。

构建完成后，可在对应 Actions 运行记录的 Artifacts 区域下载。macOS 构建默认使用临时签名；面向外部正式分发时，应配置 Apple Developer 签名证书并完成公证。

推送 `v*` 标签时，工作流还可以将安装包自动发布到单独的公有仓库。需要在私有源码仓库的 `Settings > Secrets and variables > Actions` 中配置：

- Secret `PUBLIC_RELEASE_TOKEN`：只授权公有下载仓库且具有 `Contents: Read and write` 权限的 Fine-grained PAT。
- Variable `PUBLIC_RELEASE_REPO`：公有仓库的完整名称，例如 `cjd0901/yingfeng-data-releases`。

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
