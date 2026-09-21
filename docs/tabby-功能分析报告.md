# Tabby 现有功能分析报告

> 分析对象：`D:\AiWork\work-2\tabby`（Eugeny/tabby 源码快照）
> 分析时间：2026-09
> 分析范围：全仓库结构 + 重点深入 `tabby-ssh`（SSH 客户端核心）、`tabby-core`（框架）、`tabby-electron`（平台层）、`tabby-local`（本地终端）、`tabby-terminal`（终端前端）、`tabby-settings`（设置）、`tabby-telnet`（对比参考）
> 文档用途：作为 HiShell（Rust + Tauri）开发的功能基线与技术参考

---

## 1. 项目概览

| 项 | 值 |
|---|---|
| 版本 | 1.0.231-nightly（由 `scripts/vars.mjs` 从 git tag 计算） |
| 技术栈 | Electron 43 + Angular 15 + TypeScript 4 + rxjs 7 + webpack 5 |
| 代码组织 | 扁平 monorepo（非 workspaces），每个插件包独立 `yarn.lock`，`scripts/` 统一安装/构建 |
| 许可 | MIT |
| 渲染进程 | **`target: 'node'`**（Node 集成全开，前端代码直接使用 fs/net/child_process） |

### 1.1 包职责一览

| 包 | 职责 |
|---|---|
| `app/` | Electron 外壳：主进程（`app/lib/`）、渲染入口（`app/src/`）、打包根 |
| `tabby-core` | 框架核心：DI 容器、ConfigService、Profile 系统、Vault 加密、多标签、分屏、热键、主题、i18n |
| `tabby-terminal` | 终端仿真：xterm.js 封装、会话抽象 BaseSession、中间件栈、ZMODEM |
| `tabby-local` | 本地 shell 会话：定义 `PTYInterface` 抽象、shell 探测模型 |
| `tabby-electron` | 平台绑定：node-pty 实现、文件对话框、剪贴板、通知、keychain、Windows/macOS 特性 |
| **`tabby-ssh`** | **SSH 客户端核心（本报告重点）** |
| `tabby-serial` / `tabby-telnet` | 串口 / Telnet |
| `tabby-settings` / `tabby-plugin-manager` | 设置界面 / 插件市场（npm 语义安装） |
| `tabby-linkifier` / `tabby-community-color-schemes` / `tabby-auto-sudo-password` | 链接识别 / 内置配色 / sudo 自动填密码 |
| `tabby-web`、`web/`、`tabby-web-demo` | 浏览器版容器与绑定（Node polyfill，线上为独立仓库 tabby-web） |
| `tabby-uac/` | Windows UAC 提权 C++ 助手（非 npm 包） |

依赖方向：所有插件以 `peerDependencies` 声明对 `tabby-core` / `tabby-terminal` / `tabby-settings` 的依赖；`app/package.json` 汇总全部内置插件，由 `scripts/prepackage-plugins.mjs` 预打包。

---

## 2. 整体架构

### 2.1 插件系统（HiShell 不建议照搬）

- **发现**：`app/src/plugins.ts` 扫描内置 builtin-plugins、app/node_modules、用户插件目录、`TABBY_PLUGINS` 环境变量；识别 `tabby-*`/`terminus-*` 前缀 + keywords 含 `tabby-plugin` 的 npm 包。
- **装配**：渲染入口 `bootstrap()` 动态 `require` 各插件 `forRoot()`，拼装 Angular 根模块；失败进入安全模式（仅内置插件）。
- **扩展点**：Angular multi-provider（`ProfileProvider`、`ShellProvider`、`ConfigProvider`、`SettingsTabProvider`、`HotkeyProvider`、`TerminalDecorator`、`TabContextMenuItemProvider`、`SSHProfileImporter`、`TabRecoveryProvider` 等）。
- **安装**：主进程 `app/lib/pluginManager.ts` 用 `@npmcli/arborist` 进程内 reify 安装到 `userData/plugins`。

