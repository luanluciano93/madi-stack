# Design: troca de versão de PHP lado-a-lado

> **Status:** proposta, não implementada. Item de v2 do roadmap (`PLANNING.md` §5).
> Este doc é o ponto de partida. Faça fork, discorde, melhore.

## Problema

Hoje o MadiStack instala uma única versão de PHP em `bin/php/`. Ao atualizar,
a versão anterior é substituída. Isso inviabiliza casos comuns:

- Projeto legado roda só em PHP 7.4; projeto novo requer 8.5.
- Reproduzir bug reportado por cliente que está em 8.3.
- Testar compatibilidade com versão próxima a EOL antes de migrar.

## Objetivo

Permitir múltiplas versões de PHP instaladas **ao mesmo tempo**, com troca
de versão **ativa** em 1 clique, sem reinstalar.

## Não-objetivos (v2)

- **Vhost com versão diferente da global**: cada site escolhendo seu PHP é
  v3. Exige múltiplos `php-cgi.exe` rodando em portas distintas +
  roteamento nginx por `fastcgi_pass` condicional. Complexidade explode.
- **Download automático na primeira vez que um site pede versão X**: fora
  de escopo. Instalação é explícita.
- **Compilação de extensões customizadas**: reutilizamos o zip oficial que
  já inclui o set padrão. Extensões extras continuam lendo os DLLs que já
  acompanham o zip do PHP.

## Modelo de dados

### Runtime (disco)

```
bin/
├── php/                         ← versão ativa (symlink ou cópia — ver discussão abaixo)
├── php-versions/
│   ├── 8.3.26/
│   │   ├── php.exe
│   │   ├── php-cgi.exe
│   │   └── ext/
│   ├── 8.4.14/
│   │   └── ...
│   └── 8.5.5/
│       └── ...
```

### Persistência (`madistack.toml`)

```toml
[php]
active = "8.5.5"                 # versão atualmente em bin/php/
installed = ["8.3.26", "8.4.14", "8.5.5"]
```

`installed` é redundante com `read_dir(bin/php-versions/)` mas acelera a
UI de boot (sem syscalls). Invalidar ao detectar divergência.

### `Prefs` layer

Hoje `PortConfig.php_fcgi` é global. Mantém assim. Vhosts não ganham
`php_version` ainda (seria v3).

## Fluxos

### Listar versões disponíveis upstream

`crates/sources/src/php.rs` já resolve a "latest". Estender com:

```rust
pub async fn available_versions(client: &Client) -> SourceResult<Vec<PhpRelease>>;
```

Fonte: <https://windows.php.net/downloads/releases/archives/> +
`https://windows.php.net/downloads/releases/` para as séries ativas. Formato:
filename = `php-<X.Y.Z>-nts-Win32-vs17-x64.zip`. Fazer regex estrito, não
confiar na ordem do HTML.

Cachear resposta em memória por 15 min (é navegado centenas de vezes).

### Instalar versão específica

Novo comando Tauri:

```rust
#[tauri::command]
pub async fn php_install_version(version: String, state, app) -> Result<(), String>;
```

- Valida `version` contra o regex `^[0-9]+\.[0-9]+\.[0-9]+$`.
- Resolve URL via `sources::available_versions` (ou `specific_version(v)` adicional).
- Baixa para `tmp/php-<v>.zip` com SHA256 (se publicado).
- Extrai para `bin/php-versions/<v>/`.
- Não toca em `bin/php/`.
- Persiste `installed` no `madistack.toml`.

### Ativar versão

```rust
#[tauri::command]
pub async fn php_activate_version(version: String, state, app) -> Result<(), String>;
```

Sequência:
1. Checar `version` presente em `bin/php-versions/`.
2. **Parar o php-cgi ativo** via supervisor (fica em `Stopped`).
3. Swap atômico:
   - Se `bin/php/` existe e não é link: mover pra `bin/php-versions/<current>/` (é a
     "archivação" da versão antiga — recupera layout).
   - **Decisão chave** (ver abaixo): criar symlink ou copiar?
4. Atualizar `php.active` no state, salvar `madistack.toml`.
5. Re-render `php.ini` (pode ter diferenças entre versões — templates separados
   ou o mesmo com flags condicionais).
6. (Opcional) Reiniciar php-cgi se estava rodando antes.

### UI

Nova aba **PHP** (ou subseção em Configurações):

