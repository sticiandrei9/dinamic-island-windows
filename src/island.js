import { Spring } from './lib/spring.js';
import { islandPath, islandRect } from './lib/shape.js';
import { sizeScale } from './lib/util.js';
import { layout, enqueue, advance, current, holdHead, drop, pauseHead } from './lib/activities.js';
import { ICONS, speakerIcon } from './icons.js';
import { MediaView } from './media-view.js';
import { AppsView } from './apps-view.js';
import { t, setLang, locale, applyStatic } from './lib/i18n.js';

const W = 520;
const EAR = 7;
const BUBBLE = 32;
const GAP = 8;
// lățimea pastilei compacte după activitatea din ea
const COMPACT_W = { '': 190, media: 300, timer: 250, stopwatch: 250, call: 270, meeting: 300, claude: 250, 'claude-waiting': 250, download: 300, sleep: 220 };
// mărimea insulei pentru fiecare tip de activitate tranzitorie
const TRANSIENT = {
  alert: { w: 340, h: 40, r: 18 },
  update: { w: 330, h: 36, r: 16 },
  kb: { w: 250, h: 36, r: 16 },
  volume: { w: 310, h: 36, r: 16 },
  notification: { w: 400, h: 78, r: 26 },
  screenshot: { w: 480, h: 80, r: 26 },
  claude: { w: 340, h: 40, r: 18 },
  download: { w: 420, h: 64, r: 24 },
  unlock: { w: 190, h: 32, r: 10 },
  clip: { w: 340, h: 36, r: 16 },
};
const EXPANDED_W = { home: 420, timer: 440, shelf: 440, mirror: 360, system: 420, clipboard: 420, notes: 400 };
const SPRING = { response: 0.5, damping: 0.72 };

const tauri = window.__TAURI__;
const demo = !tauri && new URLSearchParams(location.search).has('demo');
const invoke = (cmd, args) => (tauri ? tauri.core.invoke(cmd, args) : demoInvoke(cmd, args));
const $ = (id) => document.getElementById(id);

const pathEl = $('shape-path');
const bubblePathEl = $('bubble-path');
const compactEl = $('compact');
const bubbleEl = $('bubble');
const transientEl = $('transient');
const expandedEl = $('expanded');
const tabsEl = $('tabs');

// pornește de la lățime 0, deci insula „crește” la pornire
const springs = {
  w: new Spring(0, SPRING), h: new Spring(32, SPRING), r: new Spring(10, SPRING),
  bw: new Spring(0, SPRING), bg: new Spring(0, SPRING),
};
let expanded = false;
let raf = 0;
let lastFrame = 0;
let rect = { x: 0, y: 0, w: 0, h: 0 };
let lastRectKey = '';
let zoom = 1;
let bubbleLeft = Infinity; // marginea stângă a bulei (px CSS), pentru hover pe bulă
let pointerX = 0;
let uidSeq = 0; // identitatea tranzitoriilor (holdHead/pauseHead creează obiecte noi)
let updateVersion = null; // update gata de aplicat (pentru textul refăcut la schimbarea limbii)
let expandOnHover = true;
let settings = null;
let wasMediaOff = false;

const persistent = new Map(); // id → activitate (media, timer…)
let queue = []; // activități tranzitorii
let shown = null; // tranzitoria afișată acum
let tab = 'home';
const panes = new Map(); // id → { module, icon, onShow, onHide, badge }

// ---------------- API pentru module ----------------
export const island = {
  invoke,
  tauri,
  get settings() { return settings; },
  get expanded() { return expanded; },
  get shown() { return shown; },
  lastScreenshotAt: 0,
  setActivity(id, activity) {
    if (activity) persistent.set(id, { id, ...activity });
    else persistent.delete(id);
    renderActivities();
  },
  pushTransient(activity) {
    queue = enqueue(advance(queue, performance.now()), { ...activity, uid: ++uidSeq }, performance.now());
    tickTransient();
  },
  dropTransients(kind) {
    queue = drop(queue, kind, performance.now());
    tickTransient();
  },
  registerPane(id, def) {
    panes.set(id, def);
    renderTabs();
  },
  setBadge(id, text) {
    const p = panes.get(id);
    if (p) { p.badge = text; renderTabs(); }
  },
  expandTo(id) {
    clearTimeout(hoverTimer);
    setExpanded(true, id);
  },
  collapse: () => setExpanded(false),
  refresh: () => retarget(),
};

