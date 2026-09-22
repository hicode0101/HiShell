<script setup lang="ts">
/**
 * 端口转发配置/管理弹窗（参考 Tabby Forwarded ports 弹窗）
 * - 已配置规则列表：类型徽标 + 摘要 + 删除
 * - 添加表单：[绑定地址]:[绑定端口] → [目标地址]:[目标端口] + 描述 + Local/Remote/Dynamic
 * - 规则保存到当前会话的 Profile（M3 接入 russh 真实转发）
 */
import { computed, reactive, ref, watch } from 'vue'
import { FButton, FComboBox, FTextBox, FDialog, toast } from '../../ui'
import { useSessionsStore } from '../../stores/sessions'
import { useProfilesStore } from '../../stores/profiles'
import { forwardAdd, forwardRemove, forwardList } from '../../api/backend'
import { t } from '../../i18n'
import { isValidHost } from '../../utils/validate'
import type { ForwardRule } from '../../api/types'

const sessions = useSessionsStore()
const profiles = useProfilesStore()

const tab = computed(() => sessions.activeTab)
const profile = computed(() => (tab.value?.profileId ? profiles.byId(tab.value.profileId) : undefined))

const open = computed({
  get: () => sessions.portsDialogOpen && !!profile.value,
  set: (v) => (sessions.portsDialogOpen = v),
})

const rules = computed(() => profile.value?.forwardedPorts ?? [])

/* 添加表单 */
const newType = ref<'local' | 'remote' | 'dynamic'>('local')
const form = reactive({
  bindHost: '127.0.0.1',
  bindPort: '',
  targetHost: '127.0.0.1',
  targetPort: '',
  description: '',
})
const showErr = ref(false)

const errs = computed(() => {
  const e: Record<string, string> = {}
  if (!isValidHost(form.bindHost)) e.bindHost = t('validate.hostInvalid')
  if (!validPort(form.bindPort)) e.bindPort = t('validate.portInvalid')
  if (newType.value !== 'dynamic') {
    if (!form.targetHost.trim()) e.targetHost = t('validate.hostRequired')
    else if (!isValidHost(form.targetHost)) e.targetHost = t('validate.hostInvalid')
    if (!validPort(form.targetPort)) e.targetPort = t('validate.portInvalid')
  }
  return e
})

const descriptionPlaceholder = computed(() =>
  newType.value === 'dynamic' ? t('ports.descriptionDynamic') : t('ports.description'),
)

/* 动态转发的代理协议（当前仅 SOCKS5，预留给 SOCKS4 / HTTP） */
const socksType = ref('SOCKS5')
const socksTypeOptions = ['SOCKS5']

/* 运行状态：active/error 来自后端实时转发管理器（仅会话在线时有值） */
const liveStatus = ref<Record<string, { active: boolean; error: string | null }>>({})
let pollTimer = 0

async function refreshLive() {
  const sid = tab.value?.sid
  if (sid === undefined || tab.value?.status !== 'connected') {
    liveStatus.value = {}
    return
  }
  try {
    const list = await forwardList(sid)
    const map: Record<string, { active: boolean; error: string | null }> = {}
    list.forEach((r) => (map[r.id] = { active: r.active, error: r.error ?? null }))
    liveStatus.value = map
  } catch {
    liveStatus.value = {}
  }
}

watch(open, (v) => {
  resetForm()
  showErr.value = false
  if (v) {
    void refreshLive()
    pollTimer = window.setInterval(() => void refreshLive(), 2000)
  } else if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = 0
  }
})

function resetForm() {
  showErr.value = false
  newType.value = 'local'
  Object.assign(form, { bindHost: '127.0.0.1', bindPort: '', targetHost: '127.0.0.1', targetPort: '', description: '' })
}

function validPort(v: string): boolean {
  const n = Number(v)
  return Number.isInteger(n) && n >= 1 && n <= 65535
}

function ruleSummary(r: ForwardRule): string {
  if (r.type === 'dynamic') {
    return `${r.bindHost}:${r.bindPort} → ${r.description || t('ports.socksProxy')}`
  }
  return `${r.bindHost}:${r.bindPort} → ${r.targetHost}:${r.targetPort}`
}

