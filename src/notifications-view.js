// Notificările Windows (citite prin UI Automation) apar ca activitate tranzitorie de 5 s.
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  tauri.event.listen('notification', (e) => {
    const { app, title, body, icon } = e.payload;
    island.pushTransient({ kind: 'notification', app, title, body, icon: icon ?? '', duration: 5000 });
  });
}