// ---------------- media + aplicații (fila Acasă) ----------------
const media = new MediaView({
  invoke,
  onChange: () => island.setActivity('media', media.hasMedia() ? { kind: 'media' } : null),
  onOpenSource: () => setExpanded(false),
});
island.media = media;
const apps = new AppsView({ invoke, onPicked: () => setExpanded(false) });
panes.set('home', {
  module: null,
  icon: ICONS.home,
  title: 'tab.home',
  onShow: () => { media.setExpanded(true); apps.setActive(settings?.modules?.apps !== false); updateClock(); window.dispatchEvent(new Event('home-show')); },
  onHide: () => { media.setExpanded(false); apps.setActive(false); },
});

// ---------------- dimensiuni ----------------
function activeLayout() {
  return layout([...persistent.values()]);
}

function targetSize() {
  if (expanded) {
    const w = tab === 'home' && !media.hasMedia() ? 360 : EXPANDED_W[tab] ?? 420;
    const h = Math.min(330, Math.max(60, expandedEl.offsetHeight + 2));
    return { w, h, r: h > 150 ? 36 : 28 };
  }
  if (shown) return TRANSIENT[shown.kind] ?? TRANSIENT.alert;
  const { pill } = activeLayout();
  return { w: COMPACT_W[pill?.kind ?? ''] ?? 250, h: 32, r: 10 };
}

function retarget() {
  const t = targetSize();
  springs.w.setTarget(t.w);
  springs.h.setTarget(t.h);
  springs.r.setTarget(t.r);
  const showBubble = !expanded && !shown && !!activeLayout().bubble;
  springs.bw.setTarget(showBubble ? BUBBLE : 0);
  springs.bg.setTarget(showBubble ? GAP : 0);
  document.body.classList.toggle('expanded', expanded);
  document.body.classList.toggle('transient', !!shown && !expanded);
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
  const w = Math.max(0, springs.w.value), h = Math.max(0, springs.h.value), r = springs.r.value;
  pathEl.setAttribute('d', islandPath(W, w, h, r, EAR));
  compactEl.style.width = `${w}px`;
  transientEl.style.width = `${w}px`;
  transientEl.style.height = `${h}px`;

  // bula se „desprinde” din marginea dreaptă a pastilei
  const bw = Math.max(0, springs.bw.value);
  const bcx = W / 2 + w / 2 + EAR + Math.max(0, springs.bg.value) + bw / 2;
  bubbleLeft = bw > 0.5 ? bcx - bw / 2 - EAR : Infinity;
  bubblePathEl.setAttribute('d', bw > 0.5 ? islandPath(W, bw, bw, bw / 2, Math.min(EAR, bw / 4), bcx) : '');
  bubbleEl.style.left = `${bcx - BUBBLE / 2}px`;

  const pill = islandRect(W, w, h, EAR);
  rect = bw > 0.5 ? { x: pill.x, y: 0, w: bcx + bw / 2 + EAR - pill.x, h: Math.max(h, bw) } : pill;
  const key = [rect.x, rect.w, rect.h].map(Math.round).join(',');
  if (key !== lastRectKey) {
    lastRectKey = key;
    invoke('set_island_rect', { rect }).catch(() => {});
  }
}

// măsurăm conținutul insulei extinse (versuri, rânduri opționale) și adaptăm înălțimea
new ResizeObserver(() => expanded && retarget()).observe(expandedEl);

