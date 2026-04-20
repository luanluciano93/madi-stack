# MadiStack — Planejamento Técnico

> Alternativa moderna e open-source ao USBWebserver: Nginx + MariaDB + PHP + phpMyAdmin numa GUI portátil para Windows, construída em **Rust + Tauri**.

---

## 1. Visão e escopo

### Objetivo
Entregar um executável único para Windows que baixa, configura e gerencia uma stack web local completa (Nginx + MariaDB + PHP + phpMyAdmin) — sem instalação, sem mexer no registro, sem serviços do Windows, com auto-update de todos os componentes.

### Princípios de produto
- **Portátil de verdade:** tudo em caminhos relativos à pasta do `.exe`. Roda de pen drive.
- **Zero-config no primeiro boot:** abrir o `.exe` → baixa tudo → gera configs → sobe serviços.
- **Componentes oficiais sempre:** nunca forks; sempre os binários publicados pelos projetos upstream.
- **Atualizável:** cada componente com pipeline próprio de checagem de versão e swap atômico.
- **Minimalismo funcional:** se não está no caso de uso do USBWebserver ou do desenvolvedor PHP típico, não entra no MVP.
- **Performance e footprint como diferencial:** binário < 10 MB, RAM idle < 70 MB, cold start < 250 ms.

### Não-objetivos
- Suporte a Linux/macOS (podem vir como plugins na v2+).
- Uso em produção ou exposição pública.
- Apache, Node.js, Python, Ruby como componentes nativos.
- Multi-usuário, autenticação remota, painel web.

---

## 2. Stack técnica

### Escolha: Rust + Tauri + Svelte
Justificativa em camadas:

| Camada | Escolha | Porquê |
|---|---|---|
| Linguagem do core | **Rust 1.75+** (edition 2021) | binário enxuto, sem GC, type-safety, ótimas crates para Windows (`windows-rs`), async maduro (`tokio`) |
| Framework GUI | **Tauri v2** | ~5 MB de overhead, WebView2 nativo, IPC tipada, plugin system maduro |
| Frontend | **Svelte 5 + TypeScript** | reatividade fina-granular, bundle pequeno, excelente DX |
| Estilização | **TailwindCSS v4** | zero runtime CSS, iteração rápida |
| Package manager frontend | **pnpm** | store única, rápido, bom com monorepo |
| Async runtime | **tokio** (multi-thread) | industry standard, integra com reqwest/tonic |
| HTTP | **reqwest** (rustls, não openssl) | sem dependência de OpenSSL, stream de download |
| Serialização | **serde + serde_json** | padrão do ecossistema |
| Logging | **tracing + tracing-subscriber** | estruturado, integra com frontend via events |
| Build/release | **GitHub Actions** + `tauri-action` | cross-build, signing opcional, release automático |

### Decisões de arquitetura Rust

- **`#![forbid(unsafe_code)]`** no core (exceto módulo `firewall` que usa `windows-rs`).
- **Erros:** `thiserror` para erros de domínio (bibliotecas `internal`), `anyhow` no topo (app-level) quando agregando.
- **Async em tudo que faz I/O:** downloads, leitura de logs, IPC. Tarefas de CPU (SHA256) em `tokio::task::spawn_blocking`.
- **Estado compartilhado:** `Arc<RwLock<AppState>>` exposto via `tauri::State`. Writes infrequentes (configuração), reads frequentes (status polling).
- **IPC frontend ↔ backend:** comandos Tauri tipados + eventos (`app.emit()`) para streams (logs ao vivo, progresso de download).

### Dependências principais (Cargo.toml previsto)

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
tauri-plugin-fs = "2"
tauri-plugin-updater = "2"
tauri-plugin-log = "2"

tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["io"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
hex = "0.4"
zip = { version = "2", default-features = false, features = ["deflate"] }
scraper = "0.20"              # parse HTML do nginx.org
semver = "1"
tera = "1"                    # templates para nginx.conf, php.ini, my.ini
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
directories = "5"             # localização da pasta do executável
sysinfo = "0.32"              # checagem de PIDs, portas ocupadas
notify = "7"                  # watch de arquivos (tail de logs)
once_cell = "1"
parking_lot = "0.12"          # Mutex/RwLock mais rápidos que std

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_JobObjects",
    "Win32_Security",
    "Win32_NetworkManagement_WindowsFirewall",
    "Win32_UI_Shell",
]}
```

---

## 3. Arquitetura

### Estrutura de pastas (runtime, do usuário)
```
MadiStack/
├── MadiStack.exe              # GUI Tauri
├── bin/                       # componentes baixados (gitignored no repo)
│   ├── nginx/                 # nginx.exe + conf/ + html/
│   ├── php/                   # php-cgi.exe + ext/ + php.ini
│   ├── mariadb/               # mysqld.exe + share/ + bin/
│   └── phpmyadmin/            # scripts PHP
├── config/                    # configs geradas a partir de templates
│   ├── nginx.conf
│   ├── php.ini
│   ├── my.ini
│   └── sites-enabled/         # virtual hosts
├── data/
│   └── mariadb/               # datadir do MariaDB
├── www/                       # document root do usuário
├── logs/
│   ├── nginx/                 # access.log + error.log
│   ├── php/                   # fastcgi.log
│   └── mariadb/               # mysql-error.log
├── tmp/                       # downloads, extração
└── madistack.toml             # estado persistido: versões, portas, prefs
```

### Estrutura do repositório
```
madi-stack/
├── Cargo.toml                 # workspace
├── Cargo.lock
├── rust-toolchain.toml        # pin da versão
├── src-tauri/                 # binário Tauri (GUI)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/
│   └── src/
│       ├── main.rs            # #![cfg_attr(windows_subsystem = "windows")]
│       ├── commands.rs        # #[tauri::command] handlers
│       ├── events.rs          # emitters tipados
│       ├── tray.rs            # menu da bandeja
│       └── state.rs           # AppState
├── crates/                    # lógica pura, testável sem Tauri
│   ├── core/                  # tipos compartilhados, traits
│   ├── downloader/            # baixa, valida SHA256, extrai zip
│   ├── services/              # supervisor de processos (Child, JobObject)
│   ├── config-gen/            # render de templates Tera
│   ├── state-store/           # leitura/escrita do madistack.toml
│   ├── firewall/              # regras netsh via windows-rs
│   ├── updater/               # diff de versões, swap atômico, rollback
│   ├── sources/               # clients das APIs de versão (nginx, mariadb, php, pma)
│   └── logs/                  # tail de arquivos, ring buffer
├── frontend/                  # Svelte + Vite
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   └── src/
│       ├── app.html
│       ├── main.ts
│       ├── lib/
│       │   ├── components/
│       │   ├── stores/        # status, logs, config
│       │   └── ipc.ts         # wrappers tipados dos comandos Tauri
│       └── routes/
│           ├── Geral.svelte
│           ├── Nginx.svelte
│           ├── MariaDB.svelte
│           ├── PHP.svelte
│           ├── Configuracoes.svelte
│           ├── Atualizacoes.svelte
│           └── Sobre.svelte
├── templates/                 # templates Tera
│   ├── nginx.conf.tera
│   ├── php.ini.tera
│   ├── my.ini.tera
│   └── site-default.conf.tera
├── .github/workflows/
│   ├── ci.yml                 # clippy + test + build
│   └── release.yml            # tag v* → build assinado → release
└── docs/
    ├── architecture.md
    └── troubleshooting.md
```

### Fluxo de processos
```
                ┌────────────────────────┐
                │   MadiStack.exe (GUI)  │
                │        Rust + Tauri    │
                └──────────┬─────────────┘
                           │ supervisiona via tokio::process
              ┌────────────┼─────────────┐
              ▼            ▼             ▼
      ┌──────────┐ ┌──────────────┐ ┌──────────┐
      │ nginx.exe│ │php-cgi.exe   │ │mysqld.exe│
      │   :80    │ │  :9000 FCGI  │ │  :3306   │
      └──────────┘ └──────────────┘ └──────────┘
