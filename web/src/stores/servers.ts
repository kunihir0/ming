import { defineStore } from 'pinia';
import { ref } from 'vue';

export const useServerStore = defineStore('servers', () => {
  const servers = ref<any[]>([]);
  const activeServerId = ref<number | null>(null);

  const fetchServers = async () => {
    try {
      const res = await fetch('/api/servers');
      if (res.ok) {
        servers.value = await res.json();
        if (servers.value.length > 0 && activeServerId.value === null) {
          activeServerId.value = servers.value[0].id;
        }
      }
    } catch (e) {
      console.error('Failed to fetch servers:', e);
    }
  };

  const setActiveServer = (id: number) => {
    activeServerId.value = id;
  };

  return { servers, activeServerId, fetchServers, setActiveServer };
});