<script setup lang="ts">
import { computed } from 'vue';
import { useTeamStore } from '../../stores/team';

const teamStore = useTeamStore();

const members = computed(() => teamStore.team?.members || []);
const onlineCount = computed(() => members.value.filter((m: any) => m.isOnline).length);
const offlineCount = computed(() => members.value.length - onlineCount.value);
const totalDeaths = computed(() => {
  if (!teamStore.stats || !teamStore.stats.deaths) return 0;
  return Object.values(teamStore.stats.deaths).reduce((a: any, b: any) => a + b, 0);
});

const handlePromote = (steamId: number) => {
  teamStore.promoteToLeader(steamId);
};
</script>

<template>
  <div class="flex-1 flex flex-col overflow-hidden bg-rp-900/20">
    <div class="flex items-center justify-between px-4 py-2 border-b border-rp-700 bg-rp-900/50">
      <span class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase">Team</span>
    </div>
    
    <div class="flex-1 overflow-y-auto scrollbar-thin relative">
      <div v-if="teamStore.loading" class="absolute inset-0 flex items-center justify-center text-rp-500 font-mono text-xs uppercase">
        Loading...
      </div>
      <div v-else-if="members.length === 0" class="p-4 text-center text-rp-500 font-mono text-xs uppercase">
        No Team Data
      </div>
      <div 
        v-else
        v-for="member in members" 
        :key="member.steamId"
        class="flex items-center gap-3 p-3 border-b border-rp-800 hover:bg-rp-white/5 cursor-pointer transition-all"
      >
        <div class="w-0.5 h-8" :class="member.isOnline ? 'bg-rp-300 shadow-[0_0_4px_var(--color-rp-300)]' : 'bg-rp-700'"></div>
        
        <div class="flex-1">
          <div class="flex items-center gap-2 mb-0.5">
            <span class="font-mono text-xs tracking-wide" :class="member.isOnline ? 'text-rp-white' : 'text-rp-500'">{{ member.name.toUpperCase() }}</span>
            <span v-if="teamStore.team?.leaderSteamId === member.steamId" class="text-[8px] font-mono border border-rp-400 text-rp-400 px-1">LEADER</span>
            <button 
              v-else 
              @click="handlePromote(member.steamId)"
              class="text-[8px] font-mono border border-rp-700 text-rp-600 hover:text-rp-300 hover:border-rp-400 px-1 transition-colors uppercase">
              Promote
            </button>
          </div>
          <div class="font-mono text-[9px] text-rp-600 uppercase tracking-tight">
            X:{{ Math.round(member.x) }} Y:{{ Math.round(member.y) }}
            <span v-if="!member.isAlive" class="ml-2 text-red-500">DEAD</span>
          </div>
        </div>
        
        <div class="flex flex-col items-end gap-1">
          <div class="w-1.5 h-1.5 rounded-full" :class="member.isOnline ? 'bg-rp-200 shadow-[0_0_4px_var(--color-rp-200)] animate-pulse' : 'bg-rp-800'"></div>
          <span class="font-mono text-[8px] text-rp-600 uppercase">{{ member.isOnline ? 'Online' : 'Offline' }}</span>
        </div>
      </div>
    </div>

    <!-- Stats Footer -->
    <div class="border-t border-rp-700 px-4 py-3 flex justify-between bg-rp-900/50">
      <div class="text-center">
        <div class="font-mono text-xs text-rp-white">{{ onlineCount }}</div>
        <div class="font-mono text-[8px] text-rp-600 uppercase tracking-widest mt-1">Online</div>
      </div>
      <div class="w-px bg-rp-800"></div>
      <div class="text-center">
        <div class="font-mono text-xs text-rp-500">{{ offlineCount }}</div>
        <div class="font-mono text-[8px] text-rp-600 uppercase tracking-widest mt-1">Offline</div>
      </div>
      <div class="w-px bg-rp-800"></div>
      <div class="text-center flex-1">
        <div class="font-mono text-xs text-rp-white">{{ totalDeaths }}</div>
        <div class="font-mono text-[8px] text-rp-600 uppercase tracking-widest mt-1">Total Deaths</div>
      </div>
    </div>
  </div>
</template>
