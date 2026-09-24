<script setup lang="ts">
/**
 * 后端提示对话框：hostkey 首次/变更、密码、keyboard-interactive。
 * 由 sessions.prompts 队列驱动，逐条应答。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FComboBox, FTextBox, FCheckbox, FDialog, toast } from '../ui'
import { isTauri } from '../api/adapter'
import { userKeyList } from '../api/backend'
import { useSessionsStore } from '../stores/sessions'
import { sshPromptResponse } from '../api/backend'
import { t } from '../i18n'

const sessions = useSessionsStore()

const current = computed(() => sessions.prompts[0])
const kind = computed(() => (current.value?.payload.kind as string) ?? '')
const open = computed(() => !!current.value)
const title = computed(() => {
  const p = current.value?.payload
  switch (kind.value) {
    case 'hostkeyNew':
      return t('prompt.hostkeyNewTitle')
    case 'hostkeyMismatch':
      return t('prompt.hostkeyMismatchTitle')
    case 'authRetry':
      return t('prompt.authRetryTitle', { user: userOf(p), host: hostOf(p) })
    case 'keyboardInteractive':
      return t('prompt.kiTitle', { user: userOf(p), host: hostOf(p) })
    default:
      return 'HiShell'
  }
})

const answers = ref<string[]>([])

/* ---------- 统一认证对话框（authRetry，布局对齐快速连接） ---------- */
const AUTH_METHODS = [
  { value: 'password', labelKey: 'connect.auth.password' },
  { value: 'privateKey', labelKey: 'connect.auth.privateKey' },
  { value: 'userKey', labelKey: 'connect.auth.userKey' },
  { value: 'keyboardInteractive', labelKey: 'connect.auth.ki' },
  { value: 'agent', labelKey: 'connect.auth.agent' },
] as const

const authKind = ref<string>('password')
const authPassword = ref('')
const authRemember = ref(false)
const authKeyPath = ref('')
const authKeyName = ref('')
const authPassphrase = ref('')

const authMethodOptions = AUTH_METHODS.map((m) => t(m.labelKey))
const authKindLabel = computed({
  get: () => t(AUTH_METHODS.find((m) => m.value === authKind.value)?.labelKey ?? 'connect.auth.password'),
  set: (v: string) => {
    authKind.value = AUTH_METHODS.find((m) => t(m.labelKey) === v)?.value ?? 'password'
    // 切到用户密钥时预选第一把，避免停留在空占位
    if (authKind.value === 'userKey' && !authKeyName.value) {
      authKeyName.value = authKeyNames.value[0] ?? ''
    }
  },
})
const authKeyNames = ref<string[]>([])
const authError = computed(() => (current.value?.payload as any)?.error ?? '')

async function pickPrivateKey() {
  if (!isTauri) {
    toast(t('connect.browserMockToast'))
    return
  }
  const { open } = await import('@tauri-apps/plugin-dialog')
  const file = await open({ multiple: false, title: t('connect.privateKeyFile') })
  if (typeof file === 'string') authKeyPath.value = file
}

function submitAuth() {
  const body: Record<string, unknown> = { method: authKind.value }
  if (authKind.value === 'password') {
    body.password = authPassword.value
    body.rememberPassword = authRemember.value
  } else if (authKind.value === 'privateKey') {
    body.method = 'publicKey'
    body.keyPath = authKeyPath.value
    body.passphrase = authPassphrase.value
  } else if (authKind.value === 'userKey') {
    body.method = 'userKey'
    body.keyName = authKeyName.value
    body.passphrase = authPassphrase.value
  } else if (authKind.value === 'agent') {
    body.method = 'agent'
  }
  respond(body)
}