> 评价：扩展能力极强，但代价是 Angular JIT 动态组装 + 渲染进程全 Node 权限，复杂度和安全面都大。HiShell 采用 Tauri（前端无 Node），应以内建模块 + 受控事件接口替代，不复制 npm 插件机制（可留 RMI/WASM 插件为远期方向）。

### 2.2 进程模型与 IPC

三种 IPC 并存：`ipcRenderer.sendSync`（pty:spawn 等需同步返回值场景）、`invoke/handle`（app:save-config）、`electron-promise-ipc` + 主进程 `broadcast`（事件推送）。无 contextBridge，配合 `@electron/remote`，属于旧式高权限模型。

PTY 调用链示例（最有参考价值的一条链路）：
```
tabby-local Session (DI: PTYInterface)
  → tabby-electron ElectronPTYInterface.spawn()         [src/pty.ts:17]
    → ipcRenderer.sendSync('pty:spawn', options)
      → 主进程 PTYManager → node-pty.spawn()             [app/lib/pty.ts:96]
      → PTYDataQueue 流控回推（100KB 分块 + pause/resume + UTF8 边界拆分）
        → broadcast(`pty:{id}:data`) → 渲染进程 → xterm.write
```
前端侧另有 128KB 阈值 + 高低水位的 `FlowControl`（tabby-terminal `xtermFrontend.ts:54`），形成端到端背压。**这套背压设计值得 HiShell 在 Rust 事件通道上复刻。**

### 2.3 配置存储

- 文件：`%APPDATA%\tabby\config.yaml`（便携版 exe 旁 `data/`，`TABBY_CONFIG_DIRECTORY` 可覆盖），保存时双写 `config.yaml + config.yaml.backup`（atomically 原子写）。
- 格式：YAML **只存与默认值的差异**——各 `ConfigProvider` 提供默认值树，`ConfigProxy` getter 回退默认值，写入时等于默认值的字段直接删除（`__cleanup`）。这使配置文件极小、可迁移、可版本化。
- 多窗口同步：主进程串行写盘并广播 `host:config-change`。
- 版本迁移：`ConfigService.migrate()` 按整数 version 逐级升级（当前 v8）。

---

## 3. SSH 客户端功能详解（tabby-ssh）

> 核心结论：**此版本 Tabby 已彻底弃用 Node 的 `ssh2` 包，SSH 协议栈整体迁移到 Rust 的 `russh` 库（v0.1.38，以 NAPI 原生模块形态使用，声明于 `app/package.json:35`，webpack external 加载）。** HiShell 直接原生使用 russh crate，与 Tabby 同一协议栈且省去 NAPI 层。

### 3.1 分层结构

```
SSHTabComponent（UI：工具栏/跳板机装配/KI 认证面板/服务消息）
        │
SSHShellSession（tabby-terminal BaseSession 适配器：PTY/数据流/resize/登录脚本）
        │  每 shell 标签一个实例，可多个共享
SSHSession（纯传输层：连接+认证+转发+SFTP 通道；ref/unref 引用计数；session/ssh.ts，994 行）
        │
        └── russh（Rust 原生库：transport/connect/认证/channel/SFTP/agent）
```

### 3.2 连接实现（`src/session/ssh.ts`）

**传输层选择**（`start()` :375-417，优先级从上到下）：
1. `proxyCommand` → `russh.SshTransport.newCommand(argv)`（子进程 stdin/stdout；`shell-quote` 解析，含 `&&`/`|` 直接抛错）
2. 跳板机 channel → `newSshChannel(jumpChannel)`
3. SOCKS 代理 → `newSocksProxy(...)`
4. HTTP 代理 → `newHttpProxy(...)`（CONNECT）
5. 直连 → `newSocket("host:port")`

**连接参数**（:419-440）：`connect(transport, hostKeyCallback, options)`；算法偏好 = profile 配置 ∩ russh 支持列表（`src/algorithms.ts`），剔除 `none/clear`；keepalive 毫秒→秒换算（默认 5000ms / countMax 10）；连接超时 `readyTimeout`。

