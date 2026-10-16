// Pictograme în stilul SF Symbols (SVG constant, colorate cu currentColor).
const svg = (body, vb = '0 0 24 24') =>
  `<svg viewBox="${vb}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

export const ICONS = {
  home: svg('<path d="M3 11.5 12 4l9 7.5"/><path d="M5.5 10v9.5h13V10"/>'),
  timer: svg('<circle cx="12" cy="13" r="8"/><path d="M12 9v4l2.5 2.5M9.5 2.5h5"/>'),
  shelf: svg('<path d="M4 7h16v12H4zM4 7l2-3h12l2 3M9.5 11.5h5"/>'),
  mirror: svg('<rect x="3" y="6" width="13" height="12" rx="3"/><path d="m16 10.5 5-3v9l-5-3"/>'),
  system: svg('<rect x="6" y="6" width="12" height="12" rx="2.5"/><rect x="9.5" y="9.5" width="5" height="5" rx="1"/><path d="M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3"/>'),
  hourglass: svg('<path d="M6.5 3h11M6.5 21h11M7.5 3c0 5 4.5 6 4.5 9s-4.5 4-4.5 9M16.5 3c0 5-4.5 6-4.5 9s4.5 4 4.5 9"/>'),
  stopwatch: svg('<circle cx="12" cy="13.5" r="7.5"/><path d="M12 9.5v4M10 2.5h4M18.5 6l1.5-1.5"/>'),
  bolt: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M13.5 2 4.5 13.5H11L10 22l9-11.5h-6.5z"/></svg>',
  battery: svg('<rect x="2.5" y="7" width="17" height="10" rx="3"/><path d="M22 10.5v3"/><path d="M5.5 10v4" stroke-width="2.6"/>'),
  headphones: svg('<path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="7" rx="1.8"/><rect x="16.5" y="14" width="4.5" height="7" rx="1.8"/>'),
  speakerDevice: svg('<rect x="6" y="2.5" width="12" height="19" rx="3"/><circle cx="12" cy="14.5" r="3.2"/><circle cx="12" cy="7" r="1"/>'),
  mouse: svg('<rect x="6" y="3" width="12" height="18" rx="6"/><path d="M12 7v3"/>'),
  keyboard: svg('<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6.5 10h1M10.5 10h1M14.5 10h1M8 14h8"/>'),
  gamepad: svg('<path d="M7 7h10a5 5 0 0 1 4.8 6.4l-1 3.4a2.5 2.5 0 0 1-4.3.9L14.6 15H9.4l-1.9 2.7a2.5 2.5 0 0 1-4.3-.9l-1-3.4A5 5 0 0 1 7 7z"/><path d="M7.5 10v3M6 11.5h3"/><circle cx="16" cy="11" r=".6" fill="currentColor"/><circle cx="17.5" cy="12.8" r=".6" fill="currentColor"/>'),
  bluetooth: svg('<path d="m7 7 10 10-5 4.5V2.5L17 7 7 17"/>'),
  sun: svg('<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>'),
  bell: svg('<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>'),
  download: svg('<circle cx="12" cy="12" r="9.5"/><path d="M12 7.5v8M8.5 12.5 12 16l3.5-3.5"/>'),
  file: svg('<path d="M6.5 3h7l4.5 4.5V20a1 1 0 0 1-1 1H6.5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M13.5 3v4.5H18"/>'),
  micOff: svg('<rect x="9" y="2.5" width="6" height="11.5" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5v4M3.5 3.5l17 17"/>'),
  mic: svg('<rect x="9" y="2.5" width="6" height="11.5" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5v4"/>'),
  capslock: svg('<path d="M12 4 4.5 12H9v4.5h6V12h4.5z"/><path d="M9 20h6"/>'),
  wifi: svg('<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.7 15.8a5 5 0 0 1 6.6 0"/><circle cx="12" cy="19" r="1" fill="currentColor"/>'),
  wifiOff: svg('<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.7 15.8a5 5 0 0 1 6.6 0M3.5 3.5l17 17"/><circle cx="12" cy="19" r="1" fill="currentColor"/>'),
  vpn: svg('<path d="M12 2.5 4.5 5.5v5.5c0 5 3.3 8.5 7.5 10.5 4.2-2 7.5-5.5 7.5-10.5V5.5z"/><path d="m9 12 2 2 4-4"/>'),
  calendar: svg('<rect x="3.5" y="5" width="17" height="15.5" rx="3"/><path d="M3.5 10h17M8 3v4M16 3v4"/>'),
  camera: svg('<rect x="3" y="6" width="13" height="12" rx="3"/><path d="m16 10.5 5-3v9l-5-3"/>'),
  copy: svg('<rect x="8.5" y="8.5" width="12" height="12" rx="3"/><path d="M15.5 8.5v-2a3 3 0 0 0-3-3h-6a3 3 0 0 0-3 3v6a3 3 0 0 0 3 3h2"/>'),
  moon: svg('<path d="M20.5 13.2A8.5 8.5 0 1 1 10.8 3.5a6.6 6.6 0 0 0 9.7 9.7z"/>'),
  sparkle: svg('<path d="M12 3c.6 4.8 3.2 7.4 8 9-4.8 1.6-7.4 4.2-8 9-.6-4.8-3.2-7.4-8-9 4.8-1.6 7.4-4.2 8-9z"/>'),
};

// Difuzor cu 0–3 unde (sau tăiat, la mute).
export function speakerIcon(waves, muted) {
  const body = '<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor"/>';
  if (muted) return svg(`${body}<path d="m16 9.5 5 5M21 9.5l-5 5"/>`);
  const arcs = ['<path d="M15.5 9.5a3.5 3.5 0 0 1 0 5"/>', '<path d="M17.8 7.3a6.6 6.6 0 0 1 0 9.4"/>', '<path d="M20 5.2a9.6 9.6 0 0 1 0 13.6"/>'];
  return svg(body + arcs.slice(0, waves).join(''));
}
