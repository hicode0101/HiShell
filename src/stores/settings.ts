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
  /** SFTP 文件栏收藏路径（本地 / 远程各自独立） */
  sftpFavorites: { local: string[]; remote: string[] }
  /** 实时显示 AI 通过 MCP 执行的操作（主窗口右下角活动面板），默认关闭 */
  mcpShowActivity: boolean
}

export type SftpSide = 'local' | 'remote'

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
    sftpFavorites: { local: [], remote: [] },
    mcpShowActivity: false,
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
  state: () => ({ s: load() as AppSettings, settingsNav: 'appearance' }),
  getters: {
    theme: (st) => st.s.theme,
    accent: (st) => st.s.accent,
  },
  actions: {
    /** 设置页当前子选项卡（内存态）：切换主 tab 不丢失，关 tab 随会话走 */
    setSettingsNav(id: string) {
      this.settingsNav = id
    },
    patch(p: Partial<AppSettings>) {
      this.s = { ...this.s, ...p }
      localStorage.setItem(LS_KEY, JSON.stringify(this.s))
    },
    /** 收藏/取消收藏路径（已收藏则移除），返回 true = 新增收藏 */
    toggleSftpFavorite(side: SftpSide, path: string): boolean {
      const list = this.s.sftpFavorites[side]
      const idx = list.indexOf(path)
      if (idx >= 0) list.splice(idx, 1)
      else list.push(path)
      this.persistSftpFavorites()
      return idx < 0
    },
    /** 从收藏列表中删除指定路径 */
    removeSftpFavorite(side: SftpSide, path: string) {
      const list = this.s.sftpFavorites[side]
      const idx = list.indexOf(path)
      if (idx < 0) return
      list.splice(idx, 1)
      this.persistSftpFavorites()
    },
    persistSftpFavorites() {
      const fav = this.s.sftpFavorites
      this.patch({ sftpFavorites: { local: [...fav.local], remote: [...fav.remote] } })
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
