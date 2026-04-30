<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';
  import { check as checkUpdate, type Update } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { appVersion } from '$lib/version';

  // `appVersion` resolves once at app boot from `tauri.conf.json` baked into
  // the binary, so this stays in sync with `Cargo.toml` automatically.
  const repoUrl = 'https://github.com/luanluciano93/madi-stack';
  const authorUrl = 'https://github.com/luanluciano93';

  function openExternal(url: string) {
    void shellOpen(url);
  }

  /// Three UI states:
  /// - `idle`: nothing checked yet or user clicked refresh.
  /// - `checking`: round-trip to the updater endpoint in progress.
  /// - `up-to-date`: check succeeded and we're on the latest.
  /// - `available`: a newer version exists — show version + install button.
  /// - `downloading`: user clicked install, bytes streaming in.
  /// - `error`: anything blew up — signing mismatch, offline, 404 on
  ///   `latest.json`, etc. The raw error is surfaced so we can debug.
  type UpdateUi =
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'up-to-date' }
    | { kind: 'available'; update: Update }
    | { kind: 'downloading'; received: number; total: number | null }
    | { kind: 'error'; message: string };

  let ui = $state<UpdateUi>({ kind: 'idle' });

  async function checkForUpdate() {
    ui = { kind: 'checking' };
    try {
      const update = await checkUpdate();
      ui = update ? { kind: 'available', update } : { kind: 'up-to-date' };
    } catch (e) {
      ui = { kind: 'error', message: String(e) };
    }
  }

  async function installUpdate() {
    if (ui.kind !== 'available') return;
    const update = ui.update;
    ui = { kind: 'downloading', received: 0, total: null };
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          ui = { kind: 'downloading', received: 0, total: event.data.contentLength ?? null };
        } else if (event.event === 'Progress') {
          if (ui.kind === 'downloading') {
            ui = {
              kind: 'downloading',
              received: ui.received + event.data.chunkLength,
              total: ui.total,
            };
          }
        }
      });
      // On Windows the installer relaunches us automatically after the
      // passive install; relaunch() is a belt-and-suspenders for the
      // rare case where the installer exits without triggering it.
      await relaunch();
    } catch (e) {
      ui = { kind: 'error', message: String(e) };
    }
  }

  function fmtMb(bytes: number): string {
    return (bytes / 1024 / 1024).toFixed(1);
  }
</script>

<section class="space-y-4">
  <header>
    <h2 class="text-2xl font-semibold">{$_('about.title')}</h2>
  </header>

  <p
    class="max-w-xl text-sm leading-relaxed text-zinc-300 [&_code]:rounded [&_code]:bg-zinc-800 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-xs [&_code]:text-zinc-200"
  >
    <strong>MadiStack</strong>
    {@html $_('about.description')}
  </p>

  <dl class="grid max-w-md grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-sm">
    <dt class="text-zinc-500">{$_('about.version_label')}</dt>
    <dd class="font-mono">{$appVersion}</dd>
    <dt class="text-zinc-500">{$_('about.license')}</dt>
    <dd>MIT</dd>
    <dt class="text-zinc-500">{$_('about.author')}</dt>
    <dd>
      <button
        type="button"
        onclick={() => openExternal(authorUrl)}
        class="underline underline-offset-2 hover:text-brand-400"
      >
        luanluciano93
      </button>
    </dd>
    <dt class="text-zinc-500">{$_('about.repository')}</dt>
    <dd>
      <button
        type="button"
        onclick={() => openExternal(repoUrl)}
        class="font-mono underline underline-offset-2 hover:text-brand-400"
      >
        github.com/luanluciano93/madi-stack
      </button>
    </dd>
  </dl>

  <!-- Updater: one-click check against the release feed. The install
       flow is passive (no UAC dance) because our NSIS target is signed
       per-release with the same key baked into tauri.conf.json. -->
  <div class="max-w-md space-y-2 rounded-md border border-zinc-800 bg-zinc-900/60 p-3">
    <div class="flex items-center justify-between gap-3">
      <span class="text-sm font-medium">{$_('about.update_title')}</span>
      <button
        type="button"
        onclick={checkForUpdate}
        disabled={ui.kind === 'checking' || ui.kind === 'downloading'}
        class="rounded-md bg-brand-600 px-3 py-1 text-xs font-medium text-white hover:bg-brand-500 disabled:opacity-40"
      >
        {ui.kind === 'checking' ? $_('about.update_checking') : $_('about.update_check')}
      </button>
    </div>

    {#if ui.kind === 'idle'}
      <p class="text-xs text-zinc-500">{$_('about.update_idle_hint')}</p>
    {:else if ui.kind === 'up-to-date'}
      <p class="text-xs text-emerald-400">{$_('about.update_up_to_date')}</p>
    {:else if ui.kind === 'available'}
      <p class="text-xs text-amber-300">
        {$_('about.update_available', { values: { version: ui.update.version } })}
      </p>
      {#if ui.update.body}
        <pre
          class="max-h-32 overflow-y-auto rounded bg-zinc-950 p-2 font-mono text-xs text-zinc-300">{ui
            .update.body}</pre>
      {/if}
      <button
        type="button"
        onclick={installUpdate}
        class="rounded-md bg-brand-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-brand-500"
      >
        {$_('about.update_install')}
      </button>
    {:else if ui.kind === 'downloading'}
      <p class="text-xs text-zinc-300">
        {#if ui.total !== null}
          {$_('about.update_downloading_sized', {
            values: { received: fmtMb(ui.received), total: fmtMb(ui.total) },
          })}
        {:else}
          {$_('about.update_downloading')}
        {/if}
      </p>
    {:else if ui.kind === 'error'}
      <p class="text-xs text-red-400">{ui.message}</p>
    {/if}
  </div>

  <p class="pt-2 text-xs text-zinc-500">
    {$_('about.made_in')}
  </p>
</section>
