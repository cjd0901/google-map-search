# Google Maps 商家信息采集工具方案

## 0. 当前实现路线

根据当前产品选择，MVP 已改为本地浏览器爬虫路线：由 Playwright Core 驱动用户电脑上的 Edge/Chrome，滚动 Google Maps 结果列表并逐店读取详情；Rust 后端负责 SQLite、任务控制、官网邮箱采集和 CSV 导出。Places API 适配器保留为未来可选能力，不是当前运行链路。

## 1. 项目目标

开发一个 Tauri 桌面工具。用户输入关键词和目标地区后，应用查找匹配商家，展示名称、官网、电话、地址、主营业务等资料，再访问商家官网寻找公开联系邮箱，并提供任务进度、筛选、去重和合规导出能力。

## 2. 必须先明确的数据合规边界

Google Maps Platform 当前条款明确禁止抓取、批量下载、在服务之外导出或长期保存 Google Maps 内容；Places API 政策也明确指出，除少数例外外不得预取、缓存或存储其内容。`place_id` 可以长期保存，经纬度通常只允许临时缓存，其他字段应按当时适用的条款处理。

因此不建议把产品实现为“直接爬 Google Maps 页面并永久保存/导出所有商家资料”，也不实现验证码绕过、账号风控规避或登录态窃取。生产方案采用可替换的数据源层，区分两种运行模式：

### 模式 A：本地浏览器采集（当前 MVP）

- 使用 Playwright Core 驱动本机 Edge/Chrome，读取用户在浏览器中能够公开访问的结果。
- 保持单浏览器、低并发；遇到验证码时暂停并由用户手动完成，不提供绕过功能。
- 页面选择器集中在独立采集器中，Google Maps 页面变化时可以单独维护。
- 数据保存在用户本机 SQLite；是否允许保存和导出仍由使用者根据适用条款和用途确认。

### 模式 B：Google Places 参考模式（可选）

- 使用官方 Places API (New) Text Search，而不是解析 Google Maps 网页。
- Google 返回的字段只在合规范围内展示、归因和临时缓存。
- 永久保存 `place_id`；其他 Google 字段的缓存周期、显示和删除策略做成配置并以正式上线时的条款为准。
- 不把受限制的 Google 内容直接作为永久商家库或批量导出内容。
- 单次 Text Search 当前最多返回 60 条结果，并且该上限可能变化。

### 模式 C：可持久化数据源模式（生产推荐）

- 接入明确允许本地存储与导出的商业数据 API、客户自有数据、授权数据集或允许再利用的公开数据源。
- 商家官网中的公开信息独立标记为“官网来源”，按网站条款、`robots.txt` 和适用法律采集。
- CSV/XLSX 导出默认只包含允许导出的字段，并保留来源与采集时间。

如果“必须大批量导出 Google Maps 商家名称、地址和电话”是硬需求，编码前应先取得对应授权或选择许可覆盖这一用途的数据供应商。数据源适配器会让这项替换不影响界面、任务系统和官网邮箱模块。

## 3. 推荐的数据链路

1. 用户创建任务，填写关键词、国家/城市/区域和结果上限；采集固定使用简体中文。
2. 数据源适配器检索候选商家，逐页返回标准化记录。
3. 使用数据源原始 ID 去重；Google 模式使用 `place_id`。
4. 可持久化字段即时写入 SQLite；临时字段写入带过期时间的缓存。
5. 对存在官网的商家创建邮箱发现子任务。
6. 官网采集器访问限定页面，提取业务介绍和公开邮箱。
7. 用户筛选、人工复核、删除或标记记录。
8. 导出模块根据数据来源许可生成 CSV；后续可增加 XLSX 和 JSON。

任务支持暂停、继续、取消、失败重试和断点续跑。界面持续显示“已检索、已补全、官网处理中、成功、失败”的数量和当前阶段。

## 4. 字段与来源拆分

### 商家检索字段

- `source`、`source_record_id`、`source_terms_version`
- `name`：商家名称
- `primary_type`、`types`：主营分类与其他分类
- `business_status`：营业状态
- `formatted_address`：完整地址
- `country`、`region`、`city`、`postal_code`
- `latitude`、`longitude`
- `phone`、`international_phone`
- `website_url`、`domain`
- `rating`、`review_count`
- `source_url`
- `fetched_at`、`expires_at`、`export_allowed`

