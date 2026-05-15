<script setup lang="ts">
import { computed } from 'vue';
import { useServerInfoStore } from '../../stores/serverInfo';

const infoStore = useServerInfoStore();

const formatTime = (timeFloat: number) => {
  if (timeFloat === undefined || timeFloat === null) return '??:??';
  const hours = Math.floor(timeFloat);
  const minutes = Math.floor((timeFloat - hours) * 60);
  return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}`;
};

const formatWipe = (wipeTime: number) => {
  if (!wipeTime) return 'Unknown';
  const diff = wipeTime - Math.floor(Date.now() / 1000);
  if (diff <= 0) return 'Now';
  const days = Math.floor(diff / 86400);
  const hours = Math.floor((diff % 86400) / 3600);
  return `${days}d ${hours}h`;
};

const stats = computed(() => {
  const info = infoStore.info;
  const time = infoStore.time;
  
  return [
    { label: 'Players', value: info ? info.players.toString() : '0', sub: info ? `/${info.maxPlayers}` : '/0' },
    { label: 'Queue', value: info ? info.queuedPlayers.toString() : '0', sub: '' },
    { id: 'wipe', label: 'Wipe In', value: info ? formatWipe(info.wipeTime) : '??', sub: '' },
    { label: 'In-Game', value: time ? formatTime(time.time) : '??:??', sub: '' },
  ];
});

const dayProgress = computed(() => {
  const time = infoStore.time;
  if (!time) return 50; // Default middle
  // Very rough calculation for day progress
  const len = time.sunset - time.sunrise;
  let current = time.time - time.sunrise;
  if (current < 0) current = 0;
  if (current > len) current = len;
  return (current / len) * 100;
});
</script>

<template>
  <div class="border-b border-rp-700 px-4 py-4 bg-rp-900/30">
    <div class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase mb-4">Server Info</div>
    
    <div class="grid grid-cols-2 gap-y-4 gap-x-2">
      <div v-for="stat in stats" :key="stat.label" class="flex flex-col gap-1">
        <span class="font-mono text-[9px] text-rp-500 uppercase tracking-widest">{{ stat.label }}</span>
        <div class="font-mono text-lg text-rp-white leading-none">
          {{ stat.value }}<span class="text-[10px] text-rp-500 ml-0.5" v-if="stat.sub">{{ stat.sub }}</span>
        </div>
      </div>
    </div>

    <!-- Day Cycle Bar -->
    <div class="mt-5">
      <div class="flex justify-between font-mono text-[8px] text-rp-600 uppercase tracking-widest mb-1.5">
        <span>Dawn {{ infoStore.time ? formatTime(infoStore.time.sunrise) : '06:00' }}</span>
        <span>Dusk {{ infoStore.time ? formatTime(infoStore.time.sunset) : '20:00' }}</span>
      </div>
      <div class="h-1 bg-rp-800 relative overflow-hidden">
        <div class="h-full bg-rp-400" :style="{ width: `${dayProgress}%` }"></div>
        <div class="absolute top-0 h-full w-px bg-rp-white shadow-[0_0_4px_white]" :style="{ left: `${dayProgress}%` }"></div>
      </div>
    </div>
  </div>
</template>
