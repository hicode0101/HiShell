/**
 * Windows 批处理（.bat/.cmd）简易高亮：StreamLanguage 逐行词法。
 * 覆盖：REM / :: 注释、:标签、%VAR% / %1 / %%i / %~dp0 / !VAR! 变量、
 * "字符串"、常用关键字与外部命令、|&<>()=^ 操作符。
 * legacy-modes 无批处理模式，故自行实现。
 */
import { StreamLanguage } from '@codemirror/language'

/** 控制流与内部关键字 */
const KEYWORDS = new Set([
  'if', 'else', 'for', 'in', 'do', 'goto', 'call', 'exit', 'set', 'setlocal', 'endlocal',
  'shift', 'echo', 'off', 'on', 'pause', 'cls', 'not', 'exist', 'defined', 'errorlevel',
  'equ', 'neq', 'lss', 'leq', 'gtr', 'geq', 'enabledelayedexpansion', 'disabledelayedexpansion',
  'enableextensions', 'disableextensions',
])

/** 常见外部命令 / 系统工具 */
const COMMANDS = new Set([
  'cd', 'chdir', 'dir', 'del', 'erase', 'copy', 'xcopy', 'robocopy', 'move', 'ren', 'rename',
  'md', 'mkdir', 'rd', 'rmdir', 'type', 'attrib', 'net', 'sc', 'reg', 'tasklist', 'taskkill',
  'start', 'wmic', 'powershell', 'curl', 'findstr', 'find', 'sort', 'setx', 'where', 'fc',
  'comp', 'icacls', 'cacls', 'chcp', 'color', 'date', 'time', 'title', 'ver', 'vol', 'label',
  'chkdsk', 'ping', 'nslookup', 'ipconfig', 'tracert', 'route', 'arp', 'shutdown', 'runas',
  'msiexec', 'certutil', 'bitsadmin', 'schtasks', 'dism', 'sfc', 'bcdedit', 'pushd', 'popd',
  'diskpart', 'expand', 'makecab', 'tar', 'more', 'tree', 'systeminfo', 'forfiles', 'takeown',
  'wevtutil', 'vssadmin', 'driverquery', 'openfiles', 'compact', 'cipher', 'path', 'prompt',
  'verify', 'assoc', 'ftype', 'subst',
])

interface BatState {
  /** 命令位置（行首或 & ( 之后）：此处 REM/:: 是注释 */
  cmdPos: boolean
}

export const bat = StreamLanguage.define({
  name: 'batch',
  startState(): BatState {
    return { cmdPos: true }
  },
  token(stream, state) {
    // sol 须在吃掉行首空白前判断
    const sol = stream.sol()
    stream.eatSpace()
    if (stream.eol()) return null
    const cmdPos = sol || state.cmdPos

    // 注释：REM / ::（仅在命令位置；:: 同时是标签写法）
    if (cmdPos && stream.match(/^rem\b/i)) {
      stream.skipToEnd()
      return 'comment'
    }
    if (cmdPos && stream.match(/^::/)) {
      stream.skipToEnd()
      return 'comment'
    }
    state.cmdPos = false

    // 标签 :label（仅行首，避免误伤路径中的盘符/冒号）
    if (sol && stream.match(/^:[A-Za-z_][\w.-]*/)) return 'meta'

    // 变量：%~dp0、%%i（for）、%*、%1、%VAR%、%VAR:old=new%、!VAR!
    if (stream.match(/^%~[\w.-]*/) || stream.match(/^%%[a-z_]/i) || stream.match(/^%\*/) || stream.match(/^%\d+/) || stream.match(/^%[^%\s]*%/) || stream.match(/^![\w.-]+!/)) {
      return 'variable'
    }

    // 字符串（"" 转义同义，简单吞到下一个引号或行尾）
    if (stream.peek() === '"') {
      stream.next()
      while (!stream.eol()) {
        if (stream.next() === '"') break
      }
      return 'string'
    }

    // 操作符：管道/重定向/逻辑/转义/括号/比较
    if (stream.match(/^[|&<>^()]+/)) {
      state.cmdPos = true
      return 'operator'
    }
    if (stream.match(/^={1,2}/)) return 'operator'

    if (stream.match(/^\d+/)) return 'number'

    // 捕获组取词：current() 会带上 eatSpace 吃掉的前导空格，导致关键词比对落空
    const m = stream.match(/^([\w.-]+)/)
    if (m && Array.isArray(m)) {
      const w = m[1]!.toLowerCase()
      if (KEYWORDS.has(w)) return 'keyword'
      if (COMMANDS.has(w)) return 'builtin'
      return null
    }
    stream.next()
    return null
  },
})
