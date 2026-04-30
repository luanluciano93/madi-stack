// Single source of truth for the app version shown in the UI.
//
// `getVersion()` reads from `tauri.conf.json` baked into the binary at build
// time, so this stays in sync with `Cargo.toml` automatically — no more
// hand-edited literals like `'0.1.2'` rotting in `Sidebar.svelte` or
// `Sobre.svelte` after each bump.
//
// The fetch happens once on first import and gets cached in the store.
// Components subscribe and render `…` until it resolves (sub-100ms in
// practice, so most users never see the placeholder).

import { writable } from 'svelte/store';
import { getVersion } from '@tauri-apps/api/app';

export const appVersion = writable<string>('…');

void getVersion()
  .then((v) => appVersion.set(v))
  .catch(() => appVersion.set('?'));
