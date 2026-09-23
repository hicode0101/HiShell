import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { isTauri } from './api/adapter'
import './styles/fluent/tokens.css'
import './styles/global.css'

createApp(App).use(createPinia()).mount('#app')

// 屏蔽 WebView2 默认右键菜单（返回/刷新/另存为/打印/更多工具）。
// 捕获阶段全局 preventDefault：应用自带的 Fluent 右键菜单在各自元素上
// 自行触发显示，不受影响；未接管区域的系统默认菜单不再弹出。
if (isTauri) {
  document.addEventListener('contextmenu', (e) => e.preventDefault(), true)
}
