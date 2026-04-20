# MadiStack — Troubleshooting

Erros comuns e como resolver. Atualize sempre que encontrar um novo no campo.

## "VCRUNTIME140.dll está faltando"

Falta o **Microsoft Visual C++ Redistributable 2015–2022 (x64)**. Baixe e instale:
<https://aka.ms/vs/17/release/vc_redist.x64.exe>

## "Não consigo iniciar o Nginx — porta 80 em uso"

Algum outro serviço está escutando na porta 80. Candidatos comuns:
- **IIS** (Internet Information Services): `net stop W3SVC`
- **Skype** antigo (pré-2017): reconfigurar pra não usar 80/443
- **Outra stack local** (XAMPP, Laragon, WAMP): desligue antes

Alternativamente, mude a porta em **Configurações → Porta HTTP** (ex: 8080).

## "MariaDB não inicia — `Error: Can't create/write to file`"

Provavelmente o diretório `data/mariadb/` está bloqueado por outra instância. Verifique:
1. Outras instâncias de `mysqld.exe` no Gerenciador de Tarefas.
2. Permissão de escrita na pasta do MadiStack.
3. Antivírus bloqueando acesso ao diretório.

## "Firewall do Windows bloqueia toda vez"

Na primeira execução, o MadiStack pede UAC e cria as regras. Se você negou:
1. Feche o MadiStack.
2. Abra como **Administrador**.
3. Configurações → "Recriar regras de firewall".

## "O antivírus colocou o MadiStack.exe em quarentena"

Binário não assinado — falso positivo. Adicione exceção para a pasta inteira.
Assinatura de código está planejada para v1.0.

## "Caminho com acento/espaço não funciona"

Teste o caminho da pasta do MadiStack — `C:\Users\João\Área de Trabalho\` é um caso conhecido. Se estiver quebrando, tente uma pasta sem acentos temporariamente e abra um issue.

## "phpMyAdmin dá erro de conexão ao MariaDB"

Confira:
1. MariaDB está rodando (LED verde na aba MariaDB).
2. Porta em `Configurações` bate com a do phpMyAdmin.
3. Senha root configurada corretamente.
