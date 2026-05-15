<script setup lang="ts">
import { computed } from 'vue';
import { 
  Store, 
  Ship, 
  User, 
  Bomb, 
  Plane, 
  Circle
} from 'lucide-vue-next';
import { worldToMap } from '../../../shared/utils/coordinates';

const props = defineProps<{
  x: number;
  y: number;
  type: number;
  mapSize: number;
  imageWidth?: number;
  oceanMargin?: number;
  label?: string;
  rotation?: number;
  status?: 'active' | 'offline' | 'warning';
  outOfStock?: boolean;
}>();

const position = computed(() => 
  worldToMap(props.x, props.y, props.mapSize, props.imageWidth || 2000, props.oceanMargin || 0)
);

const icon = computed(() => {
  switch (props.type) {
    case 1: return User; // Player
    case 2: return Bomb; // Explosion
    case 3: return Store; // Vending Machine
    case 4: return Plane; // CH47
    case 5: return Ship; // Cargo Ship
    case 8: return Plane; // Patrol Heli (could use a different one)
    default: return Circle;
  }
});

const colorClass = computed(() => {
  if (props.outOfStock) return 'text-rp-500';
  switch (props.type) {
    case 1: return 'text-rp-accent';
    case 2: return 'text-red-500';
    case 3: return 'text-rp-200';
    case 5: return 'text-orange-500';
    case 8: return 'text-red-600';
    default: return 'text-rp-white';
  }
});
</script>

<template>
  <div 
    class="absolute -translate-x-1/2 -translate-y-1/2 transition-all duration-300 group/marker"
    :style="{ left: position.left, top: position.top }"
  >
    <!-- Label on Hover -->
    <div v-if="label" class="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-1.5 py-0.5 bg-rp-black border border-rp-600 text-[8px] font-mono text-rp-white whitespace-nowrap opacity-0 group-hover/marker:opacity-100 transition-opacity z-50">
      {{ label }} {{ outOfStock ? '[OUT OF STOCK]' : '' }}
    </div>

    <!-- Marker Icon -->
    <div 
      class="p-1 rounded-full border border-transparent transition-colors"
      :class="[colorClass, type === 2 ? 'animate-pulse' : '']"
      :style="{ transform: rotation ? `rotate(${rotation}deg)` : 'none' }"
    >
      <component :is="icon" :size="14" />
    </div>
  </div>
</template>