```

- Cada processo filho é anexado a um **Windows Job Object** com `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` — garante que os 3 morrem se a GUI crashar (nada de processos zumbis).
- stdout/stderr redirecionados para `tokio::io::BufReader` → ring buffer em memória + arquivo em `logs/` + evento Tauri para o frontend.
- Graceful shutdown: `nginx -s quit`, `mysqladmin shutdown`, SIGBREAK em `php-cgi` (fallback: `TerminateProcess`).

### Fluxo de primeira execução
1. Detecta `bin/` vazio ou `madistack.toml` ausente → splash "baixando componentes" com progresso paralelo.
2. Consulta as 4 fontes oficiais (seção 4), resolve URLs + SHA256 da última versão estável.
3. Baixa em paralelo com `reqwest` em streaming + progresso por componente.
4. Valida SHA256 em `spawn_blocking`.
5. Extrai zips para `bin/<componente>/` com `zip` crate.
6. Renderiza templates Tera → `config/*.conf` com portas padrão (80, 9000, 3306).
7. Roda `mariadb-install-db.exe --datadir=../data/mariadb`.
8. Sobe MariaDB temporariamente → define senha root (vazia, configurável) → cria DB `phpmyadmin` → importa `sql/create_tables.sql` do phpMyAdmin.
9. Checa regras de firewall via `windows-rs`; cria se necessário (único UAC prompt).
10. Persiste estado em `madistack.toml`.
11. Cai na aba Geral com serviços parados, aguardando Start.

---

## 4. Fontes oficiais dos componentes

| Componente | Endpoint | Parser | Formato |
|---|---|---|---|
| **Nginx** | `https://nginx.org/en/download.html` | `scraper` → regex `nginx-(\d+\.\d+\.\d+)\.zip` | `.zip` |
| **MariaDB** | `https://downloads.mariadb.org/rest-api/mariadb/` | `serde_json` | `.zip` x64 msvc |
| **PHP** | `https://windows.php.net/downloads/releases/releases.json` | `serde_json` | `.zip` NTS x64 VS17 |
| **phpMyAdmin** | `https://www.phpmyadmin.net/home_page/version.json` | `serde_json` | `.zip` all-languages |

- Sempre validar SHA256 publicado na página/API.
- Cachear manifesto por 1 hora (evita rate limit e hammering).
- Fallback: se a fonte estiver fora, usar última versão conhecida salva em `madistack.toml`.

---

## 5. Roadmap de execução

### Sprint 0 — Fundação (2–3 dias)
- [x] `.gitignore`, `CLAUDE.md`, `PLANNING.md`, `README.md`
- [ ] `cargo init` + workspace + `rust-toolchain.toml`
- [ ] `cargo create-tauri-app` para scaffolding + migração para estrutura definida
- [ ] Configurar `clippy` + `rustfmt` + `cargo-deny` + `cargo-audit`
- [ ] GitHub Actions `ci.yml`: fmt + clippy (`-D warnings`) + test + build Windows
- [ ] Template de PR e issues
- [ ] Licenças de terceiros: gerar `THIRD_PARTY_LICENSES.md` via `cargo-about`

### Sprint 1 — Core funcional sem GUI (1–1,5 semana)
Prova que a stack sobe de ponta a ponta. Testável via binário CLI auxiliar `madistack-cli`.

- [ ] `crates/sources`: clients para as 4 APIs oficiais com retry + cache
- [ ] `crates/downloader`: download em stream com progresso, validação SHA256, extração zip
- [ ] `crates/config-gen`: render Tera dos 3 templates (nginx, php, mariadb) + sites-enabled
- [ ] `crates/state-store`: read/write de `madistack.toml` (crate `toml`)
- [ ] `crates/services`: supervisor com `tokio::process::Command` + Job Objects
- [ ] Start/Stop/Restart individual e coordenado
- [ ] Detecção de porta ocupada via `sysinfo` + `std::net::TcpListener`
- [ ] Graceful shutdown para cada serviço (nginx -s quit, mysqladmin shutdown, kill nos workers php-cgi)
- [ ] Inicialização do MariaDB (`mariadb-install-db`, senha root, DB phpmyadmin)
- [ ] Binário CLI: `download`, `init`, `start`, `stop`, `status`, `logs tail`

### Sprint 2 — GUI Tauri sobre o core (1–1,5 semana)
- [ ] `src-tauri`: comandos expostos para todos os fluxos do CLI
- [ ] Layout base Svelte: sidebar com 6 abas + LED de status reativo
- [ ] Aba Geral: 4 atalhos (pasta `www`, localhost, phpMyAdmin, GitHub)
- [ ] Abas de serviço: botão grande Start/Stop + link de logs + link de config
- [ ] Viewer de logs ao vivo (`tauri::EventLoop` + `app.emit`)
- [ ] Aba Configurações: portas, idioma, "abrir navegador ao iniciar", "minimizar ao iniciar"
- [ ] Validação client-side + server-side de portas
- [ ] Persistência de prefs via Tauri store

### Sprint 3 — Updater, tray, extras (1 semana)
- [ ] `crates/updater`: checar, baixar, validar, swap atômico em `bin/<componente>.new/` → rename → remove old
- [ ] Rollback automático se healthcheck pós-update falhar
- [ ] Aba Atualizações com tabela de versões + botão por componente + "Verificar todos"
- [ ] System tray: ícone + menu contextual (start/stop/abrir)
- [ ] `crates/firewall`: regras Windows Firewall via `INetFwPolicy2` (windows-rs)
- [ ] Gerenciador de virtual hosts (lista subpastas de `www/` → cria `sites-enabled/*.conf` + edita `hosts`)
- [ ] HTTPS local com mkcert embutido (sub-binário ou chamada)

### Sprint 4 — Polish e release (4–6 dias)
- [ ] i18n PT-BR + EN (`svelte-i18n` ou similar)
- [ ] Dark mode (classe Tailwind + toggle)
- [ ] Primeira execução: tutorial com tooltips guiados
- [ ] Instalador opcional NSIS via `tauri build --target nsis`
- [ ] GitHub Actions `release.yml`: tag `v*` → build + assinatura (se tiver cert) → zip portátil + NSIS → release notes auto
- [ ] README com screenshots reais e GIF do fluxo de primeira execução
- [ ] `docs/troubleshooting.md` com os erros comuns

### v2 (futuro)
- Troca de versão PHP lado-a-lado (múltiplos `bin/php-8.3/`, `bin/php-8.4/`).
- Backup/restore MariaDB com interface gráfica.
- Plugins opcionais: Redis, Memcached, Composer, WP-CLI, Xdebug.
- Suporte a Linux e macOS.
- Themes customizáveis.
- Integração com ferramentas de monitoramento (métricas de requests via nginx status).

---

## 6. Convenções de engenharia

### Rust
- `rustfmt` com config default + imports ordenados.
- `clippy::pedantic` ativado no CI, com `allow` justificados.
- Erros tipados por crate com `thiserror`; `anyhow::Result` só na borda (comandos Tauri).
- Zero `unwrap()` ou `expect()` em caminho de produção. Exceções: init de estático com `OnceCell`, testes, código claramente impossível.
- `#[must_use]` em builders e funções puras.
- Módulos de teste inline (`#[cfg(test)] mod tests`) + integration tests em `tests/`.
- Cobertura-alvo de 60% nos crates de lógica pura (`downloader`, `config-gen`, `sources`, `state-store`).

### Svelte/TS
- TypeScript strict.
- Componentes pequenos (~150 linhas) em `frontend/src/lib/components/`.
- Stores em `frontend/src/lib/stores/` — uma store por domínio (`services`, `logs`, `config`).
- IPC centralizada em `frontend/src/lib/ipc.ts` com tipos gerados via `tauri-specta` (futuro) ou `zod` nos contratos.
- Sem CSS custom fora de Tailwind exceto transições específicas.

### Git
- Conventional Commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `ci:`.
- Branches: `main` protegida; features em `feat/*`, fixes em `fix/*`.
- PRs com CI verde obrigatório.

### Logging
- `tracing` com spans por comando Tauri.
- Nível padrão: `info` em release, `debug` em dev.
- Logs estruturados em `logs/madistack.log` com rotação diária.

---

## 7. Riscos e mitigações

| Risco | Impacto | Mitigação |
|---|---|---|
| PHP-FPM não existe no Windows | Alto | Usar `php-cgi.exe -b 127.0.0.1:9000` em pool (padrão da indústria) |
| Usuário baixa PHP TS por engano | Médio | URL hardcoded do NTS VS17 x64 via releases.json; nunca deixar usuário escolher |
| Falta VC++ Redistributable | Alto | Detectar DLLs (`vcruntime140.dll`) na primeira execução; mostrar link oficial; opção de baixar |
| Porta 80 / 3306 ocupada | Alto | Detectar com `TcpListener::bind` + `sysinfo` antes de iniciar; sugerir porta alternativa |
| Firewall bloqueia serviços | Médio | Criar regras via `INetFwPolicy2` na 1ª execução (UAC único) |
| Caminho com acento/espaço | Alto | Testar `C:\Users\João\Área de Trabalho\`; `shell-escape` em todos os args |
| Antivírus quarentena o `.exe` | Médio | Code signing (EV cert ~$200/ano) a partir da v1.0; instruções de whitelisting enquanto isso |
| Update corrompe instalação | Alto | Backup em `bin/<componente>.bak/`; rollback se healthcheck falhar; operação atômica via rename |
| Parse HTML do nginx.org quebra | Baixo | Testes snapshot; fallback para última versão conhecida; issue alerta |
| `tokio::process` + graceful shutdown no Windows | Médio | Usar `windows-rs` `GenerateConsoleCtrlEvent` + fallback `TerminateProcess`; testar em integration tests |
| Tamanho do download inicial (~100 MB) | Médio | Downloads paralelos + retomada em caso de falha; progresso claro |
| WebView2 runtime ausente no Win10 antigo | Baixo | Tauri v2 tem bootstrapper; instalador NSIS oferece download |

---

## 8. Decisões em aberto

- [ ] **Senha root MariaDB padrão:** vazia (USBWebserver-like) vs pedir na 1ª execução. → Proposta: vazia, com aviso amarelo na aba MariaDB.
- [ ] **Distribuição:** só zip portátil vs zip + NSIS. → Proposta: ambos na release (zip primário).
- [ ] **Code signing:** v0.x sem assinatura; avaliar custo para v1.0.
- [ ] **Telemetria:** nenhuma por padrão; opt-in só para erros (sentry?) na v1.0.
- [ ] **Tamanho final do zip:** sem componentes (~15 MB) vs com componentes embutidos (~150 MB). → Proposta: sem componentes (mais portátil, sempre atualizado, menor download).

---

## 9. Métricas de sucesso

| Métrica | Alvo MVP | Alvo v1.0 |
|---|---|---|
| Tamanho do binário `MadiStack.exe` | < 12 MB | < 8 MB |
| RAM idle (GUI aberta, serviços parados) | < 80 MB | < 50 MB |
| RAM idle (GUI fechada, tray ativo) | < 15 MB | < 10 MB |
| Cold start até primeira tela | < 400 ms | < 200 ms |
| Tempo de 1ª execução (download + init) em conexão 20 Mbps | < 3 min | < 2 min |
| Tempo de start/stop por serviço | < 2 s | < 1 s |
| Crash rate em sessões | < 1% | < 0.1% |
| Build CI (PR completo) | < 10 min | < 5 min |

---

## 10. Referências úteis

- [Tauri v2 docs](https://v2.tauri.app/) · [Tauri examples](https://github.com/tauri-apps/tauri/tree/dev/examples)
- [Svelte 5 docs](https://svelte.dev/docs/svelte/overview) · [TailwindCSS v4](https://tailwindcss.com/)
- [windows-rs](https://github.com/microsoft/windows-rs) · [tokio docs](https://tokio.rs/)
- [Nginx on Windows](https://nginx.org/en/docs/windows.html)
- [PHP on Windows FastCGI](https://www.php.net/manual/en/install.windows.iis7.php) (mesmo config serve Nginx)
- [MariaDB on Windows](https://mariadb.com/kb/en/installing-mariadb-windows-zip-packages/)
- [phpMyAdmin setup](https://docs.phpmyadmin.net/en/latest/setup.html)
