<script lang="ts">
  import { onMount } from 'svelte';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';
  import { ipc, type VhostDto } from '$lib/ipc';

  let sites = $state<VhostDto[]>([]);
  let loading = $state(true);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  async function refresh() {
    try {
      sites = await ipc.vhostList();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
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
    try {
      await ipc.vhostEnable(name, https);
      flashSuccess(
        https
          ? `${name}.test ativado com HTTPS`
          : `${name}.test ativado`,
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
    try {
      await ipc.vhostDisable(name);
      flashSuccess(`${name}.test desativado`);
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
    const scheme = ssl ? 'https' : 'http';
    void shellOpen(`${scheme}://${hostname}/`);
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
    <div>
      <h2 class="text-2xl font-semibold">Sites</h2>
      <p class="text-sm text-zinc-400">
        Cada subpasta de <span class="font-mono text-zinc-300">www/</span> vira um site servido em
        <span class="font-mono text-zinc-300">&lt;nome&gt;.test</span>. Ativar edita o arquivo
        <span class="font-mono text-zinc-300">hosts</span> do Windows (pede UAC) e recarrega o nginx.
      </p>
    </div>
    <button
      type="button"
      onclick={refresh}
      class="shrink-0 rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-200 hover:bg-zinc-800"
    >
      Atualizar lista
    </button>
  </header>

  {#if loading}
    <p class="text-sm text-zinc-500">Carregando…</p>
  {:else if sites.length === 0}
    <div class="rounded-md border border-dashed border-zinc-700 p-6 text-sm text-zinc-400">
      Nenhuma pasta em <span class="font-mono text-zinc-200">www/</span> ainda.
      Crie uma pasta como <span class="font-mono text-zinc-200">www/meusite/</span>
      com um <span class="font-mono text-zinc-200">index.php</span> e recarregue esta página.
    </div>
  {:else}
    <ul class="space-y-2">
      {#each sites as site (site.name)}
        <li
          class="flex items-center gap-3 rounded-md border border-zinc-800 bg-zinc-900/50 p-4"
        >
          <span
            class="inline-block h-2 w-2 rounded-full {site.enabled
              ? 'bg-emerald-400'
              : 'bg-zinc-600'}"
            aria-hidden="true"
          ></span>
          <div class="min-w-0 flex-1">
            <div class="font-medium">{site.name}</div>
            <div class="text-xs text-zinc-500">
              <span class="font-mono">{site.hostname}</span>
              · {site.enabled ? (site.ssl ? 'ativo · HTTPS' : 'ativo') : 'inativo'}
            </div>
          </div>
          {#if site.enabled}
            <button
              type="button"
              onclick={() => openInBrowser(site.hostname, site.ssl)}
              class="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800"
            >
              Abrir
            </button>
            <button
              type="button"
              disabled={busy !== null}
              onclick={() => disable(site.name)}
              class="rounded-md border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
            >
              {busy === site.name ? '…' : 'Desativar'}
            </button>
          {:else}
            <label class="flex items-center gap-1.5 text-xs text-zinc-400" title="Gera cert via mkcert na primeira vez (UAC uma só).">
              <input
                type="checkbox"
                checked={httpsChoice[site.name] ?? false}
                onchange={(e) => (httpsChoice[site.name] = e.currentTarget.checked)}
                class="rounded border-zinc-700 bg-zinc-900"
              />
              HTTPS
            </label>
            <button
              type="button"
              disabled={busy !== null}
              onclick={() => enable(site.name)}
              class="rounded-md bg-brand-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-brand-500 disabled:opacity-40"
            >
              {busy === site.name ? 'Ativando…' : 'Ativar'}
            </button>
          {/if}
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
