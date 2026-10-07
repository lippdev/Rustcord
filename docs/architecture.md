# Rustcord — arquitetura / ADR 001

Nome definido pelo usuário. Projeto independente, Rust, Windows primeiro. Milestone 1 é
uma demonstração local de UX, não um cliente conectado ao Discord.

## Direção após a discovery de clientes nativos

O usuário confirmou o objetivo de um cliente alternativo de conta pessoal sem
WebView. A [discovery](native-backend-discovery.md) compara implementações,
licenças, login e um experimento de extração do backend Serein. Essa direção
substitui a proposta de converter o produto em companion/bot. As restrições de
API descritas abaixo permanecem relevantes, mas aprovação oficial não é uma
dependência técnica de compilação/conexão. Interoperabilidade e autorização
contratual são questões distintas. A discovery não implementou autenticação.
O incremento seguinte adiciona [QR nativo e leituras HTTPS](native-connection.md),
com aprovação QR e leituras reais observadas no macOS. A demonstração mock segue disponível.

## Pesquisa e limite de produto (07/10/2026)

A API oficial não oferece autorização geral para substituir o cliente de uma
conta pessoal. [Self-bots](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)
são expressamente proibidos e podem causar encerramento da conta. O usuário
escolheu interoperabilidade de um cliente nativo independente; isso não implica
aprovação oficial. O login QR usa Remote Auth v2, um protocolo não OAuth público.
Não pedir senha/token manual, extrair sessões de outros aplicativos, impersonar
um navegador ou contornar MFA/CAPTCHA.

