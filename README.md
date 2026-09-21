# HiShell

Win11 Fluent Design 风格的 SSH 客户端（对标 Xshell/Xftp/Tabby），基于 **Tauri 2 + Rust + Vue 3**。

> 当前状态：**M1–M5 核心功能已实现**（russh 真实连接 / keyring 凭据 / 端口转发 / SFTP 传输引擎 /
> 断点续传 / 本地终端 PTY / Xshell·Tabby 会话导入），
> 前端已从 mock 切换到真实后端（浏览器预览仍保留 mock 模式）。
> 支持平台：**Windows x64 / Linux x64 / macOS Intel / macOS Apple Silicon**。

## 功能界面

| 视图 | 说明 |
|---|---|
| 终端视图 | 多标签会话（状态圆点/关闭/右键菜单）、xterm.js 终端（搜索/清屏）、**本地终端标签（ConPTY/openpty）**、快速连接地址栏、状态栏（协议/TERM/行列/CAP NUM） |
| 文件视图（SFTP） | 左右双栏（本地/远程真实目录）、快速链接、路径导航/排序/多选、**拖拽传输**、传输队列（进度/速度/估计剩余/暂停取消/**失败后断点续传**）+ 日志页签 |
| 新建连接 | 常规/认证（密码·私钥·KI·Agent）/ **代理连接（无·SOCKS5·HTTP CONNECT·跳板机·ProxyCommand）** / 终端参数 |
| 会话导入 | **文件 → 导入会话**：自动扫描 Xshell（.xsh，含 v5–v8 与旧版目录）与 Tabby（config.yaml），勾选导入并按 主机+端口+用户 去重 |
| 设置 | 浅色/深色主题、主题色、终端字体字号、**终端配色方案（深色/浅色/跟随应用）** |
| 国际化 | **跟随系统 / English / 简体中文** 三选，设置页显示当前语言编码（如 zh-CN）；匹配不到语言时默认英语，切换即时生效 |
| 凭据安全 | 密码/私钥口令经 **OS 钥匙串**（Windows Credential Manager / macOS Keychain / Linux Secret-Service）存储，无 dbus 环境自动回落本地文件 |
| known_hosts | SHA256 指纹校验，首次/变更弹窗（保存/仅本次/断开），持久化于配置目录 |

## 跨平台

| 平台 | 状态 | 说明 |
|---|---|---|
| Windows x64 | ✅ 主要目标 | Mica/Acrylic 窗口材质、ConPTY、命名管道 ssh-agent |
| Linux x64 | ✅ | XDG 配置目录、SSH_AUTH_SOCK、secret-service 凭据 |
| macOS Intel / Apple Silicon | ✅ | 通用二进制（lto 优化），Keychain 凭据 |

## 运行

```bash
npm install
```

**方式一：桌面应用（完整体验，含 Mica 材质/本地目录浏览）**

```bash
npm run tauri dev
```

需要 Rust 工具链（rustup + MSVC Build Tools + WebView2，Win11 自带）。

**方式二：浏览器预览（无需 Rust，mock 模式）**

```bash
npm run dev
# 打开 http://localhost:5173
```

## 本地构建（Windows）

双击或在命令行运行 `build.bat`：

```bat
build.bat           :: release 完整打包（NSIS+MSI 安装包，首次较慢）
build.bat debug     :: debug 打包（不优化，编译快，适合本地测试）
build.bat exe       :: 只出绿色版 exe（跳过安装包，最快，增量约 15 秒）
build.bat check     :: 仅类型检查 + cargo check
```

产物位置：
- 安装包：`src-tauri	arget
eleaseundle
sis\*.exe`、`...\msi\*.msi`
- 绿色版 exe：`src-tauri	arget\debug\hishell.exe`（debug）或 `...
elease\hishell.exe`

> 提示：首次 NSIS/WiX 打包工具需从 GitHub 下载，网络不通时可用 `build.bat exe` 直接出绿色版测试。

## 诊断

- 后端日志：`%APPDATA%\hishell\hishell.log`（连接/认证/主机密钥/数据泵全链路）
- 前端调试：以环境变量 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 --remote-allow-origins=*"` 启动 exe，再用 `scripts/cdp_drive.py` / `scripts/cdp_check.py` 驱动与检查界面

## 构建与发布

```bash
npm run tauri build   # 手动完整打包
npm run build         # 仅前端产物 dist/
npm run typecheck     # vue-tsc 类型检查
```

### 自动打包发布（GitHub Actions）

推送 `v*` 标签即可自动完成四平台打包并发布 GitHub Release：

```bash
git tag v0.2.0
git push origin v0.2.0
```

流程（`.github/workflows/release.yml`）：
1. 以 **tag 版本为准**，构建前自动同步版本号到 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（`scripts/sync-version.mjs`）
2. 四平台矩阵构建：Windows x64（NSIS+MSI）/ Linux x64（AppImage+deb）/ macOS Apple Silicon（arm64）/ macOS Intel（x86_64）
3. 自动创建 GitHub Release（`HiShell v{tag}`）并上传全部安装包

也支持在 Actions 页面手动触发（workflow_dispatch，输入版本号）。

> 说明：当前未配置代码签名——Windows 首次运行会有 SmartScreen 提示，macOS 需右键打开绕过 Gatekeeper；
> 如需签名，在 workflow 中补充 `APPLE_CERTIFICATE` / Windows 证书等 secrets 即可。

## 文档

- [docs/tabby-功能分析报告.md](docs/tabby-功能分析报告.md) —— Tabby 源码功能分析（SSH 连接/认证/端口转发/SFTP/配置系统）
- [docs/hishell-设计方案.md](docs/hishell-设计方案.md) —— HiShell 架构设计、前后端通信协议、Fluent UI 规范、里程碑（M1-M6）

## 里程碑

| 阶段 | 内容 | 状态 |
|---|---|---|
| M0 | 界面原型 | ✅ |
| M1 | russh 真实连接（认证/代理/跳板机/known_hosts/会话复用） | ✅ |
| M2 | Profile 持久化 + keyring 凭据 | ✅ |
| M3 | 端口转发（本地/远程/动态 SOCKS5） | ✅ |
| M4 | SFTP 真实传输（递归目录展开、暂停/取消） | ✅ |
| M5 | 本地 PTY 终端 + 终端增强（搜索/浅色配色方案）+ **断点续传** + **会话导入器（Xshell/Tabby）** | ✅ |
| M6 | 打包分发（GitHub Actions 已就绪） | ✅ 自动更新待接入 tauri-plugin-updater |

后续规划：ZMODEM（rz/sz）、终端分屏、WebGL 渲染器、iTerm 社区配色集、自动更新签名。
