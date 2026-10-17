// Centrul de control din fila Acasă: ce butoane apar și logica „Sigur?” a butonului sleep.
const ORDER = [['wifi', 'wifi'], ['bluetooth', 'bluetooth'], ['dark', 'dark'], ['night', 'night'], ['dnd', 'dnd'], ['mic_muted', 'mic']];

/** Numele butoanelor disponibile, în ordine; `null`/lipsă = indisponibil. Blocarea și sleep sunt mereu acolo. */
export function visibleButtons(state) {
  const out = ORDER.filter(([key]) => state?.[key] != null).map(([, name]) => name);
  return [...out, 'lock', 'sleep'];
}

export const SLEEP_CONFIRM_MS = 3000;

/** Primul click armează (`armedAt` = momentul, 0 = nearmat), al doilea în 3 s declanșează. */
export function sleepClick(armedAt, now) {
  if (armedAt > 0 && now - armedAt <= SLEEP_CONFIRM_MS) return { armedAt: 0, fire: true };
  return { armedAt: now, fire: false };
}
