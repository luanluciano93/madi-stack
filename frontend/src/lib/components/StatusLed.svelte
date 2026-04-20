<script lang="ts">
  let { status }: { status: 'running' | 'starting' | 'stopped' | 'crashed' } = $props();

  const color = $derived.by(() => {
    switch (status) {
      case 'running':
        return 'bg-emerald-500 shadow-emerald-500/50';
      case 'starting':
        return 'bg-amber-400 shadow-amber-400/50 animate-pulse';
      case 'crashed':
        return 'bg-red-500 shadow-red-500/50';
      default:
        return 'bg-zinc-600';
    }
  });
</script>

<span class="inline-block h-2.5 w-2.5 rounded-full shadow-[0_0_10px] {color}"></span>
