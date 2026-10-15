// Alertele de sistem (încărcător, baterie, Bluetooth) apar ca activități tranzitorii.
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  tauri.event.listen('alert', (e) => island.pushTransient({ kind: 'alert', ...e.payload, duration: 3000 }));
  // limba tastaturii și Caps Lock: scurt, iar o schimbare nouă o înlocuiește pe cea afișată
  tauri.event.listen('kb-alert', (e) => {
    island.dropTransients('kb');
    island.pushTransient({ kind: 'kb', ...e.payload, duration: 1200, urgent: true });
  });
}
