import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { router } from './router';
import { i18n } from './composables/useI18n';
import ElementPlus from 'element-plus';
import 'element-plus/dist/index.css';
import './theme/tokens.css';
import './theme/global.css';

createApp(App).use(createPinia()).use(router).use(i18n).use(ElementPlus).mount('#app');
