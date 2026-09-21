# -*- coding: utf-8 -*-
"""TerminalPane.vue 补丁：本地终端状态横幅特化 + 本地数据接线"""
import io, sys

path = r"D:\AiWork\work-2\HiShell\src\components\terminal\TerminalPane.vue"
src = io.open(path, encoding="utf-8").read()

ESC = "\x1b"

old_connected = (
    "    if (status === 'connected') {\n"
    "      entry.term.write(`\n"
    "\n"
    + ESC + "[92m[HiShell] ${t('terminal.connected')}" + ESC + "[0m\n"
    "\n"
    "`)\n"
    "    } else if (status === 'disconnected') {\n"
    "      entry.term.write(\n"
    "        `\n"
    "\n"
    + ESC + "[91m[HiShell] ${t('terminal.disconnected')}${message ? ' — ' + message : ''}" + ESC + "[0m\n"
    "\n"
    "`,\n"
    "      )\n"
    "    }"
)

new_connected = (
    "    if (status === 'connected') {\n"
    "      if (props.tab.kind === 'local') return // 本地终端开箱即连，无需横幅\n"
    "      entry.term.write(`\n"
    "\n"
    + ESC + "[92m[HiShell] ${t('terminal.connected')}" + ESC + "[0m\n"
    "\n"
    "`)\n"
    "    } else if (status === 'disconnected') {\n"
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

if old_connected not in src:
    print("FAIL: connected block not found")
    sys.exit(1)
src = src.replace(old_connected, new_connected)

# 在 sid watch 之后追加本地终端 lid 接线
anchor = (
    "    // 首次尺寸同步\n"
    "    if (entry) sshResize(sid, entry.term.cols, entry.term.rows).catch(() => {})\n"
    "  },\n"
    "  { immediate: true },\n"
    ")\n"
)
addition = anchor + """
// 本地终端（lid）：订阅 local-data，输入/尺寸走 local_* 命令
watch(
  () => props.tab.lid,
  (lid) => {
    if (!isTauri || lid === undefined) return
    if (unlistenLocal) unlistenLocal()
    unlistenLocal = onLocalDataFor(lid, (bytes) => {
      resolveEntryByLid(lid)?.term.write(bytes)
    })
    const entry = resolveEntryByLid(lid)
    if (entry && !entry.localWired) {
      entry.localWired = true
      const encoder = new TextEncoder()
      entry.term.onData((d) => {
        const tab = useSessionsStore().tabs.find((x) => x.lid === lid)
        if (tab?.lid !== undefined) {
          localWrite(tab.lid, encoder.encode(d)).catch(() => {})
        }
      })
    }
    if (entry) localResize(lid, entry.term.cols, entry.term.rows).catch(() => {})
  },
  { immediate: true },
)
"""
if anchor not in src:
    print("FAIL: sid watch anchor not found")
    sys.exit(1)
src = src.replace(anchor, addition)

# unlistenLocal 声明 + resolveEntryByLid + 清理
src = src.replace(
    "let container: HTMLElement | null = null\nlet unlistenData: (() => void) | null = null",
    "let container: HTMLElement | null = null\nlet unlistenData: (() => void) | null = null\nlet unlistenLocal: (() => void) | null = null",
)
src = src.replace(
    """function resolveEntryBySid(sid: number) {
  const tab = useSessionsStore().tabs.find((t) => t.sid === sid)
  return tab ? terminalEntry(tab.id) : undefined
}""",
    """function resolveEntryBySid(sid: number) {
  const tab = useSessionsStore().tabs.find((t) => t.sid === sid)
  return tab ? terminalEntry(tab.id) : undefined
}

function resolveEntryByLid(lid: number) {
  const tab = useSessionsStore().tabs.find((t) => t.lid === lid)
  return tab ? terminalEntry(tab.id) : undefined
}""",
)
src = src.replace(
    """onUnmounted(() => {
  window.removeEventListener('hishell-tab-command', onTabCommand)
  if (unlistenData) unlistenData()
  unlistenData = null
  detachTerminal(props.tab.id)
})""",
    """onUnmounted(() => {
  window.removeEventListener('hishell-tab-command', onTabCommand)
  if (unlistenData) unlistenData()
  unlistenData = null
  if (unlistenLocal) unlistenLocal()
  unlistenLocal = null
  detachTerminal(props.tab.id)
})""",
)

# 设置项 watch 增加配色方案
src = src.replace(
    "  () => [settings.s.theme, settings.s.terminalFontFamily, settings.s.terminalFontSize],",
    "  () => [settings.s.theme, settings.s.terminalColorScheme, settings.s.terminalFontFamily, settings.s.terminalFontSize],",
)

io.open(path, "w", encoding="utf-8", newline="\n").write(src)
print("OK")
