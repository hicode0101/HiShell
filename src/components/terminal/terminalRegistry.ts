/**
 * 终端实例注册表：每个会话标签一个 xterm 实例，切换标签时挂/卸 DOM，
 * 保留回滚缓冲；标签关闭时释放。
 * M1 起数据源从 MockShell 换为 ssh://{sid}/data 事件。
 */
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { useSettingsStore } from '../../stores/settings'
import { t as translate } from '../../i18n'
import { isTauri } from '../../api/adapter'
import type { SessionTab } from '../../api/types'

import '@xterm/xterm/css/xterm.css'

export interface TerminalEntry {
  term: Terminal
  fit: FitAddon
  search: SearchAddon
  /** 仅浏览器 mock 模式存在 */
  shell?: MockShell
  resizeObserver?: ResizeObserver
  /** 真实模式：输入/尺寸是否已接线 */
  wired: boolean
}

const registry = new Map<string, TerminalEntry>()

// 调试句柄（原型阶段临时暴露）
if (typeof window !== 'undefined') {
  ;(window as unknown as Record<string, unknown>).__termReg = registry
}

function lightTheme() {
  return {
    background: '#0c0c0c',
    foreground: '#cccccc',
    cursor: '#cccccc',
    selectionBackground: 'rgba(76,194,255,.35)',
    black: '#0c0c0c',
    red: '#e3564c',
    green: '#5dbb63',
    yellow: '#f5c451',
    blue: '#4cc2ff',
    magenta: '#e58ee5',
    cyan: '#4dc6c6',
    white: '#cccccc',
  }
}
// 原型阶段：浅色应用主题下终端同样使用深色底（对齐 Xshell 惯例）；
// 浅色终端配色作为 M5 设置项再开放。
function darkTheme() {
  return lightTheme()
}

export function getTerminal(tab: SessionTab): TerminalEntry {
  let entry = registry.get(tab.id)
  if (!entry) {
    const settings = useSettingsStore()
    const term = new Terminal({
      fontFamily: settings.s.terminalFontFamily,
      fontSize: settings.s.terminalFontSize,
      lineHeight: 1.2,
      cursorBlink: true,
      allowProposedApi: true,
      scrollback: 5000,
      theme: settings.theme === 'dark' ? darkTheme() : lightTheme(),
    })
    const fit = new FitAddon()
    const search = new SearchAddon()
    term.loadAddon(fit)
    term.loadAddon(search)

    entry = { term, fit, search, wired: false }
    registry.set(tab.id, entry)
    // 尺寸变化时同步到会话标签（状态栏显示 cols×rows）
    term.onResize(({ cols, rows }) => {
      tab.cols = cols
      tab.rows = rows
    })
    if (!isTauri) {
      // 浏览器预览：本地模拟 shell
      entry.shell = new MockShell(term, tab)
      entry.shell.start()
    }
  }
  return entry
}

export function attachTerminal(tab: SessionTab, container: HTMLElement) {
  const entry = getTerminal(tab)
  if (entry.resizeObserver) {
    entry.resizeObserver.disconnect()
    entry.resizeObserver = undefined
  }
  // xterm 实例只允许 open() 一次；再次显示时直接搬移已有 DOM element
  container.innerHTML = ''
  if (entry.term.element) {
    container.appendChild(entry.term.element)
  } else {
    entry.term.open(container)
  }
  entry.fit.fit()
  // 渲染器说明：原型阶段使用 DOM 渲染器（稳定可靠）；WebGL 渲染在 M5 里程碑
  // 参考 Tabby 的 enableWebGL + 上下文丢失恢复策略再启用。
  entry.resizeObserver = new ResizeObserver(() => {
    try {
      entry!.fit.fit()
    } catch {
      /* 容器尺寸为 0 时忽略 */
    }
  })
  entry.resizeObserver.observe(container)
  entry.term.focus()
  return entry
}

export function detachTerminal(tabId: string) {
  const entry = registry.get(tabId)
  entry?.resizeObserver?.disconnect()
  if (entry) entry.resizeObserver = undefined
}

export function disposeTerminal(tabId: string) {
  const entry = registry.get(tabId)
  if (!entry) return
  entry.resizeObserver?.disconnect()
  entry.shell?.dispose()
  entry.term.dispose()
  registry.delete(tabId)
}