**默认算法偏好**（`algorithms.ts:12-50`）：KEX 优先 `mlkem768x25519-sha256`、`curve25519-sha256`；Cipher 优先 `chacha20-poly1305@openssh.com`、AES-GCM/CTR；压缩默认 `none`。

**生命周期**：
- `init()` 装配认证方法列表 → `start()` 建 transport → connect → banner（可 skipBanner）→ 服务端断开延时 destroy → 确定用户名（缺失弹窗，取消默认 root，支持 `$ENV` 展开）→ 补插已存密码 → 认证 → 记住密码 → 恢复端口转发 → 订阅三个入站通道流（远程转发/X11/agent）→ open。
- `destroy()`：先停全部本地转发监听，再 disconnect。
- **引用计数**：`ref()/unref()`（:984-993），多标签共享一条连接，最后一个 unref 才销毁。
- **会话复用**：`SSHMultiplexerService` 以 `host:port:user:代理参数:跳板链` 为 key 的连接池；`reuseSession`（默认 true）。复用失败自动降级重试。
- **无自动重连**：断线后由 tab 层按 `behaviorOnSessionEnd`（reconnect/keep/close）处理，另有 Reconnect 按钮 + `restart-ssh-session` 热键。
- 关闭确认：`warnOnClose` 时关标签弹确认；通过检测输入 `exit\r`/Ctrl-D 判断"用户主动退出"。

### 3.3 认证体系（重点）

`AuthMethod` 联合类型（`ssh.ts:29-52`）：`none` / `prompt-password` / `hostbased` / `keyboard-interactive`（可带 savedPassword）/ `saved-password` / `publickey`（name + contents）/ `agent`（named-pipe、unix-socket、pageant 三种连接方式）。

**装配顺序**（`init()` :162-263，即尝试顺序）：
1. `none`
2. Agent（auth=Auto 或 agent）：可先尝试指定身份（私钥同名 `.pub`），再兜底"agent 全部密钥"
3. 公钥：逐个 `privateKeys`；支持 `%h`/`%r` 占位符；**若文件其实是公钥（能按公钥解析成功）则警告跳过**；未配置私钥时调用 `AutoPrivateKeyLocator` 自动收集本机密钥
4. 已存密码 → 5. keyboard-interactive（先带 savedPassword 再纯交互）→ 6. 交互输入密码 → 7. hostbased

**执行循环**（`_handleAuth()` :643-791）：先 `authenticateNone`；随后按服务器返回的 remainingMethods 过滤候选方法依次尝试。KI 认证将**已存密码自动填入 echo=false 且文本含 "password" 的 prompt**。Auto 模式配 ≥3 个私钥时若认证途中被断开会提示"私钥尝试次数过多"。

**私钥与 passphrase**（:939-982）：
- `russh.KeyPair.parse` 解析，格式支持 PEM（RSA/EC/OpenSSH 新格式）与 **PuTTY PPK**；`EC PRIVATE KEY` 头做了旧格式兼容替换。
- passphrase 以**私钥 SHA-512 哈希为 id** 存取（Vault 或 keychain）；加密类错误（KeyIsEncrypted / Ppk(Encrypted/IncorrectMac) 等）触发弹窗询问并可记住。

**Agent 探测**（:297-363）：
- Windows：先 stat 命名管道 `\\.\pipe\openssh-ssh-agent`（EBUSY 也视为存在）→ 有则用之；否则检测 Pageant；可强制指定类型/自定义管道路径。
- Unix：配置 `ssh.agentPath` 或 `$SSH_AUTH_SOCK`（支持 `@` abstract socket），校验为 socket 文件。

### 3.4 端口转发

**全库内实现（russh + Node 本地监听），不依赖 OpenSSH/plink 二进制。**

