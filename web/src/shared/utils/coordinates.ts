/**
 * Rust Map coordinate utilities.
 */

export interface Position {
  x: number;
  y: number;
}

export interface MapPosition {
  left: string;
  top: string;
}

/**
 * Normalizes map size to perfectly fit grid cells of 146.25
 */
export function normalizeMapSize(mapSize: number): number {
  const remainder = mapSize % 146.25;
  if (remainder < 120) {
    return mapSize - remainder;
  } else {
    return mapSize + (146.25 - remainder);
  }
}

/**
 * Converts Rust+ API coordinates to CSS percentage positions.
 * Rust+ API returns coords in 0-based range: x=[0, mapSize], y=[0, mapSize].
 * @param x Game X coordinate (0 to mapSize, west to east)
 * @param y Game Y coordinate (0 to mapSize, south to north)
 * @param mapSize World size in units (e.g. 4000)
 * @param imageWidth Actual generated map image width in pixels
 * @param oceanMargin Ocean margin padding in pixels from map API
 */
export function worldToMap(x: number, y: number, mapSize: number, imageWidth: number, oceanMargin: number): MapPosition {
  const playableWidth = imageWidth - 2 * oceanMargin;
  
  // Use mapSize for pixel conversion mapping so that markers mathematically respect the bounds
  const pixelX = x * (playableWidth / mapSize) + oceanMargin;
  const pixelY = imageWidth - (y * (playableWidth / mapSize) + oceanMargin);

  const left = (pixelX / imageWidth) * 100;
  const top = (pixelY / imageWidth) * 100;

  return {
    left: `${left.toFixed(3)}%`,
    top: `${top.toFixed(3)}%`,
  };
}

/**
 * Calculates the Grid Label (e.g. A1, B12) for a given world coordinate.
 * Expects 0-based coordinates from the Rust+ API.
 */
export function getGridLabel(x: number, y: number, mapSize: number): string {
  const correctedMapSize = normalizeMapSize(mapSize);
  
  const cellSize = 146.25;
  const numberOfGrids = Math.floor(correctedMapSize / cellSize);

  let lettersPart = '?';
  let numbersPart = '?';

  // For Letters (X-axis)
  let counterX = 1;
  for (let gridStart = 0; gridStart < correctedMapSize; gridStart += cellSize) {
    if (x >= gridStart && x <= gridStart + cellSize) {
      lettersPart = numberToLetters(counterX);
      break;
    }
    counterX++;
  }

  // For Numbers (Y-axis) - INVERTED
  let counterY = 1;
  for (let gridStart = 0; gridStart < correctedMapSize; gridStart += cellSize) {
    if (y >= gridStart && y <= gridStart + cellSize) {
      numbersPart = (numberOfGrids - counterY).toString();
      break;
    }
    counterY++;
  }

  return `${lettersPart}${numbersPart}`;
}

function numberToLetters(num: number): string {
  const mod = num % 26;
  let pow = Math.floor(num / 26);
  let char = '';
  
  if (mod !== 0) {
    char = String.fromCharCode(64 + mod); // 65='A'
  } else {
    char = 'Z';
    pow--;
  }
  
  return (pow > 0) ? numberToLetters(pow) + char : char;
}