watch(current, (v) => {
  const prompts = (v?.payload.prompts as { text: string; echo: boolean }[] | undefined) ?? []
  answers.value = prompts.map(() => '')
  const p: any = v?.payload ?? {}
  if (v?.payload.kind === 'authRetry') {
    authPassword.value = ''
    authRemember.value = false
    authPassphrase.value = ''
    authKeyNames.value = ((p.userKeys as string[]) ?? []).filter(Boolean)
    // 默认认证方式 = 会话配置中的原始方式（profileAuth），而不是归一化后的 method——
    // 例如 userKey 配置在认证阶段已解析为 publicKey+路径，但对话框仍应默认「用户密钥」
    const pa: string = p.profileAuth ?? ''
    if (pa === 'keyboardInteractive') {
      authKind.value = 'keyboardInteractive'
    } else if (pa === 'privateKey') {
      authKind.value = 'privateKey'
      authKeyPath.value = p.keyPath || ''
    } else if (pa === 'userKey') {
      authKind.value = 'userKey'
      authKeyName.value = p.userKeyName || authKeyNames.value[0] || ''
    } else if (pa === 'agent') {
      authKind.value = 'agent'
    } else {
      // 兼容旧载荷：按失败方式推断
      authKind.value =
        p.method === 'keyboardInteractive'
          ? 'keyboardInteractive'
          : p.method === 'publicKey'
            ? p.keyPath
              ? 'privateKey'
              : 'userKey'
            : p.method === 'agent'
              ? 'agent'
              : 'password'
      authKeyPath.value = authKind.value === 'privateKey' ? p.keyPath || '' : ''
      if (authKind.value === 'userKey' && !authKeyName.value) authKeyName.value = authKeyNames.value[0] ?? ''
    }
    // 与快速连接一致：前端实时加载用户密钥列表（payload 里的列表仅作兜底初值）
    userKeyList()
      .then((list) => {
        authKeyNames.value = list.map((k) => k.name)
        if (authKind.value === 'userKey' && !authKeyNames.value.includes(authKeyName.value)) {
          authKeyName.value = authKeyNames.value[0] ?? ''
        }
      })
      .catch(() => {})
  }
})

function respond(value: Record<string, unknown>) {
  const target = sessions.answerPrompt(current.value!.key)
  if (target) sshPromptResponse(target.sid, target.key, value).catch(() => {})
}

function respondHostkey(action: 'save' | 'once' | 'cancel') {
  respond({ action } as Record<string, unknown>)
}

function submitKi() {
  respond({ answers: answers.value.slice() })
}

/* 回车确认按提示类型映射主操作：hostkey 变更是安全告警，回车执行安全的「断开连接」而非保存新指纹 */
function onEnterConfirm() {
  switch (kind.value) {
    case 'hostkeyNew':
      respondHostkey('save')
      break
    case 'hostkeyMismatch':
      respondHostkey('cancel')
      break
    case 'authRetry':
      submitAuth()
      break
    default:
      submitKi()
  }
}

function hostOf(o: Record<string, unknown> | undefined): string {
  return (o?.host as string) ?? ''
}
function port(o: Record<string, unknown> | undefined): number {
  return (o?.port as number) ?? 22
}
function userOf(o: Record<string, unknown> | undefined): string {
  return (o?.user as string) ?? ''
}
function fingerprint(o: Record<string, unknown> | undefined): string {
  return (o?.fingerprint as string) ?? (o?.newFingerprint as string) ?? ''
}
function keyType(o: Record<string, unknown> | undefined): string {
  return (o?.keyType as string) ?? ''
}
function oldFingerprint(o: Record<string, unknown> | undefined): string {
  return (o?.oldFingerprint as string) ?? ''
}
function kiName(o: Record<string, unknown> | undefined): string {
  return (o?.name as string) ?? ''
}
function kiInstruction(o: Record<string, unknown> | undefined): string {
  return (o?.instruction as string) ?? ''
}
function kiPrompts(o: Record<string, unknown> | undefined): { text: string; echo: boolean }[] {
  return (o?.prompts as { text: string; echo: boolean }[]) ?? []
}
</script>

