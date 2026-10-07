# Rustcord — baseline da fundação desktop

Medição em 07/10/2026, Linux x86_64, Rust 1.99.0, release (`opt-level=s`, thin
LTO, strip), janela 1280×800 em Xvfb, Mesa via `LIBGL_ALWAYS_SOFTWARE=1`.
Sem runtime gamepad no build padrão. As medições não representam Windows/GPU real.

| Métrica | Resultado |
|---|---:|
| Binário Linux | 7.574.512 bytes / 7,22 MiB |
| Startup warm, main até primeira atualização de UI | 59,70 ms |
| RSS idle, métricas desligadas | 83.030.016 bytes / 79,18 MiB |
| RSS ao final do smoke | 83.423.232 bytes / 79,56 MiB |
| CPU idle por 3 segundos | 0 ticks registrados |
| CPU de construção da UI, último frame smoke | 0,229 ms |

Uma execução anterior, sem cache gráfico aquecido, registrou ~122 ms e ~88 MiB.
Não é uma amostragem estatística de startup frio. O custo do renderer de software
está incluído no RSS; não há Electron/Chromium/WebView/WGPU no processo. Zero ticks
em 3 s significa atividade abaixo da resolução dessa amostra, não consumo zero
em qualquer cenário. Não há histórico de performance de produção ainda.

FPS do painel mede frames realmente renderizados por intervalo de 1 s. A janela
redesenha sob demanda; debug/metrics cria uma atualização a cada segundo. Frame
time mede somente construção CPU da UI, não GPU/present/vsync. Startup também
não mede tempo até os pixels ficarem visíveis. Windows amostrará working set,
que não é uma métrica idêntica ao RSS Linux.

## Verificação realizada

- Formatter, clippy sem warnings e testes do reducer e adaptador de teclado.
- Build release Linux e cargo check para x86_64-pc-windows-gnu.
- Smoke gráfico: selecionar servidor/canal, enviar mensagem, reagir, abrir/fechar
  configuração, assertar estado e encerrar normalmente.
- Eventos nativos XTest com foco real na janela: clicks em servidor/canal, F1,
  Escape, digitar texto no composer e Enter. Capturas verificadas visualmente
  mostram a configuração e a mensagem `native input works` enviada.
- Árvore do build padrão sem gilrs/libudev, networking async ou browser.

Windows foi verificado em compilação/type checking; executável MSVC, drivers,
DPI, startup real e working set exigem máquina Windows. CI está configurada mas
não foi publicada/executada remotamente nesta sessão.

## Reproduzir

```sh
cargo build -p rustcord --release --locked
python3 scripts/measure.py
```

O script requer Linux, uma sessão gráfica disponível em DISPLAY e OpenGL. Grava
`artifacts/metrics.json`. O ambiente virtual desta sessão utilizou Xvfb extraído
localmente; essa preparação é somente infraestrutura de teste, não dependência
do produto. O smoke não valida autenticação nem qualquer API Discord real.

Próximo baseline: Windows/MSVC com GPU real, 10+ amostras cold/warm, histórico
longo, resize/DPI, seleção de texto, IME e perfil CPU/alocações. Meta provisória
<20 MiB de binário, <80 MiB working set idle, startup warm <300 ms e CPU idle
<1% de um core; nenhuma meta Windows está comprovada por estes números Linux.
