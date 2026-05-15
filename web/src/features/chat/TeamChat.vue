<script setup lang="ts">
import { ref, computed } from 'vue';
import { useChatStore } from '../../stores/chat';


const chatStore = useChatStore();
const activeTab = ref<'team' | 'clan'>('team');
const newMessage = ref('');

const messages = computed(() => {
  return activeTab.value === 'team' ? chatStore.teamMessages : chatStore.clanMessages;
});

const formatTime = (timestamp: number) => {
  const d = new Date(timestamp * 1000);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};



const sendMessage = async () => {
  if (!newMessage.value.trim()) return;
  if (activeTab.value === 'team') {
    await chatStore.sendTeamMessage(newMessage.value);
  } else {
    await chatStore.sendClanMessage(newMessage.value);
  }
  newMessage.value = '';
};
</script>

<template>
  <div class="flex-1 border-r border-rp-700 flex flex-col bg-rp-900/10">
    <div class="flex items-center justify-between px-4 py-2 border-b border-rp-700 bg-rp-900/50">
      <div class="flex items-center gap-4">
        <span class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase">Communications</span>
        <div class="flex gap-4">
          <button 
            @click="activeTab = 'team'"
            class="font-mono text-[9px] uppercase tracking-widest transition-all border-b px-1"
            :class="activeTab === 'team' ? 'text-rp-200 border-rp-200' : 'text-rp-600 border-transparent hover:text-rp-400'"
          >Team</button>
          <button 
            @click="activeTab = 'clan'"
            class="font-mono text-[9px] uppercase tracking-widest transition-all border-b px-1"
            :class="activeTab === 'clan' ? 'text-rp-200 border-rp-200' : 'text-rp-600 border-transparent hover:text-rp-400'"
          >Clan</button>
        </div>
      </div>
    </div>
    
    <div class="flex-1 overflow-y-auto scrollbar-thin p-3 space-y-2 flex flex-col-reverse">
      <div v-if="messages.length === 0" class="text-rp-500 font-mono text-xs uppercase text-center py-4">No messages</div>
      <div v-for="(msg, index) in [...messages].reverse()" :key="index" class="flex items-baseline gap-3 group">
        <span class="font-mono text-[8px] text-rp-600">{{ formatTime(msg.time) }}</span>
        <span class="font-mono text-[10px] min-w-[70px]" :style="{ color: msg.color }">{{ msg.name }}</span>
        <span class="font-mono text-xs tracking-wide text-rp-300">{{ msg.message }}</span>
      </div>
    </div>
    
    <div class="p-2 border-t border-rp-700 bg-rp-900/30 flex gap-2">
      <input 
        v-model="newMessage"
        @keyup.enter="sendMessage"
        type="text" 
        :placeholder="`SEND TO ${activeTab.toUpperCase()}...`" 
        class="flex-1 bg-black/40 border border-rp-700 px-3 py-1 font-mono text-[11px] text-rp-white outline-none focus:border-rp-500 transition-colors"
      />
      <button 
        @click="sendMessage"
        class="bg-rp-300 text-rp-black font-mono text-[9px] px-4 py-1 hover:bg-rp-white transition-colors uppercase tracking-widest"
      >Send</button>
    </div>
  </div>
</template>