// ---------------- activități ----------------
function renderActivities() {
  const { pill, bubble } = activeLayout();
  compactEl.dataset.kind = pill?.kind ?? '';
  bubbleEl.dataset.kind = bubble?.kind ?? '';
  if (bubble?.kind === 'media') $('b-art').src = $('c-art').src;
  if (bubble?.kind === 'timer' || bubble?.kind === 'stopwatch') {
    const p = Math.max(0, Math.min(1, bubble.progress ?? 0));
    bubbleEl.querySelector('.fg').style.strokeDashoffset = `${69.1 * (1 - p)}`;
  }
  retarget();
}

let ticker = 0;
function tickTransient() {
  const now = performance.now();
  // extinsă: tranzitoriile nu se văd, deci așteaptă strângerea; cu cursorul pe captură, ea nu expiră
  if (expanded) queue = pauseHead(queue, now);
  else if (hovering) queue = holdHead(queue, now);
  queue = advance(queue, now);
  const next = current(queue, now);
  const changed = next?.uid !== shown?.uid;
  shown = next;
  if (changed) {
    if (shown) renderTransient(shown);
    retarget();
  }
  clearInterval(ticker);
  if (queue.length) ticker = setInterval(tickTransient, 100);
}

function renderTransient(t) {
  transientEl.dataset.kind = t.kind;
  if (t.kind === 'alert' || t.kind === 'update' || t.kind === 'kb' || t.kind === 'claude') {
    transientEl.style.setProperty('--t-color', t.color ?? 'var(--fg)');
    $('t-icon').innerHTML = ICONS[t.icon] ?? ICONS.bell;
    $('t-title').textContent = t.title ?? '';
    $('t-detail').textContent = t.detail ?? '';
    const ring = $('t-ring');
    ring.classList.toggle('on', typeof t.level === 'number');
    if (typeof t.level === 'number') ring.querySelector('.fg').style.strokeDashoffset = `${56.5 * (1 - t.level)}`;
  } else if (t.kind === 'volume') {
    transientEl.style.setProperty('--t-color', 'var(--fg)');
    $('v-icon').innerHTML = t.source === 'brightness' ? ICONS.sun : speakerIcon(t.waves ?? 2, t.muted);
    $('v-fill').style.width = `${Math.round((t.muted ? 0 : t.level) * 100)}%`;
    $('v-pct').textContent = t.muted ? '—' : `${Math.round(t.level * 100)}%`;
  } else if (t.kind === 'screenshot') {
    $('s-thumb').src = t.thumb;
    $('s-size').textContent = `${t.width} × ${t.height}`;
  } else if (t.kind === 'notification') {
    $('n-icon').src = t.icon ?? '';
    $('n-app').textContent = t.app ?? '';
    $('n-title').textContent = t.title ?? '';
    $('n-body').textContent = t.body ?? '';
  }
}

// ---------------- file ----------------
const TAB_ORDER = ['home', 'timer', 'system', 'clipboard', 'notes', 'shelf', 'mirror'];
function availableTabs() {
  return [...panes.entries()]
    .filter(([, p]) => !p.module || settings?.modules?.[p.module] !== false)
    .sort(([a], [b]) => TAB_ORDER.indexOf(a) - TAB_ORDER.indexOf(b));
}

function renderTabs() {
  const list = availableTabs();
  tabsEl.replaceChildren();
  tabsEl.classList.toggle('single', list.length < 2);
  for (const [id, p] of list) {
    const b = document.createElement('button');
    b.title = t(p.title);
    b.innerHTML = p.icon; // pictogramă constantă din icons.js
    b.classList.toggle('on', id === tab);
    if (p.badge) {
      const s = document.createElement('span');
      s.className = 'badge';
      s.textContent = p.badge;
      b.append(s);
    }
    b.addEventListener('click', (e) => { e.stopPropagation(); setTab(id); });
    tabsEl.append(b);
  }
  if (!list.some(([id]) => id === tab)) setTab('home');
}

