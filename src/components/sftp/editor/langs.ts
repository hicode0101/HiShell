/**
 * 编辑器语言识别：按扩展名/常见文件名映射到 CodeMirror 语言扩展。
 * 支持官方语言包 + legacy-modes（shell/ini/toml 等），未识别时回退纯文本。
 */
import type { Extension } from '@codemirror/state'
import { StreamLanguage } from '@codemirror/language'
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'
import { yaml } from '@codemirror/lang-yaml'
import { php } from '@codemirror/lang-php'
import { python } from '@codemirror/lang-python'
import { javascript } from '@codemirror/lang-javascript'
import { css } from '@codemirror/lang-css'
import { html } from '@codemirror/lang-html'
import { shell } from '@codemirror/legacy-modes/mode/shell'
import { properties } from '@codemirror/legacy-modes/mode/properties'
import { toml } from '@codemirror/legacy-modes/mode/toml'
import { lua } from '@codemirror/legacy-modes/mode/lua'
import { ruby } from '@codemirror/legacy-modes/mode/ruby'
import { perl } from '@codemirror/legacy-modes/mode/perl'
import { sql } from '@codemirror/legacy-modes/mode/sql'
import { c, cpp, java, csharp, objectiveC } from '@codemirror/legacy-modes/mode/clike'
import { go } from '@codemirror/legacy-modes/mode/go'
import { rust } from '@codemirror/legacy-modes/mode/rust'
import { standardSQL } from '@codemirror/legacy-modes/mode/sql'
import { bat } from './batMode'

export interface EditorLang {
  /** 语言 id（同时用于格式化判定：json/xml/yaml） */
  id: string
  /** 状态栏展示名 */
  label: string
  /** 缩进单元 */
  indent: string
  /** CodeMirror 语言扩展工厂 */
  support: () => Extension
}

const SP2 = '  '
const SP4 = '    '

const LANGS: Record<string, EditorLang> = {
  json: { id: 'json', label: 'JSON', indent: SP2, support: () => json() },
  xml: { id: 'xml', label: 'XML', indent: SP2, support: () => xml() },
  yaml: { id: 'yaml', label: 'YAML', indent: SP2, support: () => yaml() },
  php: { id: 'php', label: 'PHP', indent: SP4, support: () => php() },
  python: { id: 'python', label: 'Python', indent: SP4, support: () => python() },
  shell: { id: 'shell', label: 'Shell', indent: SP4, support: () => StreamLanguage.define(shell) },
  ini: { id: 'ini', label: 'INI', indent: SP4, support: () => StreamLanguage.define(properties) },
  toml: { id: 'toml', label: 'TOML', indent: SP2, support: () => StreamLanguage.define(toml) },
  javascript: {
    id: 'javascript',
    label: 'JavaScript',
    indent: SP2,
    support: () => javascript(),
  },
  typescript: {
    id: 'typescript',
    label: 'TypeScript',
    indent: SP2,
    support: () => javascript({ typescript: true }),
  },
  jsx: {
    id: 'jsx',
    label: 'JSX',
    indent: SP2,
    support: () => javascript({ jsx: true }),
  },
  css: { id: 'css', label: 'CSS', indent: SP2, support: () => css() },
  html: { id: 'html', label: 'HTML', indent: SP2, support: () => html() },
  lua: { id: 'lua', label: 'Lua', indent: SP2, support: () => StreamLanguage.define(lua) },
  ruby: { id: 'ruby', label: 'Ruby', indent: SP2, support: () => StreamLanguage.define(ruby) },
  perl: { id: 'perl', label: 'Perl', indent: SP4, support: () => StreamLanguage.define(perl) },
  sql: { id: 'sql', label: 'SQL', indent: SP4, support: () => StreamLanguage.define(standardSQL) },
  c: { id: 'c', label: 'C', indent: SP4, support: () => StreamLanguage.define(c) },
  cpp: { id: 'cpp', label: 'C++', indent: SP4, support: () => StreamLanguage.define(cpp) },
  java: { id: 'java', label: 'Java', indent: SP4, support: () => StreamLanguage.define(java) },
  csharp: { id: 'csharp', label: 'C#', indent: SP4, support: () => StreamLanguage.define(csharp) },
  objc: { id: 'objc', label: 'Objective-C', indent: SP4, support: () => StreamLanguage.define(objectiveC) },
  go: { id: 'go', label: 'Go', indent: SP2, support: () => StreamLanguage.define(go) },
  rust: { id: 'rust', label: 'Rust', indent: SP4, support: () => StreamLanguage.define(rust) },
  bat: { id: 'bat', label: 'Batch', indent: SP4, support: () => bat },
}

