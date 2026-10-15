// Notificările Windows (citite prin UI Automation) apar ca activitate tranzitorie de 5 s.
// Snipping Tool anunță o captură și printr-o notificare (uneori înaintea capturii); insula arată deja captura.
const SNIPPING = /snip|ножниц|decupa/i;

export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  const show = ({ app, title, body, icon }) =>
    island.pushTransient({ kind: 'notification', app, title, body, icon: icon ?? '', duration: 5000 });
  tauri.event.listen('notification', (e) => {
    const n = e.payload;
    // întârziem doar notificarea Snipping Tool, ca să vedem dacă sosește captura
    if (island.settings?.modules?.screenshots === false || !SNIPPING.test(n.app ?? '')) return show(n);
    setTimeout(() => { if (Math.abs(Date.now() - island.lastScreenshotAt) >= 5000) show(n); }, 800);
  });
}
