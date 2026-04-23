# MadiStack — Troubleshooting

Problemas recorrentes e como resolver. Se o seu caso não está aqui, abra
uma [issue](https://github.com/luanluciano93/madi-stack/issues/new) com
o texto exato do erro e um screenshot da aba Geral.

> **Dica rápida:** cada aba de serviço (Nginx, MariaDB, PHP) tem um
> botão **Copiar** no LogViewer que copia as linhas visíveis com
> timestamp. Cole junto na issue — acelera o diagnóstico.

---

## 🚫 "O Windows protegeu o seu PC" ao abrir o `.exe`

**Causa:** o binário do MadiStack ainda não é assinado digitalmente com
certificado EV (planejado para v1.0). O SmartScreen barra executáveis
sem reputação.

**Solução:**
1. Clique em **Mais informações** no aviso.
2. Clique em **Executar assim mesmo**.
3. Só na primeira execução — o Windows memoriza a decisão.

---

## 🔌 "port X is already in use" ao iniciar um serviço

**Sintoma:** o serviço não sobe e aparece uma mensagem tipo:

> port 3306 is already in use (pid 16748, mysqld_usbwv8.exe) —
> outro MariaDB/MySQL já está rodando

**Causa:** outro software está escutando na mesma porta. Suspeitos comuns:

| Porta | Suspeito                                                    |
| ----- | ----------------------------------------------------------- |
| 80    | IIS, outro nginx, Skype antigo, Docker Desktop              |
| 3306  | MySQL/MariaDB instalado como serviço, XAMPP, WAMP, USBWebserver |
| 9000  | PHP-FPM de outra stack, Xdebug                              |

**Solução:** duas opções.

1. **Mude a porta do MadiStack** em Configurações (ex: HTTP 80 → 8080,
   MariaDB 3306 → 3307). Mais rápido do que parar o concorrente.
2. **Pare o concorrente:**
   - **IIS:** `Win+R` → `services.msc` → "World Wide Web Publishing
     Service" → Parar.
   - **XAMPP / WAMP / Laragon:** feche pelo control panel deles.
   - **Serviço MySQL:** `services.msc` → localize (ex: "MySQL80" ou
     "MariaDB") → Parar.

---

## 📚 `VCRUNTIME140.dll` ou `VCRUNTIME140_1.dll` não encontrado

**Sintoma:** nginx, php-cgi ou mysqld falha ao iniciar com erro sobre
DLL do Visual C++ faltando, ou "Código de erro 0xc000007b".

**Causa:** PHP (NTS x64 VS17) e MariaDB dependem do **Microsoft Visual
C++ Redistributable**. Windows 11 e Windows 10 recente normalmente já
têm, mas instalações limpas não.

**Solução:** baixe e instale o redistributable oficial:

<https://aka.ms/vs/17/release/vc_redist.x64.exe>

Reinicie o MadiStack depois.

---

## 🛡 Antivírus apaga ou bloqueia o `madistack.exe`

**Sintoma:** o arquivo some da pasta de instalação minutos depois de
baixar, ou o app não abre e nada acontece.

**Causa:** binário não assinado + comportamento de "baixar outros
exes + abrir portas de rede" dispara heurísticas genéricas.
Falso positivo — **não é bug**.

**Solução:**
1. Adicione a pasta do MadiStack como **exceção** no antivírus:
   - **Windows Defender:** Configurações → Privacidade → Proteção
     contra vírus → Gerenciar configurações → Adicionar exclusão
     → Pasta.
   - **Third-party** (Avast, Kaspersky, etc): procure "Exceções" ou
     "Exclusões" no painel.
2. Se o arquivo já foi removido, restaure da quarentena ou baixe de
   novo de <https://github.com/luanluciano93/madi-stack/releases/latest>.

Assinatura de código com certificado EV está planejada para v1.0 —
eliminará esses falsos positivos em massa.

---

## 🔒 phpMyAdmin carrega mas diz "tentou se conectar ao servidor MySQL…"

**Causa:** o phpMyAdmin abre (nginx + PHP rodando) mas não consegue
autenticar no MariaDB. Três motivos comuns:

1. **MariaDB parado** — clique Iniciar na linha MariaDB da aba Geral.
2. **Senha errada** — use `root` como usuário e a senha que aparece
   no **banner vermelho em Geral** logo após a (re)instalação do pma.
   Se você já dispensou o banner, veja `madistack-secrets.toml` na
   pasta de instalação, campo `mariadb_root_password`.
3. **Porta divergente** — se você mudou a porta do MariaDB em
   Configurações, o `config.inc.php` do pma pode estar desatualizado.
   Clique **Salvar** em Configurações (re-renderiza todos os configs)
   e reinicie nginx.

---

## 🌐 `ERR_CONNECTION_REFUSED` ao abrir `http://localhost`

**Causa:** o nginx não está rodando, ou está em outra porta.

**Solução:**
1. Aba Geral → LED do Nginx está **verde** (running)?
   - Não: clique Iniciar. Se falhar, veja "port X is already in use"
     acima.
2. Porta HTTP em Configurações é `80` ou outra? Se for `8090`,
   acesse `http://localhost:8090` (com a porta).
3. Extensão do browser (AdGuard, NoScript) bloqueando localhost?
   Teste em janela anônima.

---

## 🏷 `<nome>.test` não resolve depois de ativar um Site

**Causa:** o MadiStack editou o `hosts` mas o Windows ainda está
cacheando o DNS antigo.

**Solução:**
1. Feche **todas** as abas abertas no navegador e reabra.
2. Se persistir, rode num terminal admin: `ipconfig /flushdns`.
3. Verifique que a linha foi adicionada em
   `C:\Windows\System32\drivers\etc\hosts`:
   ```
   127.0.0.1    meusite.test    # madistack:vhost:meusite
   ```
4. Alguns antivírus (Kaspersky clássico) protegem o `hosts` contra
   escrita e revertem automaticamente. Veja o tópico do antivírus
   acima e libere a pasta do MadiStack.

---

## 🔐 HTTPS do site `.test` dá "NET::ERR_CERT_AUTHORITY_INVALID"

**Causa:** o certificado raiz do mkcert ainda não foi instalado no
trust store do Windows.

**Solução:**
1. Aba Sites → desative o site, ative de novo com a checkbox
   **HTTPS** marcada. Na primeira ativação, o MadiStack pede UAC
   para rodar `mkcert -install` — autorize.
2. Depois disso, feche **completamente** o navegador (todas as
   janelas, inclusive processos em background) e reabra. Chrome
   e Edge só leem o trust store no startup.
3. Firefox usa trust store próprio. `mkcert -install` detecta e
   instala lá também, mas se falhar abra
   `about:preferences#privacy` → View Certificates → Authorities →
   Import, e selecione o `rootCA.pem` em `%LocalAppData%\mkcert\`.

---

## 🔁 "post-swap healthcheck failed" no updater

**Causa:** o MadiStack baixou a nova versão do componente, fez o
swap, mas o serviço não subiu com os binários novos. O updater
reverte automaticamente para `bin/<componente>.bak/` (última
versão que funcionava).

**O que fazer:**
1. Confirme na aba Atualizações que o botão continua mostrando
   "em dia" com a versão antiga — o rollback funcionou.
2. Reporte em [Issues](https://github.com/luanluciano93/madi-stack/issues)
   com:
   - Qual componente falhou.
   - Versão de origem e destino.
   - Conteúdo do **Painel de Eventos** no rodapé.
   - Log do serviço em `logs/<componente>/`.

---

## 🔥 "Firewall do Windows bloqueia toda vez"

**Sintoma:** na primeira execução o Windows pede permissão de firewall
e você clicou em **Cancelar**, ou o sistema não mostrou popup e seus
sites `.test` não acessam de outros dispositivos na rede.

**Solução:**
1. Aba **Firewall** → clique em **Criar / Recriar regras**.
2. Confirme no prompt UAC — o MadiStack delega a criação das regras
   ao helper elevado, uma UAC só, sem manter privilégios depois.
3. Na aba Firewall, as 3 linhas (Nginx, MariaDB, PHP FastCGI) devem
   ficar verdes ("regra presente").

Se você só usa localhost (`127.0.0.1`), as regras **não são
necessárias** — o loopback não passa pelo firewall.

---

## 📁 "MariaDB não inicia — `Can't create/write to file`"

**Causa:** o diretório `data/mariadb/` está bloqueado por outra
instância ou sem permissão de escrita.

**Solução:**
1. Veja se há outras instâncias de `mysqld.exe` no **Gerenciador de
   Tarefas** — se sim, finalize-as.
2. Verifique se a pasta do MadiStack está em local com permissão
   de escrita (evite `C:\Program Files\` — use `C:\Users\<você>\`
   ou um pen drive).
3. Antivírus bloqueando? Adicione a pasta inteira como exceção.

---

## 🔤 Caminho com acento ou espaço

**Sintoma:** erros estranhos nos logs ao iniciar serviços, paths em
configs aparecem cortados ou com caracteres trocados.

**Casos conhecidos:** `C:\Users\João\Área de Trabalho\MadiStack\`.

**Solução temporária:** mova a pasta para um caminho sem acentos
(ex: `C:\MadiStack\`) e abra uma issue com o caminho exato que
quebrou — é bug nosso, estamos corrigindo caso a caso.

---

## 🆘 Nenhum dos casos acima

1. Abra o MadiStack.
2. Vá na aba do serviço problemático (Nginx / MariaDB / PHP).
3. Clique **Copiar** no LogViewer.
4. Abra uma [issue](https://github.com/luanluciano93/madi-stack/issues/new)
   colando os logs + descrição do que você fez.
5. Inclua a versão do Windows (`Win+R` → `winver`) e a versão do
   MadiStack (aba Sobre).
