<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { useServerStore } from '../../stores/servers';
import { useServerInfoStore } from '../../stores/serverInfo';
import { useTeamStore } from '../../stores/team';
import { useMapStore } from '../../stores/map';
import MapMarker from './components/Marker.vue';
import ExplosionMarker from './components/ExplosionMarker.vue';

const serverStore = useServerStore();
const infoStore = useServerInfoStore();
const teamStore = useTeamStore();
const mapStore = useMapStore();

const activeServerName = computed(() => {
  const server = serverStore.servers.find(s => s.id === serverStore.activeServerId);
  return server ? server.name.toUpperCase() : 'UNKNOWN SERVER';
});

const mapSize = computed(() => infoStore.info?.mapSize || 4000);

const mapUrl = computed(() => {
  if (!serverStore.activeServerId) return '';
  return `/api/server/${serverStore.activeServerId}/map/image`;
});

// Grid Calculation
const normalizedMapSize = computed(() => {
  const size = mapSize.value;
  const remainder = size % 146.25;
  if (remainder < 120) {
    return size - remainder;
  } else {
    return size + (146.25 - remainder);
  }
});

const gridCells = computed(() => {
  const count = Math.floor(normalizedMapSize.value / 146.25);
  return Array.from({ length: count }, (_, i) => i);
});

const imageWidth = computed(() => mapStore.mapMeta?.width || infoStore.mapMeta?.width || 2000);
const oceanMargin = computed(() => mapStore.mapMeta?.oceanMargin ?? infoStore.mapMeta?.margin ?? 0);

const getGridCellLabel = (index: number) => {
  const col = (index - 1) % gridCells.value.length;
  const row = Math.floor((index - 1) / gridCells.value.length);
  
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
  let letters = '';
  if (col < 26) {
    letters = alphabet[col];
  } else {
    letters = alphabet[Math.floor(col / 26) - 1] + alphabet[col % 26];
  }
  
  return `${letters}${row}`;
};
let pollInterval: any = null;

onMounted(() => {
  if (serverStore.activeServerId) {
    mapStore.fetchMarkers(serverStore.activeServerId);
    pollInterval = setInterval(() => {
      mapStore.fetchMarkers(serverStore.activeServerId!);
      teamStore.fetchTeamData(serverStore.activeServerId!);
    }, 5000);
  }
});

onUnmounted(() => {
  if (pollInterval) clearInterval(pollInterval);
});
</script>

