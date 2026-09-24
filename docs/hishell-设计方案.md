# HiShell 技术设计方案（Rust + Tauri）

> 版本：v0.1（UI 原型阶段）
> 前置文档：[tabby-功能分析报告.md](./tabby-功能分析报告.md)
> 产品定位：类 Xshell/Xftp/Tabby 的 Windows 优先 SSH 客户端，Win11 Fluent Design 风格

---

## 1. 目标与范围

### 1.1 产品目标
- SSH 客户端：用户名密码 / 私钥（含 passphrase）/ Keyboard-interactive / Agent 认证
- **连接级代理配置**：直连 / SOCKS5 / HTTP CONNECT / 跳板机（可多级）
- **多选项卡**同时连接多台服务器（参考 Xshell 布局）
- **端口转发**：本地 / 远程 / 动态（SOCKS5）
- **SFTP 双栏管理器**：左右分栏浏览本地/远程，拖拽即传输（参考 Xftp 布局），带传输队列（进度/速度/剩余时间）与日志
- 本地终端（后续版本）
- UI 遵循 **Windows 11 Fluent Design**：Mica/Acrylic 窗口、自绘标题栏、圆角、Fluent 图标与动效

### 1.2 实施策略：UI-first
本轮先交付**界面原型**：全部交互可用（标签、菜单、对话框、双栏浏览、拖拽、传输队列、设置），后端为 mock/stub；界面风格与交互经确认后，再按里程碑接入真实功能。前端 API 层设计为 **adapter 双模式**：Tauri 环境走 `invoke/event`，纯浏览器环境走 mock——原型可在浏览器直接预览，后续接后端不改组件代码。

---

## 2. 技术选型

| 层 | 选型 | 说明 |
|---|---|---|
| 桌面框架 | **Tauri 2**（Rust 1.95+） | 体积小、安全模型好、原生 WebView2 |
| 前端 | **Vue 3 + TypeScript + Vite + Pinia** | 组合式 API；Pinia 管会话/传输/设置状态 |
| 终端 | **@xterm/xterm 5**（fit/webgl/search addon） | 与 Tabby 同款，成熟稳定 |
| UI | **自研 Fluent 组件**（CSS token + Vue 组件） | 不引入重型 UI 库；还原 Win11 观感；组件清单见 §4.4 |
| 图标 | Fluent System Icons（内嵌 SVG） | 单色随主题 |
| 字体 | Segoe UI Variable / 微软雅黑；终端 Cascadia Mono、Consolas | |
| 窗口效果 | Tauri 2 window effects（Windows 11 Mica / Acrylic） | decorations:false + 自绘标题栏 |

### 2.1 Rust 侧 crate 选型（M1 起引入，与 Tabby 依赖一一对应）

| 功能 | crate | 对应 Tabby |
|---|---|---|
| SSH 协议 | **russh**（client + keys） | russh 0.1.38（NAPI）——同一 Rust 协议栈 |
| SFTP | **russh-sftp** | russh 内置 SFTP |
| 异步运行时 | **tokio**（full features） | Node 事件循环 |
| PTY（本地终端） | **portable-pty** | node-pty |
| 凭据存储 | **keyring**（Win Credential Manager） | keytar |
| SOCKS5 服务端（动态转发） | **fast-socks5** | @luminati-io/socksv5 |
| 配置序列化 | serde + serde_json（配置文件 JSON，格式类似但比 YAML 少一个解析依赖） | js-yaml |
| 特殊目录 | dirs | node 的 app.getPath |
| 更新 | tauri-plugin-updater | electron-updater |
| 对话框 | tauri-plugin-dialog | Electron dialog |

> 不做 npm 插件系统（Tabby 用 @npmcli/arborist），以内建模块 + 事件接口实现扩展点；远期可评估 WASM 插件。

---

## 3. 系统架构

### 3.1 分层

