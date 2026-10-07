# Contribuição e Git Flow

Seguimos o [Git Flow da Atlassian](https://www.atlassian.com/git/tutorials/comparing-workflows/gitflow-workflow),
com commits pequenos e verificações antes de cada commit.

## Branches

| Branch | Origem | Destino | Uso |
|---|---|---|---|
| `main` | bootstrap / release | — | Histórico de versões publicadas e estáveis |
| `develop` | `main` | release | Integração do próximo lançamento |
| `feature/<descricao>` | `develop` | `develop` | Implementações, melhorias e documentação |
| `release/<versao>` | `develop` | `main` e `develop` | Estabilização e preparação de versão |
| `hotfix/<descricao>` | `main` | `main` e `develop` | Correção urgente de versão publicada |

Não implementar diretamente em `main` ou `develop`. Usar merges `--no-ff` para
preservar o histórico das branches. Uma feature não vai diretamente a `main`.
Release recebe apenas estabilização, correções, metadados e documentação de
lançamento. Ao publicar, criar tag anotada `vX.Y.Z` em `main` e reintegrar a
release em `develop`. Hotfix também deve chegar à release ativa, se houver.

Neste estágio, `main` contém somente o bootstrap/licenças; `develop` contém a
fundação em desenvolvimento. `0.1.0` no Cargo é uma versão de trabalho, não uma
release publicada. As branches da fundação foram mantidas para consulta.

## Incrementos e commits

- Uma intenção pequena e revisável por commit, incluindo teste quando necessário.
- Testar cada incremento antes de registrá-lo; não acumular toda uma feature
  grande para criar um único commit no final.
- Não criar retrospectivamente uma sequência fictícia de implementação/testes.
  Código anterior à adoção de Git Flow é registrado como baseline existente.
- Mensagens seguem `tipo(escopo): descricao` (`feat`, `fix`, `refactor`, `test`,
  `docs`, `chore`, `ci`). Registrar no corpo verificações e limitações relevantes.
- Nunca registrar tokens, caches, binários, artefatos temporários ou toolchains.
- Não reescrever commits compartilhados. Correções posteriores recebem novos
  commits. Resolver conflitos e validar o resultado antes de concluir o merge.

Exemplo de implementação:

```sh
git switch develop
git switch -c feature/message-composer
# Implementar um incremento pequeno e validar.
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
git add crates/ui/src/lib.rs
git diff --cached --check
git commit -m "feat(ui): improve message composer" -m "Validation: fmt, clippy and workspace tests passed."
# Repetir incrementos/commits até concluir a feature.
git switch develop
git merge --no-ff feature/message-composer
```

Antes de cada commit Rust: formatter, clippy sem warnings e testes apropriados.
Antes de integrar uma feature funcional: testes do workspace e build release;
para UI/interações, verificar também o fluxo gráfico relevante. Registrar quando
Windows ou hardware real não puder ser testado. Alterações de documentação
precisam de revisão e verificação de links; CI precisa de validação do YAML e
comandos correspondentes. Não é necessário inventar testes para texto ou estilo.

O repositório público é [lippdev/Rustcord](https://github.com/lippdev/Rustcord),
com remote `origin`. A branch padrão inicial é `develop`, para expor o protótipo;
`main` permanece reservada às releases. Publicar branches `feature/*` e abrir
pull requests para `develop`; preservar os commits e integrar com merge commit.
Releases e hotfixes seguem os destinos definidos acima.
O executável está disponível em `develop`; instruções no [README](README.md).