### 官网补充字段

- `business_summary`：根据官网标题、描述及 About 页面整理的主营业务摘要
- `emails`：去重后的公开邮箱列表
- `email_source_urls`：邮箱对应的来源页面
- `email_confidence`：邮箱可信度
- `website_status`：可访问、无邮箱、超时、被拒绝、需要浏览器渲染等
- `website_scanned_at`

每个字段都记录来源和许可状态，不把 Google 分类、官网正文和人工编辑结果混为一个不可追溯的字段。

## 5. 技术架构

### 前端：React + TypeScript

- 新建任务：关键词、地区、数量和数据源。
- 任务进度：阶段、速率、成功/失败数量、暂停与取消。
- 商家结果：表格、搜索、筛选、排序、列显示和详情抽屉。
- 导出预览：明确标记不可导出字段和原因。
- 设置：API Key、并发数、超时、官网抓取范围和数据目录。

前端只负责交互和展示，不直接保存 API Key，也不直接执行跨域抓取。

### Tauri/Rust 后端

- `commands`：提供给前端的 Tauri 命令。
- `providers`：统一数据源接口及各数据源适配器。
- `providers/google_places`：Places API 参考模式。
- `crawler`：官网请求、链接发现、正文和邮箱提取。
- `jobs`：任务队列、并发控制、取消、重试和断点续跑。
- `storage`：SQLite、迁移、缓存过期和数据清理。
- `export`：按字段许可导出 CSV/XLSX/JSON。
- `security`：密钥存储、URL 校验、私网地址拦截。

建议的 Rust 依赖方向：

- HTTP 与异步：`reqwest`、`tokio`
- HTML 解析：`scraper`、`url`
- 数据库：`sqlx` + SQLite
- 序列化：`serde`、`serde_json`
- 错误与日志：`thiserror`、`tracing`
- CSV：`csv`

## 6. 本地数据模型

SQLite 至少包含：

- `search_jobs`：查询参数、状态、游标、计数、错误和时间。
- `businesses`：允许持久化的商家字段、规范化值和去重键。
- `source_snapshots`：临时数据、来源、归因、许可和过期时间。
- `business_emails`：邮箱、来源 URL、置信度和发现时间。
- `crawl_pages`：访问状态、HTTP 状态、错误和内容摘要哈希。

唯一约束优先使用来源 ID；缺少来源 ID 时再使用“规范化域名 + 电话 + 地址”的组合去重。后台清理任务会自动删除过期的受限缓存。

## 7. 可选的 Google Places 适配器

- 调用 Text Search (New) 的 `places:searchText`。
- 强制指定 Field Mask，只请求界面实际需要的字段，控制费用和响应体积。
- 基础字段如名称、地址、类型属于 Pro 层级；电话和官网等字段会触发 Enterprise 层级计费，实际开发时按最新价格表核算。
- 保存 `place_id` 以便以后刷新；不使用相同查询结果稳定不变的假设。
- 提供 Google 要求的归因，并在发布版补充用户条款和隐私政策。
- 对 EEA 账单主体启用单独的条款提示和功能开关。

## 8. 官网邮箱发现策略

1. 规范化官网 URL，仅允许 HTTP/HTTPS，并拒绝本机、内网和保留地址，避免 SSRF。
2. 检查 `robots.txt`，设置明确的 User-Agent、连接超时、响应大小上限和每域名限速。
3. 先解析首页中的 `mailto:`、可见文本、结构化数据和页脚。
4. 只跟进高价值同域链接，例如 Contact、About、Impressum、Legal、Team。
5. 默认最大深度 2、最多 10 个 HTML 页面，不下载大文件或媒体资源。
6. 对邮箱做语法校验、规范化、去重和常见无效地址过滤。
7. 根据信号评分：`mailto:` 和联系页最高，可见正文其次，简单反混淆结果较低。
8. 记录来源，不做 SMTP 探测，不推测私人邮箱，也不提交联系表单。

