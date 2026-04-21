<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';
  import StatusLed from '$lib/components/StatusLed.svelte';
  import {
    ipc,
    onInstallProgress,
    onServiceStatus,
    type ComponentInfo,
    type ComponentSlug,
    type InstallPhase,
    type ServiceStatus,
  } from '$lib/ipc';

  type InstallState = {
    phase: InstallPhase | 'idle';
    bytes?: number;
    total?: number;
    message?: string;
  };

  type Row = {
    info: ComponentInfo;
    status: ServiceStatus;
    busy: boolean;
    error: string | null;
    installed: boolean;
    install: InstallState;
  };

  let rows = $state<Row[]>([]);
  let reply = $state('');
  let installingAll = $state(false);
  let installError = $state<string | null>(null);
  let httpPort = $state(80);
  let unlistenStatus: UnlistenFn | null = null;
  let unlistenInstall: UnlistenFn | null = null;

  // First-run welcome banner. Dismissed permanently after the first install
  // succeeds OR the user clicks the close button — persisted via
  // localStorage so it doesn't show up on every cold start.
  const ONBOARDING_KEY = 'madistack.onboarded';
  let showOnboarding = $state(false);
  function loadOnboarding() {
    try {
      showOnboarding = localStorage.getItem(ONBOARDING_KEY) !== '1';
    } catch {
      // localStorage blocked — default to showing once per session.
      showOnboarding = true;
    }
  }
  function dismissOnboarding() {
    showOnboarding = false;
    try {
      localStorage.setItem(ONBOARDING_KEY, '1');
    } catch {
      // storage blocked — fall back to session-only dismissal.
    }
  }

  function openPhpMyAdmin() {
    void shellOpen(`http://localhost:${httpPort}/phpmyadmin/`);
  }

  function applyStatus(slug: ComponentSlug, status: ServiceStatus) {
    const i = rows.findIndex((r) => r.info.slug === slug);
    if (i >= 0) rows[i].status = status;
  }

  async function hydrateInitialStatus() {
    await Promise.all(
      rows.map(async (r, i) => {
        if (r.info.slug === 'phpmyadmin') return;
        try {
          rows[i].status = await ipc.serviceStatus(r.info.slug);
        } catch {
          // status is infallible on the Rust side for known slugs
        }
      }),
    );
  }

  async function start(i: number) {
    rows[i].busy = true;
    rows[i].error = null;
    try {
      await ipc.serviceStart(rows[i].info.slug);
      // Status will update via the `service-status` event from the backend.
    } catch (e) {
      rows[i].error = String(e);
    } finally {
      rows[i].busy = false;
    }
  }

  async function stop(i: number) {
    rows[i].busy = true;
    rows[i].error = null;
    try {
      await ipc.serviceStop(rows[i].info.slug);
      rows[i].status = await ipc.serviceStatus(rows[i].info.slug);
    } catch (e) {
      rows[i].error = String(e);
    } finally {
      rows[i].busy = false;
    }
  }

  async function pingBackend() {
    reply = await ipc.ping();
  }

  async function refreshInstalled() {
    await Promise.all(
      rows.map(async (r, i) => {
        try {
          rows[i].installed = await ipc.componentInstalled(r.info.slug);
        } catch {
          // leave prior value
        }
      }),
    );
  }

  function applyInstallEvent(slug: ComponentSlug, next: InstallState) {
    const i = rows.findIndex((r) => r.info.slug === slug);
    if (i < 0) return;
    rows[i].install = next;
    if (next.phase === 'done') {
      rows[i].installed = true;
    }
  }

  async function installOne(i: number) {
    const slug = rows[i].info.slug;
    installError = null;
    rows[i].install = { phase: 'resolving' };
    try {
      await ipc.componentInstall(slug);
    } catch (e) {
      rows[i].install = { phase: 'error', message: String(e) };
    }
  }

  async function installAll() {
    installingAll = true;
    installError = null;
    try {
      await ipc.installAll();
      await refreshInstalled();
    } catch (e) {
      installError = String(e);
    } finally {
      installingAll = false;
    }
  }

  function percent(r: Row): number | null {
    const { bytes, total } = r.install;
    if (!total || !bytes) return null;
    return Math.min(100, Math.round((bytes / total) * 100));
  }

  function fmtBytes(n: number | undefined): string {
    if (!n) return '';
    const mib = n / (1024 * 1024);
    return mib >= 1 ? `${mib.toFixed(1)} MiB` : `${(n / 1024).toFixed(0)} KiB`;
  }

  function phaseLabel(p: InstallPhase | 'idle'): string {
    switch (p) {
      case 'resolving':
        return 'resolvendo versão…';
      case 'downloading':
        return 'baixando';
      case 'verifying':
        return 'verificando SHA256…';
      case 'extracting':
        return 'extraindo…';
      case 'done':
        return 'instalado';
      case 'error':
        return 'erro';
      default:
        return '';
    }
  }

  onMount(async () => {
    loadOnboarding();
    const infos = await ipc.listComponents();
    rows = infos.map((info) => ({
      info,
      status: 'stopped',
      busy: false,
      error: null,
      installed: false,
      install: { phase: 'idle' } as InstallState,
    }));
    await hydrateInitialStatus();
    await refreshInstalled();
    try {
      const cfg = await ipc.getConfig();
      httpPort = cfg.ports.http;
    } catch {
      // keep default — the "Abrir" button falls back to :80
    }
    unlistenStatus = await onServiceStatus((evt) => applyStatus(evt.slug, evt.status));
    unlistenInstall = await onInstallProgress((evt) =>
      applyInstallEvent(evt.slug, {
        phase: evt.phase,
        bytes: evt.bytes,
        total: evt.total,
        message: evt.message,
      }),
    );
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenInstall?.();
  });
