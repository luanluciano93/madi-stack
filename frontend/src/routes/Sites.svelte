<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';
  import { ipc, type PortConfig, type VhostDto } from '$lib/ipc';

  let sites = $state<VhostDto[]>([]);
  let ports = $state<PortConfig | null>(null);
  let loading = $state(true);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  async function refresh() {
    try {
      const [list, cfg] = await Promise.all([ipc.vhostList(), ipc.getConfig()]);
      sites = list;
      ports = cfg.ports;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /// Resolve the public URL for a vhost. The port suffix is only included
  /// when the corresponding `ports.*` was bumped off the standard (80 for
  /// HTTP, 443 for HTTPS) — usually because something else holds it
  /// (USBWebserver / IIS on 80, WSL `wslrelay` on 443).
  function siteUrl(hostname: string, ssl: boolean): string {
    if (ssl) {
      const httpsPort = ports?.https ?? 443;
      return httpsPort === 443 ? `https://${hostname}/` : `https://${hostname}:${httpsPort}/`;
    }
    const httpPort = ports?.http ?? 80;
    return httpPort === 80 ? `http://${hostname}/` : `http://${hostname}:${httpPort}/`;
  }

  /// Same as `siteUrl` but without the scheme/path — for showing the
  /// authority inline so the user can see the port at a glance.
  function siteAuthority(hostname: string, ssl: boolean): string {
    if (ssl) {
      const httpsPort = ports?.https ?? 443;
      return httpsPort === 443 ? hostname : `${hostname}:${httpsPort}`;
    }
    const httpPort = ports?.http ?? 80;
    return httpPort === 80 ? hostname : `${hostname}:${httpPort}`;
  }

  function flashSuccess(msg: string) {
    success = msg;
    setTimeout(() => {
      if (success === msg) success = null;
    }, 4000);
  }

  /// `enableWithHttps` selecting what protocol the `Ativar` button targets
  /// for each row. Keyed by site name; default `false` (HTTP).
  let httpsChoice = $state<Record<string, boolean>>({});

  async function enable(name: string) {
    busy = name;
    error = null;
    success = null;
    const https = httpsChoice[name] ?? false;
    const t = get(_);
    try {
      await ipc.vhostEnable(name, https);
      flashSuccess(
        t(https ? 'sites.enabled_https_success' : 'sites.enabled_success', {
          values: { hostname: `${name}.test` },
        }),
      );
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }

  async function disable(name: string) {
    busy = name;
    error = null;
    success = null;
    const t = get(_);
    try {
      await ipc.vhostDisable(name);
      flashSuccess(t('sites.disabled_success', { values: { hostname: `${name}.test` } }));
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }

  function openInBrowser(hostname: string, ssl: boolean) {
    // plugin-shell routes http(s):// URLs to the OS default browser —
    // window.open inside the webview would try to navigate Tauri's own
    // WebView2 away from the app UI.
    void shellOpen(siteUrl(hostname, ssl));
  }

  onMount(() => {
    refresh();

    // Pick up folders the user creates in `www/` while the tab is already
    // open. Window focus is good enough — no need for a filesystem watcher.
    const onFocus = () => void refresh();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  });
</script>

<section class="space-y-6">
  <header class="flex items-start justify-between gap-4">
    <div data-tour="https-toggle">
      <h2 class="text-2xl font-semibold">{$_('sites.title')}</h2>
      <p class="text-sm text-zinc-400">{@html $_('sites.subtitle_html')}</p>
    </div>
    <button
      type="button"
      onclick={refresh}
      class="shrink-0 rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-500"
    >
      {$_('actions.refresh')}
    </button>
  </header>

  {#if loading}
    <p class="text-sm text-zinc-500">{$_('common.loading')}</p>
  {:else if sites.length === 0}
    <div class="rounded-md border border-dashed border-zinc-700 p-6 text-sm text-zinc-400">
      {@html $_('sites.empty_html')}
    </div>
  {:else}
    <ul class="space-y-2">
      {#each sites as site (site.name)}
        <li class="flex items-center gap-3 rounded-md border border-zinc-800 bg-zinc-900/50 p-4">
          <span
            class="inline-block h-2 w-2 rounded-full {site.enabled
              ? 'bg-emerald-400'
              : 'bg-zinc-600'}"
            aria-hidden="true"
          ></span>
          <div class="min-w-0 flex-1">
            <div class="font-medium">{site.name}</div>
            <div class="text-xs text-zinc-500">
              <span class="font-mono">{siteAuthority(site.hostname, site.ssl)}</span>
              · {site.enabled
                ? site.ssl
                  ? $_('sites.active_https')
                  : $_('sites.active')
                : $_('sites.inactive')}
            </div>
          </div>
          {#if site.enabled}
            <button
              type="button"
              onclick={() => openInBrowser(site.hostname, site.ssl)}
              class="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800"
              title={siteUrl(site.hostname, site.ssl)}
            >
              localhost
            </button>
            <button
              type="button"
              disabled={busy !== null}
              onclick={() => disable(site.name)}
              class="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
            >
              {busy === site.name ? '…' : $_('actions.disable')}
            </button>
          {:else}
            <label
              class="flex items-center gap-1.5 text-xs text-zinc-400"
              title={$_('sites.https_toggle_title')}
            >
              <input
                type="checkbox"
                checked={httpsChoice[site.name] ?? false}
                onchange={(e) => (httpsChoice[site.name] = e.currentTarget.checked)}
                class="rounded border-zinc-700 bg-zinc-900"
              />
              {$_('sites.https_toggle')}
            </label>
            <button
              type="button"
              disabled={busy !== null}
              onclick={() => enable(site.name)}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-brand-500 disabled:opacity-40"
            >
              {busy === site.name ? $_('sites.activating') : $_('actions.enable')}
            </button>
          {/if}
          <button
            type="button"
            onclick={() => ipc.openPath(site.root_dir)}
            class="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800"
            title={site.root_dir}
          >
            {$_('sites.root_label')} Dir
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if error}
    <p class="text-sm text-red-400">{error}</p>
  {:else if success}
    <p class="text-sm text-emerald-400">{success}</p>
  {/if}
</section>
