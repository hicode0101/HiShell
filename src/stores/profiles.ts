/** 连接配置（Profile）管理：Tauri 环境走后端 ProfileStore（凭据进 keyring），浏览器走 mock */
import { defineStore } from 'pinia'
import { isTauri } from '../api/adapter'
import { profileDelete as apiProfileDelete, profileList, profileReorder, profileSave, type ProfileSecretsIn } from '../api/backend'
import { mockProfileStore } from '../api/mock'
import { defaultProfile, type ProxyConfig, type SshProfile } from '../api/types'

export const useProfilesStore = defineStore('profiles', {
  state: () => ({
    profiles: [] as SshProfile[],
    loaded: false,
  }),
  getters: {
    byId: (st) => (id: string) => st.profiles.find((p) => p.id === id),
    jumpCandidates: (st) => st.profiles.filter((p) => p.proxy.kind === 'none'),
  },
  actions: {
    async init() {
      if (this.loaded) return
      if (isTauri) {
        try {
          this.profiles = await profileList()
        } catch (e) {
          console.warn('profile_list 失败，使用空列表', e)
          this.profiles = []
        }
      } else {
        this.profiles = mockProfileStore.list()
      }
      this.loaded = true
    },
    async upsert(p: SshProfile, secrets?: ProfileSecretsIn): Promise<SshProfile> {
      if (isTauri) {
        try {
          const saved = await profileSave(p, secrets)
          const idx = this.profiles.findIndex((x) => x.id === saved.id)
          if (idx >= 0) this.profiles[idx] = saved
          else this.profiles.push(saved)
          return saved
        } catch (e) {
          console.warn('profile_save 失败，回退本地', e)
          mockProfileStore.upsert(p)
          this.reloadLocal(p)
          return p
        }
      } else {
        mockProfileStore.upsert(p)
        this.reloadLocal(p)
        return p
      }
    },
    reloadLocal(p: SshProfile) {
      const idx = this.profiles.findIndex((x) => x.id === p.id)
      if (idx >= 0) this.profiles[idx] = p
      else this.profiles.push(p)
    },
    /** 后端已落库的配置（如复制生成）直接入列表 */
    pushLoaded(p: SshProfile) {
      if (!this.profiles.some((x) => x.id === p.id)) this.profiles.push(p)
    },
    async remove(id: string) {
      if (isTauri) await apiProfileDelete(id).catch(() => {})
      mockProfileStore.remove(id)
      this.profiles = this.profiles.filter((p) => p.id !== id)
    },
    /** 会话管理器拖拽排序：本地立即交换，拖完由 persistOrder 持久化 */
    moveRow(from: number, to: number) {
      if (from === to || to < 0 || from < 0 || to >= this.profiles.length || from >= this.profiles.length) return
      const [it] = this.profiles.splice(from, 1)
      this.profiles.splice(to, 0, it)
    },
    async persistOrder() {
      if (!isTauri) return
      await profileReorder(this.profiles.map((p) => p.id)).catch((e) => console.warn('排序保存失败', e))
    },
  },
})

/** 解析快速连接串 ssh://user@host:port 或 user@host:port */
export function parseQuickConnect(input: string): { username: string; host: string; port: number } | null {
  let s = input.trim()
  if (!s) return null
  if (s.startsWith('ssh://')) s = s.slice(6)
  const m = s.match(/^(?:([^@/@]+)@)?\[?([^\]@/:]+)\]?:(\d+)?$/)
  if (!m) return null
  return {
    username: m[1] || 'root',
    host: m[2],
    port: m[3] ? parseInt(m[3], 10) : 22,
  }
}

/** 快速连接生成临时 profile（不持久化） */
export function quickProfile(info: { username: string; host: string; port: number }): SshProfile {
  return defaultProfile({ name: info.host, host: info.host, port: info.port, username: info.username })
}
