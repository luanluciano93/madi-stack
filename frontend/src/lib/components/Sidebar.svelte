<script lang="ts">
  import { _ } from 'svelte-i18n';
  import logoUrl from '../../assets/logo.png';

  let { active = $bindable() } = $props<{
    active:
      | 'geral'
      | 'nginx'
      | 'mariadb'
      | 'php'
      | 'sites'
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
    { id: 'configuracoes', icon: '⚙' },
    { id: 'atualizacoes', icon: '↻' },
    { id: 'sobre', icon: 'ⓘ' },
  ] as const;
</script>

<aside
  class="flex w-14 shrink-0 flex-col border-r border-zinc-800 bg-zinc-900/60 py-4 sm:w-56"
>
  <div class="flex items-center justify-center px-3 pb-4 sm:justify-start sm:px-5">
    <img
      src={logoUrl}
      alt="MadiStack"
      class="h-8 w-8 shrink-0 rounded"
      draggable="false"
    />
    <div class="ml-3 hidden sm:block">
      <h1 class="text-lg font-bold leading-tight tracking-tight">MadiStack</h1>
      <p class="text-xs text-zinc-500">v0.1.0 — dev</p>
    </div>
  </div>

  <nav class="flex flex-col gap-0.5 px-2">
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
</aside>
