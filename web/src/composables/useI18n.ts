import { createI18n } from 'vue-i18n';
import en from '../../locales/en.json';
import zhCN from '../../locales/zh-CN.json';

const stored = localStorage.getItem('velamq.lang');
const browser = navigator.language.startsWith('zh') ? 'zh-CN' : 'en';

export const i18n = createI18n({
  legacy: false,
  locale: stored ?? browser,
  fallbackLocale: 'en',
  messages: { en, 'zh-CN': zhCN },
  missingWarn: true,
  fallbackWarn: false,
});
