# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

网吧点购系统（cybercafeShop）：顾客在客户机上自助点商品/呼叫网管，吧台主机的管理端语音播报并弹出订单卡片。Tauri 2（Rust）+ Vue 3 + Vite 5 + Element Plus + axum 0.8 + SQLite，目标平台 Windows 10+。

## 常用命令

```bash
cat package.json   # workspaces: admin + client；scripts 在此定义

npm install                 # 用 install.bat 触发也可
npm run dev:admin           # 管理端 dev（先跑 seed 脚本初始化 dev-data/，再 tauri dev）
npm run dev:client          # 用户端 dev（vite :14202，连 127.0.0.1）
npm test                    # = cargo test 服务端 crate（45 项）
npm run pack                # 一键打包 → dist/（编译两端，网页各自内嵌 exe，组装）
npm run build               # tauri build 两端（仅编译，不组装 dist）
node scripts/bump-version.js 0.2.0  # 一键改版本号（同步 package.json/tauri.conf.json/Cargo.toml/pack.js 共 9 处）

# 触发任一 tauri dev/build 前，cargo 文件锁会占用 Windows 下 src-tauri/target 里的 DLL，
# vite 已配置 watch: ignored ['**/src-tauri/target/**']，不要手动改动这条。
```

注意：`npm test` 只针对服务端 crate（`--manifest-path admin/src-tauri/server/Cargo.toml`），前端、Tauri 壳、GUI（托盘/通知卡片/穿透/警告框）没有自动化测试，后者靠 Windows 真机人工验证。测试全部用 `tempfile` 临时目录 + `LSWSHOP_DATA_DIR` 隔离，绝不碰生产/开发数据。

**测试分布（共 45 项，`cargo test` 一次跑完，所有测试都要通过再交）**：
- `server/tests/api_adversarial.rs`（29 项）：非法机台名/支付方式/数量、篡改价格、下架商品下单、路径穿越文件名、超大请求体、非本机调管理 API、并发下单销量一致性等对抗性用例。
- `server/tests/auth_shopinfo.rs`（8 项）：无签名 403、伪造/超窗签名 403、header 与 query 两种签名方式、生产模式本机免票、shopinfo 默认值与修改、缩拼自动生成与回填。
- `server/src/announce.rs`（4，单测）：`build_playlist` 纯函数（机台名拼字、跳非 ASCII/缺失文件、全中文回退 message.wav、call）。
- `server/src/auth.rs`（2，单测）：票据往返、与前端 `hmac.js`/Python 对拍的参考向量。
- `server/src/config.rs`（2，单测）：ini 解析、缺省端口。

## 开发模式

- **页面地址分岔**：管理端 dev 走 `http://localhost:14201`（vite 热重载），用户端 dev 走 `http://localhost:14202`（vite 热重载），都基于 `cfg!(debug_assertions)`；生产两端各自切换成内嵌协议（`WebviewUrl::App`）。改 `tauri.conf.json` 的 `beforeDevCommand`/`devUrl` 前先确认这层映射。
- **两端 dev 数据目录都是项目根 `dev-data/`**（`admin/src-tauri/src/lib.rs::data_dir` 和 `client/src-tauri/src/lib.rs` 里 `CARGO_MANIFEST_DIR` 相对路径）。`dev-admin.bat` 会先跑 `scripts/seed-dev-data.js`（幂等：`dev-data/` 已存在则跳过）生成测试商品库 + 音频/收款码 + 一份两端共用的 `config.ini`。
- **dev 模式门禁关闭**（`AuthMode::Off`），浏览器可直接裸访问调试。dev 的服务端（管理端）由管理端壳内嵌启动；`.bat` 或直接 `vite` 跑前端即可。
- **想裸跑服务端（无 GUI）调试 API**：`cargo run -p cybercafeShop-server`（`server/src/main.rs` 入口，读取 `LSWSHOP_DATA_DIR` 或 exe 目录下的 config.ini，默认端口 21974）。

## 架构

### 顶层结构（npm workspaces：root = admin + client）

