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
**Atualizar** repete a leitura, sem polling automático. Não há envio, reactions,
Gateway de chat, DMs, anexos, threads, fóruns ou voz. Conteúdo textual é exibido
sem interpretar Markdown/mentions. Os horários são os timestamps retornados pelo
serviço. O modo **demonstração local** permanece separado e não escreve na rede.

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

## Origem e validação

Implementação independente, sem importar fontes de outro cliente. A sequência
Remote Auth foi estudada na [discovery](native-backend-discovery.md), especialmente
[Litecord no snapshot MIT](https://github.com/Ak4ai/Litecord/blob/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b/src/auth/remote_auth.rs).
Não adotamos seu cofre, headers de navegador ou fallback de plaintext. Não há
código GPL de Concord/Dissent/Abaddon incorporado.

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
Workspace: 45 testes passaram, fmt/clippy estrito e build release passaram.

## Layout conectado

A tela de leitura usa a mesma linguagem visual da demonstração: rail de servidores
72 px, sidebar de canais 240 px, perfil no rodapé e conversa com header, avatar por
inicial, autor, timestamp compacto e corpo selecionável. Listas de servidores e
canais são virtualizadas; nomes compridos são truncados com tooltip. URLs/textos
longos quebram dentro da conversa. Sem dados de membros/avatares CDN, não exibimos
membros fictícios. O rodapé identifica envio ainda indisponível.

O layout foi inspecionado por render de meshes/font atlas egui com dados sintéticos,
sem abrir janela ou controlar o desktop. A sessão do usuário foi preservada na
versão anterior; validação interativa do novo layout/Windows continua pendente.
