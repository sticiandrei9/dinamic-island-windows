import { waveLevel } from './lib/util.js';

// Volumul și luminozitatea apar în insulă (pop-up-ul Windows e ascuns din Rust).
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  tauri.event.listen('volume', (e) => {
    const { source, level, muted } = e.payload;
    island.pushTransient({ kind: 'volume', source, level, muted, waves: waveLevel(level, muted), duration: 1500 });
  });
}
