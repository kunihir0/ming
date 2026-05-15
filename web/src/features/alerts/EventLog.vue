<script setup lang="ts">
import { useAlertStore } from '../../stores/alerts';

const alertStore = useAlertStore();
</script>

<template>
  <div class="w-[240px] flex flex-col bg-rp-900/10">
    <div class="flex items-center justify-between px-4 py-2 border-b border-rp-700 bg-rp-900/50">
      <span class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase">Event Log</span>
      <div v-if="alertStore.logs.length > 0" class="bg-rp-accent text-rp-black font-mono text-[10px] px-1.5 leading-tight">{{ alertStore.logs.length }}</div>
    </div>
    
    <div class="flex-1 overflow-y-auto scrollbar-thin p-3 space-y-2">
      <div v-if="alertStore.logs.length === 0" class="text-rp-500 font-mono text-xs uppercase text-center py-4">No events</div>
      <div v-for="log in alertStore.logs" :key="log.id" class="flex items-center gap-3 border-b border-rp-800 pb-2 group cursor-default">
        <div class="w-0.5 h-6 transition-colors" 
             :class="{
               'bg-rp-white shadow-[0_0_4px_white]': log.type === 'critical',
               'bg-rp-400': log.type === 'warning',
               'bg-rp-700': log.type === 'info'
             }">
        </div>
        
        <div class="flex-1">
          <div class="font-mono text-[9px] tracking-widest group-hover:text-rp-white transition-colors"
               :class="log.type === 'critical' ? 'text-rp-white' : 'text-rp-400'">
            {{ log.title.toUpperCase() }}
          </div>
          <div class="font-mono text-[8px] text-rp-600 mt-0.5">{{ log.time }}</div>
        </div>
      </div>
    </div>
  </div>
</template>
