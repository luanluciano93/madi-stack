<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import {
    ipc,
    onServiceLog,
    type ComponentSlug,
    type LogLine,
    type LogStream,
  } from '$lib/ipc';

  type Props = {
    component: ComponentSlug;
    /** Height of the scrolling area (tailwind class). */
    heightClass?: string;
  };

  let { component, heightClass = 'h-80' }: Props = $props();

  let lines = $state<LogLine[]>([]);
  let follow = $state(true);
  let filter = $state<'all' | LogStream>('all');
  let container: HTMLDivElement | null = null;
  let unlisten: UnlistenFn | null = null;

  const MAX_LINES = 2000;

  let visible = $derived(
    filter === 'all' ? lines : lines.filter((l) => l.stream === filter),
  );

  async function refresh() {
    // Snapshot of the full retained ring. Called on mount AND whenever the
    // component prop changes, so switching tabs loads the right backlog.
    lines = await ipc.serviceLogs(component, 0);
    await scrollIfFollowing();
  }

  async function scrollIfFollowing() {
    if (!follow || !container) return;
    await tick();
    container.scrollTop = container.scrollHeight;
  }

  function pushLine(line: LogLine) {
    // Guard against duplicates: we might receive a live event for a line
    // that was already in the initial snapshot. Sequence numbers make that
    // easy to dedupe.
    if (lines.length > 0 && line.seq <= lines[lines.length - 1].seq) return;
    lines = [...lines, line];
    if (lines.length > MAX_LINES) {
      lines = lines.slice(lines.length - MAX_LINES);
    }
    void scrollIfFollowing();
  }

  function handleScroll() {
    if (!container) return;
    const atBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight < 8;
    follow = atBottom;
  }

  function fmtTs(ms: number): string {
    const d = new Date(ms);
    return d.toLocaleTimeString('pt-BR', { hour12: false }) +
      '.' + String(d.getMilliseconds()).padStart(3, '0');
  }

  // Re-refresh whenever the component prop changes (e.g. user switches tab
  // and the same viewer is reused).
  $effect(() => {
    void component;
    void refresh();
  });

  onMount(async () => {
    unlisten = await onServiceLog((evt) => {
      if (evt.slug === component) pushLine(evt.line);
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<div class="space-y-2">
  <div class="flex items-center gap-3 text-xs">
    <div class="flex gap-1 rounded-md border border-zinc-800 p-0.5">
      {#each ['all', 'stdout', 'stderr'] as f}
        <button
          type="button"
          onclick={() => (filter = f as 'all' | LogStream)}
          class={`rounded px-2 py-0.5 ${
            filter === f
              ? 'bg-zinc-700 text-zinc-50'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          {f}
        </button>
      {/each}
    </div>
    <label class="flex items-center gap-1.5 text-zinc-400">
      <input
        type="checkbox"
        bind:checked={follow}
        class="rounded border-zinc-700 bg-zinc-900"
      />
      auto-scroll
    </label>
    <span class="ml-auto text-zinc-500">{visible.length} linhas</span>
  </div>

  <div
    bind:this={container}
    onscroll={handleScroll}
    class={`overflow-y-auto rounded-md border border-zinc-800 bg-zinc-950 p-2 font-mono text-xs leading-snug ${heightClass}`}
  >
    {#if visible.length === 0}
      <p class="text-zinc-600">(vazio — inicie o serviço para ver logs)</p>
    {/if}
    {#each visible as line (line.seq)}
      <div
        class={`flex gap-2 ${
          line.stream === 'stderr' ? 'text-red-300' : 'text-zinc-300'
        }`}
      >
        <span class="shrink-0 text-zinc-600">{fmtTs(line.ts_ms)}</span>
        <span class="break-all whitespace-pre-wrap">{line.text}</span>
      </div>
    {/each}
  </div>
</div>
