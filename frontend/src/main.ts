import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
// Side-effect import: registers locale dictionaries and picks the initial
// language before any component renders. Must run before App mounts.
import './lib/i18n';
// Side-effect import: applies persisted `html[data-theme]` so the first
// paint already reflects the user's choice (avoids a dark→light flash).
import './lib/theme';

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
