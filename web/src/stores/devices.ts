import { defineStore } from 'pinia';
import { ref } from 'vue';
import { useServerStore } from './servers';

export const useDeviceStore = defineStore('devices', () => {
  const serverStore = useServerStore();
  const devices = ref<any[]>([]); // In a real app, these IDs would be saved in the DB per user

  const toggleDevice = async (entityId: number, value: boolean) => {
    if (!serverStore.activeServerId) return;
    try {
      const res = await fetch(`/api/server/${serverStore.activeServerId}/entity/${entityId}/toggle`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ value })
      });
      if (res.ok) {
        const device = devices.value.find(d => d.id === entityId);
        if (device) device.on = value;
      }
    } catch (e) {
      console.error(e);
    }
  };

  const getEntity = async (entityId: number) => {
    if (!serverStore.activeServerId) return;
    try {
      const res = await fetch(`/api/server/${serverStore.activeServerId}/entity/${entityId}`);
      if (res.ok) {
        const data = await res.json();
        // Update or add to list
        const existing = devices.value.find(d => d.id === entityId);
        if (existing) {
          existing.on = data.entity.payload.value;
        } else {
          devices.value.push({ id: entityId, name: `Entity ${entityId}`, on: data.entity.payload.value });
        }
      }
    } catch (e) {
      console.error(e);
    }
  };

  return { devices, toggleDevice, getEntity };
});