```
┌──────────────────────────────────────────────────────┐
│ PHP                                                  │
│ Instaladas:                                          │
│   ◉ 8.5.5   (ativa)    [Abrir pasta]  [Desinstalar]  │
│   ○ 8.4.14             [Ativar]        [Desinstalar] │
│   ○ 8.3.26             [Ativar]        [Desinstalar] │
│                                                      │
│ Disponíveis para download:                           │
│   [ Versão v│]  [Instalar]                           │
│   - 8.5.6                                            │
│   - 8.4.15                                           │
│   - 7.4.33 (EOL — sem suporte)                       │
└──────────────────────────────────────────────────────┘
```

Radio button pra ativar (uma por vez). Confirmação se trocar com php-cgi
rodando ("Isso vai parar o serviço. Continuar?").

## Decisão chave: symlink vs. cópia

**Opção A: symlink `bin/php` → `bin/php-versions/<active>/`**

- Prós: troca = 1 chamada (remover+recriar symlink). Zero cópia de arquivos.
  Economiza disco (~40MB por versão extra se copiado).
- Contras: symlinks no Windows requerem:
  - **Privilégio "Create symbolic links"**: desabilitado por default pra
    contas normais. Dev Mode do Windows ativa, mas instalação típica não.
  - **Fallback**: junction points (`mklink /J`) não exigem privilégio mas só
    funcionam pra diretórios (OK pro nosso caso).
  - Antivírus/backup tools podem tratar estranhamente.

**Opção B: mover pastas (rename)**

- Prós: funciona em qualquer configuração de Windows. NTFS rename é atômico.
- Contras: mais lento se a pasta é grande. PHP sem pasta ativa momentaneamente
  entre dois renames (ordem: `bin/php/` → `bin/php-versions/<old>/`, depois
  `bin/php-versions/<new>/` → `bin/php/`).

**Recomendação**: **Opção B (junction point como fallback opcional)**.
Implementação inicial: move + rename. Se performance virar problema pra
pastas de 200MB+, adicionar junction point como otimização (detectando se
o Windows suporta).

Reutilizar o swap atômico do `crates/updater::apply` — mesma lógica, só
que a nova pasta vem do `bin/php-versions/` em vez de download novo.

## Impacto em outras partes

### `crates/services::supervisor`

`spawn_spec` pra `Component::Php` resolve o cwd como `install/bin/php/`.
Continua igual — só ativa vê. Zero mudança de código.

### `crates/updater`

Quando `updater_apply(Php)` rodar, deve **atualizar a versão ativa** ou
**tratar PHP especialmente** (deixando pra UI `php_install_version`)?

**Proposta**: `updater_apply(Php)` atualiza só a versão ativa, preservando
as outras. Internamente, depois do download:
- Renomeia `bin/php/` para `bin/php-versions/<old-active>/`.
- Renomeia nova extração pra `bin/php/`.
- Atualiza `php.active` com nova versão.

Isso torna PHP um cidadão de 2ª classe no updater genérico. Alternativa:
desligar o botão "Atualizar" do PHP na aba Atualizações quando o sistema
multi-versão estiver ativo, e orientar o usuário a usar a aba PHP.

### `templates/php.ini.tera`

Hoje é um template só. PHP 7 vs 8 têm diferenças (mysql_connect removido,
etc). Opções:
- Um template global com `{% if php_major >= 8 %}` etc.
- Templates separados `php-7.ini.tera`, `php-8.ini.tera`.

Recomendo o primeiro — evita duplicação, e a divergência real é pequena.

## Riscos

- **Compatibilidade de tools**: Composer, extensões compiladas. Em geral,
  scripts PHP puros funcionam em qualquer versão. PECL e opcache.dll
  específicos de versão já vêm no zip oficial.
- **Esqueleto aumenta bin/**: 4 versões = ~160MB a mais. Aceitável pra um
  dev tool. Expor tamanho total e botão "Desinstalar" proeminente.
- **Race condition na troca**: usuário aperta Ativar e fecha o app no meio.
  Recovery no boot: se `bin/php/` não existe mas `php.active` aponta pra
  algo, reativar automaticamente. Se `bin/php/` tem versão diferente de
  `php.active`, confiar no que está em disco e corrigir state.

## Passos de implementação (ordem recomendada)

1. `sources::available_versions(Component::Php)` — expõe lista.
2. `state-store`: adicionar `[php]` section com `active` + `installed`.
3. Backend commands `php_list_installed`, `php_list_available`,
   `php_install_version`, `php_activate_version`, `php_uninstall_version`.
4. Testes de unidade pra swap (usando tempdir + fake zips).
5. UI: aba PHP completa.
6. Revisão do fluxo de updater pra entender como se integra.
7. Documentar no README + CLAUDE.md.

Estimativa: 1-2 semanas de trabalho dedicado (alinhado com PLANNING).
