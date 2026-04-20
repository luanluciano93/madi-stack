<script lang="ts">
  import { ipc } from '$lib/ipc';

  let reply = $state('');

  async function pingBackend() {
    reply = await ipc.ping();
  }
</script>

<section class="space-y-6">
  <header>
    <h2 class="text-2xl font-semibold">Geral</h2>
    <p class="text-sm text-zinc-400">
      Atalhos rápidos e status da stack.
    </p>
  </header>

  <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
    <button
      type="button"
      class="rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-left transition hover:border-zinc-700 hover:bg-zinc-800"
    >
      <div class="text-xs text-zinc-500">Pasta</div>
      <div class="font-medium">www/</div>
    </button>
    <button
      type="button"
      class="rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-left transition hover:border-zinc-700 hover:bg-zinc-800"
    >
      <div class="text-xs text-zinc-500">Navegador</div>
      <div class="font-medium">localhost</div>
    </button>
    <button
      type="button"
      class="rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-left transition hover:border-zinc-700 hover:bg-zinc-800"
    >
      <div class="text-xs text-zinc-500">Banco</div>
      <div class="font-medium">phpMyAdmin</div>
    </button>
    <button
      type="button"
      class="rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-left transition hover:border-zinc-700 hover:bg-zinc-800"
    >
      <div class="text-xs text-zinc-500">Projeto</div>
      <div class="font-medium">GitHub</div>
    </button>
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
