function ago(seconds) {
  if (seconds < 60) return 'acum';
  const m = Math.floor(seconds / 60);
  if (m < 60) return m === 1 ? 'acum un minut' : `acum ${m} minute`;
  const h = Math.floor(m / 60);
  if (h < 24) return h === 1 ? 'acum o oră' : `acum ${h} ore`;
  const d = Math.floor(h / 24);
  return d === 1 ? 'acum o zi' : `acum ${d} zile`;
}

// Starea updater-ului (UpdateState din Rust) ca text pentru setări. `now` e în secunde unix.
export function updateStatusText(state, now) {
  switch (state?.kind) {
    case 'checking': return 'Se verifică…';
    case 'up_to_date': return `La zi · verificat ${ago(now - state.checked_at)}`;
    case 'downloading': return `Se descarcă v${state.version}…`;
    case 'ready': return `v${state.version} e gata de instalare`;
    case 'error': return `Eroare: ${state.msg}`;
    default: return 'Nicio verificare încă';
  }
}

export function monitorLabel(m, index) {
  return `Monitor ${index + 1} · ${m.width}×${m.height}${m.primary ? ' (principal)' : ''}`;
}
