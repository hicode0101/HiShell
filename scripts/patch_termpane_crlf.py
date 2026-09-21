# -*- coding: utf-8 -*-
"""TerminalPane.vue 补丁：状态横幅用 \r\n 换行（修复 LF 不回列导致的缩进），并抽出 writeStatusLine 辅助函数"""
import io, sys

path = r"D:\AiWork\work-2\HiShell\src\components\terminal\TerminalPane.vue"
src = io.open(path, encoding="utf-8").read()

ESC = "\x1b"

# 1) 挂载时"正在连接"横幅
old_connecting = (
    "  if (isTauri && props.tab.status === 'connecting') {\n"
    "    terminalEntry(props.tab.id)?.term.write(\n"
    "      `" + ESC + "[90m[HiShell] ${t('terminal.connecting', { addr: props.tab.address })}..." + ESC + "[0m\n"
    "\n"
    "`,\n"
    "    )\n"
    "  }"
)
new_connecting = (
    "  if (isTauri && props.tab.status === 'connecting') {\n"
    "    writeStatusLine(terminalEntry(props.tab.id), '90', t('terminal.connecting', { addr: props.tab.address }) + '...')\n"
    "  }"
)
if old_connecting not in src:
    print("FAIL: connecting block not found")
    sys.exit(1)
src = src.replace(old_connecting, new_connecting)

# 2) 已连接横幅
old_connected = (
    "    if (status === 'connected') {\n"
    "      if (props.tab.kind === 'local') return // 本地终端开箱即连，无需横幅\n"
    "      entry.term.write(`\n"
    "\n"
    + ESC + "[92m[HiShell] ${t('terminal.connected')}" + ESC + "[0m\n"
    "\n"
    "`)\n"
    "    } else if (status === 'disconnected') {"
)
new_connected = (
    "    if (status === 'connected') {\n"
    "      if (props.tab.kind === 'local') return // 本地终端开箱即连，无需横幅\n"
    "      writeStatusLine(entry, '92', t('terminal.connected'))\n"
    "    } else if (status === 'disconnected') {"
)
if old_connected not in src:
    print("FAIL: connected block not found")
    sys.exit(1)
src = src.replace(old_connected, new_connected)

# 3) 断开横幅
old_disconnected = (
    "      const text = props.tab.kind === 'local'\n"
    "        ? message || t('terminal.localExited')\n"
    "        : `${t('terminal.disconnected')}${message ? ' — ' + message : ''}`\n"
    "      entry.term.write(\n"
    "        `\n"
    "\n"
    + ESC + "[91m[HiShell] ${text}" + ESC + "[0m\n"
    "\n"
    "`,\n"
    "      )\n"
    "    }"
)
new_disconnected = (
    "      const text = props.tab.kind === 'local'\n"
    "        ? message || t('terminal.localExited')\n"
    "        : `${t('terminal.disconnected')}${message ? ' — ' + message : ''}`\n"
    "      writeStatusLine(entry, '91', text)\n"
    "    }"
)
if old_disconnected not in src:
    print("FAIL: disconnected block not found")
    sys.exit(1)
src = src.replace(old_disconnected, new_disconnected)

# 4) writeStatusLine 辅助函数（放在 onTabCommand 之前）
anchor = "/** 选项卡功能区事件（清屏/搜索/复制/粘贴/全选），按 tabId 过滤 */"
helper = """/**
 * 终端状态横幅：必须用 \\r\\n 换行——xterm 的裸 LF 只下移不回列，
 * 会导致横幅和紧随其后的服务器输出从光标所在列开始（缩进错位）。
 */
function writeStatusLine(entry: ReturnType<typeof terminalEntry>, color: '90' | '91' | '92', text: string) {
  entry?.term.write(`\\r\\n${ESC_PLACEHOLDER}[${color}m[HiShell] ${text}${ESC_PLACEHOLDER}[0m\\r\\n\\r\\n`)
}

"""
helper = helper.replace("ESC_PLACEHOLDER", "\\x1b")
if anchor not in src:
    print("FAIL: helper anchor not found")
    sys.exit(1)
src = src.replace(anchor, helper + anchor)

io.open(path, "w", encoding="utf-8", newline="\n").write(src)
print("OK")