async function addRule() {
  if (!profile.value) return
  showErr.value = true
  if (Object.keys(errs.value).length) {
    toast(Object.values(errs.value)[0]!, 'error')
    return
  }
  if (newType.value !== 'dynamic') {
    if (!form.targetHost.trim() || !validPort(form.targetPort)) {
      toast(t('ports.portInvalid'), 'error')
      return
    }
  }
  const rule: ForwardRule = {
    id: `fwd-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    type: newType.value,
    bindHost: form.bindHost.trim() || '127.0.0.1',
    bindPort: Number(form.bindPort),
    targetHost: newType.value === 'dynamic' ? undefined : form.targetHost.trim() || '127.0.0.1',
    targetPort: newType.value === 'dynamic' ? undefined : Number(form.targetPort),
    description: form.description.trim() || undefined,
  }
  const sid = tab.value?.sid
  const live = sid !== undefined && tab.value?.status === 'connected'
  // 会话在线：先真实下发，成功才入库；失败提示并回滚，避免下次连接反复报错
  if (live) {
    try {
      await forwardAdd(sid!, rule)
    } catch (e) {
      toast(String(e), 'error')
      return
    }
  }
  profile.value.forwardedPorts.push(rule)
  await profiles.upsert(JSON.parse(JSON.stringify(profile.value)))
  toast(live ? t('ports.toastAdded') : t('ports.toastAddedSavedOnly'), 'success')
  void refreshLive()
  resetForm()
}

async function removeRule(id: string) {
  if (!profile.value) return
  const snapshot = [...profile.value.forwardedPorts]
  profile.value.forwardedPorts = profile.value.forwardedPorts.filter((r) => r.id !== id)
  const sid = tab.value?.sid
  const live = sid !== undefined && tab.value?.status === 'connected'
  if (live) {
    try {
      await forwardRemove(sid!, id)
    } catch (e) {
      profile.value.forwardedPorts = snapshot // 下发失败回滚
      await profiles.upsert(JSON.parse(JSON.stringify(profile.value)))
      toast(String(e), 'error')
      return
    }
  }
  await profiles.upsert(JSON.parse(JSON.stringify(profile.value)))
  void refreshLive()
  toast(t('ports.toastDeleted'))
}
</script>

<template>
  <FDialog v-model:open="open" :title="t('ports.dialogTitle')" :width="620">
    <!-- 已配置列表 -->
    <div class="pf-section">{{ t('ports.forwardedPorts') }}</div>
    <div v-if="rules.length" class="pf-list">
      <div v-for="r in rules" :key="r.id" class="pf-row">
        <span class="pf-type" :class="r.type">{{ r.type === 'local' ? t('ports.local') : r.type === 'remote' ? t('ports.remote') : t('ports.dynamic') }}</span>
        <span class="pf-summary mono">{{ ruleSummary(r) }}</span>
        <span v-if="liveStatus[r.id]?.error" class="pf-state err" :title="liveStatus[r.id].error ?? ''">
          {{ t('ports.stateError') }}: {{ liveStatus[r.id].error }}
        </span>
        <span v-else-if="liveStatus[r.id]?.active" class="pf-state ok">● {{ t('ports.stateRunning') }}</span>
        <span v-else-if="liveStatus[r.id]" class="pf-state off">{{ t('ports.stateOff') }}</span>
        <span v-else-if="tab?.status === 'connected'" class="pf-state off">{{ t('ports.stateStarting') }}</span>
        <span v-else class="pf-state off">{{ t('ports.stateSavedOnly') }}</span>
        <button class="pf-del" :title="t('ports.delete')" @click="removeRule(r.id)">
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <path d="M4.5 5.5h11M8 5.5V4.3c0-.44.36-.8.8-.8h2.4c.44 0 .8.36.8.8v1.2M6 5.5l.7 9.8c.04.66.6 1.2 1.27 1.2h4.06c.66 0 1.23-.54 1.27-1.2l.7-9.8M8.4 8.5v5M11.6 8.5v5" />
          </svg>
        </button>
      </div>
    </div>
    <div v-else class="pf-empty">{{ t('ports.empty') }}</div>

    <!-- 添加表单 -->
    <div class="pf-section">{{ t('ports.addTitle') }}</div>
    <div class="pf-form-row">
      <FTextBox v-model="form.bindHost" :placeholder="t('ports.hostPlaceholder')" class="pf-host" :class="{ 'pf-invalid': showErr && errs.bindHost }" />
      <span class="pf-colon">:</span>
      <FTextBox v-model="form.bindPort" :placeholder="t('ports.portPlaceholder')" class="pf-port" :class="{ 'pf-invalid': showErr && errs.bindPort }" />
      <span class="pf-arrow">
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <path d="M3.5 10h12M10.5 5l5 5-5 5" />
        </svg>
      </span>
      <template v-if="newType !== 'dynamic'">
        <FTextBox v-model="form.targetHost" :placeholder="t('ports.targetHostPlaceholder')" class="pf-host" :class="{ 'pf-invalid': showErr && errs.targetHost }" />
        <span class="pf-colon">:</span>
        <FTextBox v-model="form.targetPort" :placeholder="t('ports.targetPortPlaceholder')" class="pf-port" :class="{ 'pf-invalid': showErr && errs.targetPort }" />
      </template>
      <template v-else>
        <FComboBox v-model="socksType" :options="socksTypeOptions" class="pf-socks" />
      </template>
    </div>
    <div class="pf-desc">
      <FTextBox v-model="form.description" :placeholder="descriptionPlaceholder" />
    </div>

    <div class="pf-bottom">
      <div class="pf-types">
        <button class="pf-type-btn" :class="{ on: newType === 'local' }" @click="newType = 'local'">{{ t('ports.tabLocal') }}</button>
        <button class="pf-type-btn" :class="{ on: newType === 'remote' }" @click="newType = 'remote'">{{ t('ports.tabRemote') }}</button>
        <button class="pf-type-btn" :class="{ on: newType === 'dynamic' }" @click="newType = 'dynamic'">{{ t('ports.tabDynamic') }}</button>
      </div>
      <div class="pf-spacer" />
      <FButton variant="accent" icon="check" @click="addRule">{{ t('ports.forwardPort') }}</FButton>
    </div>

    <template #footer>
      <span class="pf-footer-hint">{{ profile ? `${profile.name} · ${profile.host}:${profile.port}` : '' }}</span>
      <div class="pf-spacer" />
      <FButton variant="subtle" @click="open = false">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.pf-section {
  font-weight: 600;
  font-size: 14px;
  margin: 14px 0 10px;
}
.pf-section:first-child {
  margin-top: 0;
}
.pf-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.pf-row {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-alt-pressed);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  padding: 12px 14px;
}
.pf-type {
  font-weight: 700;
  font-size: 13px;
  flex: none;
  min-width: 44px;
}
.pf-type.local {
  color: var(--accent);
}
.pf-type.remote {
  color: var(--success);
}
.pf-type.dynamic {
  color: var(--warning);
}
.pf-summary {
  flex: 1;
  font-size: 12.5px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pf-state {
  flex: none;
  font-size: 11.5px;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pf-state.ok {
  color: var(--success);
}
.pf-state.off {
  color: var(--text-tertiary);
}
.pf-state.err {
  color: var(--danger);
}
.pf-del {
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  width: 26px;
  height: 26px;
  border-radius: var(--radius-sm);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex: none;
}
.pf-del:hover {
  background: var(--bg-alt-hover);
  color: var(--danger);
}
.pf-empty {
  color: var(--text-tertiary);
  font-size: 12.5px;
  background: var(--bg-alt-pressed);
  border: 1px dashed var(--stroke-control);
  border-radius: var(--radius-md);
  padding: 14px;
  text-align: center;
}
.pf-form-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.pf-host {
  flex: 1;
  min-width: 0;
}
.pf-port {
  width: 92px;
  flex: none;
}
.pf-colon,
.pf-arrow {
  color: var(--text-tertiary);
  flex: none;
  display: inline-flex;
}
.pf-socks {
  flex: 1;
  min-width: 0;
}
.pf-desc {
  margin-top: 10px;
}
.pf-bottom {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
}
.pf-types {
  display: flex;
  background: var(--bg-alt-pressed);
  border-radius: var(--radius-md);
  padding: 2px;
  gap: 2px;
}
.pf-type-btn {
  border: none;
  background: transparent;
  font: inherit;
  font-size: 12.5px;
  height: 28px;
  padding: 0 16px;
  border-radius: 5px;
  color: var(--text-secondary);
  cursor: pointer;
}
.pf-type-btn.on {
  background: var(--bg-card);
  color: var(--text-primary);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
}
.pf-spacer {
  flex: 1;
}
.pf-footer-hint {
  color: var(--text-tertiary);
  font-size: 12px;
}
</style>

.pf-invalid {
  border-color: var(--danger) !important;
}

