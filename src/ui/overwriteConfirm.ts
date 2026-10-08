/**
 * 覆盖确认组合式函数：与模板中的 <FConfirmDialog> 配对使用。
 * ensure() 先探测目标端同名冲突，无冲突直接放行；有冲突弹确认框，
 * 用户点「覆盖」resolve(true)，取消/关闭 resolve(false)。
 */
import { ref } from 'vue'
import { t } from '../i18n'
import { findOverwriteConflicts, formatConflictNames, type OverwriteCheckItem } from '../api/overwrite'

export function useOverwriteConfirm() {
  const open = ref(false)
  const message = ref('')
  let resolve: ((v: boolean) => void) | null = null

  async function ensure(items: OverwriteCheckItem[]): Promise<boolean> {
    const conflicts = await findOverwriteConflicts(items)
    if (!conflicts.length) return true
    message.value = t('sftp.overwriteMsg', { n: conflicts.length, names: formatConflictNames(conflicts) })
    open.value = true
    return new Promise((r) => (resolve = r))
  }

  /** confirm 事件（覆盖）与 update:open=false（取消/Esc/关闭）共用出口；二次调用为 no-op */
  function settle(v: boolean) {
    open.value = false
    resolve?.(v)
    resolve = null
  }

  return { open, message, ensure, settle }
}
