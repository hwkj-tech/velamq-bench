import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

export type ThemeChoice = 'system' | 'light' | 'dark';

function readTheme(): ThemeChoice {
  const value = localStorage.getItem('velamq.theme');
  return value === 'light' || value === 'dark' || value === 'system' ? value : 'dark';
}

export const useUIStore = defineStore('ui', () => {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  const theme = ref<ThemeChoice>(readTheme());
  const systemDark = ref(media.matches);
  const resolvedTheme = computed<'light' | 'dark'>(() => (theme.value === 'system' ? (systemDark.value ? 'dark' : 'light') : theme.value));
  const isDark = computed(() => resolvedTheme.value === 'dark');

  media.addEventListener('change', (event) => {
    systemDark.value = event.matches;
  });

  function setTheme(value: ThemeChoice) {
    theme.value = value;
    localStorage.setItem('velamq.theme', value);
  }

  function toggleTheme() {
    setTheme(isDark.value ? 'light' : 'dark');
  }

  return { theme, resolvedTheme, isDark, setTheme, toggleTheme };
});
