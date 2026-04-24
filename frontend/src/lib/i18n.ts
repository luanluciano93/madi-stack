import { addMessages, init, getLocaleFromNavigator, locale } from 'svelte-i18n';

import ptBR from './locales/pt-BR.json';
import en from './locales/en.json';
import es from './locales/es.json';
import nl from './locales/nl.json';
import de from './locales/de.json';
import it from './locales/it.json';
import pl from './locales/pl.json';
import ru from './locales/ru.json';
import zhCN from './locales/zh-CN.json';
import tr from './locales/tr.json';
import hu from './locales/hu.json';
import lv from './locales/lv.json';
import ro from './locales/ro.json';

/// Register bundled locale dictionaries with svelte-i18n. Keep the fallback
/// aligned with the project's primary language (PT-BR) — missing keys in
/// the active locale transparently fall back to Portuguese.
addMessages('pt-BR', ptBR);
addMessages('en', en);
addMessages('es', es);
addMessages('nl', nl);
addMessages('de', de);
addMessages('it', it);
addMessages('pl', pl);
addMessages('ru', ru);
addMessages('zh-CN', zhCN);
addMessages('tr', tr);
addMessages('hu', hu);
addMessages('lv', lv);
addMessages('ro', ro);

const STORAGE_KEY = 'madistack.locale';

export const AVAILABLE_LOCALES = [
  'pt-BR',
  'en',
  'es',
  'nl',
  'de',
  'it',
  'pl',
  'ru',
  'zh-CN',
  'tr',
  'hu',
  'lv',
  'ro',
] as const;
export type LocaleCode = (typeof AVAILABLE_LOCALES)[number];

/// Native name shown in the settings selector. Keyed by locale code so the
/// label always matches the language itself, regardless of the UI locale.
export const LOCALE_LABELS: Record<LocaleCode, string> = {
  'pt-BR': 'Português (BR)',
  en: 'English',
  es: 'Español',
  nl: 'Nederlands',
  de: 'Deutsch',
  it: 'Italiano',
  pl: 'Polski',
  ru: 'Русский',
  'zh-CN': '中文 (简体)',
  tr: 'Türkçe',
  hu: 'Magyar',
  lv: 'Latviešu',
  ro: 'Română',
};

function isLocaleCode(v: string | null): v is LocaleCode {
  return (AVAILABLE_LOCALES as readonly string[]).includes(v ?? '');
}

/// Map an arbitrary BCP-47 tag from `navigator.language` to one of our
/// supported locales. Handles both exact matches (`zh-CN`) and language-only
/// prefixes (`pt` → `pt-BR`), falling back to PT-BR when unknown.
function matchNavigatorLocale(tag: string): LocaleCode {
  const lower = tag.toLowerCase();
  if (lower.startsWith('pt')) return 'pt-BR';
  if (lower === 'zh-cn' || lower === 'zh-hans' || lower.startsWith('zh')) return 'zh-CN';
  const base = lower.split('-')[0];
  const direct = (AVAILABLE_LOCALES as readonly string[]).find(
    (code) => code.toLowerCase() === lower || code.toLowerCase() === base,
  );
  return (direct as LocaleCode | undefined) ?? 'pt-BR';
}

function pickInitialLocale(): LocaleCode {
  // Persisted user choice wins over auto-detection so language changes
  // survive reloads without touching backend state.
  const stored = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
  if (isLocaleCode(stored)) return stored;

  const navLocale = getLocaleFromNavigator();
  if (!navLocale) return 'pt-BR';
  return matchNavigatorLocale(navLocale);
}

init({
  fallbackLocale: 'pt-BR',
  initialLocale: pickInitialLocale(),
});

/// Persist the active locale. Call this from the Settings UI — svelte-i18n's
/// own `locale.set()` is reactive but doesn't auto-save.
export function setLocale(next: LocaleCode) {
  locale.set(next);
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Storage blocked (rare in a Tauri webview) — fall back to runtime-only.
  }
}
