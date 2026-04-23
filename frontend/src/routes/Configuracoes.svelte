<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { setLocale, type LocaleCode } from '$lib/i18n';
  import { theme, type Theme } from '$lib/theme';
  import {
    ipc,
    type AppConfigDto,
    type FirewallRulesStatus,
    type PortInspection,
  } from '$lib/ipc';

  // Keep the i18n runtime locale in sync with the persisted pref. The
  // backend still stores the language for backward-compat with callers
  // that haven't migrated to svelte-i18n yet (e.g. dialog texts).
  function applyLocale(value: string) {
    let next: LocaleCode;
    if (value === 'en') next = 'en';
    else if (value === 'es') next = 'es';
    else next = 'pt-BR';
    setLocale(next);
  }

  let config = $state<AppConfigDto | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state<string | null>(null);
  let inspections = $state<Record<string, PortInspection>>({});

  let fwStatus = $state<FirewallRulesStatus | null>(null);
  let fwBusy = $state(false);
  let fwError = $state<string | null>(null);
  let fwSuccess = $state<string | null>(null);

  async function refreshFirewall() {
    try {
      fwStatus = await ipc.firewallRulesStatus();
    } catch (e) {
      fwError = String(e);
    }
  }

  function flashSuccess(msg: string) {
    fwSuccess = msg;
    // Clear the flash after a few seconds so the message doesn't stick
    // around forever — long enough to read, short enough not to clutter.
    setTimeout(() => {
      if (fwSuccess === msg) fwSuccess = null;
    }, 4000);
  }

  async function ensureFirewall() {
    fwBusy = true;
    fwError = null;
    fwSuccess = null;
    try {
      await ipc.firewallEnsureRules();
      await refreshFirewall();
      flashSuccess(get(_)('config.firewall_applied'));
    } catch (e) {
      fwError = String(e);
    } finally {
      fwBusy = false;
    }
  }

  async function removeFirewall() {
    fwBusy = true;
    fwError = null;
    fwSuccess = null;
    try {
      await ipc.firewallRemoveRules();
      await refreshFirewall();
      flashSuccess(get(_)('config.firewall_removed'));
    } catch (e) {
      fwError = String(e);
    } finally {
      fwBusy = false;
    }
  }

  async function refreshPort(key: 'http' | 'mariadb' | 'php_fcgi', value: number) {
    if (!Number.isFinite(value) || value <= 0 || value > 65535) return;
    try {
      inspections[key] = await ipc.portInspect(value);
    } catch {
      // ignore — worst case we just don't show the warning
    }
  }

  async function refreshAllPorts() {
    if (!config) return;
    await Promise.all([
      refreshPort('http', config.ports.http),
      refreshPort('mariadb', config.ports.mariadb),
      refreshPort('php_fcgi', config.ports.php_fcgi),
    ]);
  }

  onMount(async () => {
    try {
      config = await ipc.getConfig();
      await refreshAllPorts();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
    await refreshFirewall();
  });

  async function save() {
    if (!config) return;
    saving = true;
    saved = false;
    error = null;
    try {
      await ipc.saveConfig(config);
      saved = true;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  interface PortWarning {
    text: string;
    /// `self` = we're the occupier (calm tone). `conflict` = someone else is
    /// holding the port and we won't be able to start.
    kind: 'self' | 'conflict';
  }

  function warnFor(key: 'http' | 'mariadb' | 'php_fcgi'): PortWarning | null {
    const ins = inspections[key];
    if (!ins || ins.free) return null;
    const o = ins.occupier;
    // svelte-i18n's `$_` is reactive inside the template but needs the
    // helper `get` when called from a plain function — import lazily.
    const t = get(_);
    if (ins.is_self) {
      return { text: t('config.port_in_use_by_self'), kind: 'self' };
    }
    if (!o) return { text: t('config.port_occupied_unknown'), kind: 'conflict' };
    const name = o.process_name ?? '<?>';
    return {
      text: t('config.port_occupied_by', { values: { pid: o.pid, name } }),
      kind: 'conflict',
    };
  }
</script>

<section class="space-y-6">
  <header>
    <h2 class="text-2xl font-semibold">{$_('config.title')}</h2>
    <p class="text-sm text-zinc-400">{$_('config.subtitle')}</p>
  </header>

  {#if loading}
    <p class="text-sm text-zinc-500">{$_('common.loading')}</p>
  {:else if !config}
    <p class="text-sm text-red-400">{$_('common.error')}: {error}</p>
  {:else}
    <div class="grid max-w-md grid-cols-2 gap-4">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('config.port_http')}</span>
        <input
          type="number"
          bind:value={config.ports.http}
          onchange={() => refreshPort('http', config!.ports.http)}
          min="1"
          max="65535"
          class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        />
        {#if warnFor('http')}
          {@const w = warnFor('http')!}
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}">{w.text}</span>
        {/if}
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('config.port_mariadb')}</span>
        <input
          type="number"
          bind:value={config.ports.mariadb}
          onchange={() => refreshPort('mariadb', config!.ports.mariadb)}
          min="1"
          max="65535"
          class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        />
        {#if warnFor('mariadb')}
          {@const w = warnFor('mariadb')!}
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}">{w.text}</span>
        {/if}
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('config.port_php')}</span>
        <input
          type="number"
          bind:value={config.ports.php_fcgi}
          onchange={() => refreshPort('php_fcgi', config!.ports.php_fcgi)}
          min="1"
          max="65535"
          class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        />
        {#if warnFor('php_fcgi')}
          {@const w = warnFor('php_fcgi')!}
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}">{w.text}</span>
        {/if}
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('config.bind_address')}</span>
        <select
          bind:value={config.ports.bind_address}
          class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        >
          <option value="127.0.0.1">{$_('config.bind_local')}</option>
          <option value="0.0.0.0">{$_('config.bind_any')}</option>
        </select>
      </label>
    </div>

    <fieldset class="max-w-md space-y-2 text-sm">
      <legend class="mb-2 text-zinc-400">{$_('config.preferences')}</legend>
      <label class="flex items-center gap-2">
        <input
          type="checkbox"
          bind:checked={config.prefs.open_browser_on_start}
          class="rounded border-zinc-700 bg-zinc-900"
        />
        {$_('config.open_browser_on_start')}
      </label>
      <label class="flex items-center gap-2">
        <input
          type="checkbox"
          bind:checked={config.prefs.minimize_to_tray_on_start}
          class="rounded border-zinc-700 bg-zinc-900"
        />
        {$_('config.minimize_on_start')}
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-zinc-400">{$_('config.language')}</span>
        <select
          bind:value={config.prefs.language}
          onchange={(e) => applyLocale(e.currentTarget.value)}
          class="w-40 rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        >
          <option value="pt-br">Português (BR)</option>
          <option value="en">English</option>
          <option value="es">Español</option>
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-zinc-400">{$_('config.theme')}</span>
        <select
          value={$theme}
          onchange={(e) => theme.set(e.currentTarget.value as Theme)}
          class="w-40 rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        >
          <option value="dark">{$_('config.theme_dark')}</option>
          <option value="light">{$_('config.theme_light')}</option>
        </select>
      </label>
    </fieldset>

    <div class="flex items-center gap-3">
      <button
        type="button"
        onclick={save}
        disabled={saving}
        class="rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
      >
        {saving ? $_('actions.saving') : $_('actions.save')}
      </button>
      {#if saved}
        <span class="text-sm text-emerald-400">{$_('actions.saved')}</span>
      {/if}
      {#if error}
        <span class="text-sm text-red-400">{error}</span>
      {/if}
    </div>

    <fieldset class="max-w-md space-y-3 text-sm">
      <legend class="mb-2 text-zinc-400">{$_('config.firewall_header')}</legend>
      <p class="text-xs text-zinc-500">{$_('config.firewall_desc')}</p>
      <ul class="space-y-1">
        {#each [
          { key: 'nginx', label: 'Nginx' },
          { key: 'mariadb', label: 'MariaDB' },
          { key: 'php_fcgi', label: 'PHP FastCGI' },
        ] as row (row.key)}
          {@const present = fwStatus?.[row.key as keyof FirewallRulesStatus] ?? false}
          <li class="flex items-center gap-2">
            <span
              class="inline-block h-2 w-2 rounded-full {present
                ? 'bg-emerald-400'
                : 'bg-zinc-600'}"
              aria-hidden="true"
            ></span>
            <span>{row.label}</span>
            <span class="text-xs text-zinc-500">
              {present ? $_('config.firewall_rule_present') : $_('config.firewall_rule_absent')}
            </span>
          </li>
        {/each}
      </ul>
      <div class="flex items-center gap-3">
        <button
          type="button"
          onclick={ensureFirewall}
          disabled={fwBusy}
          class="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
        >
          {fwBusy ? $_('config.firewall_applying') : $_('config.firewall_create')}
        </button>
        <button
          type="button"
          onclick={removeFirewall}
          disabled={fwBusy}
          class="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
        >
          {$_('config.firewall_remove')}
        </button>
        {#if fwError}
          <span class="text-sm text-red-400">{fwError}</span>
        {:else if fwSuccess}
          <span class="text-sm text-emerald-400">{fwSuccess}</span>
        {/if}
      </div>
    </fieldset>
  {/if}
</section>
