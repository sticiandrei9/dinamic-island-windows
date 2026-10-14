// Activitățile insulei, ca Live Activities pe iPhone.
// Persistente (timer, media): concurează pentru pastilă; a doua apare ca bulă.
// Tranzitorii (alertă, volum, notificare, update): o coadă, afișate pe rând, fiecare `duration` ms.

const PRIORITY = { timer: 3, stopwatch: 3, media: 2 };

export function layout(persistent) {
  const sorted = [...persistent].sort((a, b) => (PRIORITY[b.kind] ?? 0) - (PRIORITY[a.kind] ?? 0));
  return { pill: sorted[0] ?? null, bubble: sorted[1] ?? null };
}

// Adaugă o activitate tranzitorie. Volumul nu se adaugă de mai multe ori: activitatea existentă
// e actualizată pe loc și, dacă e deja afișată, își reîncepe durata.
export function enqueue(queue, activity, now) {
  if (activity.kind === 'volume') {
    const i = queue.findIndex((a) => a.kind === 'volume');
    if (i >= 0) {
      const q = [...queue];
      q[i] = { ...activity, startedAt: i === 0 ? now : undefined };
      return q;
    }
  }
  return [...queue, { ...activity, startedAt: queue.length === 0 ? now : undefined }];
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
