import { locale } from './i18n.js';

// Unități zecimale (ca în browsere), la fel în toate limbile; doar separatorul zecimal depinde de limbă.
const num = (x) => new Intl.NumberFormat(locale(), { maximumFractionDigits: x < 10 ? 1 : 0 }).format(x);

export function formatBytes(n) {
  if (n >= 1e9) return `${num(n / 1e9)} GB`;
  if (n >= 1e6) return `${num(n / 1e6)} MB`;
  return `${num(n / 1e3)} KB`;
}

export const formatRate = (bytesPerSec) => `${formatBytes(bytesPerSec)}/s`;

// „warn” de la prag în sus; o valoare lipsă (senzor indisponibil) nu avertizează
export const level = (value, warnAt) => (value != null && value >= warnAt ? 'warn' : 'ok');