<template>
  <div class="panel flex-1 flex flex-col border-r border-rp-700 relative overflow-hidden bg-rp-black">
    <!-- Panel Header -->
    <div class="flex items-center justify-between px-4 py-2 border-b border-rp-700 bg-rp-900/50 z-10">
      <div class="flex items-center gap-3">
        <span class="font-mono text-[10px] tracking-[0.2em] text-rp-300 uppercase">Tactical Map</span>
        <span class="font-mono text-[10px] text-rp-500 tracking-wider" v-if="serverStore.activeServerId">
          {{ activeServerName }} // {{ mapSize }}×{{ mapSize }}
        </span>
      </div>
      <div class="flex items-center gap-2">
        <button 
          v-for="(active, id) in mapStore.filters" 
          :key="id"
          @click="mapStore.toggleFilter(id)"
          class="font-mono text-[9px] tracking-widest border px-2 py-0.5 transition-all uppercase"
          :class="active ? 'text-rp-white border-rp-400 bg-rp-white/5' : 'text-rp-500 border-rp-700'"
        >
          {{ id }}
        </button>
      </div>
    </div>

    <!-- Map Viewport — centers the square map inside the available space -->
    <div class="flex-1 relative overflow-hidden bg-black group flex items-center justify-center">
      <div v-if="!serverStore.activeServerId" class="text-rp-500 font-mono text-sm uppercase tracking-widest">
        NO SERVER SELECTED
      </div>
      <template v-else>
        <!--
          Map Container: sized to fill the viewport while staying square.
          aspect-square + max-w-full + max-h-full keeps it from overflowing.
          The image fills this div completely (no object-contain letterboxing),
          so all absolute children (grid, markers) align 1:1 with the image pixels.
        -->
        <div class="relative aspect-square max-w-full max-h-full overflow-hidden bg-rp-900">
          <!-- Layer 0: Map image — fills the square container exactly -->
          <img 
            :src="mapUrl" 
            alt="Rust Map" 
            class="block w-full h-full select-none z-0"
          />

          <!-- Layer 1: Grid Overlay — positioned inside the playable area -->
          <div v-if="mapStore.filters.grid" class="absolute pointer-events-none overflow-hidden border border-rp-400/60 z-10"
               :style="{
                 left: `${(oceanMargin / imageWidth) * 100}%`,
                 top: `${(oceanMargin / imageWidth) * 100}%`,
                 width: `${((imageWidth - 2 * oceanMargin) / imageWidth) * 100}%`,
                 height: `${((imageWidth - 2 * oceanMargin) / imageWidth) * 100}%`
               }">
            <div 
              class="grid w-full h-full content-end" 
              :style="{ 
                gridTemplateColumns: `repeat(${gridCells.length}, ${(146.25 / mapSize) * 100}%)`,
                gridTemplateRows: `repeat(${gridCells.length}, ${(146.25 / mapSize) * 100}%)` 
              }"
            >
              <div 
                v-for="i in gridCells.length * gridCells.length" 
                :key="i" 
                class="border-[0.5px] border-white/15 flex items-start justify-start"
              >
                <span class="text-[5px] leading-none font-mono text-white/40 pl-[1px] pt-[1px]">{{ getGridCellLabel(i) }}</span>
              </div>
            </div>
          </div>

          <!-- Layer 2: Markers (Vending, Cargo, Heli, etc) -->
          <MapMarker
            v-for="marker in mapStore.filteredMarkers"
            :key="marker.id"
            :x="marker.x"
            :y="marker.y"
            :type="marker.type"
            :map-size="mapSize"
            :image-width="imageWidth"
            :ocean-margin="oceanMargin"
            :label="marker.name"
            :rotation="marker.rotation"
            :out-of-stock="marker.outOfStock"
          />

          <!-- Layer 3: Teammates -->
          <template v-if="mapStore.filters.team && teamStore.team">
            <MapMarker
              v-for="member in teamStore.team.members"
              :key="member.steamId"
              :x="member.x"
              :y="member.y"
              :type="1" 
              :map-size="mapSize"
              :image-width="imageWidth"
              :ocean-margin="oceanMargin"
              :label="member.name"
              :status="member.isOnline ? 'active' : 'offline'"
            />
          </template>

          <!-- Layer 4: Real-time Explosions (Pings) -->
          <template v-if="mapStore.filters.explosions">
            <ExplosionMarker
              v-for="expl in mapStore.explosions"
              :key="expl.id"
              :id="expl.id"
              :x="expl.x"
              :y="expl.y"
              :map-size="mapSize"
              :image-width="imageWidth"
              :ocean-margin="oceanMargin"
              :time-since="Date.now() - expl.timestamp"
            />
          </template>
        </div>

        <!-- Coordinates Overlay -->
        <div class="absolute bottom-2 right-3 font-mono text-[9px] text-rp-600 tracking-wider flex items-center gap-4 bg-rp-black/60 px-2 py-1 border border-rp-700 z-20">
          <span class="text-rp-500">SECTOR CALIBRATION ACTIVE</span>
          <span class="text-rp-300">DATA SYNC: 5s</span>
        </div>
      </template>
    </div>

    <!-- Map Footer -->
    <div class="px-4 py-2 border-t border-rp-700 flex items-center gap-6 bg-rp-900/30 z-10">
      <div class="flex items-center gap-2">
        <div class="w-1.5 h-1.5 rounded-full bg-rp-300 animate-pulse-dot shadow-[0_0_4px_var(--color-rp-300)]"></div>
        <span class="font-mono text-[10px] text-rp-white tracking-wider uppercase">Satellite Link Verified</span>
      </div>
      <div class="ml-auto flex items-center gap-4 font-mono text-[10px] text-rp-500">
        <button @click="mapStore.fetchMarkers(serverStore.activeServerId!)" class="hover:text-rp-200 transition-colors flex items-center gap-1.5 uppercase tracking-widest">
          <RefreshCw :size="10" /> Sync Intelligence
        </button>
      </div>
    </div>
  </div>
</template>

