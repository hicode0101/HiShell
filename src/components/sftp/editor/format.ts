/**
 * 文档一键格式化：JSON（原生解析重建）、XML（标签扫描缩进）、YAML（保持内容的缩进归一）。
 * 全部为纯函数：解析失败时抛 Error（带技术细节），由调用方包装成 toast。
 */

export type FormatLang = 'json' | 'xml' | 'yaml'

/** 语言 id → 可格式化类型；不可格式化返回 null */
export function formatLangOf(langId: string | null): FormatLang | null {
  if (langId === 'json') return 'json'
  if (langId === 'xml') return 'xml'
  if (langId === 'yaml') return 'yaml'
  return null
}

export function formatDocument(text: string, lang: FormatLang, indentUnit = '  '): string {
  switch (lang) {
    case 'json':
      return formatJson(text, indentUnit)
    case 'xml':
      return formatXml(text, indentUnit)
    case 'yaml':
      return formatYaml(text, indentUnit)
  }
}

/* ---------- JSON ---------- */

function formatJson(text: string, indentUnit: string): string {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    throw new Error(`JSON.parse: ${msg}`)
  }
  return JSON.stringify(value, null, indentUnit) + '\n'
}

/* ---------- XML ----------
 * 逐 token 扫描（注释/CDATA/PI/DOCTYPE/开/闭/自闭合/文本），
 * 开标签后紧跟短文本+同名闭标签时保持单行，属性区不重排。 */

interface XmlToken {
  kind: 'open' | 'close' | 'self' | 'text' | 'comment' | 'cdata' | 'pi' | 'doctype'
  name?: string
  text: string
}

const OPEN_TAG_RE = /<(\/?)([A-Za-z_][\w:.-]*)((?:"[^"]*"|'[^']*'|[^"'>])*?)(\/?)>/

function scanXml(src: string): XmlToken[] {
  const tokens: XmlToken[] = []
  let i = 0
  while (i < src.length) {
    if (src[i] !== '<') {
      const next = src.indexOf('<', i)
      const end = next < 0 ? src.length : next
      tokens.push({ kind: 'text', text: src.slice(i, end) })
      i = end
      continue
    }
    if (src.startsWith('<!--', i)) {
      const end = src.indexOf('-->', i + 4)
      const stop = end < 0 ? src.length : end + 3
      tokens.push({ kind: 'comment', text: src.slice(i, stop) })
      i = stop
      continue
    }
    if (src.startsWith('<![CDATA[', i)) {
      const end = src.indexOf(']]>', i + 9)
      const stop = end < 0 ? src.length : end + 3
      tokens.push({ kind: 'cdata', text: src.slice(i, stop) })
      i = stop
      continue
    }
    if (src.startsWith('<?', i)) {
      const end = src.indexOf('?>', i + 2)
      const stop = end < 0 ? src.length : end + 2
      tokens.push({ kind: 'pi', text: src.slice(i, stop) })
      i = stop
      continue
    }
    if (src.startsWith('<!', i)) {
      const end = src.indexOf('>', i + 2)
      const stop = end < 0 ? src.length : end + 1
      tokens.push({ kind: 'doctype', text: src.slice(i, stop) })
      i = stop
      continue
    }
    OPEN_TAG_RE.lastIndex = i
    const m = OPEN_TAG_RE.exec(src)
    if (m && m.index === i) {
      const [full, slash, , attrs, selfClose] = m as unknown as string[] & { index: number }
      const name = m[2]!
      const cleaned = `${name}${(attrs ?? '').replace(/\s+/g, ' ').trimEnd()}`
      if (slash) tokens.push({ kind: 'close', name, text: `</${name}>` })
      else if (selfClose) tokens.push({ kind: 'self', name, text: `<${cleaned} />` })
      else tokens.push({ kind: 'open', name, text: `<${cleaned}>` })
      i += full.length
      continue
    }
    // 未知形态（如非法标签）：按文本吞掉这个 '<'，避免死循环
    tokens.push({ kind: 'text', text: '<' })
    i++
  }
  return tokens
}

function emitWithIndent(out: string[], indentUnit: string, depth: number, text: string) {
  const lines = text.replace(/\r\n?/g, '\n').split('\n')
  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed) out.push(indentUnit.repeat(depth) + trimmed)
  }
}

function formatXml(src: string, indentUnit: string): string {
  const tokens = scanXml(src)
  const out: string[] = []
  let depth = 0
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]!
    switch (t.kind) {
      case 'text': {
        if (t.text.trim()) emitWithIndent(out, indentUnit, depth, t.text)
        break
      }
      case 'open': {
        // <a>text</a> 短文本保持单行
        const text = tokens[i + 1]
        const close = tokens[i + 2]
        if (text?.kind === 'text' && text.text.trim() && close?.kind === 'close' && close.name === t.name) {
          out.push(indentUnit.repeat(depth) + t.text + text.text.trim() + close.text)
          i += 2
          break
        }
        out.push(indentUnit.repeat(depth) + t.text)
        depth++
        break
      }
      case 'close': {
        depth = Math.max(0, depth - 1)
        out.push(indentUnit.repeat(depth) + t.text)
        break
      }
      case 'self':
      case 'pi':
      case 'doctype':
        out.push(indentUnit.repeat(depth) + t.text)
        break
      case 'comment':
      case 'cdata':
        emitWithIndent(out, indentUnit, depth, t.text)
        break
    }
  }
  if (!out.length) return ''
  return out.join('\n') + '\n'
}

/* ---------- YAML ----------
 * 缩进归一化（默认 2 空格）：按原始缩进宽度的层级秩重排；
 * 注释/引号/锚点等原样保留；块标量（| 或 >）内容整体跳过，
 * 避免破坏多行字符串的内部缩进。 */

function formatYaml(src: string, indentUnit: string): string {
  const unit = indentUnit.length || 2
  const rawLines = src.replace(/\r\n?/g, '\n').split('\n')
  const out: string[] = []
  const seen: number[] = [] // 升序：已出现过的原始缩进宽度
  let blockKeyIndent: number | null = null // 块标量键行的缩进
  let inBlockScalar = false
  let blankRun = 0

  for (const raw of rawLines) {
    const text = raw.replace(/\t/g, ' '.repeat(unit)).replace(/\s+$/, '')
    if (!text.trim()) {
      // 块标量内部的空行属于内容，原样保留
      if (inBlockScalar) {
        out.push('')
        continue
      }
      blankRun++
      if (blankRun <= 1) out.push('')
      continue
    }
    blankRun = 0
    const indent = text.length - text.trimStart().length
    if (inBlockScalar) {
      if (indent > (blockKeyIndent ?? -1)) {
        out.push(text)
        continue
      }
      inBlockScalar = false
    }
    while (seen.length && seen[seen.length - 1]! > indent) seen.pop()
    if (!seen.length || seen[seen.length - 1]! < indent) seen.push(indent)
    const level = seen.indexOf(indent)
    const content = text.trim()
    out.push(indentUnit.repeat(level) + content)
    // "key: |" / "key: >-2" 等块标量头（可带行尾注释）→ 后续更深缩进行为字面量内容
    if (/:\s*[|>][\d+-]*\s*(?:#.*)?$/.test(content)) {
      inBlockScalar = true
      blockKeyIndent = indent
    }
  }
  while (out.length && out[out.length - 1] === '') out.pop()
  if (!out.length) return ''
  return out.join('\n') + '\n'
}
