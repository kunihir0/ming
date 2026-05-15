<script setup lang="ts">
import { computed } from 'vue';
import { worldToMap } from '../../../shared/utils/coordinates';

const props = defineProps<{
  x: number;
  y: number;
  mapSize: number;
  imageWidth?: number;
  oceanMargin?: number;
}>();

const position = computed(() => 
  worldToMap(props.x, props.y, props.mapSize, props.imageWidth || 2000, props.oceanMargin || 0)
);
</script>

<template>
  <div 
    class="absolute -translate-x-1/2 -translate-y-1/2 pointer-events-none"
    :style="{ left: position.left, top: position.top }"
  >
    <div class="relative flex items-center justify-center">
      <!-- Expanding Ring 1 -->
      <div class="absolute w-4 h-4 rounded-full border border-red-500 animate-[ping_1.5s_cubic-bezier(0,0,0.2,1)_infinite]"></div>
      <!-- Expanding Ring 2 -->
      <div class="absolute w-8 h-8 rounded-full border border-red-500/50 animate-[ping_2s_cubic-bezier(0,0,0.2,1)_infinite]"></div>
      <!-- Core Point -->
      <div class="w-1.5 h-1.5 rounded-full bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.8)]"></div>
    </div>
  </div>
</template>
