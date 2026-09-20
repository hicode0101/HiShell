/** 连接配置（Profile）管理：Tauri 环境走后端 ProfileStore（凭据进 keyring），浏览器走 mock */
import { defineStore } from 'pinia'
import { isTauri } from '../api/adapter'
import { profileDelete as apiProfileDelete, profileList, profileSave, type ProfileSecretsIn } from '../api/backend'
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
    async remove(id: string) {
      if (isTauri) await apiProfileDelete(id).catch(() => {})
      mockProfileStore.remove(id)
      this.profiles = this.profiles.filter((p) => p.id !== id)
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
