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
3. **Gate da API oficial**: confirmar caso de uso suportado/aprovado. OAuth2 não
   autoriza automaticamente um substituto completo de contas pessoais. Se não
   houver API adequada, manter mock ou adaptar a companion oficialmente permitido.
4. **Backend autorizado**: OAuth2 e armazenamento seguro quando aplicáveis,
   REST/rate-limit, Gateway/reconnect/intents, cache limitado e eventos separados.
5. **Chat completo dentro das capabilities**: servidores/canais/DMs, attachments,
   reactions, notificações, busca e deep links conforme acesso oficial disponível.
6. **Áudio e distribuição**: voice/mute/deafen/volume via integração permitida;
   launcher/auto-update assinado, overlay isolado e serviços de plataforma.
7. **Controle e Console Mode**: reavaliar depois da usabilidade desktop. Adaptador
   de actions pode ser reaproveitado, sem ditar o visual ou acoplar o projeto.

Não implementar self-bots, tokens pessoais ou endpoints privados. Voice, vídeo,
streaming, Activities, plugins e overlay continuam fora da fundação inicial.
