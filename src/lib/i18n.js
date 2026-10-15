// Textele interfeței în română, engleză și rusă (un singur dicționar, folosit și de Rust).
import strings from '../locales.json' with { type: 'json' };

const LOCALES = { ro: 'ro-RO', en: 'en-GB', ru: 'ru-RU' };
let lang = 'ro';

export function setLang(code) {
  if (!strings[code]) return;
  lang = code;
  if (typeof document !== 'undefined') document.documentElement.lang = code;
}

export const getLang = () => lang;
export const locale = () => LOCALES[lang];

// `{n}` alege și forma de plural când textul are mai multe forme.
export function t(key, params = {}) {
  let s = strings[lang][key] ?? strings.ro[key] ?? key;
  if (typeof s === 'object') s = s[new Intl.PluralRules(lang).select(params.n ?? 0)] ?? s.other;
  return s.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m));
}

// Elementele cu `data-i18n` (text), `data-i18n-title`, `data-i18n-aria` primesc textul din limba curentă.
export function applyStatic(root = document) {
  root.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
  root.querySelectorAll('[data-i18n-title]').forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  root.querySelectorAll('[data-i18n-aria]').forEach((el) => el.setAttribute('aria-label', t(el.dataset.i18nAria)));
}
