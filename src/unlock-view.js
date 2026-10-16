import { Spring } from './lib/spring.js';

const OPEN_DELAY_MS = 250; // lacătul apare întâi, apoi se deschide
const LIFT_PX = 3;
const TURN_DEG = -20;

// Lacătul la deblocarea Windows (Rust emite `session-unlock`): intră cu fade + blur + scale (CSS),
// iar arcul urcă 3 px și se rotește −20° în jurul piciorului stâng, cu un Spring.
export function init(island) {
  const { tauri } = island;
  const shackle = document.getElementById('u-shackle');
  const spring = new Spring(0, { response: 0.5, damping: 0.72 });
  let raf = 0;

  const draw = () => shackle.setAttribute('transform', `translate(0 ${-LIFT_PX * spring.value}) rotate(${TURN_DEG * spring.value} 8 11)`);

  function play() {
    cancelAnimationFrame(raf);
    spring.value = spring.velocity = 0;
    spring.setTarget(0);
    draw();
    const t0 = performance.now();
    let last = t0;
    const frame = (now) => {
      if (island.shown?.kind !== 'unlock') return; // tranzitoria a dispărut: oprim animația
      if (now - t0 >= OPEN_DELAY_MS) spring.setTarget(1);
      const settled = spring.step(Math.min((now - last) / 1000, 0.05));
      last = now;
      draw();
      if (!(settled && spring.target === 1)) raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
  }

  window.addEventListener('unlock-show', play);
  tauri?.event.listen('session-unlock', () => island.pushTransient({ kind: 'unlock', duration: 1600, urgent: true }));
}