<template>
  <!-- 认证重试比其他提示宽 100px（620），hostkey / keyboard-interactive 保持 520 -->
  <FDialog :open="open" :title="title" :width="kind === 'authRetry' ? 620 : 520" enter-confirm @confirm="onEnterConfirm">
    <template v-if="current">
      <!-- 首次连接 hostkey -->
      <template v-if="kind === 'hostkeyNew'">
        <div class="pm-body">{{ t('prompt.hostkeyBody', { host: hostOf(current.payload), port: port(current.payload) }) }}</div>
        <div class="pm-fingerprint mono">{{ fingerprint(current.payload) }}</div>
        <div class="pm-keytype">{{ keyType(current.payload) }}</div>
      </template>

      <!-- 密钥变更 -->
      <template v-else-if="kind === 'hostkeyMismatch'">
        <div class="pm-body">{{ t('prompt.hostkeyBody', { host: hostOf(current.payload), port: port(current.payload) }) }}</div>
        <div class="pm-fingerprint mono">{{ fingerprint(current.payload) }}</div>
        <div class="pm-keytype">{{ keyType(current.payload) }}</div>
        <div class="pm-old mono">{{ t('prompt.hostkeyOld', { old: oldFingerprint(current.payload) }) }}</div>
        <div class="pm-warn">{{ t('prompt.hostkeyMismatchWarn') }}</div>
      </template>

      <!-- 统一认证：密码/密钥验证失败后重新输入或切换认证方式（布局对齐快速连接） -->
      <template v-else-if="kind === 'authRetry'">
        <div class="pm-warn">{{ authError }}</div>
        <div class="pa-grid">
          <span class="pa-label">{{ t('connect.authMethod') }}</span>
          <FComboBox v-model="authKindLabel" :options="authMethodOptions" />
          <template v-if="authKind === 'password'">
            <span class="pa-label">{{ t('connect.password') }}</span>
            <div class="pa-stack">
              <FTextBox v-model="authPassword" type="password" :placeholder="t('connect.passwordPlaceholder')" @keydown.enter="submitAuth" />
              <FCheckbox v-model="authRemember" :label="t('prompt.remember')" />
            </div>
          </template>
          <template v-else-if="authKind === 'privateKey'">
            <span class="pa-label">{{ t('connect.privateKeyFile') }}</span>
            <div class="pa-row">
              <FTextBox v-model="authKeyPath" spellcheck="false" :placeholder="t('connect.privateKeyPlaceholder')" @keydown.enter="submitAuth" />
              <FButton @click="pickPrivateKey">{{ t('common.browse') }}</FButton>
            </div>
            <span class="pa-label">{{ t('connect.passphrase') }}</span>
            <FTextBox v-model="authPassphrase" type="password" :placeholder="t('connect.passphrasePlaceholder')" @keydown.enter="submitAuth" />
          </template>
          <template v-else-if="authKind === 'userKey'">
            <span class="pa-label">{{ t('connect.userKeyLabel') }}</span>
            <FComboBox v-if="authKeyNames.length" v-model="authKeyName" :options="authKeyNames" />
            <span v-else class="pa-hint">{{ t('connect.noUserKeys') }}</span>
            <span class="pa-label">{{ t('connect.passphrase') }}</span>
            <FTextBox v-model="authPassphrase" type="password" :placeholder="t('connect.passphrasePlaceholder')" @keydown.enter="submitAuth" />
          </template>
          <template v-else-if="authKind === 'keyboardInteractive'">
            <span class="pa-label">{{ t('connect.hint') }}</span>
            <div class="pa-hint">{{ t('connect.kiHint') }}</div>
          </template>
          <template v-else>
            <span class="pa-label">{{ t('connect.hint') }}</span>
            <div class="pa-hint">{{ t('connect.agentHint') }}</div>
          </template>
        </div>
      </template>

      <!-- keyboard-interactive -->
      <template v-else>
        <div v-if="kiName(current.payload)" class="pm-body mono">{{ kiName(current.payload) }}</div>
        <div v-if="kiInstruction(current.payload)" class="pm-body">{{ t('prompt.instruction', { instruction: kiInstruction(current.payload) }) }}</div>
        <div class="pm-ki">
          <template v-for="(p, i) in kiPrompts(current.payload)" :key="i">
            <div class="pm-ki-label">{{ p.text }}</div>
            <FTextBox
              v-model="answers[i]"
              :type="p.echo ? 'text' : 'password'"
              @keydown.enter="submitKi"
            />
          </template>
        </div>
      </template>
    </template>

    <template #footer>
      <!-- hostkey 双视图按钮组 -->
      <template v-if="kind === 'hostkeyNew' || kind === 'hostkeyMismatch'">
        <FButton variant="danger" @click="respondHostkey('cancel')">{{ t('prompt.disconnect') }}</FButton>
        <div class="footer-spacer" />
        <FButton @click="respondHostkey('once')">{{ t('prompt.acceptOnce') }}</FButton>
        <FButton variant="accent" @click="respondHostkey('save')">{{ t('prompt.acceptAndSave') }}</FButton>
      </template>
      <template v-else-if="kind === 'authRetry'">
        <div class="footer-spacer" />
        <FButton variant="subtle" @click="respond({ cancel: true })">{{ t('common.cancel') }}</FButton>
        <FButton variant="accent" @click="submitAuth">{{ t('common.confirm') }}</FButton>
      </template>
      <template v-else>
        <div class="footer-spacer" />
        <FButton variant="subtle" @click="submitKi()">
          {{ t('common.cancel') }}
        </FButton>
        <FButton variant="accent" @click="submitKi()">
          {{ t('prompt.submit') }}
        </FButton>
      </template>
    </template>
  </FDialog>
