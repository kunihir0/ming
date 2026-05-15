<script setup lang="ts">
import { ref } from 'vue';
import { Zap } from 'lucide-vue-next';
import { useDeviceStore } from '../../stores/devices';

const deviceStore = useDeviceStore();
const newEntityId = ref('');

const addDevice = async () => {
  const id = parseInt(newEntityId.value, 10);
  if (!isNaN(id)) {
    await deviceStore.getEntity(id);
    newEntityId.value = '';
  }
};

const toggleDevice = (id: number, currentOn: boolean) => {
  deviceStore.toggleDevice(id, !currentOn);
};
</script>

<template>
  <div class="w-[380px] border-r border-rp-700 flex flex-col bg-rp-900/10">
    <div class="flex items-center justify-between px-4 py-2 border-b border-rp-700 bg-rp-900/50">
      <span class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase">Smart Devices</span>
      <div class="flex items-center gap-2">
        <input 
          v-model="newEntityId" 
          @keyup.enter="addDevice"
          type="text" 
          placeholder="ENTITY ID..." 
          class="w-24 bg-black/40 border border-rp-700 px-2 py-0.5 font-mono text-[9px] text-rp-white outline-none focus:border-rp-500 transition-colors uppercase"
        />
        <button @click="addDevice" class="font-mono text-[9px] border border-rp-700 px-2 py-0.5 text-rp-500 hover:text-rp-300 hover:border-rp-500 uppercase transition-all">Add</button>
      </div>
    </div>
    
    <div class="flex-1 overflow-x-auto scrollbar-thin p-3">
      <div v-if="deviceStore.devices.length === 0" class="h-full flex items-center justify-center font-mono text-xs text-rp-500 uppercase text-center px-4">
        Enter a Smart Switch ID above to connect.
      </div>
      <div v-else class="flex gap-2 h-full">
        <div 
          v-for="device in deviceStore.devices" 
          :key="device.id"
          @click="toggleDevice(device.id, device.on)"
          class="min-w-[88px] h-full border p-2 flex flex-col items-center justify-between cursor-pointer transition-all relative group"
          :class="device.on ? 'border-rp-400 bg-rp-white/5' : 'border-rp-700 bg-black/40'"
        >
          <!-- Top Indicator Line -->
          <div class="absolute top-0 left-0 right-0 h-0.5 transition-colors" :class="device.on ? 'bg-rp-300 shadow-[0_0_4px_var(--color-rp-300)]' : 'bg-transparent'"></div>
          
          <div class="mt-2 text-rp-400 group-hover:scale-110 transition-transform" :class="{ 'text-rp-200': device.on }">
            <Zap :size="20" :stroke-width="1.2" />
          </div>
          
          <div class="font-mono text-[8px] text-center leading-tight uppercase tracking-widest mt-2" :class="device.on ? 'text-rp-white' : 'text-rp-500'">
            {{ device.name }}
          </div>
          
          <div class="font-mono text-[9px] uppercase tracking-widest" :class="device.on ? 'text-rp-white font-bold' : 'text-rp-600'">
            {{ device.on ? 'ON' : 'OFF' }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