```
admin/                    管理端（吧台主机），安装版 NSIS
├─ src/                   管理端前端（Vue3，编译后内嵌 exe）
│  ├─ App.vue             主界面（订单/商品/销售/设置 + 待处理角标）
│  ├─ NotifyApp.vue       通知卡片页（独立页面 notify.html）
│  ├─ api.js              前端 → 本机 HTTP API 封装（走 127.0.0.1）
│  └─ views/               Orders / Products / Records / Settings
├─ index.html / notify.html  MPA 两个入口
└─ src-tauri/
   ├─ src/lib.rs          管理端壳：单实例/托盘/通知窗口/事件转发/穿透轮询/首启播种
   └─ server/             ★ 服务端核心（独立 lib crate cybercafeShop，可单测）
client/                   用户端（客户机），绿色单文件
│  ├─ src/                App.vue（点购界面）+ api.js + hmac.js（纯 JS HMAC）
│  └─ src-tauri/src/lib.rs 用户端壳（读 config.ini → 探测+对时 → 加载内嵌网页）
│     └─ config.ini       ★ 生产配置模板（打包时复制到 莱尚网电竞馆点购.exe 旁）
assets/                   发布用首启种子（打进管理端安装包 seed\，★ 不含商品）
├─ data/db/shop_db.db     空库（仅表结构 + 系统分类）
├─ data/image/            空
├─ data/qrcode/ … sound/  收款码占位 / 播报 wav
└─ dev-seed/              开发测试种子（33 个测试商品 + 3 张图，仅 dev 用）
dev-data/                 ★ 开发数据（随源码附带，两头共用；删了用 seed 脚本重建）
scripts/                  seed-dev-data.js（重置 dev-data）/ pack.js（一键打包）
dev-admin.bat / dev-client.bat / install.bat / pack.bat
```

### 三个可执行产物与「网页分布」模型（跨多文件才能看全）

本项目不是普通前后端，是**一个管理端 exe + 无数台客户机绿色 exe**，核心是「两端网页各自内嵌 exe，只有管理端进程提供 HTTP 服务」：

- **管理端 exe**（`admin/`）：安装版（NSIS）。Tauri 壳 + 内嵌 axum 服务进程。管理界面通过 Tauri 自定义协议（`WebviewUrl::App("index.html")`）渲染，**在 TCP 端口上不存在**，局域网浏览器物理打不开 —— 这是第一层安全。
- **服务端**（`admin/src-tauri/server/`，独立 lib crate `cybercafeShop`）：管理端进程里起的 tokio + axum，监听 `0.0.0.0:21974`，提供业务 API（无静态网页托管）。业务全在这里。
- **用户端 exe**（`client/`）：绿色单文件，网页内嵌 exe（`WebviewUrl::App`）。只负责「读 config.ini → 探测+对时 → 加载内嵌网页 + 带签名调 API」。不存任何业务数据。

数据流：客户机 `莱尚网电竞馆点购.exe` 内嵌网页 + 带签名 API（`http://吧台IP:21974`）→ 服务端 → SQLite + 播报线程 + 事件广播 → Tauri 壳 → 桌面卡片/语音。

### 服务端核心目录

```
admin/src-tauri/server/src/
├─ lib.rs       只 re-export 各模块
├─ server.rs    Router 组装 + 公开/店铺 API + AppState
├─ admin.rs     管理端本地 API（/api/admin/* 与 /api/orders，localhost_only 守卫）
├─ db.rs        SQLite 数据层 + 下单事务 + 缩拼生成/回填
├─ auth.rs      HMAC-SHA256 时间票门禁（ACCESS_KEY、verify、AuthMode）
├─ announce.rs  语音播报（FIFO 队列 + 30s 呼叫去重，纯函数 build_playlist 可单测）
└─ config.rs    config.ini 解析 + AppDirs 目录约定（LSWSHOP_DATA_DIR）
```

关键：`server.rs` 里的 `router()` 是理解整条 HTTP 链路的钥匙 —— 它用 `.layer(require_ticket)` 包住公开接口，再 merge 管理端 `admin_router`（自带 `localhost_only` 守卫），最后加 `.layer(CorsLayer::permissive())`（dev 的 vite/Tauri 协议要跨源调 API）。`build_state_with(dirs, auth_mode)` 决定门禁开不开。用户端网页已内嵌用户端 exe，不再有 `/shop/` 托管。

