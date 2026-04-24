<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { _, locale } from 'svelte-i18n';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import ServicePage from '$lib/components/ServicePage.svelte';
  import {
    ipc,
    onBackupProgress,
    onServiceStatus,
    type BackupInfo,
    type BackupProgressEvent,
    type ServiceStatus,
  } from '$lib/ipc';

  let status = $state<ServiceStatus>('stopped');
  let databases = $state<string[]>([]);
  let backups = $state<BackupInfo[]>([]);
  let selectedDb = $state<string>('');
  /// Absolute path to `{install_dir}/data/backups/`. Resolved once on
  /// mount and reused by the "Open folder" buttons — avoids a round-trip
  /// to the backend for every row click.
  let backupsDir = $state<string>('');

  /// Set while a dump is running — disables buttons and shows a progress
  /// line with bytes written. Reset on the `done` or `error` phase event.
  let progress = $state<{ database: string; bytes: number } | null>(null);
  let error = $state<string | null>(null);
  let loadingDbs = $state(false);
  let deletingFilename = $state<string | null>(null);

  let unlistenStatus: UnlistenFn | null = null;
  let unlistenProgress: UnlistenFn | null = null;

  async function refreshBackups() {
    try {
      backups = await ipc.mariadbListBackups();
    } catch (e) {
      error = String(e);
    }
  }

  /// Only runs when MariaDB is up — listing databases requires a live
  /// connection. Safe to call when stopped (returns an error we swallow).
  async function refreshDatabases() {
    if (status !== 'running') {
      databases = [];
      return;
    }
    loadingDbs = true;
    try {
      databases = await ipc.mariadbListDatabases();
      if (!selectedDb && databases.length > 0) {
        selectedDb = databases[0];
      }
      if (!databases.includes(selectedDb)) {
        selectedDb = databases[0] ?? '';
      }
    } catch (e) {
      // A transient connection error right after start is common — don't
      // blow the whole backups section up for it. Log silently and let the
      // user retry via the refresh button.
      databases = [];
      console.warn('list databases failed', e);
    } finally {
      loadingDbs = false;
    }
  }

  async function runBackup() {
    if (!selectedDb || progress) return;
    error = null;
    progress = { database: selectedDb, bytes: 0 };
    try {
      await ipc.mariadbBackup(selectedDb);
      await refreshBackups();
    } catch (e) {
      error = String(e);
    } finally {
      progress = null;
    }
  }

  async function deleteBackup(filename: string) {
    if (deletingFilename) return;
    // Native confirm dialog is fine here — this is the only destructive
    // action in the backups UI and a second click is a real safeguard.
    if (!window.confirm($_('mariadb.backup_delete_confirm', { values: { filename } }))) {
      return;
    }
    deletingFilename = filename;
    try {
      await ipc.mariadbDeleteBackup(filename);
      await refreshBackups();
    } catch (e) {
      error = String(e);
    } finally {
      deletingFilename = null;
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  /// Format using the active i18n locale so dates read naturally in the
  /// language the user selected. Falls back to ISO if the locale is not
  /// yet initialized.
  function formatDate(secs: number, loc: string | null | undefined): string {
    const d = new Date(secs * 1000);
    try {
      return d.toLocaleString(loc ?? undefined);
    } catch {
      return d.toISOString();
    }
  }

  async function openBackupsFolder() {
    if (!backupsDir) return;
    try {
      await ipc.openPath(backupsDir);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    try {
      status = await ipc.serviceStatus('mariadb');
    } catch {
      status = 'stopped';
    }
    try {
      // Windows accepts both slash flavors, but native tooling (Explorer)
      // is happiest with backslashes — match the OS convention.
      const root = await ipc.installDir();
      backupsDir = `${root}\\data\\backups`;
    } catch {
      backupsDir = '';
    }
    unlistenStatus = await onServiceStatus((e) => {
      if (e.slug !== 'mariadb') return;
      const prev = status;
      status = e.status;
      // Just came up — the DB list becomes meaningful; just went down —
      // drop the stale list rather than show names we can't back up.
      if (prev !== 'running' && status === 'running') {
        void refreshDatabases();
      }
      if (prev === 'running' && status !== 'running') {
        databases = [];
      }
    });
    unlistenProgress = await onBackupProgress((e: BackupProgressEvent) => {
      if (e.phase === 'running' && typeof e.bytes === 'number') {
        progress = { database: e.database, bytes: e.bytes };
      } else if (e.phase === 'error') {
        error = e.message ?? 'backup failed';
      }
      // `done` is implicit — the awaited `ipc.mariadbBackup` call returns
      // and clears `progress` in `runBackup`'s finally block.
    });
    await Promise.all([refreshBackups(), refreshDatabases()]);
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenProgress?.();
  });
</script>

<ServicePage slug="mariadb" title="MariaDB" />

<section class="mt-8 space-y-4">
  <header>
    <h3 class="text-lg font-semibold">{$_('mariadb.backups_title')}</h3>
    <p class="text-sm text-zinc-400">{$_('mariadb.backups_subtitle')}</p>
  </header>

  {#if status !== 'running'}
    <p class="rounded-md border border-amber-600/30 bg-amber-600/10 p-3 text-sm text-amber-300">
      {$_('mariadb.backup_needs_running')}
    </p>
  {:else}
    <div class="flex flex-wrap items-end gap-3">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('mariadb.backup_select_db')}</span>
        <select
          bind:value={selectedDb}
          disabled={loadingDbs || progress !== null || databases.length === 0}
          class="min-w-56 rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 disabled:opacity-40"
        >
          {#if databases.length === 0}
            <option value="">{$_('mariadb.backup_no_dbs')}</option>
          {:else}
            {#each databases as db (db)}
              <option value={db}>{db}</option>
            {/each}
          {/if}
        </select>
      </label>
      <button
        type="button"
        onclick={runBackup}
        disabled={!selectedDb || progress !== null}
        class="rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
      >
        {progress ? $_('mariadb.backing_up') : $_('mariadb.backup_now')}
      </button>
      <button
        type="button"
        onclick={() => void refreshDatabases()}
        disabled={loadingDbs || progress !== null}
        class="rounded-md border border-zinc-700 px-3 py-2 text-sm text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
      >
        {$_('actions.refresh')}
      </button>
    </div>
  {/if}

  {#if progress}
    <p class="text-sm text-zinc-400">
      {$_('mariadb.backup_progress_line', {
        values: { database: progress.database, size: formatSize(progress.bytes) },
      })}
    </p>
  {/if}

  {#if error}
    <p
      class="whitespace-pre-line rounded-md border border-red-600/30 bg-red-600/10 p-3 text-sm text-red-300"
    >
      {error}
    </p>
  {/if}

  <div class="space-y-2">
    <div class="flex items-center justify-between">
      <h4 class="text-sm font-medium text-zinc-300">{$_('mariadb.backups_list_title')}</h4>
      <button
        type="button"
        onclick={() => void refreshBackups()}
        class="text-xs text-zinc-400 hover:text-zinc-200"
      >
        {$_('actions.refresh')}
      </button>
    </div>

    {#if backups.length === 0}
      <p class="text-sm text-zinc-500">{$_('mariadb.backups_empty')}</p>
    {:else}
      <ul class="divide-y divide-zinc-800 rounded-md border border-zinc-800">
        {#each backups as b (b.filename)}
          <li class="flex items-center justify-between gap-3 px-3 py-2 text-sm">
            <div class="min-w-0 flex-1">
              <div class="truncate font-mono text-xs text-zinc-300">{b.filename}</div>
              <div class="mt-0.5 text-xs text-zinc-500">
                <span class="text-zinc-400">{b.database}</span>
                · {formatDate(b.created_at_secs, $locale)}
                · {formatSize(b.size_bytes)}
              </div>
            </div>
            <div class="flex shrink-0 gap-2">
              <button
                type="button"
                onclick={() => void openBackupsFolder()}
                disabled={!backupsDir}
                title={backupsDir}
                class="rounded-md border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
              >
                {$_('mariadb.open_folder')}
              </button>
              <button
                type="button"
                onclick={() => void deleteBackup(b.filename)}
                disabled={deletingFilename === b.filename}
                class="rounded-md border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
              >
                {deletingFilename === b.filename
                  ? $_('mariadb.backup_deleting')
                  : $_('mariadb.backup_delete')}
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>
