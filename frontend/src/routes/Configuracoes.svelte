<script lang="ts">
  let http = $state(80);
  let mariadb = $state(3306);
  let phpFcgi = $state(9000);
</script>

<section class="space-y-6">
  <header>
    <h2 class="text-2xl font-semibold">Configurações</h2>
    <p class="text-sm text-zinc-400">Ajuste portas e preferências gerais.</p>
  </header>

  <div class="grid max-w-md grid-cols-2 gap-4">
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-zinc-400">Porta HTTP</span>
      <input
        type="number"
        bind:value={http}
        min="1"
        max="65535"
        class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
      />
    </label>
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-zinc-400">Porta MariaDB</span>
      <input
        type="number"
        bind:value={mariadb}
        min="1"
        max="65535"
        class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
      />
    </label>
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-zinc-400">Porta PHP FastCGI</span>
      <input
        type="number"
        bind:value={phpFcgi}
        min="1"
        max="65535"
        class="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2"
      />
    </label>
  </div>
</section>
