import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { useServerStore } from './servers';

export const useServerInfoStore = defineStore('serverInfo', () => {
  const serverStore = useServerStore();
  const info = ref<any>(null);
  const time = ref<any>(null);
  const mapMeta = ref<{ width: number; height: number; margin: number } | null>(null);

  const fetchInfo = async (serverId: number) => {
    try {
      const [infoRes, timeRes, metaRes] = await Promise.all([
        fetch(`/api/server/${serverId}/info`),
        fetch(`/api/server/${serverId}/time`),
        fetch(`/api/server/${serverId}/map/meta`),
      ]);
      
      if (infoRes.ok) {
        const infoData = await infoRes.json();
        info.value = infoData.info;
      }
      
      if (timeRes.ok) {
        const timeData = await timeRes.json();
        time.value = timeData.time;
      }

      if (metaRes.ok) {
        const metaData = await metaRes.json();
        if (!metaData.error) {
          mapMeta.value = metaData;
        }
      }
    } catch (e) {
      console.error(e);
    }
  };

  watch(() => serverStore.activeServerId, (newId) => {
    if (newId) {
      fetchInfo(newId);
    } else {
      info.value = null;
      time.value = null;
    }
  }, { immediate: true });

  return { info, time, mapMeta };
});