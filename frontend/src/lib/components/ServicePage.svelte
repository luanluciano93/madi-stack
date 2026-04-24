<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { _ } from 'svelte-i18n';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import StatusLed from './StatusLed.svelte';
  import LogViewer from './LogViewer.svelte';
  import { ipc, onServiceStatus, type ComponentSlug, type ServiceStatus } from '$lib/ipc';

  type Props = {
    slug: ComponentSlug;
    title: string;
  };

  let { slug, title }: Props = $props();

  let status = $state<ServiceStatus>('stopped');
  let pid = $state<number | null>(null);
  let port = $state<number | null>(null);
  let configPath = $state<string | null>(null);
  let logPath = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | null = null;

  async function refreshConfigPath() {
    try {
      configPath = await ipc.serviceConfigPath(slug);
    } catch {
      configPath = null;
    }
  }

  async function refreshLogPath() {
    try {
      logPath = await ipc.serviceLogPath(slug);
    } catch {
      logPath = null;
    }
  }

  async function refreshPid() {
    // `service_pid` is cheap (map lookup) — safe to call on every status
    // transition. Returns null when the service isn't running.
    try {
      pid = await ipc.servicePid(slug);
    } catch {
      pid = null;
    }
  }

  let running = $derived(status === 'running');
  let ledStatus = $derived(
    status === 'stopping' ? 'starting' : (status as 'running' | 'starting' | 'stopped' | 'crashed'),
  );

  async function refreshPort() {
    try {
      const cfg = await ipc.getConfig();
      switch (slug) {
        case 'nginx':
          port = cfg.ports.http;
          break;
        case 'mariadb':
          port = cfg.ports.mariadb;
          break;
        case 'php':
          port = cfg.ports.php_fcgi;
          break;
        default:
          port = null;
      }
    } catch {
      port = null;
    }
  }

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
    await refreshPort();
    await refreshConfigPath();
    await refreshLogPath();
    await refreshPid();
    unlisten = await onServiceStatus((evt) => {
      if (evt.slug === slug) {
        status = evt.status;
        void refreshPid();
      }
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
    <span class="text-sm text-zinc-500">
      —
      {#if status === 'running' && port !== null && pid !== null}
        {$_('common.running_on_port_pid', { values: { port, pid } })}
      {:else if status === 'running' && port !== null}
        {$_('common.running_on_port', { values: { port } })}
      {:else}
        {$_(`common.${status === 'starting' || status === 'stopping' ? 'running' : status}`)}
      {/if}
    </span>
  </header>

  <div class="flex items-center gap-2">
    <button
      type="button"
      disabled={busy || running}
      onclick={start}
      class="rounded-md bg-brand-600 px-6 py-2.5 font-medium text-white hover:bg-brand-500 disabled:opacity-40"
    >
      {$_('actions.start')}
    </button>
    <button
      type="button"
      disabled={busy || !running}
      onclick={stop}
      class="rounded-md border border-zinc-700 px-6 py-2.5 font-medium hover:bg-zinc-800 disabled:opacity-40"
    >
      {$_('actions.stop')}
    </button>
    {#if configPath}
      <button
        type="button"
        onclick={() => ipc.openPath(configPath!)}
        class="rounded-md border border-zinc-700 px-4 py-2.5 text-sm font-medium hover:bg-zinc-800"
        title={$_('service.open_config_tooltip', { values: { path: configPath } })}
      >
        {$_('service.open_config')}
      </button>
    {/if}
    {#if logPath}
      <button
        type="button"
        onclick={() => ipc.openPath(logPath!)}
        class="rounded-md border border-zinc-700 px-4 py-2.5 text-sm font-medium hover:bg-zinc-800"
        title={$_('service.open_log_tooltip', { values: { path: logPath } })}
      >
        {$_('service.open_log')}
      </button>
    {/if}
    {#if error}
      <span class="ml-2 text-sm text-red-400">{error}</span>
    {/if}
  </div>

  <div class="space-y-2">
    <h3 class="text-sm font-medium text-zinc-300">{$_('service.logs_title')}</h3>
    <LogViewer component={slug} />
  </div>
</section>