```
┌────────────────────────── WebView (Vue 3) ──────────────────────────┐
│ 视图层：TitleBar/MenuBar/ToolBar/TabStrip/TerminalPane/SftpView/...   │
│ 状态层：Pinia（sessions / transfers / settings / profiles）           │
│ API 层：backend.ts 接口定义 → adapter.ts（Tauri invoke | mock）       │
└──────────────┬───────────────────────────▲──────────────────────────┘
        invoke/listen（JSON）     event emit（数据/进度/日志）
┌──────────────▼───────────────────────────┴──────────────────────────┐
│ Rust Core (tauri::command + tokio tasks)                             │
│  commands/: fs / ssh / sftp / profile / app                          │
│  ssh::SessionManager   — 会话池（id→SessionHandle，认证状态机，代理链）│
│  ssh::ForwardManager   — Local/Remote/Dynamic 转发                    │
│  sftp::SftpService     — 目录浏览 + 传输任务（并发、进度事件、队列）    │
│  profile::ProfileStore — 连接配置持久化（JSON + 默认值合并）           │
│  secret::SecretStore   — keyring 封装（密码/passphrase）              │
│  pty::LocalShell       — portable-pty（M5）                           │
└───────────────────────────────────────────────────────────────────────┘
```

### 3.2 前后端通信协议（设计定稿，原型期前端 mock 同名接口）

**Commands（invoke）**
| 命令 | 参数 → 返回 | 说明 |
|---|---|---|
| `fs_list_dir` | path → `DirEntry[]` | 本地目录（名称/大小/类型/修改时间/隐藏过滤） |
| `fs_special_dir` | kind → path | 桌面/文档/下载等已知目录 |
| `profile_list / save / delete` | → `SshProfile[]` | 连接配置 CRUD |
| `ssh_connect` | profileId → sessionId | 建立会话（含代理/跳板） |
| `ssh_disconnect` | sessionId | 断开 |
| `ssh_write / ssh_resize` | sessionId, data/cols,rows | 终端输入/尺寸 |
| `forward_add / forward_remove / forward_list` | sessionId, rule | 端口转发管理 |
| `sftp_list / sftp_mkdir / sftp_rm / sftp_rename` | sessionId, path | 远程文件操作 |
| `transfer_enqueue` | items[] → taskId[] | 入队上传/下载 |
| `transfer_pause / resume / cancel` | taskId | 队列控制 |

**Events（后端 → 前端）**
| 事件 | 载荷 | 说明 |
|---|---|---|
| `ssh://{sid}/data` | bytes(base64) | 终端输出（带序号+ack 背压，复刻 Tabby PTYDataQueue） |
| `ssh://{sid}/status` | connected/disconnected/error+msg | 会话状态（标签圆点） |
| `ssh://{sid}/banner` | text | 服务端 banner/MOTD |
| `hostkey://{sid}` | fingerprint 旧/新 | known_hosts 确认弹窗 |
| `sftp://{sid}/entries` | path, entries | 远程目录结果 |
| `transfer://{tid}/progress` | transferred, total, speed, eta | 传输进度（节流 ~100ms） |
| `transfer://{tid}/done` | ok/error | 传输结束 |
| `log://{scope}` | line | 传输/会话日志 |

### 3.3 会话生命周期（M1 目标行为）

连接流程复刻 Tabby（见分析报告 §3.2/3.3）：
transport 选择（直连/SOCKS/HTTP/跳板 channel）→ 算法协商（默认偏好对齐 Tabby：KEX 优先 mlkem768x25519/curve25519，Cipher 优先 chacha20-poly1305/AES-GCM）→ host key 校验（SHA256 base64 指纹 + 首次/变更弹窗）→ 认证顺序 **none → agent → publickey（passphrase 弹窗+记住）→ password → keyboard-interactive（自动填充已存密码）** → keepalive(5s/10) → shell channel（PTY resize 同步）→ 数据事件流。

---

## 4. UI 设计规范（Win11 Fluent Design）

### 4.1 设计 token（CSS 变量，浅/深双主题）

