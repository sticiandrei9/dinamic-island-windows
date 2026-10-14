// Alertele de sistem (încărcător, baterie, Bluetooth) apar ca activități tranzitorii.
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  tauri.event.listen('alert', (e) => island.pushTransient({ kind: 'alert', ...e.payload, duration: 3000 }));
}
