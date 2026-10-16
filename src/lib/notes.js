// Nebifatele întâi, apoi cele bifate; în fiecare grup, cele mai noi sus (la dată egală, id-ul mai mare).
export function sortNotes(list) {
  return [...list].sort((a, b) => Number(a.done) - Number(b.done) || b.at - a.at || b.id - a.id);
}

// Câte notițe nu sunt bifate (numărul din insigna filei).
export function openCount(list) {
  return list.filter((n) => !n.done).length;
}