| 类型 | 实现 | 代码位置 |
|---|---|---|
| Local | `net.createServer` 监听 → 每连接 `openTCPForwardChannel(direct-tcpip)` + socket↔channel 数据泵 | `forwards.ts:17-30`、`ssh.ts:793-822` |
| Dynamic (SOCKS5) | `@luminati-io/socksv5.createServer`（匿名认证）起本地 SOCKS5，目标地址交同一回调 | `forwards.ts:31-46` |
| Remote | `ssh.forwardTCPPort(host, port)` 请求服务端监听；入站经 `tcpChannelOpen$` 接本地 socket | `ssh.ts:506-535, 823-836` |

- 配置模型：`ForwardedPortConfig { type: Local|Remote|Dynamic, host, port, targetAddress, targetPort, description }`（`api/interfaces.ts:45-58`）。
- Profile 内可预置 `forwardedPorts`，连接成功自动恢复（:500-502）；运行中通过工具栏 Ports 弹窗增删（`SSHPortForwardingModalComponent`）。
- 通用双向泵 `setupSocketChannelEvents`（:883-932）对称处理 data/eof/close。

### 3.5 SFTP 文件传输

- 通道：认证后 `openSessionChannel → activateSFTP` 创建子系统通道，懒加载单例（`ssh.ts:365-373`）。
- 封装 `SFTPSession`（`session/sftp.ts`）：readdir/stat/mkdir/rmdir/rename/unlink/chmod/readlink，POSIX 路径。
- **下载**：`OPEN_READ` 打开后循环 `read(256KB)` → 写本地文件。
- **上传**：先写临时文件 `path + '.tabby-upload'`，全部写完后删除目标再 rename —— **近似原子替换**；失败清理临时文件。
- 并发：多文件 `Promise.all` 并发；单文件内串行；**无断点续传**（无 offset/seek）。
- UI（`sftpPanel.component.ts`，终端侧栏面板）：浏览/过滤/右键菜单（下载/递归删除/新建目录）/拖拽上传（dropZone 指令）；**文件夹上传**递归 mkdir + 逐文件；**文件夹下载**并发"递归算总大小（边算边更新进度）"+"递归下载"，支持取消；进度经 `FileUpload/FileDownload/DirectoryUpload/DirectoryDownload` 传输对象回调。
- 另有 WinSCP 集成（Windows）：生成 `scp://` URI（含跳板参数）+ WinSCP.com `/keygen` 转 PPK。

### 3.6 安全机制

- **known_hosts**：存 Tabby 自有配置（`config.store.ssh.knownHosts` 数组），非 OpenSSH 文件；指纹 = `算法 + SHA256(base64)`；开关 `ssh.verifyHostKeys`；不匹配/未知弹窗三选（接受并记住【红色警示】/仅本次/断开），带 MITM 警告与新旧指纹对比。
- **Vault（主密码加密库）**（tabby-core `vault.service.ts`）：PBKDF2-HMAC-SHA512 ×100000 + 8 字节 salt 派生 256 位 key，AES-256-CBC + 随机 IV；可加密单条秘密（SSH 密码 `ssh:password`、私钥口令 `ssh:key-passphrase`、私钥文件本体 `vault://` base64），也可加密**整份配置**（`encrypted: true` 时配置文件只剩密文块）。
- **未启用 Vault 时**：keytar 写 OS keychain（服务名 `ssh@host:port` / `ssh-privateKey:id`）。
- 主口令可"记住 N 分钟"。

### 3.7 高级功能