</template>

<style scoped>
.pm-title {
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 12px;
}
.pm-title.danger {
  color: var(--danger);
}
.pm-body {
  font-size: 13px;
  color: var(--text-primary);
  margin-bottom: 10px;
  line-height: 1.6;
  word-break: break-all;
}
.pm-fingerprint {
  font-size: 13px;
  background: var(--bg-alt-pressed);
  border-radius: var(--radius-sm);
  padding: 10px 12px;
  word-break: break-all;
  user-select: text;
}
.pm-keytype {
  margin-top: 6px;
  color: var(--text-tertiary);
  font-size: 12px;
}
.pm-old {
  margin-top: 10px;
  color: var(--text-secondary);
  font-size: 12px;
  word-break: break-all;
}
.pm-warn {
  margin-top: 10px;
  color: var(--danger);
  font-size: 12.5px;
  line-height: 1.6;
}
.pm-retry {
  margin-top: 6px;
  color: var(--danger);
  font-size: 12.5px;
}
/* 统一认证面板：紧凑布局，对齐快速连接对话框的认证区 */
.pa-grid {
  display: grid;
  grid-template-columns: 92px 1fr;
  gap: 10px 14px;
  font-size: 12.5px;
  align-items: center;
  /* 高度在原基础上加 100px（3 行 110px → 210px）。
     align-content 必须置 start：Grid 默认 stretch 会把加高的空间摊进每一行（行距变大），
     置 start 后行高/行距不变，多余空间留在底部（与快速连接 .qc-body 的加高方式一致） */
  align-content: start;
  min-height: 210px;
  margin-top: 2px;
}
.pa-label {
  color: var(--text-secondary);
  text-align: right;
}
.pa-grid :deep(.f-textbox),
.pa-grid :deep(.f-combo) {
  width: 100%;
}
.pa-row {
  display: flex;
  gap: 8px;
}
.pa-row :deep(.f-textbox) {
  flex: 1;
}
.pa-stack {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.pa-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.6;
}
.pm-remember {
  margin-top: 12px;
}
.pm-ki {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.pm-ki-label {
  font-size: 12.5px;
  color: var(--text-secondary);
}
.footer-spacer {
  flex: 1;
}
</style>
