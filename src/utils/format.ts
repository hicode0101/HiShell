/** 字节/速率/时长格式化 */
import { t } from '../i18n'
import type { AuthKind } from '../api/types'

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0 Bytes'
  const units = ['Bytes', 'KB', 'MB', 'GB', 'TB', 'PB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '--'
  return `${formatBytes(bytesPerSec)}/s`
}

export function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = s % 60
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
  return `${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
}

export function formatTime(ts: number): string {
  const d = new Date(ts)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function fileKindLabel(kind: string, name: string): string {
  if (kind === 'dir') return t('sftp.folder')
  const ext = name.includes('.') ? name.split('.').pop()!.toUpperCase() : ''
  return ext ? t('sftp.fileWithType', { ext }) : t('sftp.file')
}

/** 认证方式的本地化标签（AuthKind → connect.auth.*，未知类型回退为密码） */
export function authLabel(auth: AuthKind): string {
  const key =
    auth === 'keyboardInteractive'
      ? 'ki'
      : auth === 'priFileKey'
        ? 'privateKey'
        : auth === 'userKey'
          ? 'userKey'
          : auth === 'agent'
            ? 'agent'
            : 'password'
  return t(`connect.auth.${key}`)
}
