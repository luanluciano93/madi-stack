<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { events, lastEvent, type EventLevel } from '$lib/events';
  import {
    onServiceStatus,
    onInstallProgress,
    onUpdateProgress,
    type ComponentSlug,
  } from '$lib/ipc';
  import type { UnlistenFn } from '@tauri-apps/api/event';

  let expanded = $state(false);
  let container = $state<HTMLDivElement | null>(null);
  const COLLAPSED_KEY = 'madistack.events.expanded';

  // Persist the panel's open/closed state so a power user who wants it
  // always visible doesn't have to click every launch.
  try {
    expanded = localStorage.getItem(COLLAPSED_KEY) === '1';
  } catch {
    // storage blocked — stays collapsed by default
  }

  function toggle() {
    expanded = !expanded;
    try {
      localStorage.setItem(COLLAPSED_KEY, expanded ? '1' : '0');
    } catch {
      // storage blocked — change is session-only
    }
  }

  function colorFor(level: EventLevel): string {
    switch (level) {
      case 'error':
        return 'text-red-300';
      case 'warn':
        return 'text-amber-300';
      default:
        return 'text-zinc-300';
    }
  }

  function fmtTs(ms: number): string {
    const d = new Date(ms);
    return d.toLocaleTimeString('pt-BR', { hour12: false });
  }

  function displayName(slug: ComponentSlug): string {
    switch (slug) {
      case 'nginx':
        return 'Nginx';
      case 'mariadb':
        return 'MariaDB';
      case 'php':
        return 'PHP';
      case 'phpmyadmin':
        return 'phpMyAdmin';
      default:
        return slug;
    }
  }

  let unlistens: UnlistenFn[] = [];

  onMount(async () => {
    const lastStatus = new Map<ComponentSlug, string>();
    unlistens.push(
      await onServiceStatus((evt) => {
        // Suppress duplicate status events for the same component — the
        // Rust watcher emits on every poll, we only want transitions.
        if (lastStatus.get(evt.slug) === evt.status) return;
        lastStatus.set(evt.slug, evt.status);

        const name = displayName(evt.slug);
        if (evt.status === 'running') {
          events.push({ level: 'info', source: name, message: 'iniciado' });
        } else if (evt.status === 'stopped') {
          events.push({ level: 'info', source: name, message: 'parado' });
        } else if (evt.status === 'crashed') {
          events.push({ level: 'error', source: name, message: 'caiu' });
        }
      }),
    );

    unlistens.push(
      await onInstallProgress((evt) => {
        if (evt.phase === 'done') {
          events.push({
            level: 'info',
            source: displayName(evt.slug),
            message: `instalado ${evt.message ?? ''}`.trim(),
          });
        } else if (evt.phase === 'error') {
          events.push({
            level: 'error',
            source: displayName(evt.slug),
            message: `instalação falhou: ${evt.message ?? '?'}`,
          });
        }
      }),
    );

    unlistens.push(
      await onUpdateProgress((evt) => {
        if (evt.phase === 'done') {
          events.push({
            level: 'info',
            source: displayName(evt.slug),
            message: `atualizado ${evt.message ?? ''}`.trim(),
          });
        } else if (evt.phase === 'error') {
          events.push({
            level: 'error',
            source: displayName(evt.slug),
            message: `atualização falhou: ${evt.message ?? '?'}`,
          });
        }
      }),
    );
  });

  onDestroy(() => {
    for (const un of unlistens) un();
  });

  // Auto-scroll to the newest line whenever the list grows while the
  // panel is expanded. Ignored when collapsed — less work, no visual effect.
  $effect(() => {
    void $events;
    if (expanded && container) {
      void tick().then(() => {
        if (container) container.scrollTop = container.scrollHeight;
      });
    }
  });
</script>

<aside
  class="shrink-0 border-t border-zinc-800 bg-zinc-900/70 text-xs {expanded ? '' : 'select-none'}"
>
  <button
    type="button"
    onclick={toggle}
    class="flex w-full items-center gap-3 px-4 py-1.5 text-left hover:bg-zinc-800/60"
    title={expanded ? $_('events.collapse') : $_('events.expand')}
  >
    <span class="text-zinc-500">{expanded ? '▾' : '▸'}</span>
    <span class="font-medium text-zinc-300">{$_('events.title')}</span>
    {#if $lastEvent && !expanded}
      <span class="flex-1 truncate {colorFor($lastEvent.level)}">
        {$lastEvent.source ? `[${$lastEvent.source}] ` : ''}{$lastEvent.message}
      </span>
      <span class="shrink-0 text-zinc-500">{fmtTs($lastEvent.ts)}</span>
    {:else if !expanded}
      <span class="flex-1 text-zinc-600">{$_('events.empty')}</span>
    {:else}
      <span class="flex-1 text-right text-zinc-500">{$events.length}</span>
    {/if}
  </button>

  {#if expanded}
    <div
      bind:this={container}
      class="max-h-48 overflow-y-auto border-t border-zinc-800 px-4 py-2 font-mono leading-snug"
    >
      {#if $events.length === 0}
        <p class="text-zinc-600">{$_('events.empty')}</p>
      {/if}
      {#each $events as ev (ev.id)}
        <div class="flex gap-3 {colorFor(ev.level)}">
          <span class="shrink-0 text-zinc-600">{fmtTs(ev.ts)}</span>
          {#if ev.source}
            <span class="shrink-0 text-zinc-500">[{ev.source}]</span>
          {/if}
          <span class="break-all whitespace-pre-wrap">{ev.message}</span>
        </div>
      {/each}
    </div>
  {/if}
</aside>