| token | 浅色 | 深色 |
|---|---|---|
| --bg-base（Mica 底） | #f3f3f3 | #202020 |
| --bg-layer（内容层） | #f9f9f9 | #272727 |
| --bg-card | #ffffff | #2d2d2d |
| --bg-subtle-hover | rgba(0,0,0,.04) | rgba(255,255,255,.05) |
| --stroke-card | rgba(0,0,0,.07) | rgba(255,255,255,.08) |
| --text-primary | #1a1a1a | #ffffff |
| --text-secondary | #5f5f5f | #c8c8c8 |
| --accent | #005fb8 | #4cc2ff |
| --accent-text-on | #ffffff | #003e92 |
| --accent-hover | #1975c5 | #59ccff |
| 圆角 | 控件 4px / 卡片 8px / 窗口 8px | |
| 阴影 | flyout 0 8px 16px rgba(0,0,0,.14)；dialog 0 32px 64px | |

主题切换：`html[data-theme="dark|light"]`；Mica 由 Tauri `windowEffects` 提供，WebView 背景设为透明以透出。

### 4.2 主窗口布局（终端视图，参考 Xshell 图1）

```
┌──────────────────────────────────────────────────────────┐
│ ▣ HiShell                                    —  □  ×     │  TitleBar 40px（Mica、拖拽区）
│ 文件 编辑 查看 工具 选项卡 窗口 帮助                       │  MenuBar 36px
│ [+▼] [📂]              │ 🔒 ssh://root@1.2.3.4:22 ▼ │ ⚙ │  ToolBar 44px + 快速连接栏
│ ┌ ● yt-10808 × ┬ ● top.hicode.top × ┬ + ──────────────┐  │  TabStrip 38px
│ │ [终端|文件] │ 🔗连接 ⏻断开 │ 🧹清屏 🔍搜索 │ ✓直连 ssh://…││  SessionToolbar 38px（每选项卡功能区）
│ │                                                        │
│ │                xterm.js 终端                            │  TerminalPane（flex:1）
│ │                                                        │
│ ├────────────────────────────────────────────────────────┤
│ │ 🔒ssh://root@149.104.22.80:22 │ SSH2 │ xterm │125×36 │ 2 会话 │ ▲▼ │ CAP NUM │
└──────────────────────────────────────────────────────────┘
```
- **选项卡内功能区（SessionToolbar）**：位于每个会话选项卡内容区顶部，包含
  ① 视图切换（终端/文件，分段控件，随视图高亮）；② **端口**（插头图标，打开端口转发配置/管理弹窗）；
  ③ 连接/断开；④ 终端视图专属：清屏、搜索；⑤ 右侧连接信息：代理方式徽标（直连/SOCKS5 host:port/HTTP/跳板: 名称/ProxyCommand）+ 会话地址。
  **端口转发弹窗**（参考 Tabby Forwarded ports）：已配置规则列表（类型徽标 Local/Remote/Dynamic + `bind:port → target` 摘要 + 删除），
  添加表单 `[绑定地址]:[绑定端口] → [目标地址]:[目标端口]` + 描述 + 类型切换（动态模式下隐藏目标字段，摘要显示 SOCKS 代理），
  规则保存至所属 Profile（M3 接入 russh 真实转发）；工具菜单「端口转发管理…」同入口。
  全局工具栏因此只保留：新建连接、会话管理器、快速连接地址栏、设置。
- **视图保持挂载**：终端/文件两个视图用 v-show 切换（非 v-if）——xterm 实例不允许二次 open()，
  切换时搬移 term.element 重挂并 fit；切换往返不丢回滚内容，SFTP 传输在后台继续。
- 标签：状态圆点（绿=已连接、黄=连接中、灰=断开）、标题、×关闭；+ 新建（打开连接对话框）；中键关闭；右键菜单（重命名/复制连接/关闭/关闭其他/关闭右侧）。
- 快速连接栏：输入 `ssh://user@host:port` 回车即连（mock）。
- 状态栏：当前地址｜协议 SSH2｜TERM｜行列（来自 xterm 实时尺寸）｜会话数｜CAP/NUM 指示灯。

### 4.3 SFTP 视图（参考 Xftp 图2）

同一会话标签内「终端 ⇄ 文件」视图切换（工具栏切换钮 + 快捷键 Ctrl+Shift+F）；SFTP 视图结构：

