<script setup lang="ts">
import { Map, Users, HardDrive, Video, MessageSquare, BarChart3, Server, Store } from 'lucide-vue-next';
import { ref } from 'vue';

const activePanel = ref('map');

const navItems = [
  { id: 'map', icon: Map, label: 'Map' },
  { id: 'team', icon: Users, label: 'Team' },
  { id: 'devices', icon: HardDrive, label: 'Devices' },
  { id: 'cctv', icon: Video, label: 'CCTV' },
  { id: 'chat', icon: MessageSquare, label: 'Chat' },
  { id: 'stats', icon: BarChart3, label: 'Stats' },
  { id: 'market', icon: Store, label: 'Market' },
];
</script>

<template>
  <aside class="w-[52px] border-r border-rp-700 flex flex-col items-center py-4 gap-5 z-40 bg-rp-900">
    <button 
      v-for="item in navItems" 
      :key="item.id"
      @click="activePanel = item.id"
      class="w-9 h-9 flex items-center justify-center cursor-pointer transition-all relative group"
      :class="[
        activePanel === item.id ? 'text-rp-white border border-rp-700 bg-rp-white/5' : 'text-rp-400 hover:text-rp-200',
        item.id === 'market' ? 'mt-4' : ''
      ]"
      :title="item.label"
      @click.stop="item.id === 'market' ? $router.push('/market') : null"
    >
      <component :is="item.icon" :size="20" :stroke-width="activePanel === item.id ? 1.5 : 1.2" />
      
      <!-- Tooltip -->
      <div class="absolute left-full ml-2 px-2 py-1 bg-rp-800 border border-rp-700 text-rp-300 text-[10px] font-mono uppercase tracking-widest opacity-0 group-hover:opacity-100 pointer-events-none z-50 whitespace-nowrap transition-opacity">
        {{ item.label }}
      </div>
    </button>

    <div class="w-6 h-px bg-rp-700 my-1"></div>

    <button class="w-9 h-9 flex items-center justify-center text-rp-400 hover:text-rp-white transition-all group relative" title="Servers">
      <Server :size="20" stroke-width="1.2" />
      <div class="absolute right-1.5 bottom-1.5 w-1.5 h-1.5 rounded-full bg-green-500 shadow-[0_0_4px_rgba(34,197,94,0.5)]"></div>
      
      <div class="absolute left-full ml-2 px-2 py-1 bg-rp-800 border border-rp-700 text-rp-300 text-[10px] font-mono uppercase tracking-widest opacity-0 group-hover:opacity-100 pointer-events-none z-50 whitespace-nowrap transition-opacity">
        Servers
      </div>
    </button>
  </aside>
</template>
