import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface Alert {
  id: number;
  type: 'critical' | 'warning' | 'info';
  title: string;
  time: string;
}

export const useAlertStore = defineStore('alerts', () => {
  const logs = ref<Alert[]>([]);

  const addAlert = (type: 'critical' | 'warning' | 'info', title: string) => {
    const d = new Date();
    const time = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
    
    logs.value.unshift({
      id: Date.now(),
      type,
      title,
      time
    });

    // Keep only last 50 alerts
    if (logs.value.length > 50) {
      logs.value.pop();
    }
  };

  const clearAlerts = () => {
    logs.value = [];
  };

  return { logs, addAlert, clearAlerts };
});