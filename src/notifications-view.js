// Notificările Windows (citite prin UI Automation) apar ca activitate tranzitorie de 5 s.
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  tauri.event.listen('notification', (e) => {
    const { app, title, body, icon } = e.payload;
    // Snipping Tool anunță captura și printr-o notificare, care poate sosi înaintea capturii;
    // insula arată deja captura, deci notificarea de lângă ea nu mai apare
    setTimeout(() => {
      if (Math.abs(Date.now() - island.lastScreenshotAt) < 5000) return;
      island.pushTransient({ kind: 'notification', app, title, body, icon: icon ?? '', duration: 5000 });
    }, 800);
  });
}