/** 扩展名 → 语言 */
const EXT_LANG: Record<string, EditorLang> = {
  json: LANGS.json!,
  jsonc: LANGS.json!,
  xml: LANGS.xml!,
  xaml: LANGS.xml!,
  svg: LANGS.xml!,
  plist: LANGS.xml!,
  csproj: LANGS.xml!,
  vbproj: LANGS.xml!,
  axml: LANGS.xml!,
  yaml: LANGS.yaml!,
  yml: LANGS.yaml!,
  php: LANGS.php!,
  php3: LANGS.php!,
  php4: LANGS.php!,
  php5: LANGS.php!,
  phtml: LANGS.php!,
  py: LANGS.python!,
  pyw: LANGS.python!,
  sh: LANGS.shell!,
  bash: LANGS.shell!,
  zsh: LANGS.shell!,
  ksh: LANGS.shell!,
  ini: LANGS.ini!,
  cfg: LANGS.ini!,
  conf: LANGS.ini!,
  env: LANGS.ini!,
  properties: LANGS.ini!,
  service: LANGS.ini!,
  toml: LANGS.toml!,
  js: LANGS.javascript!,
  mjs: LANGS.javascript!,
  cjs: LANGS.javascript!,
  ts: LANGS.typescript!,
  mts: LANGS.typescript!,
  jsx: LANGS.jsx!,
  tsx: LANGS.jsx!,
  css: LANGS.css!,
  scss: LANGS.css!,
  less: LANGS.css!,
  html: LANGS.html!,
  htm: LANGS.html!,
  vue: LANGS.html!,
  lua: LANGS.lua!,
  rb: LANGS.ruby!,
  pl: LANGS.perl!,
  pm: LANGS.perl!,
  sql: LANGS.sql!,
  c: LANGS.c!,
  h: LANGS.c!,
  cpp: LANGS.cpp!,
  cc: LANGS.cpp!,
  cxx: LANGS.cpp!,
  hpp: LANGS.cpp!,
  hxx: LANGS.cpp!,
  java: LANGS.java!,
  cs: LANGS.csharp!,
  m: LANGS.objc!,
  go: LANGS.go!,
  rs: LANGS.rust!,
  bat: LANGS.bat!,
  cmd: LANGS.bat!,
}

/** 特殊文件名（无扩展名或以点开头的配置文件）→ 语言 */
const NAME_LANG: Record<string, EditorLang> = {
  makefile: LANGS.shell!,
  dockerfile: LANGS.shell!,
  bashrc: LANGS.shell!,
  zshrc: LANGS.shell!,
  profile: LANGS.shell!,
  bash_profile: LANGS.shell!,
  gitignore: LANGS.ini!,
  gitconfig: LANGS.ini!,
  editorconfig: LANGS.ini!,
  passwd: LANGS.ini!,
  hosts: LANGS.ini!,
  crontab: LANGS.shell!,
}

/** 无扩展名时取整个名字，有扩展名时取扩展名 */
function extOf(name: string): string {
  const base = name.toLowerCase()
  const dot = base.lastIndexOf('.')
  if (dot <= 0) return base
  return base.slice(dot + 1)
}

/** 按文件名识别语言；未识别返回 null（纯文本） */
export function detectLang(fileName: string): EditorLang | null {
  const base = fileName.toLowerCase().replace(/\\/g, '/').split('/').pop() ?? fileName
  const byName = NAME_LANG[base]
  if (byName) return byName
  return EXT_LANG[extOf(base)] ?? null
}

/** 无法识别高亮语言时状态栏展示用：取扩展名大写，否则「纯文本」 */
export function plainLabel(fileName: string): string {
  const base = fileName.toLowerCase().replace(/\\/g, '/').split('/').pop() ?? fileName
  const dot = base.lastIndexOf('.')
  if (dot > 0 && dot < base.length - 1) return base.slice(dot + 1).toUpperCase()
  return base.toUpperCase() || 'TEXT'
}
