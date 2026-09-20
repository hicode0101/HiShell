/**
 * HiShell 轻量 i18n 内核
 * - 语言偏好：'system' | 'en-US' | 'zh-CN'（持久化于 settings store）
 * - 解析：跟随系统时按 navigator.language 前缀匹配；匹配不到默认英语
 * - t() 响应式：语言切换后所有使用 t 的组件自动重渲染
 */
import { ref } from 'vue'
import zhCN from './locales/zh-CN'
import enUS from './locales/en-US'

export type Locale = 'zh-CN' | 'en-US'
export type LangPref = 'system' | Locale

const messages: Record<Locale, Record<string, unknown>> = {
  'zh-CN': zhCN,
  'en-US': enUS,
}

/** 当前生效语言（响应式） */
export const resolvedLocale = ref<Locale>(resolveSystem())

/** 系统语言匹配：zh* → 简体中文，en* → 英语，其余默认英语 */
export function resolveSystem(): Locale {
  const lang = (typeof navigator !== 'undefined' ? navigator.language : 'en') || 'en'
  const lower = lang.toLowerCase()
  if (lower.startsWith('zh')) return 'zh-CN'
  return 'en-US'
}

export function setLanguagePref(pref: LangPref) {
  resolvedLocale.value = pref === 'system' ? resolveSystem() : pref
}

/** 当前语言编码（如 zh-CN），供设置页显示 */
export function languageCode(): Locale {
  return resolvedLocale.value
}

export function systemLanguageCode(): string {
  return (typeof navigator !== 'undefined' && navigator.language) || 'en-US'
}

/** 翻译：支持 {param} 插值；缺失键回落英语，再缺失返回键名 */
export function t(path: string, params?: Record<string, string | number>): string {
  const walk = (obj: Record<string, unknown> | undefined): unknown =>
    path.split('.').reduce<unknown>((o, k) => (o == null ? undefined : (o as Record<string, unknown>)[k]), obj)
  let s: unknown = walk(messages[resolvedLocale.value])
  if (typeof s !== 'string') s = walk(messages['en-US'])
  if (typeof s !== 'string') return path
  let out: string = s
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      out = out.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
    }
  }
  return out
}
