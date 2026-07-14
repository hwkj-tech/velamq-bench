<script setup lang="ts">
import { CheckCircle2, Info, X, XCircle } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { useToast } from '@/composables/useToast';

const { t } = useI18n();
const { toasts, dismiss } = useToast();
</script>

<template>
  <div class="toast-stack" role="status" aria-live="polite">
    <article v-for="item in toasts" :key="item.id" class="toast-item" :data-tone="item.tone">
      <CheckCircle2 v-if="item.tone === 'success'" :size="18" />
      <XCircle v-else-if="item.tone === 'error'" :size="18" />
      <Info v-else :size="18" />
      <span>{{ item.message }}</span>
      <button class="icon-button tiny" type="button" :aria-label="t('common.close')" @click="dismiss(item.id)">
        <X :size="14" />
      </button>
    </article>
  </div>
</template>
