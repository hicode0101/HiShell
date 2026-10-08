/**
 * 传输覆盖检查：入队前探测目标端是否已有同名文件/文件夹。
 * stat 失败（目标不存在 / 无权限）一律视为无冲突，由后端真实错误兜底。
 */
import { fsStat, sftpStat } from './backend'
import { t } from '../i18n'

export interface OverwriteCheckItem {
  name: string
  /** 上传（远程冲突检查）时必填 */
  sid?: number
  direction: 'upload' | 'download'
  localPath: string
  remotePath: string
}

/** 返回目标端已存在同名条目的名称列表（保持入参顺序） */
export async function findOverwriteConflicts(items: OverwriteCheckItem[]): Promise<string[]> {
  const conflicts: string[] = []
  for (const it of items) {
    try {
      if (it.direction === 'upload') {
        // 后端 sftp_stat 对不存在的远程路径返回 null（而非错误）：null = 目标不存在
        const st = await sftpStat(it.sid ?? 0, it.remotePath.replace(/\/+$/, ''))
        if (!st) continue
      } else {
        await fsStat(it.localPath.replace(/[\\/]+$/, ''))
      }
      conflicts.push(it.name)
    } catch {
      /* 目标不存在 → 无冲突 */
    }
  }
  return conflicts
}

/** 冲突名列表 → 确认框文案，最多列 5 个，超出折叠 */
export function formatConflictNames(names: string[]): string {
  const shown = names.slice(0, 5).join('\n')
  return names.length > 5 ? shown + '\n' + t('sftp.overwriteMore', { n: names.length - 5 }) : shown
}
