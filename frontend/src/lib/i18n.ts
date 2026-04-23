import { addMessages, init, getLocaleFromNavigator, locale } from 'svelte-i18n';

import ptBR from './locales/pt-BR.json';
import en from './locales/en.json';
import es from './locales/es.json';

/// Register bundled locale dictionaries with svelte-i18n. Keep the fallback
/// aligned with the project's primary language (PT-BR) — missing keys in
/// the active locale transparently fall back to Portuguese.
addMessages('pt-BR', ptBR);
addMessages('en', en);
addMessages('es', es);

const STORAGE_KEY = 'madistack.locale';

export const AVAILABLE_LOCALES = ['pt-BR', 'en', 'es'] as const;
export type LocaleCode = (typeof AVAILABLE_LOCALES)[number];

function isLocaleCode(v: string | null): v is LocaleCode {
  return v === 'pt-BR' || v === 'en' || v === 'es';
}

function pickInitialLocale(): LocaleCode {
  // Persisted user choice wins over auto-detection so language changes
  // survive reloads without touching backend state.
  const stored = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
  if (isLocaleCode(stored)) return stored;

  const navLocale = (getLocaleFromNavigator() ?? '').toLowerCase();
  if (navLocale.startsWith('en')) return 'en';
  if (navLocale.startsWith('es')) return 'es';
  return 'pt-BR';
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