| 功能 | 实现要点 |
|---|---|
| 跳板机 | **tab 层递归**：先递归建立跳板会话并 ref，再在跳板上 `openTCPForwardChannel(目标)`，目标会话以该 channel 为 transport；跳板销毁联动目标销毁（`sshTab.component.ts:76-117`） |
| 代理 | SOCKS / HTTP CONNECT / ProxyCommand 三选一（UI 单选），仅桌面端支持 ProxyCommand |
| X11 转发 | 请求侧 `requestX11Forwarding(MIT-MAGIC-COOKIE-1, random 16B hex)`；入站解析 DISPLAY（支持 TCP / unix socket / display number）连本地 X server；失败提示装 VcXsrv/Xming |
| Agent 转发 | `requestAgentForwarding` + 入站 agent 通道桥接本地 agent socket |
| 登录脚本 | `LoginScriptsOptions`（继承进 profile），shell 打开后执行无条件脚本；支持 hooks |
| 初始目录 | profile `cwd` → 发送 ` cd -- 'dir'`（前导空格避开 HISTCONTROL）；`rememberCwd` 把实际 cwd（OSC7 报告）写回 profile |
| ZMODEM | 不在 tabby-ssh；由 tabby-terminal 的 `ZModemDecorator`（zmodem.js）注入所有会话，检测序列后弹窗接管数据流 |
| 会话恢复 | TabRecoveryProvider（token `app:ssh-tab`）+ xterm SerializeAddon 序列化（≤1000 行）回放 |

### 3.8 SSH Profile 数据模型（HiShell 需对齐）

`SSHProfileOptions`（`api/interfaces.ts:16-43`）完整字段：

```
host, port(默认22), user(默认root),
auth: null|'password'|'publicKey'|'agent'|'keyboardInteractive',
password, privateKeys: string[],          // vault:// 或 file:// 引用
keepaliveInterval(5000ms), keepaliveCountMax(10), readyTimeout,
x11, skipBanner, jumpHost(profile id), agentForward,
warnOnClose,
algorithms: { hmac, kex, cipher, serverHostKey, compression },
proxyCommand,
forwardedPorts: ForwardedPortConfig[],
socksProxyHost/Port, httpProxyHost/Port,
reuseSession(true), input(终端输入处理), cwd, rememberCwd, term('xterm-256color')
```
全局配置：`ssh.warnOnClose / verifyHostKeys(true) / agentType('auto') / agentPath / knownHosts / x11Display / winSCPPath` + 3 个热键。

---

## 4. 终端层（tabby-terminal）

- xterm.js `@xterm/xterm ^5` 全家桶：webgl 0.17 / canvas 0.6 / fit / search / serialize / ligatures / unicode11 / image(sixel)；链接由 tabby-linkifier 的 web-links addon。
- 渲染：默认 WebGL2（校验非 SwiftShader、GPU 未禁用），不支持退化 Canvas；**WebGL 上下文丢失自动恢复最多 3 次**。
- 流控：前端 128KB 阈值 + 未完成 write 高(10)/低(5)水位，与主进程 PTYDataQueue 组成端到端背压。
- 字体：主字体 + fallback 拼接；字号缩放 1.1^n；resize 用 ResizeObserver + 32ms 节流。
- 会话抽象 `BaseSession`：OSC 处理（OSC7 报告 CWD）+ 中间件栈（UTF8Splitter / streamProcessing / inputProcessing / loginScriptProcessing）+ 初始输出缓冲防丢。
- 复制粘贴：copyOnSelect、copyAsHTML（SerializeAddon，32KB 限制）、多行粘贴警告、bracketed paste、右键模式可配（Windows 默认粘贴）、中键粘贴。
- 搜索：SearchAddon + 标尺装饰；快速输入：地址栏式 Quick Connect（profile 选择器自由文本）。

## 5. 本地终端（tabby-local + tabby-electron）

- node-pty spawn（ConPTY 可选）；Windows shell 探测覆盖 CMD/Clink、PowerShell、PowerShell Core、Git Bash、Cygwin、MSYS2、cmder、VS 命令行、**WSL 发行版（注册表 Lxss 枚举，`\\wsl$` 文件系统基址）**。
- 环境注入 `TERM=xterm-256color / COLORTERM=truecolor / TERM_PROGRAM`；Windows 可从注册表重建环境块。
- Windows UAC 提权（`runAsAdministrator` + 自带 UAC.exe 助手）；提权运行本地 shell。

## 6. 关键依赖清单（Rust/Tauri 迁移视角）

