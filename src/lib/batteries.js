// Bateriile Bluetooth din fila Acasă: primele `max`, restul se numără ca „+N”.
export function visibleBatteries(list, max = 3) {
  return { shown: list.slice(0, max), more: Math.max(0, list.length - max) };
}
