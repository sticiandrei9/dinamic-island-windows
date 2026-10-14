// Indexul rândului care se cântă acum (căutare binară), sau -1 înainte de primul rând.
export function currentLine(lines, position) {
  let lo = 0, hi = lines.length - 1, ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].t <= position) { ans = mid; lo = mid + 1; } else hi = mid - 1;
  }
  return ans;
}