### 认证与安全（三层，改动前先读 auth.rs 与两端 lib.rs）

- **管理端页面**：内嵌 exe，端口上没有 → 浏览器打不开。
- **管理端 API**：只接受 `127.0.0.1`（`admin::localhost_only`，靠 `into_make_service_with_connect_info` 提供的 ConnectInfo）。客户机调了也 403。
- **公开资源**：HMAC-SHA256 时间票门禁。签名 = `HMAC(密钥, 时间戳)`，±300 秒有效。API 走 header（`x-lsw-ts`/`x-lsw-sig`），`<img>` 带不了 header 走 query（`?ts=&sig=`）。`/api/ping` 是唯一**不加密**接口（只返回服务器时间做对时）。

**密钥只编译在 exe 里，绝不下发到任何静态文件**。它拷贝了两处 + 一处运行时注入：
1. `admin/src-tauri/server/src/auth.rs` 的 `ACCESS_KEY`
2. `client/src-tauri/src/lib.rs` 的 `ACCESS_KEY`
3. 两端壳都通过 `initialization_script` 把 key/host/port/machine/offset 注入页面内存（`window.__LSWSHOP_KEY__` 等），前端 JS 从这里取来签名。

**【换密钥必须三处一起改，重新编译两端】**：改 auth.rs、client/src-tauri lib.rs 的常量，否则两端验签对不上。时钟对时：客户端启动用 `/api/ping` 算 `offset`，注入 `window.__LSWSHOP_OFFSET__`，签名用「服务器时间」而非本地时间，时钟错乱也能用。

门禁模式（`AuthMode`）：`Off`（debug 构建，方便 vite 调试）、`Ticket`（测试严格模式）、`TicketOrLocalhost`（生产：本机免票、外网卡验票）。dev 与生产由 `cfg!(debug_assertions)` 自动切换，不要手工写死。

### 数据库：双库分离

`data/db/` 下**两个独立的库**（关键设计）：
- **shop_db.db** —— 商品/分类/订单/销量。表 `shop_fl`（分类）、`shop_list`（商品）、`orders`、`order_items`。`orders`/`order_items` 是首启自动建的（在 `db.rs::Db::open` 里 `CREATE TABLE IF NOT EXISTS`）；`shop_fl`/`shop_list` 由种子库携带。
- **config.db** —— 店铺配置（`shop_config` 键值对：`shop_name`/`welcome`），首启自动建 + 写默认值。

分开的意义：换商品数据包（覆盖 `shop_db.db` + `data/image/`）不影响店名和欢迎语。改数据库结构时注意这两个库的生命周期不同。

金额以**服务端库内售价**为准，客户端报的任何 price/total 一律忽略（`OrderReq` 里根本不反序列化价格字段）。下单在事务里逐商品校验「存在且在售」，按库内价计价，同时累加 `gds_out` 销量。

### 语音播报（announce.rs）

单工作线程 + 标准 mpsc FIFO，`PlaySoundW(SND_SYNC)` 前一条完整播完才播下一条。规则：
- 下单：机台名拆成字母/数字逐个播（`0-9.wav`/`A-Z.wav`）→ `order.wav`；机台名全是中文/符号时改播 `message.wav`。
- 呼叫：机台号逐个播 → `call.wav`；同一机台 **30 秒内重复呼叫跳过**（防刷屏）。
- 机台名取 `COMPUTERNAME`（不是用户名）；缺哪个 wav 跳过哪个。

`build_playlist` 是纯函数（给定目录+机台名+事件 → 返回播放列表），有单测，改播报规则优先改它。

### 桌面通知卡片（admin/src-tauri/src/lib.rs，Windows 专用）

