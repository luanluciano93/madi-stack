<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import StatusLed from './StatusLed.svelte';
  import LogViewer from './LogViewer.svelte';
  import {
    ipc,
    onServiceStatus,
    type ComponentSlug,
    type ServiceStatus,
  } from '$lib/ipc';

  type Props = {
    slug: ComponentSlug;
    title: string;
  };

  let { slug, title }: Props = $props();

  let status = $state<ServiceStatus>('stopped');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | null = null;

  let running = $derived(status === 'running');
  let ledStatus = $derived(
    status === 'stopping' ? 'starting' : (status as 'running' | 'starting' | 'stopped' | 'crashed'),
  );

  async function start() {
    busy = true;
    error = null;
    try {
      await ipc.serviceStart(slug);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function stop() {
    busy = true;
    error = null;
    try {
      await ipc.serviceStop(slug);
      status = await ipc.serviceStatus(slug);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(async () => {
    try {
      status = await ipc.serviceStatus(slug);
    } catch {
      // known slug — status is infallible on the Rust side
    }
    unlisten = await onServiceStatus((evt) => {
      if (evt.slug === slug) status = evt.status;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<section class="space-y-6">
  <header class="flex items-center gap-3">
    <StatusLed status={ledStatus} />
    <h2 class="text-2xl font-semibold">{title}</h2>
    <span class="text-sm text-zinc-500">— {status}</span>
  </header>

  <div class="flex items-center gap-2">
    <button
      type="button"
      disabled={busy || running}
      onclick={start}
      class="rounded-md bg-emerald-600 px-6 py-2.5 font-medium text-white hover:bg-emerald-500 disabled:opacity-40"
    >
      Iniciar
    </button>
    <button
      type="button"
      disabled={busy || !running}
      onclick={stop}
      class="rounded-md border border-zinc-700 px-6 py-2.5 font-medium hover:bg-zinc-800 disabled:opacity-40"
    >
      Parar
    </button>
    {#if error}
      <span class="ml-2 text-sm text-red-400">{error}</span>
    {/if}
  </div>

  <div class="space-y-2">
    <h3 class="text-sm font-medium text-zinc-300">Logs</h3>
    <LogViewer component={slug} />
  </div>
</section>
