# Discovery: conexão nativa ao Discord

Data: 07/10/2026. Base Rustcord: `5b25205` em `develop`.

## Resultado e recomendação

O objetivo confirmado é um cliente alternativo para a conta pessoal, com UI em
Rust e sem Electron, Chromium ou WebView. A recomendação é manter nossa UI
egui/glow e construir um adaptador de rede próprio, reaproveitando componentes
selecionados do Serein depois de revisão. Não importar o aplicativo inteiro.

Serein é o candidato mais próximo em linguagem, separação de camadas e licença.
Uma prova de extração compilou os seis crates necessários a REST/Gateway sem
dependências de UI/WebView. Isso demonstra separação de build, **não demonstra
login ou comunicação real com uma conta Discord**. O login sem WebView continua
sendo a principal questão a resolver antes de chamar o app de funcional online.

## Método e snapshots

Foram consultados repositórios dos autores, manifests, licenças e código. Os
projetos abaixo foram clonados em diretório temporário, sem instalar executáveis,
abrir suas interfaces ou fornecer contas/credenciais. Datas de push não provam
manutenção regular, funcionamento em produção ou qualidade de segurança.

| Projeto | Snapshot inspecionado | Implementação / licença declarada | Uso recomendado |
|---|---|---|---|
| [Serein](https://github.com/ViceVerse-cz/Serein/tree/71152f29695ccf9daac5fd58aa12113a071f64f5) | `71152f29695ccf9daac5fd58aa12113a071f64f5` | Rust, egui/wgpu; MIT OR Apache-2.0 | Primeira referência e candidato a extração seletiva |
| [Litecord](https://github.com/Ak4ai/Litecord/tree/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b) | `e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b` | Rust, Slint; MIT | Referência adicional; não adotar diretamente |
| [Concord](https://github.com/chojs23/concord/tree/5bac7a24165d975eb140192d4f62e08ddea282ce) | `5bac7a24165d975eb140192d4f62e08ddea282ce` | Rust, ratatui; GPL-3.0-only | Referência de comportamento e login QR; sem copiar para o código permissivo |
| [Dissent, antigo gtkcord4](https://github.com/diamondburned/dissent/tree/6ff6182b1eac30d57e9c9c995942317eb42e3bf6) | `6ff6182b1eac30d57e9c9c995942317eb42e3bf6` | Go, GTK4; GPL-3.0 | Referência de UX e divisão entre UI/sessão |
| [Arikawa](https://github.com/diamondburned/arikawa/tree/b430932b3ee153985e90bf4f469b3e8241b90c8b) | `b430932b3ee153985e90bf4f469b3e8241b90c8b` | Biblioteca Go; ISC | Referência de protocolo; integração direta acrescentaria Go/FFI ou processo auxiliar |
| [Abaddon](https://github.com/uowuo/abaddon/tree/7b3a4ff97ae6490a15adaa0a792ea9225c4c9e51) | `7b3a4ff97ae6490a15adaa0a792ea9225c4c9e51` | C++, GTK3; GPL-3.0 | Referência de chat/voz; não é uma biblioteca Rust |
| [Swiftcord](https://github.com/SwiftcordApp/Swiftcord/tree/d1d1ee14535e2ef20b9da25d67281b09267adaf3) | `d1d1ee14535e2ef20b9da25d67281b09267adaf3` | Branch `v2`: página de apresentação, sem backend disponível no snapshot | Excluir como base de código; README aponta código legado em `main` |

O resultado de busca `TBNRFPS01/serein` é um fork; a inspeção e recomendação usam
o upstream `ViceVerse-cz/Serein`. Não confundir clientes de Discord com projetos
de chat independentes que apenas imitam sua aparência.

## Por que Dorion e Equicord não fornecem essa base

[Dorion](https://github.com/SpikeHD/Dorion) usa Tauri/WebView.
[Equicord](https://github.com/Equicord/Equicord) modifica o cliente existente.
[Equibop](https://github.com/Equicord/Equibop/blob/main/src/main/mainWindow.ts)
cria uma janela Electron e carrega `discord.com/app`. Eles preservam o programa
web que já implementa autenticação, estado, rede e mídia. Tirar o navegador
remove também esse programa: é preciso implementar essas camadas nativamente.
CSS, plugins e patches de módulos JavaScript não se transferem diretamente.

## Serein: o que pode ser separado

Os manifests de [REST](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/crates/discord-api/Cargo.toml)
e [Gateway](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/crates/discord-gateway/Cargo.toml)
dependem de `model`, `discord-protocol`, `client-core` e, transitivamente,
`session-cache`. Não dependem de `platform`, `ui`, `wry` ou `eframe`.

- REST usa reqwest/rustls, concorrência limitada e cooldown compartilhado;
  trata erros de autorização/rate limit e evita repetição automática de escritas
  cujo resultado ficou incerto.
- Gateway usa Tokio/tokio-tungstenite, zlib-stream, heartbeat/ACK, reconexão e
  Resume, com limites de payload e eventos.
- Modelos preservam IDs e distinguem patches ausentes, nulos e preenchidos.
  Segredo de sessão usa zeroize e Debug redigido.
- Os testes de rede examinados usam servidores HTTP/WebSocket locais e dados
  sintéticos; a documentação registra comportamentos ainda sem validação live.

Fontes: [REST](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/crates/discord-api/src/lib.rs),
[Gateway](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/crates/discord-gateway/src/lib.rs),
[segredo de sessão](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/crates/client-core/src/auth.rs).

Limitações de reaproveitamento:

1. `discord-api`/`discord-gateway` emitem comandos/eventos do `client-core` do
   Serein. Não implementam nossa trait `Backend`; um adaptador e conversão de
   modelos são necessários. A extração traz também um segundo reducer/cache.
2. Os seis crates têm aproximadamente 82 mil linhas Rust, incluindo testes.
   Importá-los integralmente ampliaria muito o protótipo. Preferir um recorte
   mínimo para sessão, navegação e histórico, com origem/licenças preservadas.
3. O workspace declara Rust 1.98; Rustcord declara 1.92. O experimento usou
   1.98.1 e não confirma suporte a 1.92. Tratar MSRV na implementação futura.
4. O renderer wgpu, o login, áudio, extensões, SQLite e assets do app Serein
   não fazem parte do recorte. TLS traz dependências nativas como aws-lc-sys;
   ausência de WebView não significa ausência de código C no build.
5. A [documentação de autenticação](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/docs/authentication.md)
   descreve login hospedado em WebView temporário, handoff não OAuth e entrada
   manual de sessão. Não atende automaticamente ao requisito sem WebView.

## Litecord: motivos para não escolher o backend integral

[Gateway](https://github.com/Ak4ai/Litecord/blob/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b/src/gateway/client.rs)
mistura conexão com coordenação de voz. O módulo de
[credenciais](https://github.com/Ak4ai/Litecord/blob/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b/src/auth/vault.rs)
importa tipos da UI Slint. No snapshot, a chave de criptografia não Windows é
derivada de machine-id, usuário e constante: esses dados não são um segredo do
usuário. A gravação também tem fallback JSON sem criptografia quando a proteção
falha. Não reutilizar esse armazenamento no Rustcord.

São achados de inspeção estática, não uma auditoria completa. Os benchmarks dos
READMEs não foram reproduzidos; não usá-los como metas já alcançadas pelo Rustcord.

## Login inteiramente nativo

Há implementações de QR/Remote Auth em
[Concord](https://github.com/chojs23/concord/blob/5bac7a24165d975eb140192d4f62e08ddea282ce/src/discord/qr_auth.rs)
e [Litecord](https://github.com/Ak4ai/Litecord/blob/e834aeef1d362a7f25e10b6c9b0cabf9b5f4200b/src/auth/remote_auth.rs).
[Arikawa](https://github.com/diamondburned/arikawa/blob/b430932b3ee153985e90bf4f469b3e8241b90c8b/api/remote_auth.go)
também tem troca de ticket. É um candidato técnico a login sem navegador
embutido, com confirmação pelo dono da conta no celular. Não é OAuth2 público,
nem prova de aceitação atual pelo serviço. Não foi exercitado nesta discovery.

Um OAuth2 aberto no navegador externo não libera, por si só, o acesso completo
da conta pessoal. Login por senha pode exigir desafios que uma UI puramente
nativa não consegue apresentar. Portanto, o próximo estudo de autenticação deve
validar o fluxo QR e seus estados de expiração/cancelamento/rejeição, sem assumir
que existe fallback automático. Não contornar MFA/CAPTCHA nem extrair sessões de
outros aplicativos. Credenciais ficam no app/cofre do sistema, nunca em chat,
logs, fixtures ou commits; sem cofre, trabalhar apenas na sessão em memória.

## Licenças e relação com o Discord

Serein declara [MIT](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/LICENSE-MIT)
ou [Apache-2.0](https://github.com/ViceVerse-cz/Serein/blob/71152f29695ccf9daac5fd58aa12113a071f64f5/LICENSE-APACHE),
próximas da licença do Rustcord. A incorporação deve manter atribuições,
licenças e proveniência, além de revisar dependências do recorte efetivamente
distribuído. A licença da raiz não cobre automaticamente bibliotecas e assets.
GPL de Concord/Dissent/Abaddon não deve ser copiada para depois declarar o
resultado inteiro como MIT/Apache; qualquer adoção exigiria revisar a estratégia
de distribuição. Arikawa é ISC, mas continua sendo código Go.

A direção escolhida é estudar interoperabilidade de um cliente alternativo,
não transformar o produto em um bot. Isso não equivale a autorização do Discord:
os [scopes públicos](https://docs.discord.com/developers/topics/oauth2) continuam
limitados e a [regra de self-bots](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)
continua aplicável. Aprovação para publicar não é uma dependência técnica de
compilação; suporte oficial, aceitação técnica e permissão contratual são questões
distintas. Não foi identificada aprovação oficial para esses clientes.

## Primeiro incremento proposto

1. IDs estáveis para conta/servidor/canal/mensagem e rascunhos por ID; remover a
   dependência dos índices do mock antes de receber snapshots reais.
2. Interface assíncrona de backend com comandos/eventos, cancelamento, geração
   de sessão e filas limitadas. Preservar mock para validação determinística.
3. Recorte REST/Gateway com fixture de READY, histórico paginado e eventos
   create/update/delete. Testar HTTP 401/403/429, heartbeat sem ACK, Resume
   recusado, payload limitado e eventos atrasados de sessão anterior.
4. Spike separado de login QR nativo e cofre do sistema. Tratar rejeição de
   desafio como interrupção; não incluir mecanismos de bypass.
5. Validação manual pelo dono da conta: login, canais e uma página de histórico;
   depois uma mensagem explícita e resposta recebida, comparadas no cliente
   oficial. Conexão aberta ou testes locais não contam como chat real funcionando.

Voz, streaming e plugins ficam depois desse fluxo. Áudio requer transporte,
dispositivos, codecs e criptografia DAVE; referências atuais incluem o
[libdave oficial](https://github.com/discord/libdave) e código DAVE no Abaddon.
Não assumir que um transporte antigo de voz permanece compatível.

## Evidência do experimento

Foi criado um workspace temporário com os seis crates listados acima, sem alterar
seus fontes. O manifest manteve as definições herdadas de package, dependências
utilizadas e lints; os demais membros, patches de mídia/login e dependências
não utilizadas foram excluídos. As licenças raiz foram conservadas.

O lockfile original precisou ser ajustado para o novo grafo: a tentativa inicial
com `--locked` recusou a atualização. A execução sem `--locked` gerou o lockfile
do recorte. Isso não é uma validação do workspace desktop completo do upstream.

- Ambiente: macOS, rustc/cargo 1.98.1.
- `cargo check --workspace --all-targets`: passou no workspace extraído.
- `cargo test -p discord-api -p discord-gateway --locked --lib`: passou,
  62 testes REST e 49 Gateway (111 no total).
- `cargo metadata --locked --format-version 1`: 196 pacotes no grafo resolvido,
  incluindo dependências para outros targets; nenhum `wry`, `webkit`, `tauri`,
  `gtk`, `egui`, `eframe`, `wgpu` ou pacote de WebView. Esse número não representa
  196 pacotes compilados no macOS nem uma auditoria de licenças completa.
- SHA-256 do lockfile do recorte:
  `7858888755ced8253961030d23e962d762ed81f9ee7dad00e64eedf1583df2ba`.
- Sem validação gráfica, benchmarks, build Windows ou autenticação real.
- Nenhum código/dependência de terceiros foi incorporado ao build Rustcord.

Para reproduzir: obter o snapshot Serein indicado, copiar os seis diretórios de
`crates/` para um workspace separado; no manifest raiz, manter apenas esses
members e as entradas workspace usadas por seus manifests; manter package/lints,
remover patches não utilizados e copiar as licenças. Copiar o lockfile upstream,
executar o check acima para ajustar o grafo, depois executar
`cargo test -p discord-api -p discord-gateway --locked --lib` e
`cargo metadata --locked --format-version 1` para conferir a ausência de UI.
