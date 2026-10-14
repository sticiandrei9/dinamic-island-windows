import { formatTime, currentPosition, dominantColor, seekFromPointer } from './lib/util.js';

const PLAY = '<svg viewBox="0 0 20 20"><path d="M5.5 3.3v13.4c0 .8.9 1.3 1.6.9l10.6-6.7c.6-.4.6-1.3 0-1.7L7.1 2.4c-.7-.4-1.6.1-1.6.9z"/></svg>';
const PAUSE = '<svg viewBox="0 0 20 20"><rect x="4" y="3" width="4.4" height="14" rx="1.4"/><rect x="11.6" y="3" width="4.4" height="14" rx="1.4"/></svg>';
const PLACEHOLDER = 'data:image/svg+xml,' + encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 56 56"><rect width="56" height="56" fill="#2c2c2e"/>' +
  '<path d="M23 38a5 5 0 1 1-2-4V18l14-3v18a5 5 0 1 1-2-4V20l-10 2z" fill="#8e8e93"/></svg>');

const $ = (id) => document.getElementById(id);

export class MediaView {
  constructor({ invoke, onChange, onOpenSource }) {
    this.invoke = invoke;
    this.onChange = onChange;
    this.dragging = false;
    this.info = null;
    this.receivedAt = 0;
    this.timer = 0;
    this.playBtn = $('x-play');
    this.playBtn.innerHTML = PLAY;
    document.querySelectorAll('.controls button').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const action = btn.dataset.action;
        if (action === 'play_pause' && this.info) {
          // feedback imediat; Rust confirmă în ≤ 500 ms
          this.update({ ...this.info, position: currentPosition(this.info, this.receivedAt, performance.now()), playing: !this.info.playing });
        }
        this.invoke('media_control', { action }).catch(() => {});
      });
    });

    // bara de progres: tragere = seek (doar dacă sursa permite)
    const track = document.querySelector('.progress .track');
    const seekAt = (e) => {
      const r = track.getBoundingClientRect();
      return seekFromPointer(e.clientX, r.left, r.width, this.info?.duration ?? 0);
    };
    track.addEventListener('pointerdown', (e) => {
      if (!this.info?.can_seek) return;
      e.stopPropagation();
      this.dragging = true;
      track.setPointerCapture(e.pointerId);
      this.paint(seekAt(e));
    });
    track.addEventListener('pointermove', (e) => this.dragging && this.paint(seekAt(e)));
    track.addEventListener('pointerup', (e) => {
      if (!this.dragging) return;
      this.dragging = false;
      const seconds = seekAt(e);
      // poziție optimistă până confirmă Rust
      this.info = { ...this.info, position: seconds };
      this.receivedAt = performance.now();
      this.paint(seconds);
      this.invoke('media_seek', { seconds }).catch(() => {});
    });
    track.addEventListener('click', (e) => e.stopPropagation());

    // click pe copertă sau titlu → aplicația care cântă vine în față
    const open = (e) => {
      if (!this.info) return;
      e.stopPropagation();
      this.invoke('media_open_source').catch(() => {});
      onOpenSource?.();
    };
    for (const id of ['x-art', 'x-title', 'c-art']) $(id).addEventListener('click', open);
  }

  hasMedia() {
    return !!this.info;
  }

  update(info) {
    const prev = this.info;
    this.info = info;
    this.receivedAt = performance.now();
    document.body.classList.toggle('has-media', !!info);
    document.body.classList.toggle('playing', !!info?.playing);
    if (info) {
      const trackChanged = !prev || prev.title !== info.title || prev.artist !== info.artist;
      if (trackChanged) {
        this.setText($('x-title'), info.title || 'Necunoscut');
        this.setText($('x-artist'), info.artist || info.source || '');
      }
      if (!prev || prev.thumbnail !== info.thumbnail) this.setArt(info.thumbnail || PLACEHOLDER);
      this.playBtn.innerHTML = info.playing ? PAUSE : PLAY;
      document.body.classList.toggle('no-timeline', !(info.duration > 0));
      document.body.classList.toggle('can-seek', !!info.can_seek);
      if (trackChanged) window.dispatchEvent(new CustomEvent('media-track', { detail: info }));
      this.tick();
    } else {
      window.dispatchEvent(new CustomEvent('media-track', { detail: null }));
    }
    if (!!prev !== !!info) this.onChange();
  }

  setText(el, text) {
    el.textContent = text;
    el.classList.remove('slide');
    void el.offsetWidth;
    el.classList.add('slide');
  }

  setArt(src) {
    for (const img of [$('c-art'), $('x-art')]) {
      img.src = src;
      img.classList.remove('swap');
      void img.offsetWidth;
      img.classList.add('swap');
    }
    const probe = new Image();
    probe.onload = () => {
      const c = document.createElement('canvas');
      c.width = c.height = 12;
      const ctx = c.getContext('2d', { willReadFrequently: true });
      ctx.drawImage(probe, 0, 0, 12, 12);
      document.documentElement.style.setProperty('--accent', dominantColor(ctx.getImageData(0, 0, 12, 12).data));
    };
    probe.src = src;
  }

  setExpanded(on) {
    clearInterval(this.timer);
    if (on) {
      this.tick();
      this.timer = setInterval(() => this.tick(), 250);
    }
  }

  position() {
    return currentPosition(this.info, this.receivedAt, performance.now());
  }

  tick() {
    const info = this.info;
    if (!info || !(info.duration > 0) || this.dragging) return;
    this.paint(this.position());
  }

  paint(pos) {
    const info = this.info;
    if (!info || !(info.duration > 0)) return;
    $('x-pos').textContent = formatTime(pos);
    $('x-rem').textContent = '-' + formatTime(info.duration - pos);
    $('x-fill').style.width = `${(pos / info.duration) * 100}%`;
  }
}
