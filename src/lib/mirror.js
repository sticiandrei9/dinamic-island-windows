// Următoarea cameră din listă (circular); null dacă nu există camere.
export function nextCamera(cams, currentId) {
  if (!cams.length) return null;
  const i = cams.findIndex((c) => c.deviceId === currentId);
  return cams[(i + 1) % cams.length].deviceId;
}

// Camera salvată în setări, doar dacă mai e conectată; altfel null = camera implicită.
export function pickCamera(cams, savedId) {
  return savedId && cams.some((c) => c.deviceId === savedId) ? savedId : null;
}