function setTab(id) {
  if (id === tab && document.querySelector(`.pane[data-pane="${id}"]`).classList.contains('on')) return;
  if (expanded) panes.get(tab)?.onHide?.();
  tab = id;
  document.body.dataset.tab = id;
  document.querySelectorAll('.pane').forEach((p) => p.classList.toggle('on', p.dataset.pane === id));
  tabsEl.querySelectorAll('button').forEach((b, i) => b.classList.toggle('on', availableTabs()[i]?.[0] === id));
  if (expanded) panes.get(id)?.onShow?.();
  requestAnimationFrame(retarget);
}

function setExpanded(on, toTab) {
  if (expanded === on && !toTab) return;
  if (on) {
    const { pill } = activeLayout();
    const wanted = toTab ?? (pill?.kind === 'timer' || pill?.kind === 'stopwatch' ? 'timer' : 'home');
    const target = availableTabs().some(([id]) => id === wanted) ? wanted : 'home';
    if (expanded) { setTab(target); return; }
    expanded = true;
    tab = '';
    setTab(target);
  } else {
    expanded = false;
    panes.get(tab)?.onHide?.();
  }
  retarget();
}

// hover: extindere după 120 ms, strângere după 220 ms (ca să nu „pâlpâie”)
let hoverTimer = 0;
let hovering = false;
function onHover(inside) {
  hovering = inside;
  clearTimeout(hoverTimer);
  // pe o tranzitorie cu butoane (captura), cursorul o ține afișată în loc să extindă insula
  if (inside && shown?.interactive) return;
  if (inside && !expandOnHover) return; // în modul „doar la click”, hover-ul doar strânge
  hoverTimer = setTimeout(() => setExpanded(inside, inside ? hoverTab() : undefined), inside ? 120 : 220);
}

// cursorul pe bulă: se deschide fila activității din bulă (altfel cea a pastilei)
function hoverTab() {
  if (pointerX < bubbleLeft) return undefined;
  const kind = bubbleEl.dataset.kind;
  return kind === 'timer' || kind === 'stopwatch' ? 'timer' : kind ? 'home' : undefined;
}
document.addEventListener('pointermove', (e) => { pointerX = e.clientX / zoom; });

$('stage').addEventListener('click', () => {
  if (!expanded && !shown?.interactive) {
    clearTimeout(hoverTimer);
    setExpanded(true);
  }
});
bubbleEl.addEventListener('click', (e) => {
  e.stopPropagation();
  const kind = bubbleEl.dataset.kind;
  island.expandTo(kind === 'timer' || kind === 'stopwatch' ? 'timer' : 'home');
});

function updateClock() {
  const now = new Date();
  $('x-time').textContent = now.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });
  const date = now.toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' });
  $('x-date').textContent = date.charAt(0).toUpperCase() + date.slice(1);
}
setInterval(() => expanded && tab === 'home' && updateClock(), 1000);

// ---------------- setări ----------------
function applySettings(st) {
  if (!st) return;
  settings = st;
  setLang(st.resolved_language ?? 'ro');
  applyStatic();
  if (expanded && tab === 'home') updateClock();
  if (updateVersion) $('x-update-text').textContent = t('update.available', { v: updateVersion });
  media.relabel();
  if (expanded && tab === 'home') apps.setActive(st.modules.apps !== false);
  zoom = sizeScale(st.size);
  $('stage').style.zoom = zoom;
  expandOnHover = st.expand_on !== 'click';
  document.body.classList.toggle('no-apps', !st.modules.apps);
  if (!st.modules.media && media.hasMedia()) media.update(null);
  // repornit din setări: Rust emite doar la schimbări, deci cerem starea curentă
  if (st.modules.media && wasMediaOff && tauri) invoke('media_current').then((i) => i && media.update(i)).catch(() => {});
  wasMediaOff = !st.modules.media;
  renderTabs();
  window.dispatchEvent(new CustomEvent('island-settings', { detail: st }));
}

