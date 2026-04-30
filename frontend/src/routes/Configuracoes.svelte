<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { AVAILABLE_LOCALES, LOCALE_LABELS, setLocale, type LocaleCode } from '$lib/i18n';
  import { theme, type Theme } from '$lib/theme';
  import { tour } from '$lib/tour';
  import {
    ipc,
    onPasswordReset,
    onServiceStatus,
    type AppConfigDto,
    type Language,
    type MariadbPasswordStatus,
    type PortInspection,
  } from '$lib/ipc';

  // The backend stores `Language` as kebab-case lowercase (`pt-br`, `zh-cn`),
  // while svelte-i18n uses BCP-47 codes (`pt-BR`, `zh-CN`). Map one to the
  // other so the persisted pref and the runtime locale stay in sync.
  const LANG_TO_LOCALE: Record<Language, LocaleCode> = {
    'pt-br': 'pt-BR',
    en: 'en',
    es: 'es',
    nl: 'nl',
    de: 'de',
    it: 'it',
    pl: 'pl',
    ru: 'ru',
    'zh-cn': 'zh-CN',
    tr: 'tr',
    hu: 'hu',
    lv: 'lv',
    ro: 'ro',
  };

  // Keep the i18n runtime locale in sync with the persisted pref. The
  // backend still stores the language for backward-compat with callers
  // that haven't migrated to svelte-i18n yet (e.g. dialog texts).
  function applyLocale(value: string) {
    const next = LANG_TO_LOCALE[value as Language] ?? 'pt-BR';
    setLocale(next);
  }

  /// Entries driving the <select>. Values are the backend-facing Language
  /// codes (lowercase kebab); labels are each language's native name.
  const LANGUAGE_OPTIONS: ReadonlyArray<{ value: Language; label: string }> = AVAILABLE_LOCALES.map(
    (code) => ({
      value: code.toLowerCase() as Language,
      label: LOCALE_LABELS[code],
    }),
  );

  let config = $state<AppConfigDto | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state<string | null>(null);
  let inspections = $state<Record<string, PortInspection>>({});

  let mariadbPassword = $state<string | null>(null);
  let mariadbPasswordRevealed = $state(false);
  let mariadbPasswordCopied = $state(false);

  /// Drift state of the password vs. the running mysqld. `null` while
  /// the first probe is in flight, then one of the discriminated
  /// variants. Only `drift` triggers the re-sync banner.
  let mariadbPasswordStatus = $state<MariadbPasswordStatus | null>(null);
  let resyncInput = $state('');
  let resyncBusy = $state(false);
  let resyncError = $state<string | null>(null);
  /// Set after a successful resync so the banner shows a green
  /// confirmation for ~2s before disappearing as the next probe
  /// flips the status to `in_sync`.
  let resyncSucceeded = $state(false);

  /// State for the "I lost the password" recovery: skip-grant-tables
  /// reset. Lives next to `resync*` because they're sibling fixes for
  /// the same drift condition — but they can't share spinners since
  /// either may run while the other's button stays clickable.
  let resetBusy = $state(false);
  let resetError = $state<string | null>(null);
  let resetSucceeded = $state(false);

  async function refreshMariadbPassword() {
    try {
      mariadbPassword = await ipc.mariadbRootPassword();
    } catch {
      // keep previous value — surfacing an error here would be noise
      mariadbPassword = null;
    }
  }

  async function refreshMariadbPasswordStatus() {
    try {
      mariadbPasswordStatus = await ipc.mariadbPasswordCheck();
    } catch {
      // best-effort — the banner just stays hidden
      mariadbPasswordStatus = { status: 'probe_error' };
    }
  }

  async function resyncMariadbPassword() {
    if (resyncBusy) return;
    resyncBusy = true;
    resyncError = null;
    resyncSucceeded = false;
    const t = get(_);
    try {
      await ipc.mariadbPasswordSave(resyncInput);
      resyncSucceeded = true;
      resyncInput = '';
      // Refresh both: the displayed (masked) password value and the
      // probe state. The latter flipping to `in_sync` is what hides
      // the banner.
      await Promise.all([refreshMariadbPassword(), refreshMariadbPasswordStatus()]);
    } catch (e) {
      // Backend returns stable keys (`access_denied`, `unreachable`,
      // `empty_password`, `probe_error`, `read_secrets:...`,
      // `write_secrets:...`). Map the known ones; fall through to the
      // raw error otherwise so we can debug unexpected branches.
      const raw = String(e);
      if (raw.includes('access_denied')) {
        resyncError = t('config.mariadb_root_password.resync_error_access_denied');
      } else if (raw.includes('unreachable')) {
        resyncError = t('config.mariadb_root_password.resync_error_unreachable');
      } else if (raw.includes('empty_password')) {
        resyncError = t('config.mariadb_root_password.resync_error_empty');
      } else {
        resyncError = raw;
      }
    } finally {
      resyncBusy = false;
    }
  }

  /// 24-char alphanumeric, matching `madi_services::secrets::generate_password`.
  /// Generated client-side so the user gets a strong default without
  /// having to invent one mid-recovery; they can still copy it from
  /// "Revelar senha" once the reset finishes.
  function generateRandomPassword(): string {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    const buf = new Uint32Array(24);
    crypto.getRandomValues(buf);
    return Array.from(buf, (n) => chars[n % chars.length]).join('');
  }

  async function resetMariadbPassword() {
    if (resetBusy) return;
    const t = get(_);
    if (!confirm(t('config.mariadb_root_password.reset_confirm'))) return;
    resetBusy = true;
    resetError = null;
    resetSucceeded = false;
    try {
      await ipc.mariadbPasswordReset(generateRandomPassword());
      resetSucceeded = true;
      // Refresh both: the freshly-set password value is now what
      // `mariadb_root_password` returns, and the probe should flip to
      // `in_sync`, hiding the banner.
      await Promise.all([refreshMariadbPassword(), refreshMariadbPasswordStatus()]);
    } catch (e) {
      const raw = String(e);
      // The Rust side returns stable string keys (see
      // `mariadb_password_reset` docs). Map the few we want to localise;
      // anything else falls through as the raw error so unexpected
      // branches stay debuggable.
      if (raw.includes('binary_missing')) {
        resetError = t('config.mariadb_root_password.reset_error_binary_missing');
      } else if (raw.includes('skip_grant_boot_timeout')) {
        resetError = t('config.mariadb_root_password.reset_error_boot_timeout');
      } else if (raw.includes('alter_failed')) {
        resetError = t('config.mariadb_root_password.reset_error_alter_failed');
      } else {
        resetError = raw;
      }
    } finally {
      resetBusy = false;
    }
  }

  async function copyMariadbPassword() {
    if (!mariadbPassword) return;
    try {
      await navigator.clipboard.writeText(mariadbPassword);
      mariadbPasswordCopied = true;
      setTimeout(() => (mariadbPasswordCopied = false), 1500);
    } catch {
      // clipboard blocked — user can still reveal and select manually
    }
  }

  type PortKey = 'http' | 'https' | 'mariadb' | 'php_fcgi';

  async function refreshPort(key: PortKey, value: number) {
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
      refreshPort('https', config.ports.https),
      refreshPort('mariadb', config.ports.mariadb),
      refreshPort('php_fcgi', config.ports.php_fcgi),
    ]);
  }

  /// Tear-down for the `service-status` listener. We re-probe the
  /// password whenever MariaDB transitions to `running` so a user who
  /// changed their password through phpMyAdmin and then restarts the
  /// service sees the banner without having to leave/re-enter the tab.
  let unlistenServiceStatus: (() => void) | null = null;
  /// `mariadb-password-reset` listener. Used so that even when a
  /// concurrent reset runs from somewhere else (or this tab is opened
  /// mid-reset) the banner reflects the result without manual refresh.
  let unlistenPasswordReset: (() => void) | null = null;

  onMount(async () => {
    try {
      config = await ipc.getConfig();
      await refreshAllPorts();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
    await Promise.all([refreshMariadbPassword(), refreshMariadbPasswordStatus()]);

    unlistenServiceStatus = await onServiceStatus((ev) => {
      if (ev.slug === 'mariadb' && ev.status === 'running') {
        // Brief delay so the just-started mysqld is past `--bootstrap`
        // chatter and accepting connections — otherwise the probe
        // races and reports `unreachable`.
        setTimeout(() => void refreshMariadbPasswordStatus(), 800);
      }
    });
    unlistenPasswordReset = await onPasswordReset((ev) => {
      if (ev.phase === 'done') {
        void refreshMariadbPassword();
        void refreshMariadbPasswordStatus();
      }
    });
  });

  onDestroy(() => {
    if (unlistenServiceStatus) {
      unlistenServiceStatus();
      unlistenServiceStatus = null;
    }
    if (unlistenPasswordReset) {
      unlistenPasswordReset();
      unlistenPasswordReset = null;
    }
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

  function warnFor(key: PortKey): PortWarning | null {
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
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}"
            >{w.text}</span
          >
        {/if}
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-400">{$_('config.port_https')}</span>
        <input
          type="number"
          bind:value={config.ports.https}
          onchange={() => refreshPort('https', config!.ports.https)}
          min="1"
          max="65535"
          class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
        />
        {#if warnFor('https')}
          {@const w = warnFor('https')!}
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}"
            >{w.text}</span
          >
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
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}"
            >{w.text}</span
          >
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
          <span class="text-xs {w.kind === 'self' ? 'text-emerald-400' : 'text-amber-400'}"
            >{w.text}</span
          >
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
          {#each LANGUAGE_OPTIONS as opt (opt.value)}
            <option value={opt.value}>{opt.label}</option>
          {/each}
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

    <fieldset class="max-w-md space-y-2 text-sm">
      <legend class="mb-2 text-zinc-400">{$_('config.mariadb_root_password.legend')}</legend>
      {#if mariadbPassword}
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-zinc-400">{$_('config.mariadb_root_password.label')}:</span>
          <code class="rounded bg-zinc-950 px-2 py-0.5 font-mono text-zinc-100">
            {mariadbPasswordRevealed ? mariadbPassword : '••••••••••••'}
          </code>
          <button
            type="button"
            onclick={() => (mariadbPasswordRevealed = !mariadbPasswordRevealed)}
            class="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-0.5 text-xs hover:bg-zinc-800"
          >
            {mariadbPasswordRevealed
              ? $_('config.mariadb_root_password.hide')
              : $_('config.mariadb_root_password.show')}
          </button>
          <button
            type="button"
            onclick={copyMariadbPassword}
            class="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-0.5 text-xs hover:bg-zinc-800"
          >
            {$_('actions.copy')}
          </button>
          {#if mariadbPasswordCopied}
            <span class="text-xs text-emerald-400">{$_('config.mariadb_root_password.copied')}</span
            >
          {/if}
        </div>
        <p class="text-xs text-zinc-500">{$_('config.mariadb_root_password.hint')}</p>
      {:else}
        <p class="text-xs text-zinc-500">{$_('config.mariadb_root_password.not_initialized')}</p>
      {/if}

      <!-- Drift banner: only renders when the live password probe
           reports the secrets value is wrong. Stays hidden in
           `in_sync` / `unreachable` / `no_secret` / `probe_error`. -->
      {#if mariadbPasswordStatus?.status === 'drift'}
        <div class="rounded-md border border-amber-700/60 bg-amber-950/40 p-3 text-amber-100">
          <p class="text-sm font-medium">{$_('config.mariadb_root_password.drift_title')}</p>
          <p class="mt-1 text-xs text-amber-200/80">
            {$_('config.mariadb_root_password.drift_hint')}
          </p>
          <form
            class="mt-2 flex flex-wrap items-center gap-2"
            onsubmit={(e) => {
              e.preventDefault();
              void resyncMariadbPassword();
            }}
          >
            <input
              type="password"
              bind:value={resyncInput}
              placeholder={$_('config.mariadb_root_password.drift_placeholder')}
              class="flex-1 rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1 text-xs text-zinc-100"
              autocomplete="off"
              spellcheck="false"
            />
            <button
              type="submit"
              disabled={resyncBusy || resyncInput.length === 0}
              class="rounded-md bg-brand-600 px-3 py-1 text-xs font-medium text-white hover:bg-brand-500 disabled:opacity-40"
            >
              {resyncBusy
                ? $_('config.mariadb_root_password.drift_saving')
                : $_('config.mariadb_root_password.drift_save')}
            </button>
          </form>
          {#if resyncError}
            <p class="mt-2 text-xs text-red-300">{resyncError}</p>
          {/if}

          <!-- Skip-grant escape hatch: for users who don't have the
               current password and can't fill the input above. Stops
               MariaDB for ~5–10s and ALTERs root to a freshly-generated
               value, then restarts. Confirm dialog gates it because
               open PHP/CLI connections will be dropped. -->
          <div class="mt-3 border-t border-amber-700/40 pt-2">
            <p class="text-xs text-amber-200/80">
              {$_('config.mariadb_root_password.reset_hint')}
            </p>
            <button
              type="button"
              onclick={resetMariadbPassword}
              disabled={resetBusy}
              class="mt-1 rounded-md border border-amber-700 bg-transparent px-3 py-1 text-xs text-amber-200 hover:bg-amber-900/40 disabled:opacity-40"
            >
              {resetBusy
                ? $_('config.mariadb_root_password.resetting')
                : $_('config.mariadb_root_password.reset_via_skip_grant')}
            </button>
            {#if resetError}
              <p class="mt-2 text-xs text-red-300">{resetError}</p>
            {/if}
          </div>
        </div>
      {:else if resyncSucceeded || resetSucceeded}
        <p class="text-xs text-emerald-400">
          {$_('config.mariadb_root_password.drift_success')}
        </p>
      {/if}
    </fieldset>

    <fieldset class="max-w-md space-y-2 text-sm">
      <legend class="mb-2 text-zinc-400">{$_('tour.config_header')}</legend>
      <p class="text-xs text-zinc-500">{$_('tour.config_desc')}</p>
      <button
        type="button"
        onclick={() => tour.restart()}
        class="rounded-md border border-zinc-700 bg-zinc-900 px-3 py-1.5 text-xs text-zinc-200 hover:bg-zinc-800"
      >
        {$_('tour.config_restart')}
      </button>
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
  {/if}
</section>