```
│ ┌ 本地 ● 桌面 × ┐    ┌ 远程 ● yt-10808 (/root) × ┐        │  左右窗格各自 tab 头
│ │[<][>][路径▼][★][🔄]│ [同左]                              │  路径工具条
│ │快速链接│名称 大小 类型 修改时间│  │名称 大小 类型 修改时间 属性│  │  左栏带快速链接侧条
│ │        │…文件列表…        │◄►│…文件列表…                  │  │  可拖分隔条
│ ├──────── 传输 │ 日志 ─────────────────────────────────┤    │  底部队列面板（可折叠）
│ │名称 状态 ▓▓▓░进度 大小 本地路径 ↔ 远程路径 速度 剩余 经过│   │
│ ├────────────────────────────────────────────────────────┤
│ │ 已连接 149.104.22.80:22        二进制  文件:85 文件夹:30 │    │  SFTP 状态栏
```
- 交互：双击进目录（面包屑+路径下拉历史）、↑上级、刷新、点列头排序、Ctrl/Shift 多选；
- **拖拽**：左↔右栏拖文件/文件夹 → 加入传输队列（方向图标 ↔）；从系统资源管理器拖入 → 上传队列；
- 传输队列：进度条、速度、估计剩余、经过时间实时刷新（mock 定时器模拟）；操作列 暂停/继续/取消；完成后移入日志页签记录。

### 4.4 自研 Fluent 组件清单（src/ui/）

FButton（Accent/Standard/Subtle、图标按钮）、FTextBox（清除钮/占位）、FComboBox、FCheckbox、FRadioGroup、FSwitch、FDialog（Canvas 层、8px 圆角、32px 标题）、FContextMenu（键盘导航）、FProgressBar、FDataGrid（列排序/多选/拖拽源/放置目标）、FTabStrip（通用标签）、FTooltip（延迟显示）、FSlider。

### 4.5 国际化（i18n）

- 自研轻量内核（`src/i18n/index.ts`）：`t('path.key', params)` 响应式翻译，支持 `{param}` 插值；键缺失回落英语，再缺失显示键名。
- **语言偏好三选**：`跟随系统 / English / 简体中文`（设置 → 外观 → 语言，持久化）。
- **解析规则**：跟随系统时按 `navigator.language` 前缀匹配——`zh* → zh-CN`，`en* → en-US`，**匹配不到默认 en-US**。
- 设置页实时显示**当前语言编码**（如 `zh-CN`；跟随系统时附注检测到的系统语言）。
- 语言包：`src/i18n/locales/zh-CN.ts`（源语言）、`en-US.ts`；全部界面文案（菜单/对话框/表格列头/右键菜单/toast/传输日志/模拟终端输出）均走 t()，切换即时生效。

### 4.6 菜单（中文，数据驱动，集中管理于 i18n 语言包）

文件：新建连接 Ctrl+N / 新建 SFTP / 导入会话(占位) / 打开配置目录 / 退出
编辑：复制 Ctrl+Shift+C / 粘贴 Ctrl+Shift+V / 全选
查看：终端视图 / 文件视图 Ctrl+Shift+F / 深色主题 / 浅色主题 / 设置
工具：端口转发(占位) / 选项 / 快捷键(占位)
选项卡：下一个 Ctrl+Tab / 上一个 Ctrl+Shift+Tab / 关闭 Ctrl+W / 关闭其他
窗口：会话列表（动态勾选）
帮助：关于 HiShell

---

## 5. 数据模型

### 5.1 SshProfile（对齐 Tabby 字段，JSON 存储）