依赖 JavaScript 才显示内容的网站，第一版标记为“需要浏览器渲染”。后续再按真实命中率决定是否增加 Playwright sidecar，避免过早增加安装体积和维护成本。

## 9. 稳定性与安全

- API Key 放入系统凭据存储或 Rust 后端配置，不进入前端源码、日志和导出文件。
- API 请求设置全局速率限制，对 `429`、`5xx` 使用指数退避。
- 官网请求按域名限并发，避免集中访问同一站点。
- 所有网络任务可取消；应用关闭前保存游标和任务状态。
- 限制响应大小、重定向次数、页面数量和任务最大结果数。
- 导出前进行字段级许可检查，保留来源和采集时间以便审计。

## 10. MVP 界面

- 左侧导航：新建任务、任务记录、商家库、设置。
- 顶部：当前任务、运行状态、暂停/继续/取消。
- 查询区：数据源、关键词、地区、结果上限、开始按钮。
- 统计区：找到商家数、有官网数、找到邮箱数、失败数。
- 结果表：名称、主营业务、电话、地址、官网、邮箱、来源、状态。
- 详情抽屉：完整字段、来源、过期时间、官网采集日志、邮箱来源页面。

## 11. 分阶段实施

### 阶段一：应用骨架

- 建立前后端模块、统一错误结构和日志。
- 接入 SQLite，创建任务、商家、来源快照和邮箱数据表。
- 完成 API Key 安全存储。
- 完成导航、任务表单和结果表格。

### 阶段二：商家检索

- 定义统一数据源接口。
- 实现 Playwright Google Maps 列表滚动、详情读取和人工验证提示。
- 实现分页、配额控制、暂停/取消、重试和断点续跑。
- 实现来源追踪、去重和导出控制。

### 阶段三：官网邮箱发现

- 实现安全 URL 校验、robots、限速和 HTML 解析。
- 实现高价值页面发现、邮箱提取、评分和来源追踪。
- 将官网状态和邮箱结果实时推送到界面。

### 阶段四：质量与发布

- 补齐单元测试、模拟 API 测试和失败恢复测试。
- 优化大结果表格性能和任务日志。
- 完成 Windows 打包、数据备份/迁移、隐私说明和使用文档。

## 12. MVP 验收标准

- 可以用“关键词 + 地区”创建并运行检索任务。
- 每条记录展示来源，能区分临时 Google 内容和可持久化内容。
- 重启后可以查看历史任务并继续未完成任务。
- 对有官网的商家能够发现公开邮箱，并展示来源页面。
- 同一商家和邮箱不会重复入库。
- 单条失败不会中止整个任务，错误可查看并重试。
- CSV 导出自动排除不允许导出的字段。
- API Key 不出现在前端资源、普通日志或导出结果中。
- 过期缓存能够自动清理。

## 13. 开发前需要确认的产品决策

1. Google 数据是只用于应用内参考，还是业务上必须永久保存并批量导出。
2. 若必须持久化，准备采用哪家具有相应授权的数据源。
3. 首发平台是否只做 Windows。
4. 单次任务预期规模，是几十、数百还是数万条。
5. MVP 导出只做 CSV，还是必须同时支持 XLSX。
6. 是否需要多语言关键词、多个地区批量组合或代理。

当前默认：Windows 首发、本机 Edge/Chrome、单浏览器低速采集、MVP 先做 CSV、不加入代理、自动登录或验证码绕过。对外发布或商业化前，建议切换到许可明确的数据源，或先完成针对目标用途的合规审查。

## 14. 官方依据（核对日期：2026-09-05）

- Places API Text Search (New)：https://developers.google.com/maps/documentation/places/web-service/text-search
- Places API 数据字段与计费层级：https://developers.google.com/maps/documentation/places/web-service/data-fields
- Places API 政策与归因：https://developers.google.com/maps/documentation/places/web-service/policies
- Google Maps Platform 服务条款：https://cloud.google.com/maps-platform/terms
- Google Maps Platform 服务专项条款：https://cloud.google.com/maps-platform/terms/maps-service-terms
- Places API SKU 说明：https://developers.google.com/maps/billing-and-pricing/sku-details
