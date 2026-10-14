// Punctele de confidențialitate (ca pe iPhone): portocaliu = microfon, verde = cameră.
export function init(island) {
  const { tauri } = island;
  if (!tauri) return;
  const row = document.getElementById('x-privacy');

  function part(color, label, apps) {
    const span = document.createElement('span');
    const dot = document.createElement('i');
    dot.style.background = color;
    span.append(dot, `${label}: ${apps.join(', ')}`);
    return span;
  }

  function apply(p) {
    const mic = p?.mic ?? [];
    const cam = p?.cam ?? [];
    document.body.classList.toggle('mic-on', mic.length > 0);
    document.body.classList.toggle('cam-on', cam.length > 0);
    const parts = [];
    if (mic.length) parts.push(part('var(--orange)', 'Microfon', mic));
    if (cam.length) parts.push(part('var(--green)', 'Cameră', cam));
    row.replaceChildren(...parts);
    row.hidden = parts.length === 0;
    island.refresh();
  }

  tauri.event.listen('privacy', (e) => apply(e.payload))
    .then(() => island.invoke('privacy_current'))
    .then(apply)
    .catch(() => {});
}
