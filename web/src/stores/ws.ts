import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { useServerStore } from './servers';
import { useChatStore } from './chat';
import { useTeamStore } from './team';
import { useAlertStore } from './alerts';
import { useMapStore } from './map';

export const useWsStore = defineStore('ws', () => {
  const serverStore = useServerStore();
  const chatStore = useChatStore();
  const teamStore = useTeamStore();
  const alertStore = useAlertStore();
  const mapStore = useMapStore();
  
  const isConnected = ref(false);
  const reconnectAttempts = ref(0);
  let socket: WebSocket | null = null;

  const connect = (serverId: number) => {
    if (socket) {
      socket.close();
    }

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/api/server/${serverId}/ws`;
    
    socket = new WebSocket(wsUrl);

    socket.onopen = () => {
      isConnected.value = true;
      reconnectAttempts.value = 0;
      console.log(`WS Connected to server ${serverId}`);
      alertStore.addAlert('info', 'WS CONNECTED');
    };

    socket.onclose = () => {
      isConnected.value = false;
      console.log('WS Disconnected');
      alertStore.addAlert('warning', 'WS DISCONNECTED');
      
      // Auto reconnect
      if (reconnectAttempts.value < 5) {
        setTimeout(() => {
          reconnectAttempts.value++;
          if (serverStore.activeServerId === serverId) {
            connect(serverId);
          }
        }, 3000 * Math.pow(1.5, reconnectAttempts.value));
      }
    };

    socket.onerror = (error) => {
      console.error('WS Error:', error);
      socket?.close();
    };

    socket.onmessage = (event) => {
      try {
        const payload = JSON.parse(event.data);
        handleMessage(payload, serverId);
      } catch (e) {
        console.error('Failed to parse WS message:', e);
      }
    };
  };

  const handleMessage = (payload: any, serverId: number) => {
    switch (payload.type) {
      case 'broadcast':
        const broadcast = payload.data;
        if (broadcast.teamMessage) {
          // Add to team chat
          chatStore.teamMessages.push(broadcast.teamMessage.message);
        } else if (broadcast.clanMessage) {
          chatStore.clanMessages.push(broadcast.clanMessage.message);
        } else if (broadcast.teamChanged || broadcast.clanChanged) {
          // Refresh team data
          teamStore.fetchTeamData(serverId);
          alertStore.addAlert('info', 'TEAM DATA UPDATED');
        } else if (broadcast.entityChanged) {
          alertStore.addAlert('warning', `ENTITY ${broadcast.entityChanged.entityId} TRIGGERED`);
        }
        break;
      case 'markers':
        mapStore.markers = payload.data;
        break;
      case 'explosion':
        mapStore.addExplosion(payload.data);
        alertStore.addAlert('critical', 'EXPLOSION DETECTED');
        break;
      case 'cargo_spawned':
        alertStore.addAlert('critical', 'CARGO SHIP SPAWNED');
        break;
      case 'camera_motion':
        alertStore.addAlert('critical', `MOTION AT CAMERA ${payload.camera_id}`);
        break;
    }
  };

  watch(() => serverStore.activeServerId, (newId) => {
    if (newId) {
      connect(newId);
    } else if (socket) {
      socket.close();
    }
  }, { immediate: true });

  return { isConnected, reconnectAttempts };
});