// ---------------- evenimente din Rust ----------------
if (tauri) {
  const self = tauri.webviewWindow.getCurrentWebviewWindow();
  invoke('get_settings').then(applySettings).catch(() => {});
  tauri.event.listen('settings-changed', (e) => applySettings(e.payload));
  // evenimente trimise doar acestei insule (fiecare monitor are fereastra lui)
  self.listen('island-hover', (e) => onHover(!!e.payload));
  // scurtătura de tastatură: deschide/strânge; deschisă fără mouse pe ea, se strânge singură după 8 s
  let toggleTimer = 0;
  self.listen('island-toggle', () => {
    clearTimeout(toggleTimer);
    setExpanded(!expanded);
    if (expanded) toggleTimer = setTimeout(() => !hovering && setExpanded(false), 8000);
  });
  self.listen('island-visible', (e) => {
    document.body.classList.toggle('offscreen', !e.payload);
    if (!e.payload) { hovering = false; setExpanded(false); }
  });
  // după ce ascultăm, cerem și starea curentă (evenimentele de dinainte de încărcare s-au pierdut)
  const mediaOn = () => settings?.modules?.media !== false;
  let mediaSeq = 0;
  tauri.event.listen('media-update', (e) => {
    if (!mediaOn()) return;
    const p = e.payload ?? null;
    const my = ++mediaSeq;
    // fără copertă în mesaj și fără una afișată (fereastră nouă): cerem starea completă;
    // un răspuns întârziat nu suprascrie un mesaj mai nou
    if (p && !('thumbnail' in p) && !media.info?.thumbnail) {
      invoke('media_current').then((i) => my === mediaSeq && media.update(i ?? p)).catch(() => my === mediaSeq && media.update(p));
    } else media.update(p);
  })
    .then(() => invoke('media_current'))
    .then((info) => { if (info && mediaOn() && !media.hasMedia()) media.update(info); })
    .catch(() => {});

  // update-uri: anunț în pastilă + rândul „Aplică” în insula extinsă
  let announced = '';
  const onUpdateState = (st) => {
    const ready = st?.kind === 'ready';
    updateVersion = ready ? st.version : null;
    $('x-update').hidden = !ready;
    if (!ready) return;
    $('x-update-text').textContent = t('update.available', { v: st.version });
    if (announced === st.version) return;
    announced = st.version;
    island.pushTransient({ kind: 'update', icon: 'download', color: 'var(--green)', title: t('update.title'), detail: `v${st.version}`, duration: 4000 });
  };
  tauri.event.listen('update-state', (e) => onUpdateState(e.payload));
  invoke('get_update_state').then(onUpdateState).catch(() => {});
  tauri.event.listen('update-installing', (e) => {
    setExpanded(false);
    island.pushTransient({ kind: 'update', icon: 'download', color: 'var(--green)', title: t('update.installing'), detail: `v${e.payload}`, duration: 120000 });
  });
  tauri.event.listen('update-failed', () => island.dropTransients('update'));
  $('x-update-apply').addEventListener('click', (e) => {
    e.stopPropagation();
    invoke('apply_update').catch(() => {});
  });
} else {
  // în browser: hover calculat local, cu același dreptunghi ca în Rust
  let inside = false;
  document.addEventListener('pointermove', (e) => {
    const x = e.clientX / zoom, y = e.clientY / zoom;
    const now = x >= rect.x && x <= rect.x + rect.w && y >= rect.y && y <= rect.y + rect.h;
    if (now !== inside) { inside = now; onHover(now); }
  });
  document.addEventListener('pointerleave', () => { inside = false; onHover(false); });
  applySettings({ size: 'normal', expand_on: 'hover', modules: {} });
  if (demo) startDemo();
}

document.body.dataset.tab = 'home';
renderTabs();
retarget();

