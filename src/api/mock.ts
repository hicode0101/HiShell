/** 浏览器预览模式的 mock 数据与内存 profile 存储 */
import { sleep } from './adapter'
import type { DirEntry, SshProfile } from './types'
import { defaultProfile, type ProxyConfig } from './types'

export { sleep }

const day = 24 * 3600 * 1000
const now = Date.now()

function file(size: number, ageDays = 1): MockNode {
  return { kind: 'file', size, mtime: now - ageDays * day }
}
function dir(children: Record<string, MockNode> = {}, ageDays = 3): MockNode {
  return { kind: 'dir', mtime: now - ageDays * day, children }
}

interface MockNode {
  kind: 'dir' | 'file'
  size?: number
  mtime: number
  children?: Record<string, MockNode>
}

export const mockSpecialDirs: Record<string, string> = {
  home: 'C:\\Users\\demo',
  desktop: 'C:\\Users\\demo\\Desktop',
  documents: 'C:\\Users\\demo\\Documents',
  downloads: 'C:\\Users\\demo\\Downloads',
  pictures: 'C:\\Users\\demo\\Pictures',
  music: 'C:\\Users\\demo\\Music',
  videos: 'C:\\Users\\demo\\Videos',
}

const mockFs: Record<string, MockNode> = {
  'C:\\Users\\demo\\Desktop': dir({
    部署脚本: dir({ 'deploy.ps1': file(4_212, 5), 'rollback.sh': file(2_310, 5) }, 6),
    项目资料: dir({ '架构设计.docx': file(1_048_576, 0.2), '接口清单.xlsx': file(88_233, 2) }, 0.5),
    '截图 2026-09-18.png': file(1_852_214, 2),
    'HiShell-原型说明.md': file(24_118, 0.1),
  }),
  'C:\\Users\\demo\\Documents': dir({
    会议纪要: dir({ '0918-周会.md': file(12_002, 2) }, 2),
    '服务器台账.xlsx': file(66_512, 8),
    'SSH-密钥说明.txt': file(2_204, 15),
  }),
  'C:\\Users\\demo\\Downloads': dir({
    'xshell-8-setup.exe': file(52_428_800, 0.5),
    'russh-docs.pdf': file(8_912_884, 3),
    'fast-socks5-0.1.2.crate': file(412_883, 9),
    '证书包.zip': file(1_048_576, 4),
  }),
  'C:\\Users\\demo\\Pictures': dir({ '壁纸-1.jpg': file(3_145_728, 30), '壁纸-2.jpg': file(2_621_440, 30) }),
  'C:\\Users\\demo\\Music': dir({}),
  'C:\\Users\\demo\\Videos': dir({ '录屏-演示.mp4': file(104_857_600, 1) }),
  'C:\\Users\\demo': dir({
    Desktop: dir({}),
    Documents: dir({}),
    Downloads: dir({}),
    '.ssh': dir({ 'id_ed25519': file(411, 60), 'id_ed25519.pub': file(98, 60), 'known_hosts': file(1_024, 1) }, 60),
  }),
}

function normalizeKey(path: string): string {
  return path.replace(/\/$/, '').replace(/\\$/, '')
}

export async function mockListDir(path: string): Promise<DirEntry[]> {
  await sleep(120)
  const node = mockFs[normalizeKey(path)]
  if (!node || node.kind !== 'dir') return []
  return Object.entries(node.children ?? {}).map(([name, child]) => ({
    name,
    kind: child.kind === 'dir' ? 'dir' : 'file',
    size: child.size ?? 0,
    mtime: child.mtime,
  }))
}

const mockRemoteRoot: Record<string, DirEntry[]> = {
  '/root': [
    { name: '..', kind: 'dir', size: 4096, mtime: now - 30 * day },
    { name: 'cert', kind: 'dir', size: 4096, mtime: now - 38 * day, attrs: 'drwxr-xr-x' },
    { name: 'docker-compose.yml', kind: 'file', size: 1_842, mtime: now - 5 * day, attrs: '-rw-r--r--' },
    { name: '.bashrc', kind: 'file', size: 3_096, mtime: now - 200 * day, attrs: '-rw-r--r--' },
    { name: 'app.log', kind: 'file', size: 84_233_216, mtime: now - 0.2 * day, attrs: '-rw-r--r--' },
    { name: 'backup-20260918.tar.gz', kind: 'file', size: 1_073_741_824, mtime: now - 2 * day, attrs: '-rw-------' },
  ],
  '/root/cert': [
    { name: '..', kind: 'dir', size: 4096, mtime: now - 30 * day },
    { name: 'server.crt', kind: 'file', size: 2_048, mtime: now - 38 * day, attrs: '-rw-r--r--' },
    { name: 'server.key', kind: 'file', size: 1_704, mtime: now - 38 * day, attrs: '-rw-------' },
    { name: 'ca-bundle.crt', kind: 'file', size: 6_944, mtime: now - 38 * day, attrs: '-rw-r--r--' },
  ],
}

export async function mockListRemoteDir(path: string): Promise<DirEntry[]> {
  await sleep(180)
  return (
    mockRemoteRoot[path] ?? [
      { name: '..', kind: 'dir', size: 4096, mtime: now - day },
      { name: `mock-${path.split('/').pop() || 'dir'}`, kind: 'dir', size: 4096, mtime: now - day, attrs: 'drwxr-xr-x' },
      { name: 'readme.txt', kind: 'file', size: 1_233, mtime: now - day, attrs: '-rw-r--r--' },
    ]
  )
}

/** 远程/本地列表统一入口（mock） */
export async function mockListLocalOrRemote(side: 'local' | 'remote', path: string): Promise<DirEntry[]> {
  return side === 'local' ? mockListDir(path) : mockListRemoteDir(path)
}

/* ---------- 内存 profile 存储（浏览器模式） ---------- */

const LS_KEY = 'hishell.profiles'

function seedProfiles(): SshProfile[] {
  return [
    defaultProfile({
      id: 'profile-yt-10808',
      name: 'yt - 10808',
      host: '149.104.22.80',
      port: 22,
      username: 'root',
      auth: 'password',
      password: '',
    }),
    defaultProfile({
      id: 'profile-hicode',
      name: 'top.hicode.top',
      host: 'top.hicode.top',
      port: 22,
      username: 'root',
      auth: 'password',
      password: '',
    }),
    defaultProfile({
      id: 'profile-jump-demo',
      name: '内网机器（经 SOCKS5 代理）',
      host: '10.0.0.128',
      port: 22,
      username: 'admin',
      auth: 'priFileKey',
      privateKeyPath: 'C:\\Users\\demo\\.ssh\\id_ed25519',
      proxy: { kind: 'socks5', host: '127.0.0.1', port: 1080 } as ProxyConfig,
    }),
  ]
}

const store = {
  list(): SshProfile[] {
    try {
      const raw = localStorage.getItem(LS_KEY)
      if (raw) return JSON.parse(raw)
    } catch {
      /* ignore */
    }
    const seeded = seedProfiles()
    localStorage.setItem(LS_KEY, JSON.stringify(seeded))
    return seeded
  },
  upsert(p: SshProfile) {
    const list = store.list()
    const idx = list.findIndex((x) => x.id === p.id)
    if (idx >= 0) list[idx] = p
    else list.push(p)
    localStorage.setItem(LS_KEY, JSON.stringify(list))
  },
  remove(id: string) {
    localStorage.setItem(LS_KEY, JSON.stringify(store.list().filter((p) => p.id !== id)))
  },
}

export const mockProfileStore = store
export function mockProfiles(): SshProfile[] {
  return store.list()
}
