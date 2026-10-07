# Rustcord — roadmap revisado

O esclarecimento do usuário substitui a prioridade inicial de TV/gamepad:
primeiro um Discord nativo em Rust, com design e usabilidade familiares.

1. **M1 — fundação desktop mock**: workspace, janela nativa, rail de servidores,
   canais, conversa, membros, perfil, composer, troca de canais, mensagens e
   reações locais, menu simples, métricas e CI. Sem runtime de gamepad/modo TV.
2. **M2 — fidelidade e usabilidade desktop**: comparar com referência do Discord
   escolhida pelo usuário, melhorar tipografia/ícones, seleção/cópia de texto,
   mensagens agrupadas, scroll/virtualização, composer multiline/IME, atalhos,
   menus e acessibilidade. Validar Windows/DPI e medir startup/working set/CPU.
3. **Discovery do backend nativo**: [concluída](native-backend-discovery.md).
   Objetivo confirmado: conta pessoal e ausência de WebView. Serein é candidato
   a reaproveitamento seletivo; login QR sem WebView ainda exige spike. OAuth2
   comum não libera um substituto completo, e interoperabilidade não é aprovação.
4. **Backend alternativo nativo**: começar com IDs estáveis e comandos/eventos
   assíncronos; depois REST/rate-limit, Gateway/reconnect/Resume, cache limitado,
   autenticação nativa e cofre do sistema. Validar primeiro com transportes locais
   e depois com uma sessão controlada pelo dono da conta.
5. **Chat conectado**: servidores/canais/DMs e mensagens primeiro; depois
   attachments, reactions, notificações, busca e deep links, conforme capacidades
   efetivamente implementadas e verificadas.
6. **Áudio e distribuição**: voice/mute/deafen/volume via integração permitida;
   launcher/auto-update assinado, overlay isolado e serviços de plataforma.
7. **Controle e Console Mode**: reavaliar depois da usabilidade desktop. Adaptador
   de actions pode ser reaproveitado, sem ditar o visual ou acoplar o projeto.

Não automatizar ações abusivas, extrair credenciais de outros aplicativos ou
contornar MFA/CAPTCHA/restrições do serviço. Voice, vídeo, streaming, Activities,
plugins e overlay continuam fora da fundação inicial. A discovery registra os
limites dos termos e as incertezas da conexão alternativa; não houve sessão real.

## Progresso M2 — composer e ações de mensagem

Concluído: editor multiline, Enter/Shift+Enter, rascunhos por canal, undo/cursor
isolados, proteção de envio durante composição IME/repeat, menu contextual
(mouse/F2), cópia de texto, resposta editável com referência e cancelamento por
Escape. Tudo permanece local/mock, sem dependências novas.

Próximo incremento recomendado: agrupamento de mensagens por autor/horário e
histórico virtualizado, preservando scroll ao receber mensagens/alterar canal.
Validar longos históricos, seleção/cópia e resize; testar Windows/MSVC com GPU,
DPI e IMEs reais antes de release. A fidelidade visual ainda exige refinamento de
ícones/tipografia e comparação com uma referência desktop específica do Discord.
