<script lang="ts">
  import { onMount } from 'svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import EventLog from '$lib/components/EventLog.svelte';
  import Coachmark from '$lib/components/Coachmark.svelte';
  import { bindTourNavigation, tour, type TourRoute } from '$lib/tour';
  import Geral from './routes/Geral.svelte';
  import Nginx from './routes/Nginx.svelte';
  import MariaDB from './routes/MariaDB.svelte';
  import PHP from './routes/PHP.svelte';
  import Configuracoes from './routes/Configuracoes.svelte';
  import Atualizacoes from './routes/Atualizacoes.svelte';
  import Sites from './routes/Sites.svelte';
  import Firewall from './routes/Firewall.svelte';
  import Sobre from './routes/Sobre.svelte';

  type Route =
    | 'geral'
    | 'nginx'
    | 'mariadb'
    | 'php'
    | 'sites'
    | 'firewall'
    | 'configuracoes'
    | 'atualizacoes'
    | 'sobre';

  let active = $state<Route>('geral');

  // Hand the tour engine a setter for our router so it can flip tabs as
  // the user advances through coach-marks. The cast is safe — `TourRoute`
  // is a structural copy of `Route` kept in tour.ts to avoid an import
  // cycle with this file.
  bindTourNavigation((route: TourRoute) => {
    active = route as Route;
  });

  onMount(() => {
    tour.startIfFirstRun();
  });
</script>

<!-- Top-level layout: sidebar + main content grow to fill the viewport;
     the EventLog footer floats at the bottom and can be expanded or
     collapsed by the user. `min-h-0` on the horizontal row is needed so
     the main `overflow-auto` actually scrolls instead of pushing the
     EventLog off-screen. -->
<div class="flex h-full flex-col">
  <div class="flex min-h-0 flex-1">
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
      {:else if active === 'sites'}
        <Sites />
      {:else if active === 'firewall'}
        <Firewall />
      {:else if active === 'configuracoes'}
        <Configuracoes />
      {:else if active === 'atualizacoes'}
        <Atualizacoes />
      {:else if active === 'sobre'}
        <Sobre />
      {/if}
    </main>
  </div>

  <EventLog />
</div>

<Coachmark />
