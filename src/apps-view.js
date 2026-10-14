const MAX_ICONS = 9;

export class AppsView {
  constructor({ invoke, onPicked }) {
    this.invoke = invoke;
    this.onPicked = onPicked;
    this.el = document.getElementById('x-apps');
    this.timer = 0;
    this.key = '';
  }

  setActive(on) {
    clearInterval(this.timer);
    if (on) {
      this.refresh();
      this.timer = setInterval(() => this.refresh(), 2000);
    }
  }

  async refresh() {
    let list;
    try {
      list = await this.invoke('list_windows');
    } catch {
      return;
    }
    if (!Array.isArray(list)) return;
    const key = list.map((w) => `${w.hwnd}:${w.title}`).join('|');
    if (key === this.key) return;
    this.key = key;
    this.render(list);
  }

  render(list) {
    this.el.replaceChildren();
    for (const w of list.slice(0, MAX_ICONS)) {
      const btn = document.createElement('button');
      btn.title = w.title;
      if (w.icon) {
        const img = document.createElement('img');
        img.src = w.icon;
        img.alt = '';
        btn.append(img);
      } else {
        btn.textContent = (w.exe || w.title || '?').charAt(0).toUpperCase();
      }
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.invoke('focus_window', { hwnd: w.hwnd }).catch(() => { this.key = ''; this.refresh(); });
        this.onPicked();
      });
      this.el.append(btn);
    }
    if (list.length > MAX_ICONS) {
      const more = document.createElement('span');
      more.className = 'more';
      more.textContent = `+${list.length - MAX_ICONS}`;
      this.el.append(more);
    }
  }
}