```ts
interface SshProfile {
  id: string; name: string; group?: string;
  host: string; port: number;            // 默认 22
  username: string;                      // 默认 root
  auth: 'password' | 'priFileKey' | 'keyboardInteractive' | 'agent' | 'auto';
  password?: string;                     // 原型期明文存 localStorage；M2 移 keyring
  privateKeyPath?: string; privateKeyPassphrase?: string;
  // 连接方式（互斥）
  proxy: 
    | { kind: 'none' }
    | { kind: 'socks5'; host: string; port: number; username?: string; password?: string }
    | { kind: 'http'; host: string; port: number; username?: string; password?: string }
    | { kind: 'jump'; profileId: string }        // 跳板机=另一 profile，可递归
    | { kind: 'proxyCommand'; command: string }; // M1+
  keepaliveInterval: number;             // 5000ms
  keepaliveCountMax: number;             // 10
  connectTimeout: number;                // 10000ms
  term: string;                          // xterm-256color
  forwardedPorts: ForwardRule[];
  behaviorOnSessionEnd: 'reconnect' | 'keep' | 'close';
}
interface ForwardRule {
  id: string; type: 'local' | 'remote' | 'dynamic';
  bindHost: string; bindPort: number;
  targetHost?: string; targetPort?: number;  // dynamic 无需
  description?: string;
}
```

### 5.2 运行时对象
SessionTab（标签状态机：connecting→connected→disconnected；viewMode: terminal|sftp）、TransferItem（name/direction/localPath/remotePath/total/transferred/speed/eta/status/elapsed）、DirEntry（name/size/kind/mtime/attrs）。

---

## 6. 后端模块设计（M1+ 落地要点）

- `SessionManager`：`DashMap<u64, SessionHandle>`；SessionHandle 持 russh client + mpSc 通道；引用计数支持**会话复用**（同 profile 多标签共享连接，复刻 Tabby Multiplexer 语义）。
- 跳板机：递归解析 profile 依赖链（防环，深度上限 3），在跳板上 `openTCPForwardChannel` 作为目标 transport（复刻 Tabby tab 层方案，移入 Rust 单侧）。
- known_hosts：JSON 存储 `{host, port, keyType, fingerprint}`；指纹 SHA256 base64；前端弹窗三选（记住/仅本次/断开）。
- SFTP 传输器：每任务 tokio task；读块 256KB（对齐 Tabby）；上传先写 `.hishell-upload` 临时文件再 rename；并发数=4/会话；进度事件 100ms 节流；**扩展：分块 offset 记录 → 断点续传（超越 Tabby）**。
- 背压：前端 ack 机制，未 ack 超过 512KB 时 pause channel 读取（复刻 PTYDataQueue 100KB 分块 + 高低水位）。
- `ProfileStore`：`%APPDATA%\hishell\profiles.json` + `settings.json`；写盘原子替换 + `.backup` 双写（借鉴 Tabby）。

## 7. 里程碑

| 阶段 | 内容 | 状态 |
|---|---|---|
| **M0 UI 原型** | Fluent 界面 + 全交互 + mock 后端 + adapter 机制 + 双文档 | ✅ |
| **M1 SSH 真实连接** | russh 连接/认证/代理/跳板 + xterm 真会话 + known_hosts | ✅ |
| **M2 配置与安全** | ProfileStore 持久化 + keyring 密码/口令 | ✅（导入器待做） |
| **M3 端口转发** | Local/Remote/Dynamic + 转发管理弹窗实时下发 | ✅ |
| **M4 SFTP 真实传输** | russh-sftp 目录/上传/下载/递归 + 队列真实进度/暂停/取消 | ✅（断点续传待做） |
| M5 终端增强 | 本地 PTY + 主题配色/搜索/ZMODEM(评估)/分屏/字体连字 | |
| M6 打包分发 | NSIS/MSI + 代码签名 + tauri-plugin-updater 自动更新 | |

## 8. 本轮原型实现说明

- 运行：`npm install` → `npm run tauri dev`（完整体验，需 Rust 工具链）；或 `npm run dev`（浏览器预览，mock 模式，Mica 效果以静态渐变近似）。
- adapter：`src/api/adapter.ts` 检测 `__TAURI_INTERNALS__` 决定真实/mock 实现，组件零感知。
- mock 行为：`fs_list_dir` 生成桌面/文档等模拟目录（Tauri 环境下列真实目录）；远程列表固定模拟 `/root` 内容；连接 800ms 延迟后置绿点；传输队列按模拟速度推进进度。
- 目录结构见仓库；Rust 侧本期仅 `fs_list_dir / fs_special_dir / app_info` 三个 command + 空模块占位（ssh/sftp/profile 注明 M1 填充）。