层叠卡片有一套较绕的窗口机制，涉及多个跨文件约定：
- 卡片实际总高由前端 `NotifyApp.vue` **按常量推导**（标题 32 + 商品行 26×N + 合计 40 + 按钮 56），**不量 DOM**（避免渲染竞态），然后 `invoke('notify_sync', height)` 告诉 Rust。
- Rust `notify_sync` 据此设窗口尺寸、吸附右下角、显示/隐藏；`apply_rounded_region` 用 `SetWindowRgn` 把**窗口本身**裁圆角（不是透明窗口透出桌面）。
- 透明区**鼠标穿透**：`spawn_passthrough_polling` 每 50ms 轮询 `GetCursorPos`，光标在卡片区=可点，透明区动态开 `set_ignore_cursor_events`（不能用一次性 WS_EX_TRANSPARENT，否则卡片本身也点不了）。
- 事件流：服务端 `broadcast` → 壳里事件转发线程 `emit_to("notify", "tf-event", ev)` / `emit_to("main", ...)` → 前端 listen。`notify` 窗口的显示/尺寸由页面量好内容后调 `notify_sync` 完成，壳只订阅不直接控制。
- 点 X = 收起到托盘（HTTP 服务不能停）；托盘右键「退出」才是真退出。开机自启由设置页开关（`--hidden` 拉起时不显示主窗口）。

开发/调试这几个 GUI 行为只有 Windows 真机能验证，别假设它们在 CI 里能测。

## 关键约定与陷阱

- **Element Plus 程序化 API（`ElMessage`/`ElMessageBox`）禁止手动 import**，否则丢样式。本项目用 `unplugin-auto-import` + `unplugin-vue-components` 按需加载（参考 Landisk）。手动 import 是常见踩坑点。
- **Client 的 vite `base: './'` 必须保留**：生产网页内嵌 exe（tauri 协议），相对路径保证资源正确加载；改回 `/` 会让页面去请求 `/assets/*` → 404 白屏。
- **两端 dev 都读写项目根 `dev-data/`**（debug 断言），与生产完全隔离，只动这一个目录即可联调。想真机联测改 `dev-data/config.ini` 的 `host` 为吧台机 IP。测试数据乱了删掉 `dev-data/` 重新 `node scripts/seed-dev-data.js`。
- **`LSWSHOP_DATA_DIR` 环境变量**可覆盖数据根目录（测试隔离用）；生产默认 = exe 所在目录。
- **dev 与生产门禁、数据目录都在两份 config.ini 里**：管理端只读 `[server] port`；用户端读 `host`/`port`/`contact`（连不上吧台时警告框显示）。生产 config.ini 由首启播种写默认（无 host/contact，那两行属用户端）。
- **管理端首启播种（仅生产，`seed_if_missing`）**：安装包带 `seed/`（空库 + 音频 + 收款码占位，**不含商品**），数据目录缺什么补什么；`data/image`（商品图）首启为空，靠部署时灌「商品数据包」覆盖。
- **打包链**（`scripts/pack.js`）：tauri build 管理端（NSIS 内嵌管理端 exe+网页+seed）→ tauri build 用户端（网页内嵌 exe）→ 组装 `dist/`。
- **不要把 package-lock.json 打进源码包**：Linux 生成的 lockfile 在 Windows 会缺平台可选依赖，对方 `npm install` 会报错；源码包内用 `install.bat` 重新生成。（`package-lock.json` 已被 `.gitignore` 忽略，别手动跟踪它。）

## 改动指引

- 加/改 API：先改 `db.rs`（数据层）→ `admin.rs` 或 `server.rs`（路由）→ 前端 `api.js`。注意管理端 API 都在 `admin_router` 的 `localhost_only` 守卫下；公开 API 都在 `server.rs` 的 `protected` 里（会被门禁包住）。
- 改商品/分类 SQL：`shop_list`/`shop_fl` 字段在图中保留了不少预留列（`gds_bt_count`、`gds_gys`、`gds_js` 等），别当垃圾删，`db.rs` 的 `upsert_product` 依赖它们的默认值。
- 改 UI（用户端）：改 `client/src` 后，dev 用 vite 热重载直接看；生产要重新 `npm run pack`（重新编译用户端 exe）才能在客户机生效。
