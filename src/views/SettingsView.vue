<script setup lang="ts">
import { computed, ref } from 'vue'
import { useSettingsStore, ACCENT_PRESETS, type TerminalColorScheme } from '../stores/settings'
import { FButton, FComboBox, toast } from '../ui'
import { appInfo } from '../api/backend'
import { t, languageCode, systemLanguageCode, type LangPref } from '../i18n'

const settings = useSettingsStore()
const info = ref('')

appInfo().then((i) => (info.value = `${i.name} ${i.version} — ${i.mode}`))

const nav = computed(() => [
  { id: 'appearance', label: t('settings.appearance'), icon: '★' },
  { id: 'terminal', label: t('settings.terminalNav'), icon: '>' },
  { id: 'connection', label: t('settings.connection'), icon: '⇄' },
  { id: 'update', label: t('settings.update'), icon: '↓' },
  { id: 'about', label: t('settings.about'), icon: 'i' },
])
const activeNav = ref('appearance')

/* 语言（跟随系统 / English / 简体中文） */
const LANG_OPTIONS: { pref: LangPref; fixedLabel: string }[] = [
  { pref: 'system', fixedLabel: '' }, // fixedLabel 留空 → 使用本地化标签
  { pref: 'en-US', fixedLabel: 'English' },
  { pref: 'zh-CN', fixedLabel: '简体中文' },
]
const langLabels = computed(() =>
  LANG_OPTIONS.map((o) => ({ pref: o.pref, label: o.fixedLabel || (o.pref === 'system' ? t('settings.followSystem') : o.pref) })),
)
const langModel = computed({
  get: () => {
    const cur = langLabels.value.find((o) => o.pref === settings.s.language)
    return cur?.label ?? t('settings.followSystem')
  },
  set: (label: string) => {
    const o = langLabels.value.find((x) => x.label === label)
    if (o) settings.patch({ language: o.pref })
  },
})
const langLabelList = computed(() => langLabels.value.map((o) => o.label))

const themeModel = computed({
  get: () => (settings.theme === 'dark' ? t('settings.dark') : t('settings.light')),
  set: (v: string) => settings.patch({ theme: v === t('settings.dark') ? 'dark' : 'light' }),
})

const TERMINAL_FONTS = [
  { label: 'Cascadia Mono', value: "'Cascadia Mono', 'Cascadia Code', Consolas, monospace" },
  { label: 'Consolas', value: 'Consolas, monospace' },
  { label: 'Courier New', value: "'Courier New', monospace" },
  { label: 'Sarasa Mono SC', value: "'Sarasa Mono SC', Consolas, monospace" },
]
const fontModel = computed({
  get: () => TERMINAL_FONTS.find((f) => f.value === settings.s.terminalFontFamily)?.label ?? TERMINAL_FONTS[0]!.label,
  set: (label: string) => {
    const f = TERMINAL_FONTS.find((x) => x.label === label)
    if (f) settings.patch({ terminalFontFamily: f.value })
  },
})
const fontLabels = TERMINAL_FONTS.map((f) => f.label)
const fontSizes = ['12', '13', '14', '15', '16', '18', '20']
const fontSizeModel = computed({
  get: () => String(settings.s.terminalFontSize),
  set: (v: string) => settings.patch({ terminalFontSize: Number(v) || 14 }),
})

/* 终端配色方案 */
const SCHEMES: { value: TerminalColorScheme; label: () => string }[] = [
  { value: 'dark', label: () => t('settings.termSchemeDark') },
  { value: 'light', label: () => t('settings.termSchemeLight') },
  { value: 'followApp', label: () => t('settings.termSchemeFollow') },
]
const schemeLabels = computed(() => SCHEMES.map((s) => s.label()))
const schemeModel = computed({
  get: () => SCHEMES.find((s) => s.value === settings.s.terminalColorScheme)?.label() ?? SCHEMES[0]!.label(),
  set: (label: string) => {
    const s = SCHEMES.find((x) => x.label() === label)
    if (s) settings.patch({ terminalColorScheme: s.value })
  },
})
</script>

