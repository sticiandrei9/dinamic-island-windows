import { speakerIcon } from './icons.js';

// Ieșirea audio în fila Acasă: dispozitivul curent; un click arată lista, alt click schimbă ieșirea.
export function init(island) {
  const { invoke, tauri } = island;
  if (!tauri) return;
  const row = document.getElementById('x-out');
  const cur = Object.assign(document.createElement('button'), { className: 'out-cur' });
  const list = Object.assign(document.createElement('div'), { className: 'out-list', hidden: true });
  row.append(cur, list);
  let enabled = true;
  let outputs = [];

  const close = () => { list.hidden = true; row.classList.remove('open'); island.refresh(); };

  function render() {
    // cu un singur dispozitiv n-ai ce alege
    row.hidden = !enabled || outputs.length < 2;
    const def = outputs.find((o) => o.default);
    cur.innerHTML = speakerIcon(2, false);
    cur.append(Object.assign(document.createElement('span'), { textContent: def?.name ?? '' }));
    list.replaceChildren(...outputs.map((o) => {
      const b = Object.assign(document.createElement('button'), { textContent: o.name, className: o.default ? 'on' : '' });
      b.addEventListener('click', async (e) => {
        e.stopPropagation();
        close();
        await invoke('audio_set_output', { id: o.id }).catch((err) => invoke('log_js', { msg: `ieșire audio: ${err}` }));
        refresh();
      });
      return b;
    }));
    island.refresh();
  }

  async function refresh() {
    if (!enabled) return render();
    outputs = await invoke('audio_outputs').catch(() => []);
    render();
  }

  cur.addEventListener('click', (e) => {
    e.stopPropagation();
    list.hidden = !list.hidden;
    row.classList.toggle('open', !list.hidden);
    island.refresh();
  });
  window.addEventListener('home-show', () => { close(); refresh(); });
  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.output !== false; render(); });
}
