<script setup lang="ts">
/**
 * 后端提示对话框：hostkey 首次/变更、密码、keyboard-interactive。
 * 由 sessions.prompts 队列驱动，逐条应答。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FTextBox, FCheckbox, FDialog } from '../ui'
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
    case 'password':
      return t('prompt.passwordTitle', { user: userOf(p), host: hostOf(p) })
    case 'keyboardInteractive':
      return t('prompt.kiTitle', { user: userOf(p), host: hostOf(p) })
    default:
      return 'HiShell'
  }
})

const password = ref('')
const remember = ref(false)
const answers = ref<string[]>([])

watch(current, (v) => {
  password.value = ''
  remember.value = false
  const prompts = (v?.payload.prompts as { text: string; echo: boolean }[] | undefined) ?? []
  answers.value = prompts.map(() => '')
})

function respond(value: Record<string, unknown>) {
  const target = sessions.answerPrompt(current.value!.key)
  if (target) sshPromptResponse(target.sid, target.key, value).catch(() => {})
}

function respondHostkey(action: 'save' | 'once' | 'cancel') {
  respond({ action } as Record<string, unknown>)
}

function submitPassword() {
  respond({ password: password.value, remember: remember.value })
  password.value = ''
}

function submitKi() {
  respond({ answers: answers.value.slice() })
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
  <FDialog :open="open" :title="title" :width="520">
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

      <!-- 密码 -->
      <template v-else-if="kind === 'password'">
        <div class="pm-title">{{ t('prompt.passwordTitle', { user: userOf(current.payload), host: hostOf(current.payload) }) }}</div>
        <FTextBox v-model="password" type="password" :placeholder="t('prompt.passwordPlaceholder')" @keydown.enter="submitPassword" />
        <div class="pm-remember">
          <FCheckbox v-model="remember" :label="t('prompt.remember')" />
        </div>
      </template>

      <!-- keyboard-interactive -->
      <template v-else>
        <div class="pm-title">{{ t('prompt.kiTitle', { user: userOf(current.payload), host: hostOf(current.payload) }) }}</div>
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
      <template v-else>
        <div class="footer-spacer" />
        <FButton variant="subtle" @click="kind === 'password' ? (password = '', submitPassword()) : submitKi()">
          {{ t('common.cancel') }}
        </FButton>
        <FButton variant="accent" @click="kind === 'password' ? submitPassword() : submitKi()">
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
