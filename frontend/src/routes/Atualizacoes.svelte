<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { _ } from 'svelte-i18n';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import {
    ipc,
    onUpdateProgress,
    type ComponentSlug,
    type UpdatePhase,
    type UpdateStatusDto,
  } from '$lib/ipc';

  type Progress = {
    phase: UpdatePhase | 'idle';
    bytes?: number;
    total?: number;
    message?: string;
  };

  type Row = UpdateStatusDto & { progress: Progress };

  const DISPLAY_NAME: Record<ComponentSlug, string> = {
    nginx: 'Nginx',
    php: 'PHP',
    mariadb: 'MariaDB',
    phpmyadmin: 'phpMyAdmin',
  };

  let rows = $state<Row[]>([]);
  let checking = $state(false);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | null = null;

  async function check() {
    checking = true;
    error = null;
    try {
      const list = await ipc.updaterCheck();
      rows = list.map((s) => ({ ...s, progress: { phase: 'idle' } }));
    } catch (e) {
      error = String(e);
    } finally {
      checking = false;
    }
  }

  async function apply(i: number) {
    const slug = rows[i].slug;
    rows[i].progress = { phase: 'downloading' };
    try {
      const newVersion = await ipc.updaterApply(slug);
      rows[i].current = newVersion;
      rows[i].update_available = false;
    } catch (e) {
      rows[i].progress = { phase: 'error', message: String(e) };
    }
  }

  async function rollback(i: number) {
    const slug = rows[i].slug;
    try {
      await ipc.updaterRollback(slug);
      await check();
    } catch (e) {
      rows[i].progress = { phase: 'error', message: String(e) };
    }
  }

  function applyEvent(slug: ComponentSlug, next: Progress) {
    const i = rows.findIndex((r) => r.slug === slug);
    if (i >= 0) rows[i].progress = next;
  }

  function percent(r: Row): number | null {
    const { bytes, total } = r.progress;
    if (!total || !bytes) return null;
    return Math.min(100, Math.round((bytes / total) * 100));
  }

  function fmtBytes(n: number | undefined): string {
    if (!n) return '';
    const mib = n / (1024 * 1024);
    return mib >= 1 ? `${mib.toFixed(1)} MiB` : `${(n / 1024).toFixed(0)} KiB`;
  }

  function phaseLabel(p: UpdatePhase | 'idle'): string {
    switch (p) {
      case 'downloading':
        return 'baixando';
      case 'verifying':
        return 'verificando SHA256…';
      case 'extracting':
        return 'extraindo e trocando…';
      case 'done':
        return 'atualizado';
      case 'error':
        return 'erro';
      default:
        return '';
    }
  }

  onMount(async () => {
    unlisten = await onUpdateProgress((evt) =>
      applyEvent(evt.slug, {
        phase: evt.phase,
        bytes: evt.bytes,
        total: evt.total,
        message: evt.message,
      }),
    );
    await check();
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<section class="space-y-4">
  <header class="flex items-start justify-between gap-3">
    <div>
      <h2 class="text-2xl font-semibold">{$_('updates.title')}</h2>
      <p class="text-sm text-zinc-400">{$_('updates.subtitle')}</p>
    </div>
    <button
      type="button"
      onclick={check}
      disabled={checking}
      class="shrink-0 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
    >
      {checking ? $_('actions.updating') : $_('actions.check_for_updates')}
    </button>
  </header>

  {#if error}
    <p class="text-sm text-red-400">{error}</p>
  {/if}

  {#if checking && rows.length === 0}
    <div class="flex min-h-[240px] flex-col items-center justify-center gap-3 text-zinc-400">
      <span
        class="inline-block h-8 w-8 animate-spin rounded-full border-2 border-zinc-700 border-t-brand-500"
        aria-hidden="true"
      ></span>
      <p class="text-sm">{$_('common.loading')}</p>
    </div>
  {:else if rows.length === 0 && !error}
    <p class="text-sm text-zinc-500">{$_('updates.empty')}</p>
  {/if}

  <div class="space-y-2">
    {#each rows as row, i (row.slug)}
      {@const inFlight =
        row.progress.phase !== 'idle' &&
        row.progress.phase !== 'done' &&
        row.progress.phase !== 'error'}
      {@const pct = percent(row)}
      <div
        class="flex flex-col gap-2 rounded-lg border border-zinc-800 bg-zinc-900/60 p-3"
      >
        <div class="flex items-center gap-3">
          <div class="min-w-0 flex-1">
            <div class="font-medium">{DISPLAY_NAME[row.slug]}</div>
            <div class="text-xs text-zinc-500">
              {$_('updates.installed_label')}:
              {#if row.current}
                <span class="font-mono text-zinc-300">{row.current}</span>
              {:else if row.installed_on_disk}
                <span class="font-mono text-amber-400" title={$_('updates.unknown_version_tooltip')}>{$_('common.unknown_version')}</span>
              {:else}
                <span class="font-mono text-zinc-500">—</span>
              {/if}
              · {$_('updates.available_label')}:
              <span
                class="font-mono {row.update_available
                  ? 'text-emerald-400'
                  : 'text-zinc-400'}"
              >
                {row.available}
              </span>
            </div>
          </div>
          {#if row.update_available}
            <button
              type="button"
              disabled={inFlight}
              onclick={() => apply(i)}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-sm text-white hover:bg-brand-500 disabled:opacity-40"
            >
              {inFlight ? $_('actions.updating') : $_('actions.update')}
            </button>
          {:else if row.current}
            <span class="text-xs text-zinc-500">{$_('common.up_to_date')}</span>
          {:else if row.installed_on_disk}
            <button
              type="button"
              disabled={inFlight}
              onclick={() => apply(i)}
              class="rounded-md border border-amber-500/60 px-3 py-1.5 text-sm text-amber-300 hover:bg-amber-500/10 disabled:opacity-40"
              title={$_('updates.sync_tooltip')}
            >
              {inFlight ? $_('updates.syncing') : $_('actions.sync_version')}
            </button>
          {:else}
            <span class="text-xs text-zinc-500">{$_('common.not_installed')}</span>
          {/if}
          {#if row.current}
            <button
              type="button"
              onclick={() => rollback(i)}
              class="rounded-md border border-zinc-800 px-2.5 py-1.5 text-xs text-zinc-400 hover:bg-zinc-800"
              title={$_('updates.rollback_tooltip', { values: { slug: row.slug } })}
            >
              {$_('actions.rollback')}
            </button>
          {/if}
        </div>
        {#if inFlight || (row.progress.phase === 'error' && row.progress.message)}
          <div class="flex items-center gap-2 text-xs text-zinc-400">
            <span class="min-w-[9rem]">{phaseLabel(row.progress.phase)}</span>
            {#if row.progress.phase === 'downloading' && pct !== null}
              <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-zinc-800">
                <div
                  class="h-full bg-brand-500 transition-all"
                  style="width: {pct}%"
                ></div>
              </div>
              <span class="font-mono text-zinc-500">{pct}%</span>
              <span class="font-mono text-zinc-600">{fmtBytes(row.progress.bytes)}</span>
            {:else if row.progress.phase === 'downloading'}
              <div class="h-1.5 flex-1 animate-pulse rounded-full bg-zinc-800"></div>
              <span class="font-mono text-zinc-600">{fmtBytes(row.progress.bytes)}</span>
            {:else if row.progress.phase === 'error'}
              <span class="text-red-400">{row.progress.message}</span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</section>
