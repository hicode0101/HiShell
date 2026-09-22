<script setup lang="ts">
/**
 * 新建用户密钥生成向导（参考 Xshell 四步流程）：
 * 1. 生成密钥参数（类型 + 长度）→ 2. 生成公钥对（位动画）→
 * 3. 用户密钥信息（名称/注释/口令）→ 4. 公钥注册（格式预览 + 保存为文件）
 * 完成后密钥写入 ~/.ssh/<name> 与 ~/.ssh/<name>.pub。
 */
import { computed, onUnmounted, ref, watch } from 'vue'
import { FButton, FComboBox, FDialog, FTextBox, toast } from '../ui'
import {
  userKeyGenerateCancel,
  userKeyGenerateFinish,
  userKeyGenerateStart,
  userKeySavePublic,
} from '../api/backend'
import { toRfc4716 } from '../utils/pubkeyFormat'
import { t } from '../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{
  (e: 'update:open', v: boolean): void
  (e: 'generated', name: string): void
}>()

/* ---------- 第 1 步：参数 ---------- */
const keyType = ref('RSA')
const typeOptions = ['RSA', 'ECDSA', 'ED25519']
const bitsModel = ref('2048')
const bitsOptions = computed(() => {
  if (keyType.value === 'RSA') return ['2048', '3072', '4096', '8192']
  if (keyType.value === 'ECDSA') return ['256', '384', '521']
  return ['256']
})
const bitsFixed = computed(() => keyType.value === 'ED25519')

watch(keyType, () => {
  bitsModel.value = keyType.value === 'RSA' ? '2048' : '256'
})

/* ---------- 第 2 步：生成 ---------- */
const genId = ref('')
const publicBlob = ref('')
const generating = ref(false)
const genDone = ref(false)
const genError = ref('')
const bitsText = ref('')
let bitsTimer = 0

/* ---------- 第 3 步：信息 ---------- */
const keyName = ref('')
const keyComment = ref('')
const pass1 = ref('')
const pass2 = ref('')

/* ---------- 第 4 步：公钥注册 ---------- */
const pubFormat = ref('SSH2 - OpenSSH')
const formatOptions = ['SSH2 - OpenSSH', 'SSH2 - IETF SECSH']

const step = ref(1)

function resetAll() {
  step.value = 1
  keyType.value = 'RSA'
  bitsModel.value = '2048'
  genId.value = ''
  publicBlob.value = ''
  generating.value = false
  genDone.value = false
  genError.value = ''
  bitsText.value = ''
  keyName.value = ''
  keyComment.value = ''
  pass1.value = ''
  pass2.value = ''
  pubFormat.value = 'SSH2 - OpenSSH'
  stopBitsAnim()
}

watch(
  () => props.open,
  (v) => {
    if (v) resetAll()
  },
)

/* 位动画（Xshell 风格的 0/1 随机滚动） */
function startBitsAnim() {
  stopBitsAnim()
  const cols = 42
  const rows = 6
  bitsTimer = window.setInterval(() => {
    const lines: string[] = []
    for (let r = 0; r < rows; r++) {
      let line = ''
      for (let c = 0; c < cols; c++) line += Math.random() > 0.5 ? '1' : '0'
      lines.push(line)
    }
    bitsText.value = lines.join('\n')
  }, 60)
}
function stopBitsAnim() {
  if (bitsTimer) {
    clearInterval(bitsTimer)
    bitsTimer = 0
  }
}
onUnmounted(stopBitsAnim)

async function startGenerate() {
  // 重新生成时丢弃上一次的结果
  if (genId.value) void userKeyGenerateCancel(genId.value)
  genId.value = crypto.randomUUID()
  generating.value = true
  genDone.value = false
  genError.value = ''
  startBitsAnim()
  try {
    const info = await userKeyGenerateStart(genId.value, keyType.value, Number(bitsModel.value))
    publicBlob.value = info.publicBlob
    genDone.value = true
  } catch (e) {
    genError.value = String(e)
  } finally {
    generating.value = false
    stopBitsAnim()
  }
}

