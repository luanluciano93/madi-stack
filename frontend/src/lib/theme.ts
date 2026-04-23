import { writable } from 'svelte/store';

/// Possible theme values. `dark` is the default — MadiStack's brand is a
/// dark-first UI. `light` flips tokens via `html[data-theme='light']`
/// overrides defined in `app.css`.
export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'madistack.theme';

function initialTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'light' || stored === 'dark') return stored;
  } catch {
    // storage blocked — fall through to default
  }
  return 'dark';
}

/// Apply the theme to the root element. Called on import and on every
/// change so a full-page reload reflects the persisted choice.
function applyToDom(theme: Theme) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.theme = theme;
}

export const theme = writable<Theme>(initialTheme());

theme.subscribe((value) => {
  applyToDom(value);
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    // storage blocked — persistence is best-effort only
  }
});

export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}
