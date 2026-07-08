<script setup lang="ts">
import { ref, onMounted } from 'vue';

interface MarketItem {
  id: string;
  name: string;
  price: number;
}

const items = ref<MarketItem[]>([]);

onMounted(async () => {
  // Fetch ticker data
  try {
    const res = await fetch('/api/market/ticker');
    if (res.ok) {
      const data = await res.json();
      items.value = data.data;
    }
  } catch (e) {
    console.error('Failed to fetch market ticker', e);
  }
});
</script>

<template>
  <div class="min-h-screen bg-rp-black text-rp-white font-mono p-6">
    <header class="flex justify-between items-center mb-8 border-b border-rp-700 pb-4">
      <div class="flex items-center space-x-4">
        <button @click="$router.push('/')" class="p-2 border border-rp-700 bg-rp-800 hover:bg-rp-700 text-rp-400 transition-colors">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>
        </button>
        <h1 class="text-3xl font-display uppercase tracking-widest text-rp-accent">Rust Vending Exchange</h1>
      </div>
      <div class="flex items-center space-x-4">
        <span class="text-xs text-rp-400">STATUS:</span>
        <span class="px-2 py-1 bg-green-900 text-green-400 text-xs border border-green-700">LIVE</span>
      </div>
    </header>

    <main class="grid grid-cols-1 lg:grid-cols-4 gap-6">
      <!-- Sidebar / Ticker -->
      <div class="col-span-1 border border-rp-700 bg-rp-800 p-4">
        <h2 class="text-sm uppercase tracking-widest mb-4 text-rp-400 border-b border-rp-700 pb-2">Top Traded</h2>
        
        <div v-if="items.length === 0" class="text-xs text-rp-500 py-4 text-center">
          NO DATA AVAILABLE
        </div>
        
        <ul class="space-y-2">
          <li v-for="item in items" :key="item.id" class="flex justify-between items-center p-2 hover:bg-rp-700 cursor-pointer border border-transparent hover:border-rp-600 transition-colors">
            <span class="text-sm">{{ item.name }}</span>
            <span class="text-xs text-rp-accent">{{ item.price }} SCRAP</span>
          </li>
        </ul>
      </div>

      <!-- Main Chart Area -->
      <div class="col-span-1 lg:col-span-3 border border-rp-700 bg-rp-800 p-4 flex flex-col">
        <div class="flex justify-between items-center mb-4">
          <h2 class="text-lg uppercase tracking-widest">Select an Item</h2>
          <div class="space-x-2">
            <button class="px-3 py-1 bg-rp-700 text-xs hover:bg-rp-600">24H</button>
            <button class="px-3 py-1 bg-rp-700 text-xs hover:bg-rp-600">7D</button>
          </div>
        </div>
        
        <div class="flex-1 border border-rp-700 flex items-center justify-center min-h-[400px] relative overflow-hidden bg-rp-black/50">
          <!-- Placeholder for chart -->
          <div class="absolute inset-0 grid grid-cols-6 grid-rows-4 gap-px pointer-events-none opacity-10">
            <div v-for="n in 24" :key="n" class="border border-rp-accent/20"></div>
          </div>
          <p class="text-rp-500 text-sm tracking-widest z-10">WAITING FOR SELECTION</p>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
/* Optional specific overrides if needed */
</style>
