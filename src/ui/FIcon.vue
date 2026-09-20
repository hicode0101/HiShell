<script setup lang="ts">
import { computed } from 'vue'
import { ICONS } from './icons'

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 20 })

const markup = computed(() => ICONS[props.name] ?? '')
</script>

<template>
  <svg
    class="f-icon"
    :width="size"
    :height="size"
    viewBox="0 0 20 20"
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linecap="round"
    stroke-linejoin="round"
    v-html="markup"
  />
</template>

<style scoped>
.f-icon {
  flex: none;
  display: inline-block;
  vertical-align: middle;
}
</style>