<template>
  <div class="settings-view">
    <div class="sv-nav">
      <div class="sv-nav-title">{{ t('settings.title') }}</div>
      <div
        v-for="item in nav"
        :key="item.id"
        class="sv-nav-item"
        :class="{ on: activeNav === item.id }"
        @click="activeNav = item.id"
      >
        <span class="sv-nav-icon">{{ item.icon }}</span>
        {{ item.label }}
      </div>
    </div>

    <div class="sv-content">
      <template v-if="activeNav === 'appearance'">
        <div class="sv-title">{{ t('settings.appearance') }}</div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.language') }}</div>
          <div class="sv-grid">
            <span class="sv-label">{{ t('settings.language') }}</span>
            <FComboBox v-model="langModel" :options="langLabelList" />
          </div>
          <div class="lang-code">
            {{ settings.s.language === 'system' ? t('settings.currentLangSystem', { code: languageCode(), system: systemLanguageCode() }) : t('settings.currentLang', { code: languageCode() }) }}
          </div>
        </div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.appTheme') }}</div>
          <div class="theme-cards">
            <div class="theme-card" :class="{ on: settings.theme === 'light' }" @click="settings.patch({ theme: 'light' })">
              <div class="preview light">
                <div class="pv-bar" /><div class="pv-body" />
              </div>
              <span>{{ t('settings.light') }}</span>
            </div>
            <div class="theme-card" :class="{ on: settings.theme === 'dark' }" @click="settings.patch({ theme: 'dark' })">
              <div class="preview dark">
                <div class="pv-bar" /><div class="pv-body" />
              </div>
              <span>{{ t('settings.dark') }}</span>
            </div>
          </div>
        </div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.accentColor') }}</div>
          <div class="accent-row">
            <button
              v-for="c in ACCENT_PRESETS"
              :key="c"
              class="accent-dot"
              :class="{ on: settings.accent === c }"
              :style="{ background: c }"
              @click="settings.patch({ accent: c })"
            />
          </div>
        </div>
      </template>

      <template v-else-if="activeNav === 'terminal'">
        <div class="sv-title">{{ t('settings.terminalNav') }}</div>
        <div class="sv-block">
          <div class="sv-grid">
            <span class="sv-label">{{ t('settings.font') }}</span>
            <FComboBox v-model="fontModel" :options="fontLabels" />
            <span class="sv-label">{{ t('settings.fontSize') }}</span>
            <FComboBox v-model="fontSizeModel" :options="fontSizes" />
            <span class="sv-label">{{ t('settings.colorScheme') }}</span>
            <FComboBox v-model="schemeModel" :options="schemeLabels" />
          </div>
        </div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.terminalHint') }}</div>
        </div>
      </template>

      <template v-else-if="activeNav === 'connection'">
        <div class="sv-title">{{ t('settings.connection') }}</div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.connectionHint') }}</div>
        </div>
      </template>

      <template v-else-if="activeNav === 'update'">
        <div class="sv-title">{{ t('settings.update') }}</div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.updateHint') }}</div>
          <FButton @click="toast(t('settings.versionToast'))">{{ t('settings.checkUpdate') }}</FButton>
        </div>
      </template>

      <template v-else>
        <div class="sv-title">{{ t('settings.about') }}</div>
        <div class="sv-block">
          <div class="about-logo">&gt;_</div>
          <div class="sv-hint">{{ info || 'HiShell' }}</div>
          <div class="sv-hint">Tauri 2 + Rust + Vue 3 · russh · Win11 Fluent Design</div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  display: flex;
  height: 100%;
  min-height: 0;
}
.sv-nav {
  width: 180px;
  flex: none;
  padding: 12px 8px;
  border-right: 1px solid var(--stroke-divider);
  overflow: auto;
}
.sv-nav-title {
  font-size: 15px;
  font-weight: 600;
  padding: 4px 10px 12px;
}
.sv-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-primary);
  font-size: 13px;
}
.sv-nav-item:hover {
  background: var(--bg-alt-hover);
}
.sv-nav-item.on {
  background: var(--bg-selected);
  font-weight: 600;
}
.sv-nav-icon {
  width: 16px;
  text-align: center;
  color: var(--accent);
}
.sv-content {
  flex: 1;
  overflow: auto;
  padding: 20px 28px;
}
.sv-title {
  font-size: 20px;
  font-weight: 600;
  margin-bottom: 18px;
}
.sv-block {
  margin-bottom: 24px;
}
.sv-row-label {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 10px;
}
.sv-grid {
  display: grid;
  grid-template-columns: 90px 260px;
  gap: 12px;
  align-items: center;
}
.sv-label {
  color: var(--text-secondary);
  text-align: right;
  font-size: 12.5px;
}
.sv-hint {
  color: var(--text-tertiary);
  font-size: 12.5px;
  line-height: 1.6;
  margin-bottom: 12px;
}
.lang-code {
  margin: 10px 0 0 102px;
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
}
.theme-cards {
  display: flex;
  gap: 12px;
}
.theme-card {
  cursor: pointer;
  text-align: center;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.theme-card.on span {
  color: var(--accent);
  font-weight: 600;
}
.preview {
  width: 120px;
  height: 76px;
  border-radius: var(--radius-md);
  border: 2px solid transparent;
  overflow: hidden;
  margin-bottom: 6px;
}
.theme-card.on .preview {
  border-color: var(--accent);
}
.preview.light {
  background: #f3f3f3;
}
.preview.light .pv-bar {
  height: 18px;
  background: #ffffff;
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}
.preview.light .pv-body {
  margin: 8px;
  height: calc(100% - 34px);
  background: #ffffff;
  border-radius: 3px;
}
.preview.dark {
  background: #202020;
}
.preview.dark .pv-bar {
  height: 18px;
  background: #2d2d2d;
}
.preview.dark .pv-body {
  margin: 8px;
  height: calc(100% - 34px);
  background: #333;
  border-radius: 3px;
}
.accent-row {
  display: flex;
  gap: 10px;
}
.accent-dot {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
  outline: 2px solid transparent;
  outline-offset: 2px;
}
.accent-dot.on {
  outline-color: var(--accent);
}
.about-logo {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  background: var(--accent);
  color: var(--accent-text-on);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 22px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 12px;
}
</style>
