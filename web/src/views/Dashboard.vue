<script setup lang="ts">
import { onMounted } from 'vue';
import { useAuthStore } from '../stores/auth';
import { useServerStore } from '../stores/servers';
import { useWsStore } from '../stores/ws';
import Header from '../features/navigation/Header.vue';
import Sidebar from '../features/navigation/Sidebar.vue';
import WSStatusBar from '../features/status/WSStatusBar.vue';
import TacticalMap from '../features/map/TacticalMap.vue';
import TeamList from '../features/team/TeamList.vue';
import ServerStats from '../features/team/ServerStats.vue';
import DeviceManager from '../features/devices/DeviceManager.vue';
import TeamChat from '../features/chat/TeamChat.vue';
import EventLog from '../features/alerts/EventLog.vue';

const auth = useAuthStore();
const serverStore = useServerStore();

onMounted(async () => {
  console.log('RUST+ // COMMAND INTERFACE INITIALIZING...');
  await auth.fetchUser();
  
  if (!auth.isAuthenticated) {
    // If not authenticated, show a login screen or redirect
    return;
  }

  await serverStore.fetchServers();
  
  // Initialize the WS store to start listening for active server changes
  useWsStore();
});

const login = () => {
  window.location.href = '/api/auth/discord/login';
};
</script>

<template>
  <div v-if="auth.isInitializing" class="h-screen flex items-center justify-center bg-rp-black text-rp-400 font-mono text-sm tracking-widest uppercase">
    INITIALIZING...
  </div>

  <div v-else-if="!auth.isAuthenticated" class="h-screen flex items-center justify-center bg-rp-black text-rp-white font-body selection:bg-rp-accent selection:text-rp-black p-4">
    <div class="max-w-md w-full bg-rp-800 border border-rp-700 p-8 text-center shadow-2xl">
      <div class="mb-6 flex justify-center">
         <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-rp-accent"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"></path></svg>
      </div>
      <h1 class="text-2xl font-display tracking-widest uppercase mb-2">Access Denied</h1>
      <p class="text-sm text-rp-300 mb-8">You must authenticate via Discord to access the Command Interface.</p>
      <button @click="login" class="w-full border border-rp-600 bg-rp-700 hover:bg-rp-accent hover:text-rp-black text-rp-white font-mono text-xs uppercase tracking-widest py-3 transition-colors">
        Login with Discord
      </button>
    </div>
  </div>

  <div v-else class="h-screen flex flex-col bg-rp-black text-rp-white font-body selection:bg-rp-accent selection:text-rp-black">
    <Header />

    <div class="flex-1 flex pt-14 overflow-hidden">
      <Sidebar />

      <main class="flex-1 flex flex-col overflow-hidden">
        <!-- TOP ROW: MAP + TEAM -->
        <div class="flex flex-1 overflow-hidden">
          <TacticalMap />
          
          <div class="w-[280px] flex flex-col border-r border-rp-700">
            <ServerStats />
            <TeamList />
          </div>
        </div>

        <!-- BOTTOM ROW: DEVICES + CHAT + ALERTS -->
        <div class="h-[220px] border-t border-rp-700 flex overflow-hidden">
          <DeviceManager />
          <TeamChat />
          <EventLog />
        </div>
      </main>
    </div>

    <WSStatusBar />
  </div>
</template>