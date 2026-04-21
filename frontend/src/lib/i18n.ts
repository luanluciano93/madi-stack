import { addMessages, init, getLocaleFromNavigator, locale } from 'svelte-i18n';

import ptBR from './locales/pt-BR.json';
import en from './locales/en.json';

/// Register bundled locale dictionaries with svelte-i18n. Keep the fallback
/// aligned with the project's primary language (PT-BR) — missing keys in
/// the active locale transparently fall back to Portuguese.
addMessages('pt-BR', ptBR);
addMessages('en', en);

const STORAGE_KEY = 'madistack.locale';

function pickInitialLocale(): string {
  // Persisted user choice wins over auto-detection so language changes
  // survive reloads without touching backend state.
  const stored = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
  if (stored === 'pt-BR' || stored === 'en') return stored;

  const navLocale = getLocaleFromNavigator();
  if (navLocale && navLocale.toLowerCase().startsWith('en')) return 'en';
  return 'pt-BR';
}

init({
  fallbackLocale: 'pt-BR',
  initialLocale: pickInitialLocale(),
});

/// Persist the active locale. Call this from the Settings UI — svelte-i18n's
/// own `locale.set()` is reactive but doesn't auto-save.
export function setLocale(next: 'pt-BR' | 'en') {
  locale.set(next);
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Storage blocked (rare in a Tauri webview) — fall back to runtime-only.
  }
}

export const AVAILABLE_LOCALES = ['pt-BR', 'en'] as const;
export type LocaleCode = (typeof AVAILABLE_LOCALES)[number];
