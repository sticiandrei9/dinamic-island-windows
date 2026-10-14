export function formatTime(sec) {
  if (!Number.isFinite(sec) || sec < 0) sec = 0;
  const s = Math.floor(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (n) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${p(m)}:${p(ss)}` : `${m}:${p(ss)}`;
}

// Poziția redării acum, extrapolată din ultimul eveniment primit de la Rust.
export function currentPosition(info, receivedAtMs, nowMs) {
  if (!info) return 0;
  let pos = info.position + (info.playing ? (nowMs - receivedAtMs) / 1000 : 0);
  if (info.duration > 0) pos = Math.min(pos, info.duration);
  return Math.max(0, pos);
}

// Culoarea de accent a copertei: medie ponderată cu saturația, luminată ca să se vadă pe negru.
export function dominantColor(rgba) {
  let r = 0, g = 0, b = 0, wsum = 0;
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    const R = rgba[i], G = rgba[i + 1], B = rgba[i + 2], A = rgba[i + 3];
    if (A < 128) continue;
    const max = Math.max(R, G, B);
    const min = Math.min(R, G, B);
    const sat = max === 0 ? 0 : (max - min) / max;
    const w = sat * sat * (max / 255) + 0.001;
    r += R * w; g += G * w; b += B * w; wsum += w;
  }
  if (wsum === 0) return 'rgb(255, 255, 255)';
  let R = r / wsum, G = g / wsum, B = b / wsum;
  const max = Math.max(R, G, B);
  if (max < 140) {
    const k = 140 / Math.max(max, 1);
    R *= k; G *= k; B *= k;
  }
  const c = (v) => Math.round(Math.min(255, v));
  return `rgb(${c(R)}, ${c(G)}, ${c(B)})`;
}

// Scara insulei pentru setarea „Mărime” (aceleași valori ca în Rust, Size::scale).
export function sizeScale(size) {
  return { small: 0.85, normal: 1, large: 1.15 }[size] ?? 1;
}
