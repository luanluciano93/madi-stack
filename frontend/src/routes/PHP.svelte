<script lang="ts">
  import StatusLed from '$lib/components/StatusLed.svelte';
  let status = $state<'stopped' | 'running'>('stopped');
</script>

<section class="space-y-6">
  <header class="flex items-center gap-3">
    <StatusLed {status} />
    <h2 class="text-2xl font-semibold">PHP (FastCGI)</h2>
  </header>

  <button
    type="button"
    class="rounded-md bg-emerald-600 px-6 py-3 font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
    disabled
  >
    Iniciar
  </button>

  <p class="text-sm text-zinc-500">Sprint 1 — supervisor ainda não implementado.</p>
</section>
