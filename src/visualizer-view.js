import { Spring } from './lib/spring.js';

// Barele (pastila și cele mari din Acasă) urmează sunetul real: `viz` = 5 benzi 0..1 din Rust.
// Fără date 1 s (modul oprit, pauză, captură indisponibilă) revine animația CSS.
export function init(island) {
  const bars = [...document.querySelectorAll('.viz')].map((g) => [...g.querySelectorAll('i')]);
  const springs = Array.from({ length: 5 }, () => new Spring(0, { response: 0.15, damping: 0.8 }));
  let lastAt = 0;
  let prev = 0;
  let raf = 0;

  const setLive = (on) => {
    document.querySelectorAll('.viz').forEach((g) => g.classList.toggle('live', on));
    if (!on) bars.flat().forEach((i) => { i.style.transform = ''; });
  };

  const frame = (now) => {
    if (now - lastAt > 1000) {
      raf = 0;
      setLive(false);
      return;
    }
    const dt = Math.min(0.05, (now - prev) / 1000);
    prev = now;
    springs.forEach((s) => s.step(dt));
    for (const group of bars) {
      group.forEach((el, i) => {
        const v = Math.min(1, Math.max(0, springs[i].value));
        el.style.transform = `scaleY(${(0.15 + 0.85 * v).toFixed(3)})`;
      });
    }
    raf = requestAnimationFrame(frame);
  };

  const onBands = (bands) => {
    lastAt = performance.now();
    bands.forEach((v, i) => springs[i]?.setTarget(v));
    if (!raf) {
      setLive(true);
      prev = lastAt;
      raf = requestAnimationFrame(frame);
    }
  };

  if (island.tauri) {
    island.tauri.event.listen('viz', (e) => onBands(e.payload));
  } else {
    const p = new URLSearchParams(location.search);
    if (p.has('demo') && p.has('viz')) {
      // benzi false: un „bas” care pulsează și înalte mai agitate
      setInterval(() => {
        const t = performance.now() / 1000;
        onBands([0, 1, 2, 3, 4].map((i) => Math.max(0, 0.55 + 0.4 * Math.sin(t * (3 + i * 1.7)) * Math.random() - i * 0.06)));
      }, 33);
    }
  }
}
