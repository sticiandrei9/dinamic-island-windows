import { Spring } from './lib/spring.js';
import { islandPath, islandRect } from './lib/shape.js';
import { MediaView } from './media-view.js';
import { AppsView } from './apps-view.js';

const W = 520;
const EAR = 7;
const SIZES = {
  idle: { w: 190, h: 32, r: 10 },
  active: { w: 300, h: 32, r: 10 },
  expandedMedia: { w: 420, h: 206, r: 36 },
  expandedIdle: { w: 360, h: 140, r: 32 },
};
const SPRING = { response: 0.5, damping: 0.72 };

const tauri = window.__TAURI__;
const demo = !tauri && new URLSearchParams(location.search).has('demo');
const invoke = (cmd, args) => (tauri ? tauri.core.invoke(cmd, args) : demoInvoke(cmd, args));

const pathEl = document.getElementById('shape-path');
const compactEl = document.getElementById('compact');

// pornește de la lățime 0, deci insula „crește” la pornire
const springs = { w: new Spring(0, SPRING), h: new Spring(32, SPRING), r: new Spring(10, SPRING) };
let expanded = false;
let raf = 0;
let lastFrame = 0;
let rect = { x: 0, y: 0, w: 0, h: 0 };
let lastRectKey = '';

const media = new MediaView({ invoke, onChange: retarget });
const apps = new AppsView({ invoke, onPicked: () => setExpanded(false) });

function targetSize() {
  if (expanded) return media.hasMedia() ? SIZES.expandedMedia : SIZES.expandedIdle;
  return media.hasMedia() ? SIZES.active : SIZES.idle;
}

function retarget() {
  const t = targetSize();
  springs.w.setTarget(t.w);
  springs.h.setTarget(t.h);
  springs.r.setTarget(t.r);
  document.body.classList.toggle('expanded', expanded);
  if (!raf) {
    lastFrame = performance.now();
    raf = requestAnimationFrame(frame);
  }
}

function frame(now) {
  const dt = Math.min(0.05, (now - lastFrame) / 1000);
  lastFrame = now;
  let settled = true;
  for (const s of Object.values(springs)) settled = s.step(dt) && settled;
  if (settled) for (const s of Object.values(springs)) s.snap();
  render();
  raf = settled ? 0 : requestAnimationFrame(frame);
}

function render() {
  const w = springs.w.value, h = springs.h.value, r = springs.r.value;
  pathEl.setAttribute('d', islandPath(W, w, h, r, EAR));
  compactEl.style.width = `${Math.max(0, w)}px`;
  rect = islandRect(W, Math.max(0, w), Math.max(0, h), EAR);
  const key = [rect.x, rect.w, rect.h].map(Math.round).join(',');
  if (key !== lastRectKey) {
    lastRectKey = key;
    invoke('set_island_rect', { rect }).catch(() => {});
  }
}

function setExpanded(on) {
  if (expanded === on) return;
  expanded = on;
  media.setExpanded(on);
  apps.setActive(on);
  updateClock();
  retarget();
}

// hover: extindere după 120 ms, strângere după 220 ms (ca să nu „pâlpâie”)
let hoverTimer = 0;
function onHover(inside) {
  clearTimeout(hoverTimer);
  hoverTimer = setTimeout(() => setExpanded(inside), inside ? 120 : 220);
}

document.getElementById('stage').addEventListener('click', () => {
  if (!expanded) {
    clearTimeout(hoverTimer);
    setExpanded(true);
  }
});

function updateClock() {
  const now = new Date();
  document.getElementById('x-time').textContent = now.toLocaleTimeString('ro-RO', { hour: '2-digit', minute: '2-digit' });
  const date = now.toLocaleDateString('ro-RO', { weekday: 'long', day: 'numeric', month: 'long' });
  document.getElementById('x-date').textContent = date.charAt(0).toUpperCase() + date.slice(1);
}
setInterval(() => expanded && updateClock(), 1000);

if (tauri) {
  tauri.event.listen('island-hover', (e) => onHover(!!e.payload));
  // după ce ascultăm, cerem și starea curentă (evenimentele de dinainte de încărcare s-au pierdut)
  tauri.event.listen('media-update', (e) => media.update(e.payload ?? null))
    .then(() => invoke('media_current'))
    .then((info) => { if (info && !media.hasMedia()) media.update(info); })
    .catch(() => {});
  // ascunsă cât timp o aplicație rulează pe tot ecranul
  tauri.event.listen('island-visible', (e) => {
    document.body.classList.toggle('offscreen', !e.payload);
    if (!e.payload) setExpanded(false);
  });
} else {
  // în browser: hover calculat local, cu același dreptunghi ca în Rust
  let inside = false;
  document.addEventListener('pointermove', (e) => {
    const now = e.clientX >= rect.x && e.clientX <= rect.x + rect.w && e.clientY >= rect.y && e.clientY <= rect.y + rect.h;
    if (now !== inside) { inside = now; onHover(now); }
  });
  document.addEventListener('pointerleave', () => { inside = false; onHover(false); });
  if (demo) startDemo();
}

retarget();

// ---------------- mod demo (doar în browser, pentru verificat designul) ----------------
function demoArt(hue) {
  const c = document.createElement('canvas');
  c.width = c.height = 120;
  const g = c.getContext('2d');
  const grad = g.createLinearGradient(0, 0, 120, 120);
  grad.addColorStop(0, `hsl(${hue} 80% 55%)`);
  grad.addColorStop(1, `hsl(${(hue + 60) % 360} 70% 25%)`);
  g.fillStyle = grad;
  g.fillRect(0, 0, 120, 120);
  return c.toDataURL();
}
const demoTracks = [
  { title: 'Blinding Lights', artist: 'The Weeknd', hue: 350, duration: 200 },
  { title: 'Midnight City', artist: 'M83', hue: 260, duration: 243 },
];
let demoIdx = 0;
let demoInfo = null;
function demoEmit(patch) {
  demoInfo = { ...demoInfo, ...patch };
  media.update(demoInfo);
}
function startDemo() {
  setTimeout(() => {
    const t = demoTracks[0];
    demoInfo = { title: t.title, artist: t.artist, album: '', thumbnail: demoArt(t.hue), playing: true, position: 37, duration: t.duration, source: 'Spotify' };
    media.update(demoInfo);
  }, 900);
}
async function demoInvoke(cmd, args) {
  if (!demo) return null;
  if (cmd === 'media_control' && demoInfo) {
    if (args.action === 'play_pause') return null; // starea e deja comutată optimist
    demoIdx = (demoIdx + (args.action === 'next' ? 1 : demoTracks.length - 1)) % demoTracks.length;
    const t = demoTracks[demoIdx];
    demoEmit({ title: t.title, artist: t.artist, thumbnail: demoArt(t.hue), position: 0, duration: t.duration, playing: true });
  }
  if (cmd === 'list_windows') {
    return ['Chrome', 'Spotify', 'Code', 'Explorer', 'Discord'].map((n, i) => ({ hwnd: i + 1, title: n, exe: n.toLowerCase() + '.exe', icon: null }));
  }
  return null;
}
