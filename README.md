<div align="center">

# 🚀 MadiStack

**Uma alternativa moderna e open-source ao USBWebserver para Windows.**

MadiStack reúne Nginx, MariaDB, PHP e phpMyAdmin em uma GUI portátil — é só escolher as portas, clicar em iniciar e colocar seu site na pasta `www`. Auto-updater integrado mantém todos os componentes sempre atualizados.

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2B-blue.svg)](#)
[![Built with Wails](https://img.shields.io/badge/Built%20with-Wails-ff4d4d.svg)](https://wails.io)
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
- ⚡ Start/stop/restart de cada serviço com um clique
- 🔌 Portas configuráveis (HTTP, HTTPS, MySQL, PHP-FPM)
- 📂 Atalhos rápidos: abrir pasta `www`, localhost, phpMyAdmin
- 📊 Visualizador de logs em tempo real para Nginx e PHP
- 🔧 Editor de configs integrado com syntax highlighting
- 🌐 Suporte a múltiplos sites (virtual hosts) — rode vários projetos ao mesmo tempo
- 🔒 HTTPS local com um clique usando certificados self-signed
- 📢 Integração com system tray — roda discreto em segundo plano
- 🇧🇷 Suporte completo em Português (BR) e Inglês

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
- [x] Configuração de portas
- [x] Auto-updater
- [ ] Gerenciador de virtual hosts
- [ ] HTTPS local com integração ao mkcert
- [ ] Troca de versão do PHP
- [ ] Redis e Memcached como componentes opcionais
- [ ] Integração com Composer e WP-CLI
- [ ] Backup/restore de bancos MariaDB
- [ ] Dark mode 🌙

## 🏗 Tech Stack

- **Backend:** [Go](https://go.dev/) + [Wails v2](https://wails.io/)
- **Frontend:** [Svelte](https://svelte.dev/) + TypeScript
- **Build:** GitHub Actions com releases automatizadas

## 🛠 Compilando do Código-Fonte

### Pré-requisitos
- [Go](https://go.dev/) 1.22+
- [Node.js](https://nodejs.org/) 20+
- [Wails CLI](https://wails.io/docs/gettingstarted/installation): `go install github.com/wailsapp/wails/v2/cmd/wails@latest`

### Build

```bash
# Clone o repositório
git clone https://github.com/luanluciano93/MadiStack.git
cd MadiStack

# Instale as dependências do frontend
cd frontend && npm install && cd ..

# Rode em modo de desenvolvimento (com hot reload)
wails dev

# Gere o binário de produção
wails build
```

Binário gerado em: `build/bin/MadiStack.exe`

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
