<script setup lang="ts">
import { Hexagon, User } from 'lucide-vue-next';
import { useAuthStore } from '../../stores/auth';
import { useServerStore } from '../../stores/servers';
import { computed } from 'vue';

const auth = useAuthStore();
const serverStore = useServerStore();

const activeServerName = computed(() => {
  if (!serverStore.activeServerId || serverStore.servers.length === 0) return 'NO SERVER SELECTED';
  const server = serverStore.servers.find(s => s.id === serverStore.activeServerId);
  return server ? server.name.toUpperCase() : 'UNKNOWN SERVER';
});
</script>

<template>
  <header class="fixed top-0 left-0 right-0 z-50 h-14 flex items-center px-6 border-b border-rp-700 bg-rp-black/80 backdrop-blur-md">
    <!-- Logo -->
    <div class="flex items-center gap-3 min-w-[200px]">
      <div class="text-rp-accent">
        <Hexagon :size="28" stroke-width="1.5" />
      </div>
      <div>
        <div class="font-display text-xl tracking-widest text-rp-white leading-none">RUST+</div>
        <div class="font-mono text-[9px] tracking-[0.4em] text-rp-400 uppercase">Command Interface</div>
      </div>
    </div>

    <!-- Server Selector -->
    <div class="flex-1 flex items-center justify-center">
      <div class="font-mono text-xs text-rp-300 tracking-widest uppercase border border-rp-600 px-4 py-1.5 cursor-pointer flex items-center gap-3 hover:border-rp-400 transition-colors relative group">
        <span class="w-1.5 h-1.5 rounded-full shadow-[0_0_6px_var(--color-rp-200)] animate-pulse-dot"
          :class="serverStore.activeServerId ? 'bg-rp-200' : 'bg-red-500'"></span>
        <span>{{ activeServerName }}</span>
        <svg width="10" height="6" viewBox="0 0 10 6" fill="none" class="text-rp-500"><path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5"/></svg>

        <!-- Dropdown for servers -->
        <div class="absolute top-full mt-1 left-0 w-full bg-rp-800 border border-rp-700 hidden group-hover:block z-50">
          <div 
            v-for="s in serverStore.servers" 
            :key="s.id"
            @click="serverStore.setActiveServer(s.id)"
            class="px-4 py-2 hover:bg-rp-700 hover:text-rp-white transition-colors"
          >
            {{ s.name.toUpperCase() }}
          </div>
          <div v-if="serverStore.servers.length === 0" class="px-4 py-2 text-rp-500 italic">No Servers Paired</div>
        </div>
      </div>
    </div>

    <!-- Right Controls -->
    <div class="flex items-center gap-5 min-w-[200px] justify-end">
      <!-- User Avatar -->
      <div class="flex items-center gap-2 cursor-pointer group" @click="auth.logout()">
        <img v-if="auth.user && auth.user.avatar" 
          :src="`https://cdn.discordapp.com/avatars/${auth.user.discord_id}/${auth.user.avatar}.png`" 
          class="w-8 h-8 border border-rp-600 group-hover:border-rp-300 transition-colors object-cover" />
        <div v-else class="w-8 h-8 border border-rp-600 group-hover:border-rp-300 transition-colors flex items-center justify-center">
          <User :size="16" class="text-rp-400 group-hover:text-rp-200" />
        </div>
        <span class="font-mono text-xs text-rp-400 group-hover:text-rp-200 transition-colors">
          {{ auth.user ? auth.user.username.toUpperCase() : 'GUEST' }}
        </span>
      </div>
    </div>
  </header>
</template>
