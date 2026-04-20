<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Geral from './routes/Geral.svelte';
  import Nginx from './routes/Nginx.svelte';
  import MariaDB from './routes/MariaDB.svelte';
  import PHP from './routes/PHP.svelte';
  import Configuracoes from './routes/Configuracoes.svelte';
  import Atualizacoes from './routes/Atualizacoes.svelte';
  import Sobre from './routes/Sobre.svelte';

  type Route =
    | 'geral'
    | 'nginx'
    | 'mariadb'
    | 'php'
    | 'configuracoes'
    | 'atualizacoes'
    | 'sobre';

  let active = $state<Route>('geral');
</script>

<div class="flex h-full">
  <Sidebar bind:active />

  <main class="min-w-0 flex-1 overflow-auto p-4 md:p-8">
    {#if active === 'geral'}
      <Geral />
    {:else if active === 'nginx'}
      <Nginx />
    {:else if active === 'mariadb'}
      <MariaDB />
    {:else if active === 'php'}
      <PHP />
    {:else if active === 'configuracoes'}
      <Configuracoes />
    {:else if active === 'atualizacoes'}
      <Atualizacoes />
    {:else if active === 'sobre'}
      <Sobre />
    {/if}
  </main>
</div>
