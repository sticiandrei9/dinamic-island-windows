import { Spring } from './lib/spring.js';

// Barele (pastila și cele mari din Acasă) urmează sunetul real: `viz` = 5 benzi 0..1 din Rust.
// Un mesaj numai cu zero (tăcere / mute) lasă barele jos, fără expirare. Fără niciun mesaj 1 s
// (modul oprit, captură indisponibilă), la pauză sau cu modulul oprit revine animația CSS.
export function init(island) {
  const bars = [...document.querySelectorAll('.viz')].map((g) => [...g.querySelectorAll('i')]);
  const springs = Array.from({ length: 5 }, () => new Spring(0, { response: 0.15, damping: 0.8 }));
  let lastAt = 0;
  let prev = 0;
  let raf = 0;
  let resting = false; // ultimul mesaj a fost tăcere: barele stau jos până vine sunet

  const setLive = (on) => {
    document.querySelectorAll('.viz').forEach((g) => g.classList.toggle('live', on));
    if (!on) bars.flat().forEach((i) => { i.style.transform = ''; });
  };

  const stop = () => {
    cancelAnimationFrame(raf);
    raf = 0;
    resting = false;
    setLive(false);
  };

  const render = () => {
    for (const group of bars) {
      group.forEach((el, i) => {
        const v = Math.min(1, Math.max(0, springs[i].value));
        el.style.transform = `scaleY(${(0.15 + 0.85 * v).toFixed(3)})`;
      });
    }
  };

  const frame = (now) => {
    if (!document.body.classList.contains('playing') || (!resting && now - lastAt > 1000)) return stop();
    const dt = Math.min(0.05, Math.max(0, (now - prev) / 1000));
    prev = now;
    springs.forEach((s) => s.step(dt));
    if (resting && springs.every((s) => s.isSettled())) {
      // barele au ajuns jos: rămân așa (`.live`), fără cadre până la următorul mesaj
      springs.forEach((s) => s.snap());
      render();
      raf = 0;
      return;
    }
    render();
    raf = requestAnimationFrame(frame);
  };

  const onBands = (bands) => {
    lastAt = performance.now();
    resting = bands.every((v) => v === 0);
    bands.forEach((v, i) => springs[i]?.setTarget(v));
    if (!raf) {
      setLive(true);
      prev = lastAt;
      raf = requestAnimationFrame(frame);
    }
  };

  window.addEventListener('island-settings', (e) => { if (e.detail.modules?.visualizer === false) stop(); });

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
