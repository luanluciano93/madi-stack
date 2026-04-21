import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
// Side-effect import: registers locale dictionaries and picks the initial
// language before any component renders. Must run before App mounts.
import './lib/i18n';

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