[OAuth2](https://docs.discord.com/developers/topics/oauth2) permite apenas os
scopes autorizados: identify/guilds não equivalem a ler/enviar todas as mensagens
como usuário. `messages.read` é descrito para RPC local; `rpc` requer aprovação
para parceiros e controla um cliente Discord existente. Não resolve o requisito
de independência. Social SDK tem comunicação com acesso limitado e termos
próprios; não assumir que autoriza um cliente geral de guilds/DMs.
Bots são identidades próprias, com permissões/intents, não substitutos da conta
pessoal. [Developer Policy](https://docs.discord.com/developers/policies/developer-policy)
exige respeito a privacidade, autorização e limites. O projeto permanece um
cliente alternativo de conta pessoal, conforme a direção confirmada pelo usuário.
As capacidades e limitações são documentadas sem presumir suporte oficial.

Fontes consultadas: artigo oficial acima e arquivos atuais do repositório oficial
[discord-api-docs](https://github.com/discord/discord-api-docs/tree/main/developers)
(`topics/oauth2.mdx`, `policies/developer-policy.mdx`,
`discord-social-sdk/core-concepts/oauth2-scopes.mdx`). As páginas web de docs
retornaram HTTP 403 aqui; os respectivos fontes oficiais foram acessíveis.
A Developer Policy completa foi consultada também no [suporte oficial](https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy); o arquivo do repositório aponta para essa página. O milestone inicial era inteiramente mock; a conexão experimental é descrita abaixo.

## Comparação de UI

Versões estáveis consultadas pela API crates.io: GPUI 0.2.2, iced 0.14.0,
egui/eframe 0.36.2, Slint 1.18.1. Valores de RAM/tamanho dependem do renderer,
fonts, plataforma e recursos; a comparação abaixo é qualitativa, não benchmark.

| Opção | RAM / tamanho / velocidade | Maturidade e Windows | Visual e layouts | Foco de gamepad |
|---|---|---|---|---|
| GPUI | GPU rápida; stack de texto/render pesada em relação a uma slice simples | Usada no Zed; API ainda evolui; Win32/DirectWrite disponível | Excelente liberdade e layouts flexíveis | Modelo próprio necessário, eventos e foco disponíveis |
| iced | Retained/reactiva; WGPU padrão pesa, tiny-skia é alternativa | Maduro, multiplataforma, Windows | Widgets compostos, layouts complexos, styling consistente | IDs e operações de foco; navegação própria |
| egui | Immediate mode; glow evita WGPU; redesenho sob demanda limita CPU | Ecossistema amplo, eframe/winit suporta Windows | Pintura/customização fácil; layout complexo exige disciplina e virtualização | Grafo/reducer explícito simples, separado de widgets |
| Slint | Potencial muito baixo com software renderer; medir CPU em telas grandes | Produto maduro, Windows | Declarativa, animações/layouts fortes | Propriedades de foco e ponte de actions necessárias |

Fontes: [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
[iced](https://iced.rs/), [egui](https://github.com/emilk/egui),
[Slint](https://github.com/slint-ui/slint). Slint possui opções GPL/comercial e
Royalty-free com condições específicas; verificar licença antes de adoção.

**Escolha: egui/eframe + glow**, sem WGPU, Electron, WebView ou Chromium.
“Native” aqui significa janela e loop de eventos nativos, widgets desenhados
em Rust/OpenGL; não são controles padrão Win32. A escolha facilita foco
independente, prototipagem visual e troca futura do frontend. Risco: driver
OpenGL do Windows/VMs; validar hardware Intel/AMD/NVIDIA e fallback antes de
release público. `opt-level=s`, thin LTO, stripping; comparar com opt-level=3
antes de fixar uma política definitiva de velocidade versus tamanho.

## Dependências e direção de dados

```text
backend nativo -> SessionEvent -> AppState/reducer -> Ui view
                                              ^                    |
mouse / teclado / gilrs -> AppAction -----------+ <-----------------+
platform -> amostragem de métricas / futuros serviços de SO
app -> composição, janela, lifecycle
```

- `discord-core`: modelos de domínio, trait `Backend`, capabilities e mock.
  Não importa UI/input.
- `discord-network`: QR/Remote Auth v2, worker Tokio e leituras HTTPS com
  reqwest/rustls. Não importa UI/input; reutiliza os modelos do domínio.
- `app-state`: seleção, foco por região/item, telas e reducer puro.
- `input`: ações sem domínio Discord e mapeamento de teclado. Deadzone,
  histerese e worker de gamepad ficam em módulo opcional fora do app padrão.
- `ui`: converte eventos egui em actions e desenha estado. Mouse escolhe alvos
  pelo mesmo reducer; biblioteca nativa cuida do editor de texto.
- `platform`: métricas RSS/working set e futuros serviços nativos.
- `app`: binário, composição e opções de execução.

## Fundação do backend nativo

`AccountId`, `ServerId`, `ChannelId` e `MessageId` são tipos distintos sobre u64.
Índices continuam apenas como posição de navegação da UI. A troca de snapshot
valida IDs duplicados antes de alterar o estado, restaura seleção por identidade
(inclusive quando um canal muda de servidor) e descarta rascunhos de canais
removidos ou de outra conta. IDs locais esgotados não fazem overflow nem apagam
o texto não enviado.

`backend_channel` separa `BackendConnection` (UI) de `BackendWorker` (adaptador),
com 16 slots de comandos e 16 de eventos. A UI usa apenas operações não
bloqueantes. Publicação numa fila cheia devolve o evento ao produtor; o mock
retém um snapshot pendente para tentar novamente. Um callback sem dependência
de egui acorda a UI após publicação, sem um timer permanente de polling.
`wait_command` bloqueia apenas um worker dedicado; não deve rodar na UI nem
diretamente num executor Tokio. O adaptador de rede futuro precisará conectar
esse transporte ao seu runtime e limitar também bytes decodificados: limites
de slots sozinhos não limitam o tamanho de snapshots.

Eventos carregam `SessionGeneration`; `begin_session` avança a geração e limpa
estado/rascunhos. Eventos de gerações anteriores são ignorados. Reconnect da
mesma sessão não deve chamar `begin_session`. Cancelar ou soltar a conexão
marca cancelamento, fecha o sender de comandos e solta a fila de eventos;
cancelamento não depende de haver um slot livre. O coordenador futuro deve
cancelar a conexão antiga antes de iniciar outra. O editor e o scroll também
incluem a geração, impedindo undo de texto de uma sessão anterior.

O app solicita o snapshot inicial por esse transporte e o mock o entrega antes
das ações de UI no primeiro frame. `RequestSnapshot` é o único comando de backend
na demonstração local; mensagens e reações continuam no reducer local. Requisitar
outro snapshot mock repõe seus dados de demonstração. O modo conectado usa o
worker independente de `discord-network`; veja [o fluxo e os limites](native-connection.md).
Não há cofre ou código Serein incorporado.

Evitar a dependência cíclica UI/input: as duas produzem comandos, estado é a
fonte de verdade. Backend futuro deve emitir eventos em fila limitada com
cancelamento/backpressure; UI não espera rede. Cache de mensagens limitado e
paginação/virtualização necessários para expandir o histórico. O modo conectado
limita a leitura a 50 mensagens do canal atual; inclui mídia CDN e Gateway
com assinatura limitada de membros, eventos de mensagens e Resume. Usa Tokio, reqwest/rustls e
tokio-tungstenite. Possui filas de oito slots e cancelamento da tarefa inteira,
inclusive com I/O pendente ou fila cheia. Cada tentativa possui seu próprio
receiver; sair/cancelar descarta a tentativa e todos os seus eventos.
A tela de leitura mantém seleção por IDs em `OnlineView`, separada do reducer
de edição local, para não simular envio/reação em canais reais. Unificar a
coordenação de sessão no reducer será necessário ao implementar envio e unificar o estado conectado.

Pesquisa para etapa futura, desativada no app atual: **gilrs 0.11.2**, mapeamentos padronizados, hotplug, Windows Gaming Input (WGI) padrão no Windows,
udev no Linux. SDL3 é alternativa abrangente mas acrescenta runtime/build C;
GameInput oferece potencial suporte Windows mais amplo mas aumenta código
específico. O adaptador opcional bloqueia aguardando eventos e acorda UI só quando necessário;
um stick mantido usa timers. Desconexão limpa o estado do stick. O adaptador opcional reporta erros por eventos. Sem polling render a 60 FPS em idle.

## Revisão de escopo: desktop primeiro

Após a primeira demonstração, o usuário esclareceu que o objetivo imediato é
**Rustcord: usabilidade e design familiar do Discord, implementados em Rust sem
WebView**, sem inventar uma linguagem visual de console nem implementar controle
na experiência atual. Esta decisão substitui a proposta inicial de TV/gamepad.

Layout: rail de servidores, sidebar de canais com perfil na base, header do canal,
lista de mensagens com avatares/autor/horário, composer e lista de membros.
Paleta e densidade aproximam o Discord desktop dark clássico. Não há identidade
visual de console, grandes cartões, modo TV ou foco verde na interface. O objetivo
é fidelidade de estrutura/interações; esta slice ainda não reproduz todos os
componentes, fontes proprietárias, menus ou recursos do aplicativo oficial.

A UI oficial usa tecnologia web (DOM/CSS/JavaScript) e não é uma biblioteca Rust
nativa reutilizável. Retirar o navegador implica reimplementar o frontend. egui,
eframe, winit e gilrs são componentes existentes reutilizados, não uma UI Discord
completa. Reutilizar código de terceiros exige avaliar licença, maturidade e
compatibilidade com os limites da API. Não foi encontrado/validado um frontend
oficial Rust que pudesse simplesmente ser incorporado. Bibliotecas de bots Rust
não resolvem autenticação de contas pessoais nem fornecem a UI oficial.

Mouse seleciona servidores/canais e permite scroll; editor permite digitar e enviar
com Enter. Clique direito abre ações locais de mensagem. F1 abre configuração,
Escape fecha, atalhos de seleção ficam na camada de actions. Essas interações
pertencem à demonstração mock; a conexão inicial oferece apenas leitura.
Gamepad permanece apenas como módulo experimental opcional (`input/gamepad`),
fora do build e runtime padrão. Não há worker de gamepad ou dependência gilrs no
aplicativo padrão. Integração com controles/Console Mode deve ser reconsiderada
somente depois de provar a usabilidade desktop.

## Composer e respostas locais — incremento M2

`AppState` é a fonte de verdade dos rascunhos: texto limitado a 2.000 caracteres
Unicode e ID opcional da mensagem respondida. `UpdateDraft`, `SendDraft`,
`ReplyTo` e `CancelReply` são comandos independentes do dispositivo. A UI mantém
somente o buffer do editor e estado de composição/foco. Cada conversa tem um ID
egui distinto, isolando cursor e undo. Rascunhos vivem só em memória, limitados
pelos canais do snapshot; as chaves são identidades conta/canal. Editor, undo e
scroll incluem também a geração da sessão. Antes de dados reais, estabelecer
orçamento global/expiração para rascunhos e estado dos editores, além de limites
de bytes nos adaptadores de protocolo.

Enter envia apenas sem modificadores, com foco no editor, sem repetição e sem
composição IME ativa; Shift+Enter insere linha. Frames com eventos IME não enviam
mensagens: confirmar um candidato não pode enviar o texto. Testes sintetizam
esses eventos; validar IMEs reais e acessibilidade no Windows continua pendente.
O editor cresce até seis linhas visuais e depois permite scroll interno.

Responder abre o composer e conserva o texto. Escape/cancelar remove somente a
referência; enviar limpa apenas o rascunho atual. Referências usam o ID da mensagem,
sem apontar para outra mensagem se o histórico descartar o original. Originais
já descartados exibem aviso; enviar não conserva um alvo que não existe mais.
A lista é limitada a 200 mensagens por canal e ainda não é virtualizada.

Menu de mensagem é um popup nativo egui, com responder, reação local e copiar.
Clipboard pertence ao adaptador UI/plataforma (`ctx.copy_text`), nunca ao core
Discord. F2 usa o mesmo menu e reducer; ao confirmar a opção de cópia, a UI emite
o comando de clipboard antes de fechar o menu. Configuração permanece modal.

## Performance e observabilidade

Sem runtime async nem rede. Sem redraw contínuo; debug amostra métricas a 1 Hz,
release sem métricas não cria esse timer. RSS no Linux, working set no Windows
(são aproximações diferentes); FPS = frames efetivamente renderizados por
intervalo, não taxa máxima da GPU. Frame time = trabalho CPU de construção da
UI, não GPU/present/vsync. Startup = entrada em main até primeira atualização
UI, não tempo até pixels apresentados. `--smoke-test` permite execução gráfica
limitada; `--metrics` habilita painel em release. Registrar medições, ambiente
e limitações no documento de performance. Metas provisórias Windows: binário
<20 MiB, working set idle <80 MiB, primeira UI <300 ms (warm), CPU idle <1% de
um core sem painel; são metas de aceitação, não resultados comprovados.

## Extensão futura

Autenticação/DMs/attachments/reactions/notificações dependem de capabilities
oficiais. Voice é serviço separado com comandos/eventos, ownership de áudio e
política de recursos; overlay é frontend separado que consome snapshots e actions.
Console Mode/Steam Input podem injetar AppActions via adaptador, sem dependência
obrigatória ou acoplamento do core. Deep links/launcher/auto-update ficam em
platform/app com validação de argumentos, assinatura e consentimento adequado.
