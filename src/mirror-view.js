import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { nextCamera, pickCamera } from './lib/mirror.js';

const IDLE_MS = 60_000;

// Oglinda: camera web în insulă, pornită doar cât fila e deschisă.
export function init(island) {
  const { tauri, invoke } = island;
  if (!tauri) return;

  const style = document.createElement('style');
  style.textContent = `
    .mr { position: relative; width: 300px; height: 170px; margin: 0 auto; border-radius: 18px; overflow: hidden; background: #111; }
    .mr video { width: 100%; height: 100%; object-fit: cover; transform: scaleX(-1); display: block; }
    .mr-msg { position: absolute; inset: 0; display: none; place-items: center; text-align: center; color: var(--fg2); font-size: 12.5px; padding: 0 20px; cursor: pointer; }
    .mr-msg svg { width: 26px; height: 26px; margin: 0 auto 6px; display: block; }
    .mr.off .mr-msg { display: grid; }
    .mr.off video { visibility: hidden; }
    .mr-switch { position: absolute; right: 8px; bottom: 8px; width: 30px; height: 30px; border-radius: 50%; border: 0; background: rgba(0,0,0,.55); color: #fff; display: none; place-items: center; cursor: pointer; }
    .mr-switch svg { width: 16px; height: 16px; }
    .mr.multi:not(.off) .mr-switch { display: grid; }
  `;
  document.head.append(style);

  const pane = document.querySelector('.pane[data-pane="mirror"]');
  const box = document.createElement('div');
  box.className = 'mr off';
  const video = Object.assign(document.createElement('video'), { autoplay: true, muted: true, playsInline: true });
  const msg = document.createElement('div');
  msg.className = 'mr-msg';
  const sw = document.createElement('button');
  sw.className = 'mr-switch';
  sw.dataset.i18nTitle = 'mirror.switch';
  sw.title = t('mirror.switch');
  sw.innerHTML = ICONS.camera; // pictogramă constantă
  box.append(video, msg, sw);
  pane.append(box);

  let stream = null;
  let cams = [];
  let current = null;
  let idle = 0;

  let msgKey = null;
  function message(key, withIcon) {
    msgKey = key;
    const text = t(key);
    msg.replaceChildren();
    if (withIcon) {
      const i = document.createElement('div');
      i.innerHTML = ICONS.camera;
      msg.append(i);
    }
    msg.append(document.createTextNode(text));
    box.classList.add('off');
  }

  function stop() {
    gen++; // anulează un start() în curs
    clearTimeout(idle);
    if (stream) invoke('set_mirror_active', { active: false }).catch(() => {});
    stream?.getTracks().forEach((t) => t.stop()); // ledul camerei se stinge
    stream = null;
    video.srcObject = null;
  }

  function armIdle() {
    clearTimeout(idle);
    idle = setTimeout(() => { stop(); message('mirror.idle', true); }, IDLE_MS);
  }

  let gen = 0;
  async function start(deviceId) {
    stop();
    const my = ++gen;
    try {
      const video_c = { width: 640, height: 360, ...(deviceId ? { deviceId: { exact: deviceId } } : {}) };
      const s = await navigator.mediaDevices.getUserMedia({ video: video_c, audio: false });
      // între timp: alt start(), altă filă sau insula strânsă → camera se oprește imediat
      if (my !== gen || !island.expanded || document.body.dataset.tab !== 'mirror') { s.getTracks().forEach((t) => t.stop()); return; }
      stream = s;
      video.srcObject = stream;
      invoke('set_mirror_active', { active: true }).catch(() => {});
      box.classList.remove('off');
      msgKey = null; // camera merge: niciun mesaj de refăcut
      current = stream.getVideoTracks()[0]?.getSettings().deviceId ?? deviceId;
      cams = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === 'videoinput');
      box.classList.toggle('multi', cams.length > 1);
      armIdle();
    } catch (e) {
      stop();
      message('mirror.unavailable', true);
    }
  }

  async function show() {
    cams = (await navigator.mediaDevices.enumerateDevices().catch(() => [])).filter((d) => d.kind === 'videoinput');
    start(pickCamera(cams, island.settings?.mirror_camera));
  }

  msg.addEventListener('click', (e) => { e.stopPropagation(); start(current); });
  pane.addEventListener('pointermove', () => stream && armIdle());
  sw.addEventListener('click', (e) => {
    e.stopPropagation();
    const id = nextCamera(cams, current);
    if (!id) return;
    if (island.settings) invoke('set_settings', { settings: { ...island.settings, mirror_camera: id } }).catch(() => {});
    start(id);
  });

  // limba schimbată: mesajul afișat se reface
  window.addEventListener('island-settings', () => { if (msgKey) message(msgKey, true); });

  island.registerPane('mirror', { module: 'mirror', icon: ICONS.mirror, title: 'tab.mirror', onShow: show, onHide: stop });
}