</script>

<section class="space-y-6">
  <header class="flex items-start justify-between gap-3">
    <div>
      <h2 class="text-2xl font-semibold">Geral</h2>
      <p class="text-sm text-zinc-400">Controle e status dos serviços.</p>
    </div>
    {#if rows.some((r) => !r.installed)}
      <button
        type="button"
        onclick={installAll}
        disabled={installingAll || rows.some((r) => r.install.phase !== 'idle' && r.install.phase !== 'done' && r.install.phase !== 'error')}
        class="shrink-0 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
      >
        {installingAll ? 'Baixando…' : 'Baixar tudo'}
      </button>
    {/if}
  </header>

  {#if showOnboarding && rows.length > 0 && rows.some((r) => !r.installed)}
    <div class="rounded-md border border-brand-500/40 bg-brand-500/10 p-4 text-sm">
      <div class="flex items-start gap-3">
        <span class="text-lg" aria-hidden="true">👋</span>
        <div class="flex-1 space-y-1">
          <div class="font-medium text-brand-400">Primeira vez por aqui?</div>
          <p class="text-zinc-300">
            Clique em <span class="font-medium">Baixar tudo</span> para buscar nginx, PHP, MariaDB
            e phpMyAdmin das fontes oficiais (primeiro uso leva ~2 min). Depois, em cada linha,
            o botão <span class="font-medium">Iniciar</span> sobe o serviço. Coloque seus sites
            em subpastas de <code class="rounded bg-zinc-800 px-1 py-0.5 text-xs">www/</code> e
            a aba <span class="font-medium">Sites</span> transforma cada uma em
            <code class="rounded bg-zinc-800 px-1 py-0.5 text-xs">&lt;nome&gt;.test</code>.
          </p>
        </div>
        <button
          type="button"
          onclick={dismissOnboarding}
          class="text-zinc-500 hover:text-zinc-200"
          aria-label="Dispensar"
        >
          ✕
        </button>
      </div>
    </div>
  {/if}

  {#if installError}
    <p class="text-sm text-red-400">Falha ao instalar tudo: {installError}</p>
  {/if}

  <div class="space-y-2">
    {#each rows as row, i (row.info.slug)}
      {@const isPma = row.info.slug === 'phpmyadmin'}
      {@const inFlight =
        row.install.phase !== 'idle' &&
        row.install.phase !== 'done' &&
        row.install.phase !== 'error'}
      {@const pct = percent(row)}
      <div
        class="flex flex-col gap-2 rounded-lg border border-zinc-800 bg-zinc-900/60 p-3"
      >
        <div class="flex items-center gap-3">
          <StatusLed
            status={row.status === 'stopping'
              ? 'starting'
              : (row.status as 'running' | 'starting' | 'stopped' | 'crashed')}
          />
          <div class="min-w-0 flex-1">
            <div class="font-medium">{row.info.name}</div>
            <div class="text-xs text-zinc-500">
              {#if !row.installed && !inFlight}
                não instalado
              {:else if isPma}
                servido pelo nginx
              {:else}
                {row.status}
              {/if}
              {#if row.error}
                <span class="ml-2 text-red-400">— {row.error}</span>
              {/if}
            </div>
          </div>
          {#if !row.installed}
            <button
              type="button"
              disabled={inFlight || installingAll}
              onclick={() => installOne(i)}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-sm text-white hover:bg-brand-500 disabled:opacity-40"
            >
              {inFlight ? 'Instalando…' : 'Instalar'}
            </button>
          {:else if isPma}
            <button
              type="button"
              onclick={openPhpMyAdmin}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-sm text-white hover:bg-brand-500"
              title={`Abrir http://localhost:${httpPort}/phpmyadmin/ no navegador padrão`}
            >
              Abrir
            </button>
          {:else}
            <button
              type="button"
              disabled={row.busy || row.status === 'running'}
              onclick={() => start(i)}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-sm text-white hover:bg-brand-500 disabled:opacity-40"
            >
              Iniciar
            </button>
            <button
              type="button"
              disabled={row.busy || row.status !== 'running'}
              onclick={() => stop(i)}
              class="rounded-md border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800 disabled:opacity-40"
            >
              Parar
            </button>
          {/if}
        </div>
        {#if inFlight || (row.install.phase === 'error' && row.install.message)}
          <div class="flex items-center gap-2 text-xs text-zinc-400">
            <span class="min-w-[9rem]">{phaseLabel(row.install.phase)}</span>
            {#if row.install.phase === 'downloading' && pct !== null}
              <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-zinc-800">
                <div
                  class="h-full bg-brand-500 transition-all"
                  style="width: {pct}%"
                ></div>
              </div>
              <span class="font-mono text-zinc-500">{pct}%</span>
              <span class="font-mono text-zinc-600">{fmtBytes(row.install.bytes)}</span>
            {:else if row.install.phase === 'downloading'}
              <div class="h-1.5 flex-1 animate-pulse rounded-full bg-zinc-800"></div>
              <span class="font-mono text-zinc-600">{fmtBytes(row.install.bytes)}</span>
            {:else if row.install.phase === 'error'}
              <span class="text-red-400">{row.install.message}</span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="rounded-lg border border-zinc-800 bg-zinc-900/60 p-4">
    <button
      type="button"
      onclick={pingBackend}
      class="rounded-md bg-brand-600 px-3 py-1.5 text-sm text-white hover:bg-brand-500"
    >
      ping backend
    </button>
    {#if reply}
      <span class="ml-3 font-mono text-sm text-emerald-400">{reply}</span>
    {/if}
  </div>
</section>
