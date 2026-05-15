import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { useServerStore } from './servers';

export const useTeamStore = defineStore('team', () => {
  const serverStore = useServerStore();
  const team = ref<any>(null);
  const stats = ref<any>(null);
  const loading = ref(false);

  const fetchTeamData = async (serverId: number) => {
    loading.value = true;
    try {
      const [teamRes, statsRes] = await Promise.all([
        fetch(`/api/server/${serverId}/team`),
        fetch(`/api/server/${serverId}/stats`)
      ]);

      if (teamRes.ok) {
        const teamData = await teamRes.json();
        team.value = teamData.team;
      }
      
      if (statsRes.ok) {
        stats.value = await statsRes.json();
      }
    } catch (e) {
      console.error('Failed to fetch team data:', e);
    } finally {
      loading.value = false;
    }
  };

  const promoteToLeader = async (steamId: number) => {
    if (!serverStore.activeServerId) return;
    try {
      await fetch(`/api/server/${serverStore.activeServerId}/team/promote`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ steam_id: steamId })
      });
      await fetchTeamData(serverStore.activeServerId);
    } catch (e) {
      console.error('Failed to promote user:', e);
    }
  };

  watch(() => serverStore.activeServerId, (newId) => {
    if (newId) {
      fetchTeamData(newId);
    } else {
      team.value = null;
      stats.value = null;
    }
  }, { immediate: true });

  return { team, stats, loading, fetchTeamData, promoteToLeader };
});