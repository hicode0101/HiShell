/** 应用设置（主题/语言/终端外观），持久化 localStorage */
import { defineStore } from 'pinia'
import { watch } from 'vue'
import { setLanguagePref, type LangPref } from '../i18n'

export type ThemeMode = 'light' | 'dark'
/** 终端配色方案：固定深色 / 固定浅色 / 跟随应用主题 */
export type TerminalColorScheme = 'dark' | 'light' | 'followApp'

export interface AppSettings {
  theme: ThemeMode
  accent: string
  language: LangPref
  terminalFontFamily: string
  terminalFontSize: number
  terminalColorScheme: TerminalColorScheme
}

const LS_KEY = 'hishell.settings'

export const ACCENT_PRESETS = ['#005fb8', '#0078d4', '#00b294', '#498205', '#c239b3', '#ca5010', '#744da9', '#c42b1c']

function defaults(): AppSettings {
  return {
    theme: 'light',
    accent: '#005fb8',
    language: 'system',
    terminalFontFamily: "'Cascadia Mono', 'Cascadia Code', Consolas, monospace",
    terminalFontSize: 14,
    terminalColorScheme: 'dark',
  }
}

function load(): AppSettings {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (raw) return { ...defaults(), ...JSON.parse(raw) }
  } catch {
    /* ignore */
  }
  return defaults()
}

export const useSettingsStore = defineStore('settings', {
  state: () => ({ s: load() as AppSettings }),
  getters: {
    theme: (st) => st.s.theme,
    accent: (st) => st.s.accent,
  },
  actions: {
    patch(p: Partial<AppSettings>) {
      this.s = { ...this.s, ...p }
      localStorage.setItem(LS_KEY, JSON.stringify(this.s))
    },
    applyToDocument() {
      const apply = () => {
        document.documentElement.dataset.theme = this.s.theme
        document.documentElement.style.setProperty('--accent', this.s.accent)
        // 深色主题下 accent 文字反色
        document.documentElement.style.setProperty(
          '--accent-text-on',
          this.s.theme === 'dark' ? '#002744' : '#ffffff',
        )
        // 语言：同步到 i18n 内核（t() 响应式刷新全部界面文案）
        setLanguagePref(this.s.language)
      }
      apply()
      watch(
        () => ({ ...this.s }),
        () => apply(),
        { deep: true },
      )
    },
  },
})
