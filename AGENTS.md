# Instruções persistentes — Rustcord

## Produto

- Rustcord é primeiro um cliente desktop nativo em Rust com design e usabilidade
  familiares ao Discord. Não inventar um design de console/TV nesta fase.
- Não usar Electron, Chromium, WebView ou carregar discord.com/app.
- Gamepad/TV/Console Mode não são prioridade atual; adaptador experimental fica
  opcional e fora do build/runtime padrão.
- Construir um cliente alternativo open source para conta pessoal, com login
  nativo e interoperabilidade de rede. O usuário autorizou essa direção em
  07/10/2026, substituindo a restrição anterior ao backend mock.
- Não contornar MFA/CAPTCHA, extrair sessões de outros aplicativos ou automatizar
  spam. Credenciais somente no processo/cofre, nunca em logs, chat ou commits.
- Não confundir interoperabilidade com aprovação oficial do Discord. Validar
  consumo de RAM com medições; não prometer economia antes de medir.
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