export function terminalEntry(tabId: string) {
  return registry.get(tabId)
}

/** 应用设置变化（字体/主题）到所有已存在的终端 */
export function applySettingsToTerminals() {
  const settings = useSettingsStore()
  for (const entry of registry.values()) {
    entry.term.options.fontFamily = settings.s.terminalFontFamily
    entry.term.options.fontSize = settings.s.terminalFontSize
    entry.term.options.theme = settings.theme === 'dark' ? darkTheme() : lightTheme()
    try {
      entry.fit.fit()
    } catch {
      /* ignore */
    }
  }
}

/**
 * Mock Shell：本地回显 + 假提示符。
 * M1 起由真实 SSH 数据通道替换（本类整体删除）。
 */
export class MockShell {
  private term: Terminal
  private tab: SessionTab
  private line = ''
  private history: string[] = []
  private historyIdx = -1
  private dataHandler: (d: string) => void

  constructor(term: Terminal, tab: SessionTab) {
    this.term = term
    this.tab = tab
    this.dataHandler = (d) => this.onData(d)
    term.onData(this.dataHandler)
  }

  start() {
    const t = this.term
    t.writeln('')
    t.writeln(`\x1b[38;5;39m  ${translate('mockShell.title')}\x1b[0m`)
    t.writeln('  ------------------------------------------')
    t.writeln('  ' + translate('mockShell.line1'))
    t.writeln(
      translate('mockShell.line2a') +
        '\x1b[32m' + translate('mockShell.help') + '\x1b[0m' +
        translate('mockShell.line2b'),
    )
    t.writeln('')
    this.prompt()
  }

  private prompt() {
    this.term.write(`\r\n\x1b[38;5;46m${this.tab.title}\x1b[0m:\x1b[38;5;33m~\x1b[0m# `)
    this.line = ''
  }

  private onData(data: string) {
    for (const ch of data) {
      if (ch === '\r') {
        this.exec(this.line.trim())
      } else if (ch === '\x7f') {
        if (this.line.length > 0) {
          this.line = this.line.slice(0, -1)
          this.term.write('\b \b')
        }
      } else if (ch === '\x03') {
        this.term.write('^C')
        this.prompt()
      } else if (ch === '\x04') {
        this.tab.status = 'disconnected'
        this.term.writeln('\r\n\x1b[90m' + translate('mockShell.closed') + '\x1b[0m')
      } else if (ch >= ' ') {
        this.line += ch
        this.term.write(ch)
      }
    }
  }

  private exec(cmd: string) {
    if (cmd) this.history.unshift(cmd)
    this.historyIdx = -1
    const t = this.term
    const [name, ...args] = cmd.split(/\s+/)
    switch (name) {
      case '':
        break
      case 'help':
        t.writeln('\r\n' + translate('mockShell.helpBody') + '\r\n' +
          '  ' + translate('mockShell.helpLs') + '\r\n' +
          '  ' + translate('mockShell.helpEcho') + '\r\n' +
          '  ' + translate('mockShell.helpClear') + '\r\n' +
          '  ' + translate('mockShell.helpExit'))
        break
      case 'ls':
        t.writeln('\r\n\x1b[38;5;33mcert\x1b[0m  docker-compose.yml  app.log  backup-20260918.tar.gz')
        break
      case 'pwd':
        t.writeln('\r\n/root')
        break
      case 'whoami':
        t.writeln('\r\nroot')
        break
      case 'uname':
        t.writeln('\r\nLinux localhost 5.15.0-91-generic #96-Ubuntu SMP x86_64 GNU/Linux')
        break
      case 'date':
        t.writeln('\r\n' + new Date().toString())
        break
      case 'echo':
        t.writeln('\r\n' + args.join(' '))
        break
      case 'clear':
        t.clear()
        break
      case 'exit':
        this.tab.status = 'disconnected'
        t.writeln('\r\n\x1b[90m' + translate('mockShell.sessionEnded') + '\x1b[0m')
        return
      default:
        t.writeln('\r\n' + translate('mockShell.notFound', { cmd: name }))
    }
    this.prompt()
  }

  dispose() {
    // xterm 的 onData 监听随 term.dispose 一并释放
  }
}
