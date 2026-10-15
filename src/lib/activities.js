// Activitățile insulei, ca Live Activities pe iPhone.
// Persistente (timer, media): concurează pentru pastilă; a doua apare ca bulă.
// Tranzitorii (alertă, volum, notificare, update): o coadă, afișate pe rând, fiecare `duration` ms.

const PRIORITY = { call: 4, timer: 3, stopwatch: 3, media: 2 };

export function layout(persistent) {
  const sorted = [...persistent].sort((a, b) => (PRIORITY[b.kind] ?? 0) - (PRIORITY[a.kind] ?? 0));
  return { pill: sorted[0] ?? null, bubble: sorted[1] ?? null };
}

// Adaugă o activitate tranzitorie. Volumul nu se adaugă de mai multe ori: activitatea existentă
// e actualizată pe loc și, dacă e deja afișată, își reîncepe durata.
export function enqueue(queue, activity, now) {
  // volumul trece în fața cozii (feedback imediat); ce era afișat reîncepe după el
  if (activity.kind === 'volume') {
    const rest = queue.filter((a) => a.kind !== 'volume').map((a) => ({ ...a, startedAt: undefined }));
    return [{ ...activity, startedAt: now }, ...rest];
  }
  const q = [...queue, { ...activity, startedAt: queue.length === 0 ? now : undefined }];
  // cel mult 3 notificări în așteptare (în afară de cea afișată): cele vechi se pierd
  const waiting = q.slice(1).filter((a) => a.kind === 'notification');
  const drop = new Set(waiting.slice(0, Math.max(0, waiting.length - 3)));
  return q.filter((a) => !drop.has(a));
}

// Scoate tranzitoriile terminate; următoarea începe exact când s-a terminat precedenta.
export function advance(queue, now) {
  const q = [...queue];
  while (q.length && q[0].startedAt + q[0].duration <= now) {
    const end = q[0].startedAt + q[0].duration;
    q.shift();
    if (q.length) q[0] = { ...q[0], startedAt: end };
  }
  return q;
}

export function current(queue, now) {
  const head = queue[0];
  return head && head.startedAt + head.duration > now ? head : null;
}

// Cât cursorul e pe o tranzitorie interactivă (captura cu butoane), ea nu expiră:
// rămân cel puțin `keepMs` după ultima ținere.
export function holdHead(queue, now, keepMs = 1500) {
  const head = queue[0];
  if (!head?.interactive || head.startedAt + head.duration >= now + keepMs) return queue;
  return [{ ...head, startedAt: now + keepMs - head.duration }, ...queue.slice(1)];
}

// Scoate tranzitoriile de un anumit tip; dacă a plecat cea afișată, următoarea începe acum.
export function drop(queue, kind, now) {
  const q = queue.filter((a) => a.kind !== kind);
  if (q.length && q[0].startedAt == null) q[0] = { ...q[0], startedAt: now };
  return q;
}
