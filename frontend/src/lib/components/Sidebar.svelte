<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { ipc } from '$lib/ipc';
  import { theme, toggleTheme } from '$lib/theme';
  import logoUrl from '../../assets/logo.png';

  /// Open the install directory (where `madistack.exe` lives) in Explorer.
  /// Lazy `await` — the backend path doesn't change at runtime.
  async function openInstallDir() {
    try {
      const dir = await ipc.installDir();
      await ipc.openPath(dir);
    } catch {
      // Best-effort — an error here would be rare (spawn failure).
    }
  }

  /// Launch a terminal (Windows Terminal or PowerShell) with cwd at the
  /// install directory. Handy for running `composer`, `mysql -u root`,
  /// `git clone`, etc. without leaving the app.
  async function openTerminal() {
    try {
      const dir = await ipc.installDir();
      await ipc.openTerminal(dir);
    } catch {
      // Spawn failures surface as a notification — for now keep quiet.
    }
  }

  let { active = $bindable() } = $props<{
    active:
      | 'geral'
      | 'nginx'
      | 'mariadb'
      | 'php'
      | 'sites'
      | 'firewall'
      | 'configuracoes'
      | 'atualizacoes'
      | 'sobre';
  }>();

  // Label is resolved lazily via the `$_` store so switching locale at
  // runtime re-renders without reload. Each tab's i18n key matches its id.
  const tabs = [
    { id: 'geral', icon: '⌂' },
    { id: 'nginx', icon: '◐' },
    { id: 'mariadb', icon: '◑' },
    { id: 'php', icon: '◉' },
    { id: 'sites', icon: '◇' },
    { id: 'firewall', icon: '🛡' },
    { id: 'configuracoes', icon: '⚙' },
    { id: 'atualizacoes', icon: '↻' },
    { id: 'sobre', icon: 'ⓘ' },
  ] as const;
</script>

<aside class="flex w-14 shrink-0 flex-col border-r border-zinc-800 bg-zinc-900/60 py-4 sm:w-56">
  <div class="flex items-center justify-center px-3 pb-4 sm:justify-start sm:px-5">
    <img src={logoUrl} alt="MadiStack" class="h-8 w-8 shrink-0 rounded" draggable="false" />
    <div class="ml-3 hidden sm:block">
      <h1 class="text-lg font-bold leading-tight tracking-tight">MadiStack</h1>
      <p class="text-xs text-zinc-500">v0.1.2 — dev</p>
    </div>
  </div>

  <nav class="flex flex-1 flex-col gap-0.5 px-2">
    {#each tabs as tab}
      {@const label = $_(`nav.${tab.id}`)}
      <button
        type="button"
        title={label}
        class="flex items-center gap-3 rounded-md px-3 py-2 text-left text-sm transition-colors
               hover:bg-zinc-800
               {active === tab.id ? 'bg-zinc-800 text-white' : 'text-zinc-400'}"
        onclick={() => (active = tab.id)}
      >
        <span class="w-4 shrink-0 text-center text-zinc-500">{tab.icon}</span>
        <span class="hidden sm:inline">{label}</span>
      </button>
    {/each}
  </nav>

  <!-- Footer actions: theme toggle + open the install folder in Explorer +
       spawn a terminal there. Stays pinned to the bottom of the sidebar
       regardless of how many nav tabs there are. -->
  <div class="mt-2 flex flex-col gap-0.5 px-2">
    <button
      type="button"
      onclick={toggleTheme}
      title={$theme === 'dark' ? $_('nav.switch_to_light') : $_('nav.switch_to_dark')}
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-white"
    >
      <span class="w-4 shrink-0 text-center text-zinc-500">{$theme === 'dark' ? '☀' : '🌙'}</span>
      <span class="hidden truncate sm:inline"
        >{$theme === 'dark' ? $_('nav.switch_to_light') : $_('nav.switch_to_dark')}</span
      >
    </button>
    <button
      type="button"
      onclick={openInstallDir}
      title={$_('nav.open_install_dir')}
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-white"
    >
      <span class="w-4 shrink-0 text-center text-zinc-500">📂</span>
      <span class="hidden truncate sm:inline">{$_('nav.open_install_dir')}</span>
    </button>
    <button
      type="button"
      onclick={openTerminal}
      title={$_('nav.open_terminal')}
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-white"
    >
      <span class="w-4 shrink-0 text-center text-zinc-500">⌨</span>
      <span class="hidden truncate sm:inline">{$_('nav.open_terminal')}</span>
    </button>
  </div>
</aside>
