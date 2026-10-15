import { t } from './i18n.js';

function ago(seconds) {
  if (seconds < 60) return t('ago.now');
  const m = Math.floor(seconds / 60);
  if (m < 60) return t('ago.minutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('ago.hours', { n: h });
  return t('ago.days', { n: Math.floor(h / 24) });
}

// Starea updater-ului (UpdateState din Rust) ca text pentru setări. `now` e în secunde unix.
export function updateStatusText(state, now) {
  switch (state?.kind) {
    case 'checking': return t('update.checking');
    case 'up_to_date': return t('update.upToDate', { ago: ago(now - state.checked_at) });
    case 'downloading': return t('update.downloading', { v: state.version });
    case 'ready': return t('update.ready', { v: state.version });
    case 'error': return t('error', { v: state.msg });
    default: return t('update.none');
  }
}

export function monitorLabel(m, index) {
  return t('settings.monitorLabel', { n: index + 1, w: m.width, h: m.height }) + (m.primary ? t('settings.primarySuffix') : '');
}
