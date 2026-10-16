// O filă (pane) e disponibilă dacă modulul ei nu e oprit; `module` poate fi un nume sau o listă
// (disponibilă dacă măcar unul din module nu e oprit, ex. fila Timer cu `timer` oprit și `sleep` pornit).
export function paneAvailable(module, modules) {
  if (!module) return true;
  return [module].flat().some((m) => modules?.[m] !== false);
}
