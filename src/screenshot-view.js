import { t } from './lib/i18n.js';

// Captura de ecran (Win+Shift+S): miniatura apare 6 s, cu „Salvează” și „Pe raft”.
export function init(island) {
  const { tauri, invoke } = island;
  if (!tauri) return;

  function act(command, doneKey) {
    return (e) => {
      e.stopPropagation();
      const shot = island.shown;
      if (shot?.kind !== 'screenshot') return;
      island.dropTransients('screenshot');
      invoke(command, { id: shot.id })
        .then(() => island.pushTransient({ kind: 'alert', icon: command === 'screenshot_to_shelf' ? 'shelf' : 'download', title: t(doneKey), color: 'var(--green)', duration: 1800 }))
        .catch((err) => island.pushTransient({ kind: 'alert', icon: 'bell', title: t('shot.failed'), detail: String(err), color: 'var(--red)', duration: 3000 }));
    };
  }

  document.getElementById('s-save').addEventListener('click', act('screenshot_save', 'shot.saved'));
  document.getElementById('s-shelf').addEventListener('click', act('screenshot_to_shelf', 'shot.shelved'));
  document.getElementById('s-close').addEventListener('click', (e) => { e.stopPropagation(); island.dropTransients('screenshot'); });

  tauri.event.listen('screenshot', (e) => {
    island.lastScreenshotAt = Date.now();
    island.dropTransients('screenshot'); // o captură nouă o înlocuiește pe cea veche
    island.pushTransient({ kind: 'screenshot', interactive: true, ...e.payload, duration: 6000 });
  });
}
