import { readonly, ref } from 'vue';

export type ToastTone = 'success' | 'error' | 'info';

export interface ToastItem {
  id: number;
  tone: ToastTone;
  message: string;
}

const toasts = ref<ToastItem[]>([]);
let nextId = 1;

function push(message: string, tone: ToastTone = 'info') {
  const id = nextId++;
  toasts.value = [{ id, tone, message }, ...toasts.value].slice(0, 3);
  window.setTimeout(() => dismiss(id), 4200);
}

function dismiss(id: number) {
  toasts.value = toasts.value.filter((toast) => toast.id !== id);
}

export function useToast() {
  return {
    toasts: readonly(toasts),
    push,
    success: (message: string) => push(message, 'success'),
    error: (message: string) => push(message, 'error'),
    info: (message: string) => push(message, 'info'),
    dismiss,
  };
}
