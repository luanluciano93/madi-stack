<div align="center">

# 🚀 MadiStack

**Uma alternativa moderna e open-source ao USBWebserver para Windows.**

MadiStack reúne Nginx, MariaDB, PHP e phpMyAdmin em uma GUI portátil — é só escolher as portas, clicar em iniciar e colocar seu site na pasta `www`. Auto-updater integrado mantém todos os componentes sempre atualizados.

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2B-blue.svg)](#)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-dea584.svg)](https://www.rust-lang.org/)
[![Built with Tauri](https://img.shields.io/badge/Built%20with-Tauri-24C8DB.svg)](https://tauri.app/)
[![GitHub release](https://img.shields.io/github/v/release/luanluciano93/MadiStack?include_prereleases)](https://github.com/luanluciano93/MadiStack/releases)

[Download](#-download) · [Funcionalidades](#-funcionalidades) · [Início Rápido](#-início-rápido) · [Screenshots](#-screenshots) · [FAQ](#-faq)

</div>

---

## ✨ Por que MadiStack?

Cansado do **USBWebserver** travado no Apache 2.2 e em versões antigas do PHP? Acha o **XAMPP** pesado demais? O MadiStack entrega uma stack moderna sem complicação:

- 🔄 **Sempre atualizado** — updates com um clique para cada componente, direto das fontes oficiais
- 🪶 **Leve** — sem instalar serviço, sem mexer no registro, sem inchaço
- 📦 **Realmente portátil** — roda de qualquer pasta, inclusive de pen drive
- 🎯 **Focado** — sem módulos que você não usa, sem UI poluída, só o essencial
- 🆓 **Open source** — licença MIT, auditável, com contribuições bem-vindas

## 🎯 Funcionalidades

### Componentes da Stack
- **Nginx** (mainline mais recente) — servidor web rápido e moderno
- **MariaDB** (stable mais recente) — substituto drop-in do MySQL, com melhor performance
- **PHP** (stable mais recente, Non-Thread Safe) — com FastCGI via `php-cgi.exe`
- **phpMyAdmin** (stable mais recente) — gerenciamento de banco familiar

### Funcionalidades da GUI
- ⚡ Start/stop de cada serviço com um clique — shutdown gracioso (`nginx -s quit`, `mysqladmin shutdown`)
- 🔌 Portas configuráveis + detecção de conflito ("porta 3306 em uso por `mysqld_usbwv8.exe` — pare o USBWebserver")
- 📂 Atalhos: abrir phpMyAdmin no navegador padrão, abrir sites `.test`
- 📊 Visualizador de logs em tempo real com seleção + botão copiar
- 🌐 Virtual hosts — cada subpasta de `www/` vira `<nome>.test` com nginx + hosts + reload automáticos
- 🔒 HTTPS local via **mkcert** embutido — uma UAC pra adicionar o CA raiz, certificados válidos pra sempre
- 🔥 Firewall do Windows em lote (um único UAC pra nginx + mysqld + php-cgi)
- 📢 System tray — roda discreto em segundo plano
- 🔄 Updater inteligente — download com SHA256, retry em erro transiente, smoke test pós-swap, rollback automático se o serviço não subir

### Feito para Desenvolvedores
- 🔄 Auto-update de todos os componentes direto das fontes oficiais
- 🧩 Troca de versão do PHP sem reinstalar
- 📝 Edite `php.ini`, `nginx.conf`, `my.ini` pela GUI
- 🚦 Detecção de conflito de portas antes de iniciar serviços
- 🔥 Gerenciamento automático das regras do Firewall do Windows

## 📥 Download

Pegue a versão mais recente na [**página de Releases**](https://github.com/luanluciano93/MadiStack/releases).

> **Requisitos:** Windows 10 ou superior. [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (já vem no Windows 11, instalação automática no Windows 10).

## 🚀 Início Rápido

1. Baixe o `MadiStack-x.x.x.zip` da página de Releases
2. Extraia em qualquer lugar (pen drive funciona perfeitamente)
3. Execute `MadiStack.exe`
4. Na primeira execução, os componentes são baixados e configurados automaticamente (~2 minutos, só uma vez)
5. Clique em **Start** nas abas Nginx e MariaDB
6. Coloque seu projeto PHP na pasta `www/`
7. Abra http://localhost no navegador — pronto! 🎉

## 📸 Screenshots

> *Screenshots em breve — projeto em desenvolvimento ativo.*

## 🗺 Roadmap

- [x] Stack principal: Nginx + MariaDB + PHP + phpMyAdmin
- [x] GUI com start/stop de um clique
- [x] Configuração de portas + detecção de conflito
- [x] Auto-updater com smoke test e rollback automático
- [x] Gerenciador de virtual hosts (subpastas em `www/` viram `<nome>.test`)
- [x] HTTPS local via mkcert embutido — 1 UAC e certificado válido pra sempre
- [x] System tray + logs ao vivo com botão copiar
- [x] Regras de Firewall do Windows via helper elevado (uma UAC só)
- [ ] i18n completo PT-BR + EN
- [ ] Dark mode 🌙 + tutorial na primeira execução
- [ ] Troca de versão do PHP lado-a-lado
- [ ] Redis e Memcached como componentes opcionais
- [ ] Integração com Composer e WP-CLI
- [ ] Backup/restore de bancos MariaDB

## 🏗 Tech Stack

- **Core:** [Rust](https://www.rust-lang.org/) 1.75+ com [Tokio](https://tokio.rs/) async runtime
- **GUI:** [Tauri v2](https://tauri.app/) (WebView2 nativo do Windows)
- **Frontend:** [Svelte 5](https://svelte.dev/) + TypeScript + [TailwindCSS](https://tailwindcss.com/)
- **Build & Release:** GitHub Actions + `tauri-action`

**Por que Rust + Tauri?** Binário final ~8 MB, RAM idle < 70 MB, cold start < 250 ms, e segurança de memória em tempo de compilação. Sem runtime pesado, sem Electron, sem surpresas.

## 🛠 Compilando do Código-Fonte

### Pré-requisitos
- [Rust](https://rustup.rs/) 1.75+ (toolchain `stable-x86_64-pc-windows-msvc`)
- [Node.js](https://nodejs.org/) 20+ e [pnpm](https://pnpm.io/) 9+
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (já presente no Windows 11)
- Microsoft C++ Build Tools (pacote "Desktop development with C++" do Visual Studio 2022)

### Build

```bash
# Clone o repositório
git clone https://github.com/luanluciano93/MadiStack.git
cd MadiStack

# Instale o CLI do Tauri (uma vez por máquina)
cargo install tauri-cli --version "^2.0" --locked

# Instale as dependências do frontend
cd frontend && pnpm install && cd ..

# Rode em modo de desenvolvimento (hot reload Rust + Svelte)
cargo tauri dev

# Gere o binário de produção
cargo tauri build
```

Binários gerados em `target/release/`:

| Arquivo | Uso |
|---------|-----|
| `madistack.exe` | Binário portátil principal |
| `madistack-system-helper.exe` | Helper elevado (UAC) — deve ficar no mesmo diretório do principal |
| `bundle/nsis/MadiStack_*.exe` | Instalador NSIS (inclui ambos os binários) |
| `bundle/msi/MadiStack_*.msi` | Instalador MSI |

> Antes de `cargo tauri build`, rode `cargo build --release -p madistack-system-helper` para que o helper seja incluído no bundle.

## 🤝 Contribuindo

Contribuições são muito bem-vindas! Seja um bug report, sugestão de feature ou pull request — tudo ajuda.

1. Faça um fork do projeto
2. Crie sua branch de feature (`git checkout -b feature/minha-feature`)
3. Commit suas mudanças (`git commit -m 'Adiciona minha feature'`)
4. Faça push para a branch (`git push origin feature/minha-feature`)
5. Abra um Pull Request

## ❓ FAQ

<details>
<summary><b>Qual a diferença do MadiStack pro XAMPP, Laragon ou WAMP?</b></summary>

- **XAMPP** usa Apache e é bastante pesado. MadiStack usa Nginx e é leve.
- **Laragon** é closed-source. MadiStack é MIT, totalmente aberto.
- **WAMP** não tem uma atualização significativa da UI há anos. MadiStack tem GUI moderna com auto-update para todos os componentes.
</details>

<details>
<summary><b>Por que Nginx em vez de Apache?</b></summary>

Nginx é mais rápido, consome menos memória, tem configuração mais simples e é o padrão de produção hoje. Se você faz deploy em servidor real, provavelmente vai rodar Nginx lá também.
</details>

<details>
<summary><b>Por que MariaDB em vez de MySQL?</b></summary>

MariaDB é um substituto drop-in do MySQL, mantido pelo criador original, totalmente open source (MySQL tem complicações de licença da Oracle), e normalmente mais rápido. Seu código não precisa de nenhuma mudança.
</details>

<details>
<summary><b>Funciona no macOS ou Linux?</b></summary>

Ainda não. MadiStack é Windows-only por enquanto. Usuários Linux já têm gerenciadores de pacote nativos (`apt`, `pacman`), e no macOS tem Homebrew e Laravel Herd.
</details>

<details>
<summary><b>É realmente portátil? Posso rodar de um pen drive?</b></summary>

Sim. MadiStack não mexe no registro, não instala serviços, e guarda todos os dados na própria pasta. Copie a pasta pra onde quiser e continua funcionando.
</details>

<details>
<summary><b>Posso usar em produção?</b></summary>

**Não.** MadiStack é uma ferramenta de desenvolvimento. Para produção, use um servidor Linux com Nginx/MariaDB configurados com hardening apropriado.
</details>

## 📄 Licença

MadiStack é distribuído sob a [Licença MIT](LICENSE).

### Componentes de Terceiros

MadiStack baixa e utiliza os seguintes componentes, cada um com sua própria licença:

| Componente | Licença | Site |
|-----------|---------|---------|
| Nginx | [BSD 2-cláusulas](https://nginx.org/LICENSE) | [nginx.org](https://nginx.org/) |
| MariaDB | [GPL v2](https://mariadb.com/kb/en/mariadb-license/) | [mariadb.org](https://mariadb.org/) |
| PHP | [PHP License v3.01](https://www.php.net/license/) | [php.net](https://www.php.net/) |
| phpMyAdmin | [GPL v2](https://www.phpmyadmin.net/license/) | [phpmyadmin.net](https://www.phpmyadmin.net/) |

## 👤 Autor

**Luan Luciano**

- GitHub: [@luanluciano93](https://github.com/luanluciano93)
- Projetos: [NexusAAC](https://github.com/luanluciano93/NexusAAC) · [TibiaTrace](https://github.com/luanluciano93/TibiaTrace)

## 🌟 Apoie o Projeto

Se o MadiStack te poupou tempo, considere:

- ⭐ Dar uma estrela no repositório — ajuda demais!
- 🐛 Reportar bugs ou sugerir features
- 🔀 Enviar pull requests
- 📢 Compartilhar com outros devs

---

<div align="center">

Feito com ❤️ em Birigui, Brasil 🇧🇷

</div>