function pad(n: number) {
  return n < 10 ? '0' + n : String(n)
}

function fillKeyInfoDefaults() {
  const bits = bitsModel.value
  keyName.value = `id_${keyType.value.toLowerCase()}_${bits}`
  const d = new Date()
  const ymd = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}`
  keyComment.value = `${keyType.value.toLowerCase()} ${bits}-${ymd}`
  pass1.value = ''
  pass2.value = ''
}

/** OpenSSH 单行公钥 → RFC4716 转换已抽取到 utils/pubkeyFormat（此处直接使用） */
const pubText = computed(() => {
  if (!publicBlob.value) return ''
  const comment = keyComment.value.trim()
  if (pubFormat.value === 'SSH2 - IETF SECSH') return toRfc4716(publicBlob.value, comment)
  return comment ? `${publicBlob.value} ${comment}` : publicBlob.value
})

const nextDisabled = computed(() => step.value === 2 && (generating.value || !genDone.value))

async function goNext() {
  if (nextDisabled.value) return
  if (step.value === 1) {
    step.value = 2
    await startGenerate()
    return
  }
  if (step.value === 2) {
    fillKeyInfoDefaults()
    step.value = 3
    return
  }
  if (step.value === 3) {
    const name = keyName.value.trim()
    if (!/^[A-Za-z0-9._-]+$/.test(name)) {
      toast(t('userKeys.wizNameInvalid'), 'error')
      return
    }
    if (pass1.value !== pass2.value) {
      toast(t('userKeys.wizPassMismatch'), 'error')
      return
    }
    step.value = 4
  }
}

function goPrev() {
  if (generating.value) return
  if (step.value > 1) step.value -= 1
}

async function finish() {
  try {
    const paths = await userKeyGenerateFinish({
      genId: genId.value,
      name: keyName.value.trim(),
      comment: keyComment.value.trim(),
      passphrase: pass1.value,
    })
    toast(t('userKeys.wizSuccess', { name: paths.name }), 'success')
    emit('generated', paths.name)
    emit('update:open', false)
  } catch (e) {
    toast(String(e), 'error')
  }
}

async function saveAsFile() {
  try {
    const path = await userKeySavePublic(keyName.value.trim(), pubText.value)
    toast(t('userKeys.wizSavedTo', { path }), 'success')
  } catch (e) {
    toast(String(e), 'error')
  }
}

function close() {
  // 未完成落盘的暂存密钥一律丢弃（finish 后 cancel 是无害空操作）
  if (genId.value) void userKeyGenerateCancel(genId.value)
  stopBitsAnim()
  emit('update:open', false)
}
</script>

<template>
  <FDialog
    :open="open"
    :title="t('userKeys.wizTitle')"
    :width="640"
    @update:open="(v: boolean) => !v && close()"
  >
    <div class="wiz">
      <div class="wiz-step-title">{{ t(step === 1 ? 'userKeys.wizParams' : step === 2 ? 'userKeys.wizPubGen' : step === 3 ? 'userKeys.wizKeyInfo' : 'userKeys.wizPubReg') }}</div>

      <!-- 第 1 步：生成密钥参数 -->
      <div v-if="step === 1" class="wiz-body">
        <p class="wiz-hint">{{ t('userKeys.wizIntro') }}</p>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizKeyType') }}</label>
          <FComboBox v-model="keyType" :options="typeOptions" class="wiz-combo" />
        </div>
        <p class="wiz-hint sub">{{ t('userKeys.wizLengthHint') }}</p>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizKeyLength') }}</label>
          <FComboBox v-model="bitsModel" :options="bitsOptions" class="wiz-combo bits" :disabled="bitsFixed" />
          <span class="unit">{{ t('userKeys.bitsUnit') }}</span>
        </div>
        <p class="wiz-hint bottom">{{ t('userKeys.wizClickNext') }}</p>
      </div>

      <!-- 第 2 步：生成公钥对 -->
      <div v-else-if="step === 2" class="wiz-body">
        <p class="wiz-hint">
          {{ generating ? t('userKeys.wizGenerating', { bits: bitsModel, type: keyType }) : genError ? t('userKeys.wizGenFailed') + '：' + genError : t('userKeys.wizGenDone') }}
        </p>
        <div class="bits-box">{{ bitsText }}</div>
        <p v-if="genDone" class="wiz-hint">
          {{ t('userKeys.wizGenDone') }}<br />{{ t('userKeys.wizGenDoneHint') }}
        </p>
        <p v-else-if="!generating && genError" class="wiz-hint">{{ t('userKeys.wizClickRetry') }}</p>
      </div>

      <!-- 第 3 步：用户密钥信息 -->
      <div v-else-if="step === 3" class="wiz-body">
        <p class="wiz-hint">{{ t('userKeys.wizNamePrompt') }}</p>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizKeyName') }}</label>
          <FTextBox v-model="keyName" class="wiz-input" spellcheck="false" />
        </div>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizComment') }}</label>
          <FTextBox v-model="keyComment" class="wiz-input" spellcheck="false" />
        </div>
        <p class="wiz-hint">{{ t('userKeys.wizPassPrompt') }}</p>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizPass') }}</label>
          <FTextBox v-model="pass1" type="password" class="wiz-input" />
        </div>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizConfirm') }}</label>
          <FTextBox v-model="pass2" type="password" class="wiz-input" />
          <span class="unit">{{ t('userKeys.wizConfirmHint') }}</span>
        </div>
        <p class="wiz-hint bottom">{{ t('userKeys.wizNextHint') }}</p>
      </div>

      <!-- 第 4 步：公钥注册 -->
      <div v-else class="wiz-body">
        <p class="wiz-hint">{{ t('userKeys.wizRegIntro') }}</p>
        <div class="wiz-row">
          <label>{{ t('userKeys.wizPubFormat') }}</label>
          <FComboBox v-model="pubFormat" :options="formatOptions" class="wiz-combo" />
        </div>
        <textarea class="pub-area" readonly :value="pubText" spellcheck="false" />
        <div class="wiz-save">
          <FButton @click="saveAsFile">{{ t('userKeys.wizSaveAsFile') }}</FButton>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="wiz-footer">
        <FButton :disabled="step === 1 || generating" @click="goPrev">{{ t('userKeys.wizPrev') }}</FButton>
        <FButton v-if="step < 4" variant="accent" :disabled="nextDisabled" @click="goNext">
          {{ t('userKeys.wizNext') }}
        </FButton>
        <FButton v-else variant="accent" @click="finish">{{ t('userKeys.wizFinish') }}</FButton>
        <FButton @click="close">{{ t('userKeys.wizCancel') }}</FButton>
      </div>
    </template>
  </FDialog>
</template>

<style scoped>
.wiz {
  display: flex;
  flex-direction: column;
  min-height: 300px;
}
.wiz-step-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  padding-bottom: 10px;
  border-bottom: 1px solid var(--stroke-divider);
  margin-bottom: 14px;
}
.wiz-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
}
.wiz-hint {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-secondary);
  line-height: 1.7;
}
.wiz-hint.sub {
  margin: -4px 0 0;
}
.wiz-hint.bottom {
  margin-top: auto;
  padding-top: 10px;
}
.wiz-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.wiz-row label {
  width: 88px;
  flex: none;
  text-align: right;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.wiz-combo {
  width: 190px;
  flex: none;
}
.wiz-combo.bits {
  width: 100px;
}
.wiz-input {
  width: 300px;
}
.unit {
  font-size: 12.5px;
  color: var(--text-tertiary);
}
.bits-box {
  min-height: 96px;
  background: #000;
  color: #fff;
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 13px;
  letter-spacing: 1px;
  line-height: 1.5;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  white-space: pre;
  overflow: hidden;
}
.pub-area {
  flex: 1;
  min-height: 130px;
  resize: vertical;
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  color: var(--text-primary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 11.5px;
  line-height: 1.55;
  padding: 8px 10px;
  outline: none;
}
.pub-area:focus {
  border-color: var(--accent);
}
.wiz-save {
  display: flex;
  justify-content: flex-end;
}
.wiz-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
