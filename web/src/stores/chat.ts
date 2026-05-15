import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { useServerStore } from './servers';

export const useChatStore = defineStore('chat', () => {
  const serverStore = useServerStore();
  const teamMessages = ref<any[]>([]);
  const clanMessages = ref<any[]>([]);

  const fetchTeamChat = async (serverId: number) => {
    try {
      const res = await fetch(`/api/server/${serverId}/team/chat`);
      if (res.ok) {
        const data = await res.json();
        teamMessages.value = data.messages || [];
      }
    } catch (e) {
      console.error(e);
    }
  };

  const fetchClanChat = async (serverId: number) => {
    try {
      const res = await fetch(`/api/server/${serverId}/clan/chat`);
      if (res.ok) {
        const data = await res.json();
        clanMessages.value = data.messages || [];
      }
    } catch (e) {
      console.error(e);
    }
  };

  const sendTeamMessage = async (message: string) => {
    if (!serverStore.activeServerId || !message.trim()) return;
    try {
      await fetch(`/api/server/${serverStore.activeServerId}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message })
      });
      // Assuming WS will push the new message, but we can also manually fetch
      await fetchTeamChat(serverStore.activeServerId);
    } catch (e) {
      console.error(e);
    }
  };

  const sendClanMessage = async (message: string) => {
    if (!serverStore.activeServerId || !message.trim()) return;
    try {
      await fetch(`/api/server/${serverStore.activeServerId}/clan/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message })
      });
      await fetchClanChat(serverStore.activeServerId);
    } catch (e) {
      console.error(e);
    }
  };

  watch(() => serverStore.activeServerId, (newId) => {
    if (newId) {
      fetchTeamChat(newId);
      fetchClanChat(newId);
    } else {
      teamMessages.value = [];
      clanMessages.value = [];
    }
  }, { immediate: true });

  return { teamMessages, clanMessages, sendTeamMessage, sendClanMessage };
});