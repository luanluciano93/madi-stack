<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onDestroy, onMount, tick } from 'svelte';
  import { tour, tourActive, tourCurrentStep, tourProgress } from '$lib/tour';

  /// Position of the popover on screen, measured from the anchor. Kept in
  /// viewport coordinates (CSS `position: fixed`) so scroll doesn't drift
  /// the popover away from the element it points at.
  let anchorRect = $state<DOMRect | null>(null);
  let popoverEl = $state<HTMLDivElement | null>(null);

  const POPOVER_GAP = 12; // px between the anchor and the popover.
  const POPOVER_WIDTH = 320; // matches the tailwind class `w-80`.

  /// Locate the anchor element by `data-tour` attribute and measure it.
  /// Called whenever the step changes or the window resizes; an anchor
  /// that's absent (route not yet mounted) leaves `anchorRect = null` so
  /// we render nothing until a later retry finds it.
  function measureAnchor(): void {
    const step = $tourCurrentStep;
    if (!step) {
      anchorRect = null;
      return;
    }
    const el = document.querySelector<HTMLElement>(`[data-tour="${step.anchor}"]`);
    if (!el) {
      anchorRect = null;
      return;
    }
    anchorRect = el.getBoundingClientRect();
  }

  /// Retry-measure for up to ~1s after a step change. Routes are loaded via
  /// `{#if}` blocks in App.svelte, so the target may not be in the DOM on
  /// the first tick after we flip the active tab. We poll every ~60ms
  /// until we find it (or give up silently).
  async function measureWithRetry() {
    for (let attempt = 0; attempt < 16; attempt++) {
      await tick();
      measureAnchor();
      if (anchorRect) return;
      await new Promise((r) => setTimeout(r, 60));
    }
  }

  $effect(() => {
    // Re-measure on every step change so the popover repositions when the
    // tour advances. Reading `$tourCurrentStep` here makes this effect
    // re-run on transitions.
    if ($tourCurrentStep) {
      measureWithRetry();
    } else {
      anchorRect = null;
    }
  });

  function onResize() {
    measureAnchor();
  }

  onMount(() => {
    window.addEventListener('resize', onResize);
    window.addEventListener('scroll', onResize, true);
  });

  onDestroy(() => {
    window.removeEventListener('resize', onResize);
    window.removeEventListener('scroll', onResize, true);
  });

  /// Compute popover placement. Tries to place below the anchor; falls
  /// back to above when there isn't room. Horizontally clamps within the
  /// viewport so the popover never pokes off-screen.
  function placementStyle(rect: DOMRect | null): string {
    if (!rect) return 'display: none;';
    const viewportH = window.innerHeight;
    const viewportW = window.innerWidth;
    const popoverH = popoverEl?.offsetHeight ?? 160;

    const below = rect.bottom + POPOVER_GAP + popoverH <= viewportH;
    const top = below ? rect.bottom + POPOVER_GAP : rect.top - popoverH - POPOVER_GAP;

    let left = rect.left + rect.width / 2 - POPOVER_WIDTH / 2;
    left = Math.max(12, Math.min(left, viewportW - POPOVER_WIDTH - 12));

    return `top: ${Math.max(12, top)}px; left: ${left}px; width: ${POPOVER_WIDTH}px;`;
  }

  function highlightStyle(rect: DOMRect | null): string {
    if (!rect) return 'display: none;';
    const pad = 6;
    return `top: ${rect.top - pad}px; left: ${rect.left - pad}px; width: ${rect.width + pad * 2}px; height: ${rect.height + pad * 2}px;`;
  }
</script>

{#if $tourActive && $tourCurrentStep}
  <!-- Backdrop: dims the rest of the UI and catches clicks outside the
       highlighted element. A transparent "cutout" over the anchor itself
       keeps it visually prominent and still interactive (though the tour
       currently doesn't require clicking through — Next advances). -->
  <div
    class="pointer-events-auto fixed inset-0 z-40 bg-black/60 transition-opacity"
    role="presentation"
    onclick={() => tour.skip()}
    onkeydown={(e) => {
      if (e.key === 'Escape') tour.skip();
    }}
    tabindex="-1"
  ></div>

  {#if anchorRect}
    <!-- Halo around the anchor — a ring-shaped highlight that visually
         lifts the target above the dimmed backdrop. Non-interactive so
         clicks fall through to the backdrop / popover. -->
    <div
      class="pointer-events-none fixed z-40 rounded-lg ring-2 ring-brand-500 ring-offset-2 ring-offset-zinc-950"
      style={highlightStyle(anchorRect)}
    ></div>
  {/if}

  <div
    bind:this={popoverEl}
    class="fixed z-50 rounded-lg border border-zinc-700 bg-zinc-900 p-4 shadow-xl"
    style={placementStyle(anchorRect)}
    role="dialog"
    aria-modal="true"
    aria-labelledby="coachmark-title"
  >
    <h3 id="coachmark-title" class="text-sm font-semibold text-brand-400">
      {$_($tourCurrentStep.titleKey)}
    </h3>
    <p class="mt-1 text-sm text-zinc-300">
      {$_($tourCurrentStep.bodyKey)}
    </p>

    <div class="mt-4 flex items-center justify-between gap-2">
      <span class="text-xs text-zinc-500">
        {$_('tour.progress', {
          values: { current: $tourProgress.current, total: $tourProgress.total },
        })}
      </span>
      <div class="flex gap-2">
        <button
          type="button"
          onclick={() => tour.skip()}
          class="rounded-md border border-zinc-700 px-3 py-1 text-xs text-zinc-300 hover:bg-zinc-800"
        >
          {$_('tour.skip')}
        </button>
        <button
          type="button"
          onclick={() => tour.next()}
          class="rounded-md bg-brand-600 px-3 py-1 text-xs font-medium text-white hover:bg-brand-500"
        >
          {$tourProgress.current < $tourProgress.total ? $_('tour.next') : $_('tour.finish')}
        </button>
      </div>
    </div>
  </div>
{/if}
