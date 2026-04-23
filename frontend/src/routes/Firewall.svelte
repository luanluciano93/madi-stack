<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { ipc, type FirewallRulesStatus } from '$lib/ipc';

  let fwStatus = $state<FirewallRulesStatus | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  async function refresh() {
    try {
      fwStatus = await ipc.firewallRulesStatus();
    } catch (e) {
      error = String(e);
    }
  }

  function flashSuccess(msg: string) {
    success = msg;
    setTimeout(() => {
      if (success === msg) success = null;
    }, 4000);
  }

  async function ensureRules() {
    busy = true;
    error = null;
    success = null;
    try {
      await ipc.firewallEnsureRules();
      await refresh();
      flashSuccess(get(_)('config.firewall_applied'));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function removeRules() {
    busy = true;
    error = null;
    success = null;
    try {
      await ipc.firewallRemoveRules();
      await refresh();
      flashSuccess(get(_)('config.firewall_removed'));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(refresh);
</script>

<section class="max-w-2xl space-y-6">
  <header>
    <h2 class="text-2xl font-semibold">{$_('config.firewall_header')}</h2>
  </header>

  <div
    class="space-y-3 text-sm leading-relaxed text-zinc-400 [&_code]:rounded [&_code]:bg-zinc-800 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-zinc-300 [&_strong]:text-zinc-200"
  >
    {@html $_('config.firewall_desc_html')}
  </div>

  <ul class="space-y-1 text-sm">
    {#each [
      { key: 'nginx', label: 'Nginx' },
      { key: 'mariadb', label: 'MariaDB' },
      { key: 'php_fcgi', label: 'PHP FastCGI' },
    ] as row (row.key)}
      {@const present = fwStatus?.[row.key as keyof FirewallRulesStatus] ?? false}
      <li class="flex items-center gap-2">
        <span
          class="inline-block h-2 w-2 rounded-full {present ? 'bg-brand-500' : 'bg-zinc-600'}"
          aria-hidden="true"
        ></span>
        <span>{row.label}</span>
        <span class="text-xs text-zinc-500">
          {present ? $_('config.firewall_rule_present') : $_('config.firewall_rule_absent')}
        </span>
      </li>
    {/each}
  </ul>

  <div class="flex flex-wrap items-center gap-3">
    <button
      type="button"
      onclick={ensureRules}
      disabled={busy}
      class="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-500 disabled:opacity-40"
    >
      {busy ? $_('config.firewall_applying') : $_('config.firewall_create')}
    </button>
    <button
      type="button"
      onclick={removeRules}
      disabled={busy}
      class="rounded-md border border-zinc-700 px-3 py-1.5 text-sm text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
    >
      {$_('config.firewall_remove')}
    </button>
    {#if error}
      <span class="text-sm text-red-400">{error}</span>
    {:else if success}
      <span class="text-sm text-emerald-400">{success}</span>
    {/if}
  </div>
</section>
