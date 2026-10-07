# Conexão nativa experimental — 07/10/2026

O app inicia em um painel nativo inspirado no [login oficial](https://discord.com/login).
A referência foi inspecionada em perfil temporário sem sessão, a 1280 × 800: cartão
784 × 412, padding 32, formulário à esquerda e QR 176 × 176 à direita. O layout,
cores e fundo em degradê são desenhados em egui; não carregamos HTML, scripts,
fontes proprietárias, logos ou ilustrações do site. Campos de e-mail/senha estão
desativados e indicam indisponibilidade; o botão funcional inicia o QR.

 **Entrar com QR** inicia uma tentativa; o dono da
conta escaneia pelo app Discord no celular e confirma ali. **Cancelar** interrompe
a tentativa. **Sair** descarta a sessão e dados de exibição. Fechar o app também
encerra a sessão local: não existe persistência, cofre ou login automático. Sair
não chama um endpoint de revogação remota nesta etapa.

Depois de conectar: escolher servidor → canal de texto → até 50 mensagens.
**Atualizar** repete a leitura; o Gateway entrega mensagens novas, edições (releitura
com debounce de 750 ms), exclusões e exclusões em lote. REST permanece disponível
quando a conexão ao vivo falha. Avatares de usuários/servidores, anexos com link,
prévias estáticas de imagens, títulos/descrições de embeds e referência de resposta
são exibidos. Downloads de imagens aceitam somente os hosts HTTPS do CDN Discord,
sem Authorization, preservando parâmetros de URLs assinadas. Imagens externas de
embeds só carregam quando há URL do proxy Discord. GIF é apresentado como imagem
estática. Nomes de membros e status são recebidos pelo serviço, sem fabricar presença.

Categorias, canais de voz/stage e fóruns aparecem com apresentação própria. Selecionar
voz não tenta ler esse canal como texto: a tela informa que áudio ainda não está
implementado. Não há envio, reactions de rede, DMs, paginação de histórico, navegação
de threads/fóruns, transporte de áudio/vídeo nem login persistente. Markdown/mentions
continuam como texto; o timestamp é exibido em UTC quando retornado com esse timezone.
O modo **demonstração local** continua separado e não escreve na rede.

## Implementação

`discord-network` não depende de UI, browser ou WebView. Um worker dedicado roda
um runtime Tokio current-thread. Eventos/comandos têm oito slots; a UI nunca
aguarda I/O. Drop dispara cancelamento por oneshot, interrompendo I/O e publicação
bloqueada em fila cheia. Geração RSA acontece no worker, fora da UI; durante esse
cálculo síncrono a interrupção só é observada quando ele termina.

Remote Auth v2: hello → chave pública RSA-2048/SPKI → nonce OAEP-SHA256 → prova
SHA256/base64url → fingerprint/QR → identidade escaneada → aprovação e ticket →
troca HTTPS → token criptografado. É interoperabilidade com um protocolo não
OAuth público, sem garantia de compatibilidade ou aprovação oficial. O cliente
se identifica como Rustcord; não copia headers de um navegador. Rejeições e
desafios interrompem o fluxo, sem fallback de senha ou mecanismos de bypass.

O worker conserva a credencial em memória; nenhum evento entrega o token à UI.
Buffers de segredo são Zeroizing; o header Authorization é marcado sensitive.
Não há logs de payloads, URLs QR, tickets ou credenciais. Isso não garante
apagamento de todas as cópias temporárias internas das bibliotecas HTTP/TLS.
Erros expostos são mensagens estáticas, sem corpo bruto retornado pelo servidor.
HTTPS valida certificados e redirecionamentos estão desativados.

Limites: 64 KiB por mensagem/frame de Remote Auth e resposta de login; 4 MiB por
resposta REST antes do parse JSON; até mil servidores, mil canais por resposta,
50 mensagens e 32 KiB de texto por mensagem. Esses limites não equivalem a um
orçamento total de heap: os modelos JSON e de exibição também alocam memória.
A UI mantém somente os canais de um servidor e o histórico de um canal.
401 encerra a sessão; 403 não fabrica mensagens; 429 aplica cooldown compartilhado
às leituras, sem retry automático. Durante uma leitura a seleção fica bloqueada,
mas Sair continua disponível. Respostas de outros IDs não mudam a conversa atual.

## Gateway, membros e mídia

WebSocket JSON v9, sem compressão, identifica o app como Rustcord. Heartbeat com
ACK e número de sequência; READY/RESUMED inicializam as assinaturas. Reconexão
limitada a cinco tentativas adicionais com backoff 1/2/4/8/16 s; Resume usa sessão
em memória e sequência, e sessão inválida descarta esses dados. Rejeição de
credenciais volta ao login. Ao reconectar a UI relê o histórico para cobrir lacunas.
Não há polling REST contínuo nem chamadas de escrita de mensagens.

A assinatura de membros (opcode 37) pede só posições 0–199 do canal atual e
remove a assinatura do servidor anterior ao mudar para outro canal/servidor.
SYNC/INSERT/UPDATE/DELETE/INVALIDATE são reduzidos em ordem, conservando grupos
como posições sem inventar membros. O painel é uma janela limitada da lista do
canal, não a lista completa do servidor. Indicador de carregamento vira aviso
após 20 s sem resposta. Mudanças de estado de voz recebidas para o servidor atual
podem mostrar participantes; ocupação inicial completa de todas as salas ainda
não é garantida. Não são solicitados microfone nem dispositivos de áudio.

Gateway limita mensagens/frames a 8 MiB. CDN: corpo até 8 MiB, dimensões até
8192 × 8192 e limite de alocação de decode de 32 MiB; thumbnails até 320 × 240.
A UI conserva até 64 texturas em cache LRU por sessão e inicia um download por
vez, apenas para imagens visíveis. Mídia, REST e Gateway são futuros separados;
uma imagem lenta não bloqueia o heartbeat nem a leitura de histórico. Filas e
sessão são canceladas ao sair. Fotos ausentes/falhas usam inicial, sem download
externo alternativo. Esses limites não constituem benchmark de RAM do processo.

## Origem e validação

Implementação independente, sem importar fontes de outro cliente. A sequência
Remote Auth foi estudada na [discovery](native-backend-discovery.md), especialmente
[Litecord no snapshot MIT](https://github.com/Ak4ai/Litecord/blob/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b/src/auth/remote_auth.rs).
Não adotamos seu cofre, headers de navegador ou fallback de plaintext. Não há
código GPL de Concord/Dissent/Abaddon incorporado.

Formato de avatares/CDN segue a [referência oficial](https://github.com/discord/discord-api-docs/blob/main/developers/reference.mdx).
A assinatura de membros é interoperabilidade de protocolo observada por
[pesquisadores do Userdoccers](https://github.com/discord-userdoccers/discord-userdoccers/issues/191),
implementada independentemente, sem importar fontes de clientes GPL.
Leituras seguem estruturas de [User](https://docs.discord.com/developers/resources/user),
[Guild](https://docs.discord.com/developers/resources/guild) e
[Channel](https://docs.discord.com/developers/resources/channel).
O uso de sessão pessoal não é equivalente a OAuth2 público: os limites registrados
na discovery continuam distintos do funcionamento técnico.

Testes sintéticos exercitam o fluxo completo WebSocket/RSA/ticket, nonce inválido,
expiração, cancelamento, aprovação fora de ordem, fila cheia/espera interrompida,
IDs grandes sem perda de precisão, isolamento de respostas, 401/403/429 e corpo
HTTP acima do limite. Eles não provam funcionamento com uma conta real.
A confirmação de login e comparação de canais/mensagens com o cliente oficial
exigem validação pelo dono da conta. Windows/GPU/DPI também seguem pendentes.
Economia de RAM é objetivo, ainda sem benchmark equivalente com Discord online.

Verificação gráfica em macOS: painel de login e geração de QR real confirmados
na janela do build release. O dono da conta confirmou o QR no celular; perfil, servidores, canais e mensagens
reais foram observados na janela. Não houve envio nem comparação sistemática com
o cliente oficial.
Esse fluxo anterior tinha 45 testes; o incremento de conteúdo/Gateway tem 55
testes, fmt/clippy estrito e build release. Os novos fluxos foram exercitados
com transportes locais e render gráfico sintético; validação live com a conta
do usuário e Windows permanece pendente.

## Layout conectado

A tela usa rail de servidores 72 px, sidebar de canais 240 px, perfil no rodapé,
conversa com fotos, autor, timestamp, corpo selecionável, anexos/embeds e painel
direito de membros 224 px. O botão Membros alterna esse painel. Servidores/membros
são virtualizados e canais respeitam categorias/posição; fotos baixadas substituem
as iniciais. Falhas REST são mostradas na conversa, sem parecer um canal vazio.
URLs/textos longos quebram dentro da conversa. O rodapé identifica envio ainda
indisponível e distingue conexão ao vivo carregando/conectada/desconectada.

O layout foi inspecionado por render dos meshes/font atlas/texturas egui a
1280 × 800 com dados sintéticos: imagem/anexo, avatar, voz e membros com status.
Sem controlar o desktop nem aprovar um QR pelo usuário. Testes também cobrem
mensagem nova/exclusão durante histórico em andamento e respostas de outro canal.
A validação interativa da nova versão com conta real/Windows continua pendente.

Símbolos: fallback opcional Apple Symbols (macOS), Segoe UI Symbol (Windows),
Noto Sans Symbols2/DejaVu Sans (Linux), lendo um único arquivo do sistema de até
4 MiB. Fontes do sistema não são redistribuídas e a cobertura depende do SO; isso
não implementa emoji colorido nem cobertura universal de idiomas. Formas verticais
de apresentação Unicode nos nomes são mostradas como barras compatíveis,
preservando os nomes originais no domínio e os IDs das conversas.
