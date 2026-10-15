import { currentLine } from './lib/lyrics.js';

// Versuri sincronizate (LRCLIB) în fila Acasă: rândul curent alb, următorul estompat.
export function init(island) {
  const { tauri, invoke } = island;
  if (!tauri) return;
  const box = document.getElementById('x-lyrics');
  const cur = box.querySelector('.ly-cur');
  const next = box.querySelector('.ly-next');
  let lines = null;
  let shown = -2;
  let request = 0;
  let enabled = true;

  function show(on) {
    document.body.classList.toggle('has-lyrics', on);
    island.refresh();
  }

  function load(info, retry = false) {
    const id = retry ? request : ++request;
    lines = null;
    shown = -2;
    show(false);
    if (!info || !enabled) return;
    const track = { title: info.title ?? '', artist: info.artist ?? '', album: info.album ?? '', duration: info.duration ?? 0 };
    invoke('get_lyrics', { track })
      .then((res) => {
        // între timp s-a schimbat piesa: răspunsul nu mai e pentru ce cântă acum
        if (id !== request || !res?.length) return;
        lines = res;
        render(true);
        show(true);
      })
      // rețea sau LRCLIB ocupat: încă o încercare peste 15 s, dacă piesa e aceeași
      .catch(() => { if (!retry) setTimeout(() => id === request && load(info, true), 15000); });
  }

  function render(force) {
    if (!lines || !island.media.hasMedia()) return;
    const i = currentLine(lines, island.media.position() + 0.15);
    if (i === shown && !force) return;
    shown = i;
    cur.textContent = i >= 0 ? lines[i].text : '♪';
    next.textContent = lines[i + 1]?.text ?? '';
    box.classList.remove('change');
    void box.offsetWidth;
    box.classList.add('change');
  }

  window.addEventListener('media-track', (e) => load(e.detail));
  window.addEventListener('island-settings', (e) => {
    const on = e.detail.modules.lyrics !== false;
    if (on === enabled) return;
    enabled = on;
    load(on && island.media.hasMedia() ? island.media.info : null);
  });
  setInterval(() => island.expanded && render(false), 250);
  // piesa poate fi deja pornită când se încarcă modulul
  if (island.media.hasMedia()) load(island.media.info);
}
