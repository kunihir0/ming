<script setup lang="ts">
import { useWsStore } from '../../stores/ws';

const wsStore = useWsStore();
const version = 'v1.0.0';
</script>

<template>
  <div class="fixed bottom-0 left-[52px] right-0 h-[20px] border-t border-rp-700 flex items-center gap-6 px-4 z-50 bg-rp-black/90 backdrop-blur-sm">
    <div class="flex items-center gap-2">
      <div class="w-1.5 h-1.5 rounded-full" :class="wsStore.isConnected ? 'bg-rp-200 shadow-[0_0_4px_var(--color-rp-200)]' : 'bg-red-500 shadow-[0_0_4px_var(--color-red-500)]'"></div>
      <span class="font-mono text-[9px] tracking-wider" :class="wsStore.isConnected ? 'text-rp-500' : 'text-red-500'">WS:{{ wsStore.isConnected ? 'CONNECTED' : 'DISCONNECTED' }}</span>
    </div>
    
    <span class="font-mono text-[9px] text-rp-600 tracking-widest uppercase">Reconnects: {{ wsStore.reconnectAttempts }}</span>
    
    <div class="flex-1"></div>
    
    <span class="font-mono text-[9px] text-rp-600 tracking-widest uppercase">{{ version }}</span>
  </div>
</template>