| Node 依赖 | 能力 | Rust/Tauri 对应 |
|---|---|---|
| `russh 0.1.38`（NAPI） | SSH/SFTP/agent | **russh + russh-sftp crate 原生使用**（零迁移成本） |
| `node-pty` | PTY | **portable-pty**（wezterm，ConPTY/openpty） |
| `keytar` | OS 凭据 | **keyring crate**（或 tauri-plugin-stronghold 对应 Vault） |
| `@luminati-io/socksv5` | SOCKS5 服务端 | fast-socks5 / 手写（动态转发用） |
| `@xterm/xterm ^5` 全家桶 | 终端前端 | **直接保留**（WebView 内可用） |
| zmodem.js | ZMODEM | 保留 / 远期 Rust 实现 |
| `serialport` | 串口 | serialport crate（远期） |
| `@npmcli/arborist` | 插件安装 | 无对应物——HiShell 不做 npm 插件 |
| `electron-updater` | 更新 | tauri-plugin-updater |
| `fontmanager-redux` | 字体枚举 | font-kit |
| glasstron / blurbehind | 窗口模糊 | Tauri 2 内置 window effects（mica/acrylic） |
| `windows-native-registry` | 注册表 | winreg crate |
| 进程树/子 shell PID/cwd | 系统信息 | sysinfo + 平台 API |

## 7. HiShell 功能基线（差距矩阵）

| Tabby 功能 | HiShell MVP | HiShell 后续 |
|---|---|---|
| SSH 连接（直连/SOCKS/HTTP/ProxyCommand） | ✅ 直连 + SOCKS5 + HTTP + 跳板机 | ProxyCommand |
| 认证：密码/私钥(+passphrase)/Agent/KI | ✅ 密码/私钥/KI，Agent | hostbased |
| known_hosts 校验 + 指纹弹窗 | ✅ | 导入 OpenSSH known_hosts |
| 多标签多会话 + 会话复用 | ✅ 多标签 | 连接复用池 |
| 端口转发 L/R/D | ✅（M3） | X11、agent 转发 |
| SFTP 双栏 + 拖拽 + 传输队列 | ✅（M4） | 断点续传（超越 Tabby）、限速 |
| 本地 shell（WSL/PowerShell/…） | M5 | UAC 提权 |
| 终端：WebGL/搜索/主题/ZMODEM/分屏 | 基础 + 主题/搜索（M5） | ZMODEM、分屏、ligatures |
| 配置：YAML 差异存储 + Vault/keychain | JSON profile + keyring | 主密码 Vault、配置同步 |
| Telnet/串口 | — | 视需求 |
| npm 插件系统 | — | 不做（内建模块化替代） |

---

## 附：关键文件速查

| 机制 | 文件（相对 tabby/） |
|---|---|
| SSH 传输/认证/转发核心 | `tabby-ssh/src/session/ssh.ts` |
| 三种端口转发监听器 | `tabby-ssh/src/session/forwards.ts` |
| SFTP 封装与上传下载 | `tabby-ssh/src/session/sftp.ts` |
| 跳板机装配 | `tabby-ssh/src/components/sshTab.component.ts` |
| Profile 模型/默认值 | `tabby-ssh/src/api/interfaces.ts`、`src/profiles.ts`、`src/algorithms.ts` |
| known_hosts | `tabby-ssh/src/services/sshKnownHosts.service.ts` |
| 密码存储双轨制 | `tabby-ssh/src/services/passwordStorage.service.ts` |
| 会话复用池 | `tabby-ssh/src/services/sshMultiplexer.service.ts` |
| Vault 加密 | `tabby-core/src/services/vault.service.ts` |
| 配置差异存储 | `tabby-core/src/services/config.service.ts` |
| PTY 流控 | `app/lib/pty.ts`、`tabby-terminal/src/frontends/xtermFrontend.ts` |
| xterm 前端/渲染恢复 | `tabby-terminal/src/frontends/xtermFrontend.ts` |
