# Instruções persistentes — Rustcord

## Produto

- Rustcord é primeiro um cliente desktop nativo em Rust com design e usabilidade
  familiares ao Discord. Não inventar um design de console/TV nesta fase.
- Não usar Electron, Chromium, WebView ou carregar discord.com/app.
- Gamepad/TV/Console Mode não são prioridade atual; adaptador experimental fica
  opcional e fora do build/runtime padrão.
- Backend atual é mock. Não implementar self-bot, tokens pessoais, endpoints
  privados ou bypasses. Conferir limites oficiais antes de integração real.
- Manter domínio/backend, estado, UI, input e plataforma separados.

## Processo solicitado pelo usuário

- Seguir [CONTRIBUTING.md](CONTRIBUTING.md), baseado no Git Flow da Atlassian.
- Começar implementações em `feature/*` a partir de `develop`.
- Fazer incrementos pequenos; validar cada incremento e criar seu commit antes
  de avançar ao próximo. Registrar os testes realizados no corpo do commit.
- Rust: fmt, clippy e testes apropriados antes do commit. UI funcional requer
  também verificação gráfica relevante antes de integrar a feature.
- Documentação/CI: validação adequada ao tipo de mudança. Não criar testes que
  apenas reproduzem a implementação ou testes desnecessários para texto/estilo.
- Não commitar diretamente em main/develop; integrar com merge --no-ff.
- Releases partem de develop e retornam a main/develop com tag; hotfixes partem
  de main e retornam às duas, e à release ativa se aplicável.
- Não reescrever histórico compartilhado nem commitar segredos/artefatos.
- Concluir mudanças autorizadas sem pedir confirmações repetidas.
