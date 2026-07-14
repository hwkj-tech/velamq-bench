import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api/client';
import type { BrokerProfile, PayloadProfile } from '@/api/types';

export const useProfilesStore = defineStore('profiles', () => {
  const brokers = ref<BrokerProfile[]>([]);
  const payloads = ref<PayloadProfile[]>([]);
  async function load() {
    const [brokerList, payloadList] = await Promise.all([api.brokers(), api.payloads()]);
    brokers.value = brokerList;
    payloads.value = payloadList;
  }
  return { brokers, payloads, load };
});
