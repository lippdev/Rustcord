# Rustcord

Discord-style desktop chat UI in Rust, without Electron, Chromium or WebView.
Native window and rendering through egui/eframe + OpenGL; Windows is the primary
target. Familiar Discord layout: server rail, channels, messages, members,
profile and message composer.

![Current native mock UI](docs/preview.png)

**Experimental native QR login and read-only Discord access are implemented.**
Start with **Entrar com QR**, scan using the Discord mobile app, and confirm there.
Select a server and text channel to fetch the latest 50 messages; **Atualizar**
reloads them. Session credentials stay inside the network worker in memory, with
no password/token entry or saved login. This is an independent alternative client,
not an officially approved integration. Live account validation is still pending.

Sending, live chat updates, DMs, attachments and voice are not implemented online.
**Abrir demonstração local** opens the earlier mock/composer prototype; its sends
and reactions never reach Discord. The screenshot above shows that local demo.
See [native connection](docs/native-connection.md) for implementation and validation,
and [backend discovery](docs/native-backend-discovery.md) for the research.

## Run

Install [Rust stable](https://rustup.rs/) (minimum 1.92). On Windows, install
Visual Studio Build Tools with Desktop development with C++ and Windows SDK.
Use a working OpenGL graphics driver.

```sh
git clone https://github.com/lippdev/Rustcord.git
cd rustcord
git switch develop
cargo run -p rustcord
cargo run -p rustcord --release
cargo run -p rustcord --release -- --metrics
```

Linux development needs a C linker, pkg-config, libudev only for the optional
gamepad feature, an OpenGL runtime, libxkbcommon, and an X11/Wayland display.

```sh
sudo apt-get install build-essential pkg-config libgl1 libxkbcommon0
```

In the **local demonstration**, select a server and channel with the mouse. Type in the composer and press
Enter or Send to add a local message; Shift+Enter inserts a new line. Drafts and
pending replies are kept separately for each channel while the app is open.
Right-click a message to reply, toggle a local reaction, or copy its full text.
Replies use the composer and show a reference to the original message. F3 replies
to the selected message; Escape cancels the reply without deleting its text.
F2 opens the selected message menu; Up/Down and Enter choose an action.
The native text editor supports selection, clipboard shortcuts and undo/redo;
undo history is isolated by channel. Text is limited to 2,000 Unicode characters
and local history to 200 messages per channel.
The profile's `...` button opens settings; F1 opens settings and Escape closes.
Ctrl+PageUp/PageDown changes server; Ctrl+Up/Down changes channel. Settings, drafts and
mock messages reset at exit. In the local demo, debug metrics are enabled by default; release
metrics require `--metrics` or enabling them in settings.

The controller/TV experience from the first prototype was removed from the
application after clarification of the goal. Gamepad code is parked behind the
optional `input/gamepad` feature, not compiled or initialized in the default app.

## Check

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p rustcord --release --locked
cargo run -p rustcord --release -- --smoke-test
```

The graphical smoke test exercises local channel selection, sending, reaction,
and settings, asserts state, and closes in four seconds. A display is required.
CI is configured for Windows and Linux. Physical Windows validation is pending.

## Structure

`app` composes the native window; `discord-core` owns models/mock/backend boundary;
`discord-network` owns QR authentication and bounded HTTPS reads;
`app-state` owns local demo state/actions; `input` maps input independently of Discord;
`ui` renders the desktop layout; `platform` samples process metrics.

[Architecture](docs/architecture.md) · [Roadmap](docs/roadmap.md) ·
[Measurements](docs/performance.md)

Official Discord frontend code is not a reusable native Rust UI: it depends on
the web runtime. The project reuses native UI libraries and reimplements the
familiar layout. Visual fidelity and full desktop interactions remain works in
progress. No official logos/assets or proprietary fonts are bundled.

## Contributing

Use small tested commits on Git Flow branches. See [CONTRIBUTING.md](CONTRIBUTING.md).
The current development build is on [`develop`](https://github.com/lippdev/Rustcord/tree/develop);
`main` is reserved for releases and currently remains the GitHub default branch. Repository: [lippdev/Rustcord](https://github.com/lippdev/Rustcord).

MIT OR Apache-2.0. Not affiliated with Discord.
