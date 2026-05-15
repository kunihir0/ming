import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

export interface MapMarker {
  id: number;
  type: number;
  x: number;
  y: number;
  name?: string;
  outOfStock?: boolean;
  steamId?: string;
  rotation?: number;
  alpha?: number;
}

export const useMapStore = defineStore('map', () => {
  const markers = ref<MapMarker[]>([]);
  const explosions = ref<any[]>([]);
  const teamMembers = ref<any[]>([]);
  
  const filters = ref({
    vending: true,
    cargo: true,
    heli: true,
    explosions: true,
    team: true,
    grid: true,
  });

  const toggleFilter = (key: keyof typeof filters.value) => {
    filters.value[key] = !filters.value[key];
  };

  const mapMeta = ref<{ width: number; height: number; oceanMargin: number } | null>(null);
  const apiMapSize = ref<number | null>(null);

  const fetchMarkers = async (serverId: number) => {
    try {
      const res = await fetch(`/api/server/${serverId}/markers`);
      if (res.ok) {
        const data = await res.json();
        markers.value = data.markers;
        
        if (data.mapMeta) {
          mapMeta.value = data.mapMeta;
        }
        if (data.mapSize) {
          apiMapSize.value = data.mapSize;
        }

        // Debug: log first marker coordinates and meta to verify coordinate ranges
        if (data.markers?.length > 0) {
          const m = data.markers[0];
          console.log('[MAP DEBUG]', {
            firstMarker: { x: m.x, y: m.y, type: m.type, name: m.name },
            mapMeta: data.mapMeta,
            mapSize: data.mapSize,
          });
        }
      }
    } catch (e) {
      console.error('Failed to fetch markers:', e);
    }
  };

  const addExplosion = (pos: { x: number, y: number }) => {
    const id = Math.random();
    explosions.value.push({ id, ...pos });
    
    // Automatically remove after 30 seconds
    setTimeout(() => {
      explosions.value = explosions.value.filter(e => e.id !== id);
    }, 30000);
  };

  // Filtered markers
  const filteredMarkers = computed(() => {
    return markers.value.filter(m => {
      if (m.type === 3 && !filters.value.vending) return false; // Vending Machine
      if (m.type === 5 && !filters.value.cargo) return false; // Cargo Ship
      if ((m.type === 8 || m.type === 4) && !filters.value.heli) return false; // Heli / CH47
      if (m.type === 2 && !filters.value.explosions) return false; // Explosion
      return true;
    });
  });

  return {
    markers,
    explosions,
    teamMembers,
    filters,
    filteredMarkers,
    toggleFilter,
    fetchMarkers,
    addExplosion,
    mapMeta,
    apiMapSize,
  };
});
