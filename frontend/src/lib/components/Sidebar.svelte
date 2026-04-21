<script lang="ts">
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

  const tabs = [
    { id: 'geral', label: 'Geral', icon: '⌂' },
    { id: 'nginx', label: 'Nginx', icon: '◐' },
    { id: 'mariadb', label: 'MariaDB', icon: '◑' },
    { id: 'php', label: 'PHP', icon: '◉' },
    { id: 'sites', label: 'Sites', icon: '◇' },
    { id: 'configuracoes', label: 'Configurações', icon: '⚙' },
    { id: 'atualizacoes', label: 'Atualizações', icon: '↻' },
    { id: 'sobre', label: 'Sobre', icon: 'ⓘ' },
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
      <button
        type="button"
        title={tab.label}
        class="flex items-center gap-3 rounded-md px-3 py-2 text-left text-sm transition-colors
               hover:bg-zinc-800
               {active === tab.id ? 'bg-zinc-800 text-white' : 'text-zinc-400'}"
        onclick={() => (active = tab.id)}
      >
        <span class="w-4 shrink-0 text-center text-zinc-500">{tab.icon}</span>
        <span class="hidden sm:inline">{tab.label}</span>
      </button>
    {/each}
  </nav>
</aside>
