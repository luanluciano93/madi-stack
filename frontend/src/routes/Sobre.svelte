<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';

  const version = '0.1.0';
  const repoUrl = 'https://github.com/luanluciano93/MadiStack';
  const authorUrl = 'https://github.com/luanluciano93';

  function openExternal(url: string) {
    void shellOpen(url);
  }
</script>

<section class="space-y-3">
  <header>
    <h2 class="text-2xl font-semibold">{$_('about.title')}</h2>
  </header>

  <p class="max-w-xl text-sm text-zinc-300">
    <strong>MadiStack</strong> — {$_('about.description')}
  </p>

  <dl class="grid max-w-md grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-sm">
    <dt class="text-zinc-500">{$_('about.version_label')}</dt>
    <dd class="font-mono">{version}</dd>
    <dt class="text-zinc-500">{$_('about.license')}</dt>
    <dd>MIT</dd>
    <dt class="text-zinc-500">{$_('about.author')}</dt>
    <dd>
      <button
        type="button"
        onclick={() => openExternal(authorUrl)}
        class="text-brand-400 underline-offset-2 hover:underline"
      >
        luanluciano93
      </button>
    </dd>
    <dt class="text-zinc-500">{$_('about.repository')}</dt>
    <dd>
      <button
        type="button"
        onclick={() => openExternal(repoUrl)}
        class="font-mono text-brand-400 underline-offset-2 hover:underline"
      >
        github.com/luanluciano93/MadiStack
      </button>
    </dd>
  </dl>

  <p class="pt-2 text-xs text-zinc-500">
    {$_('about.made_in')}
  </p>
</section>
