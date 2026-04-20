# MadiStack — Arquitetura

Documento complementar a [PLANNING.md](../PLANNING.md). Aqui entram detalhes técnicos que não cabem no planejamento de alto nível.

## Workspace

```
madi-stack/
├── src-tauri/        # binário GUI (Tauri v2)
├── crates/
│   ├── core/          # tipos compartilhados
│   ├── sources/       # clients das APIs de versão
│   ├── downloader/    # download + SHA256 + unzip
│   ├── config-gen/    # render de templates Tera
│   ├── state-store/   # madistack.toml
│   ├── services/      # supervisor de processos (JobObject)
│   ├── firewall/      # netsh/INetFwPolicy2 via windows-rs
│   ├── updater/       # diff + swap atômico + rollback
│   └── logs/          # tail + ring buffer
├── frontend/          # Svelte 5 + TS + Tailwind v4
└── templates/         # *.tera
```

## Convenções de `unsafe`

- `crates/core`, `sources`, `downloader`, `config-gen`, `state-store`, `updater`, `logs`: `#![forbid(unsafe_code)]`.
- `crates/services`: `unsafe` permitido exclusivamente para Windows Job Objects.
- `crates/firewall`: `unsafe` permitido exclusivamente para COM / `windows-rs`.
- Todo bloco `unsafe` deve ter um comentário `// SAFETY:` explicando a invariante preservada.

## Ciclo de vida dos processos gerenciados

```
GUI start click
    ↓
services::start(component)
    ↓
tokio::process::Command::spawn  (CREATE_SUSPENDED)
    ↓
AssignProcessToJobObject  (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
    ↓
ResumeThread
    ↓
[run]
    ↓
services::stop(handle) → graceful shutdown específico
    ↓ (timeout)
TerminateProcess fallback
```

Se a GUI crashar sem chamar `stop`, o kernel termina os filhos automaticamente quando o Job Object fecha.

## IPC frontend ↔ backend

- **Comandos** (request/response): `#[tauri::command]` em `src-tauri/src/commands.rs`, wrappers em `frontend/src/lib/ipc.ts`.
- **Eventos** (push do backend): `app.emit("log:nginx", &line)`, `listen("log:nginx", ...)` no frontend. Usado para logs ao vivo, progresso de download, status changes.

## Cache de versões (sources)

O módulo `sources` faz cache em memória do último manifesto buscado por 1 hora. Sem cache em disco — se a GUI reabre, refaz a consulta (é barato).
