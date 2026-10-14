// Forma notch-ului de MacBook: lipit de marginea de sus, colțuri rotunjite jos
// și „urechi” concave sus, care se topesc în marginea ecranului.
const K = 0.5523; // aproximarea unui sfert de cerc cu o curbă Bézier cubică

const f = (n) => Math.round(n * 100) / 100;

export function islandPath(canvasWidth, w, h, r, ear) {
  w = Math.max(0, w);
  h = Math.max(0, h);
  r = Math.max(0, Math.min(r, h / 2, w / 2));
  ear = Math.max(0, Math.min(ear, h - r, w / 4));
  const x0 = (canvasWidth - w) / 2;
  const x1 = x0 + w;
  return [
    `M${f(x0 - ear)} 0`,
    `C${f(x0 - ear + K * ear)} 0 ${f(x0)} ${f(ear - K * ear)} ${f(x0)} ${f(ear)}`,
    `L${f(x0)} ${f(h - r)}`,
    `C${f(x0)} ${f(h - r + K * r)} ${f(x0 + r - K * r)} ${f(h)} ${f(x0 + r)} ${f(h)}`,
    `L${f(x1 - r)} ${f(h)}`,
    `C${f(x1 - r + K * r)} ${f(h)} ${f(x1)} ${f(h - r + K * r)} ${f(x1)} ${f(h - r)}`,
    `L${f(x1)} ${f(ear)}`,
    `C${f(x1)} ${f(ear - K * ear)} ${f(x1 + ear - K * ear)} 0 ${f(x1 + ear)} 0`,
    'Z',
  ].join(' ');
}

export function islandRect(canvasWidth, w, h, ear) {
  return { x: (canvasWidth - w) / 2 - ear, y: 0, w: w + 2 * ear, h };
}