// modulele noi se încarcă după ce API-ul insulei există
for (const mod of ['./lyrics-view.js', './timer-view.js', './privacy-view.js', './alerts-view.js', './volume-view.js', './notifications-view.js', './screenshot-view.js', './shelf-view.js', './mirror-view.js', './output-view.js', './calendar-view.js', './claude-view.js', './downloads-view.js', './unlock-view.js', './system-view.js', './clipboard-view.js', './sleep-view.js', './notes-view.js']) {
  import(mod)
    .then((m) => {
      m.init?.(island);
      // setările au sosit probabil înainte ca modulul să asculte: i le dăm acum
      if (settings) window.dispatchEvent(new CustomEvent('island-settings', { detail: settings }));
    })
    .catch((e) => { if (!String(e).includes('Failed to fetch')) logJs(`${mod}: ${e?.stack ?? e}`); });
}

// erorile JS ajung în log-ul aplicației (altfel s-ar pierde în webview)
function logJs(msg) {
  console.error(msg);
  if (tauri) invoke('log_js', { msg: String(msg).slice(0, 2000) }).catch(() => {});
}
window.addEventListener('error', (e) => logJs(`${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener('unhandledrejection', (e) => logJs(`promisiune respinsă: ${e.reason?.stack ?? e.reason}`));

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
function startDemo() {
  setTimeout(() => {
    const t = demoTracks[0];
    demoInfo = { title: t.title, artist: t.artist, album: '', thumbnail: demoArt(t.hue), playing: true, position: 37, duration: t.duration, source: 'Spotify' };
    media.update(demoInfo);
  }, 900);
  const p = new URLSearchParams(location.search);
  if (p.has('timer')) setTimeout(() => island.setActivity('timer', { kind: 'timer', progress: 0.6 }), 1200);
  if (p.has('alert')) setTimeout(() => island.pushTransient({ kind: 'alert', icon: 'headphones', title: 'AirPods Pro', detail: 'Conectat', level: 0.8, color: 'var(--fg)', duration: 4000 }), 1500);
  if (p.has('volume')) setTimeout(() => island.pushTransient({ kind: 'volume', level: 0.62, waves: 2, duration: 3000 }), 1500);
  if (p.has('claude')) {
    // sesiuni Claude Code simulate: lucrează → te așteaptă → lucrează → a terminat → două sesiuni
    const sess = (id, project, state) => ({ id, project, state, hwnd: 1, since: Date.now() });
    const send = (ms, list) => setTimeout(() => window.dispatchEvent(new CustomEvent('claude-demo', { detail: list })), ms);
    send(1200, [sess('a', 'insula', 'working')]);
    send(6000, [sess('a', 'insula', 'waiting')]);
    send(14000, [sess('a', 'insula', 'working')]);
    send(18000, [sess('a', 'insula', 'done')]);
    send(19000, []);
    send(22000, [sess('a', 'insula', 'working'), sess('b', 'api', 'working')]);
  }
  if (p.has('notif')) setTimeout(() => island.pushTransient({ kind: 'notification', app: 'Discord', title: 'Andrei', body: 'Intri pe server diseară?', duration: 5000 }), 1500);
}
async function demoInvoke(cmd, args) {
  if (!demo) return null;
  if (cmd === 'media_control' && demoInfo) {
    if (args.action === 'play_pause') return null; // starea e deja comutată optimist
    demoIdx = (demoIdx + (args.action === 'next' ? 1 : demoTracks.length - 1)) % demoTracks.length;
    const t = demoTracks[demoIdx];
    demoInfo = { ...demoInfo, title: t.title, artist: t.artist, thumbnail: demoArt(t.hue), position: 0, duration: t.duration, playing: true };
    media.update(demoInfo);
  }
  if (cmd === 'list_windows') {
    return ['Chrome', 'Spotify', 'Code', 'Explorer', 'Discord'].map((n, i) => ({ hwnd: i + 1, title: n, exe: n.toLowerCase() + '.exe', icon: null }));
  }
  return null;
}
