//! Native login/read-only view. The local composer is never mounted for a real session.
use discord_core::{ChannelId, ServerId, Snapshot};
use discord_network::{Command, Connection, Event};
use eframe::egui::{self, Color32};
mod connected;

#[derive(Default)]
pub(super) struct OnlineView {
    connection: Option<Connection>,
    snapshot: Option<Snapshot>,
    qr: Option<egui::TextureHandle>,
    status: &'static str,
    server: Option<ServerId>,
    channel: Option<ChannelId>,
    busy: bool,
}
impl OnlineView {
    fn poll(&mut self, ctx: &egui::Context) {
        for _ in 0..8 {
            let event = match self.connection.as_mut().map(Connection::event) {
                Some(Ok(Some(event))) => event,
                Some(Err(error)) => {
                    if self.status.is_empty() || self.busy {
                        self.status = error;
                    }
                    self.connection = None;
                    self.snapshot = None;
                    self.qr = None;
                    self.busy = false;
                    break;
                }
                _ => break,
            };
            self.apply(event, ctx);
        }
    }
    fn apply(&mut self, event: Event, ctx: &egui::Context) {
        match event {
            Event::Qr(url) => match qr_image(&url) {
                Ok(image) => {
                    self.qr =
                        Some(ctx.load_texture("login-qr", image, egui::TextureOptions::NEAREST));
                    self.busy = false;
                    self.status = "Escaneie pelo app Discord no celular e confirme o login.";
                }
                Err(_) => {
                    self.connection = None;
                    self.qr = None;
                    self.busy = false;
                    self.status = "Não foi possível gerar o QR. Tente novamente.";
                }
            },
            Event::Scanned => {
                self.qr = None;
                self.busy = true;
                self.status =
                    "Confirme o login no celular. Carregaremos seus servidores em seguida.";
            }
            Event::Connected(snapshot) => {
                self.qr = None;
                self.server = None;
                self.channel = None;
                self.snapshot = Some(snapshot);
                self.busy = false;
                self.status = "Conectado. Selecione um servidor para carregar os canais.";
            }
            Event::Channels(server, channels) if self.server == Some(server) => {
                if let Some(snapshot) = &mut self.snapshot {
                    for item in &mut snapshot.servers {
                        item.channels.clear();
                        if item.id == server {
                            item.channels = channels.clone();
                        }
                    }
                }
                self.busy = false;
                self.status = "Selecione um canal de texto para ler as últimas 50 mensagens.";
            }
            Event::History(channel, messages) if self.channel == Some(channel) => {
                if let Some(snapshot) = &mut self.snapshot
                    && let Some(item) = snapshot
                        .servers
                        .iter_mut()
                        .flat_map(|s| &mut s.channels)
                        .find(|c| c.id == channel)
                {
                    item.messages = messages;
                }
                self.busy = false;
                self.status =
                    "Histórico carregado. Use Atualizar para buscar as mensagens mais recentes.";
            }
            Event::Error(error) => {
                self.qr = None;
                self.busy = false;
                self.status = error;
            }
            _ => {} // Responses never change a different selected conversation.
        }
    }
    fn request(&mut self, command: Command) {
        let result = self
            .connection
            .as_ref()
            .ok_or("Entre novamente para carregar dados.")
            .and_then(|c| c.command(command));
        match result {
            Ok(()) => {
                self.busy = true;
                self.status = "Carregando…";
            }
            Err(error) => {
                self.busy = false;
                self.status = error;
            }
        }
    }
    pub fn ui(&mut self, root: &mut egui::Ui) -> bool {
        let ctx = root.ctx().clone();
        self.poll(&ctx);
        if self.snapshot.is_none() {
            match super::login::draw(
                root,
                self.qr.as_ref(),
                self.status,
                self.busy,
                self.connection.is_some(),
            ) {
                super::login::Action::Start => {
                    let wake = ctx.clone();
                    match Connection::start(move || wake.request_repaint()) {
                        Ok(connection) => {
                            self.connection = Some(connection);
                            self.busy = true;
                            self.status = "Preparando login…";
                        }
                        Err(_) => {
                            self.status = "Não foi possível iniciar a conexão. Tente novamente."
                        }
                    }
                }
                super::login::Action::Cancel => *self = Self::default(),
                super::login::Action::Demo => return true,
                super::login::Action::None => {}
            }
            return false;
        }
        self.connected_ui(root);

        false
    }
}
fn qr_image(url: &str) -> Result<egui::ColorImage, qrcode::types::QrError> {
    let code = qrcode::QrCode::new(url.as_bytes())?;
    let width = code.width();
    let side = width + 8;
    let mut pixels = vec![Color32::WHITE; side * side];
    for y in 0..width {
        for x in 0..width {
            if code[(x, y)] == qrcode::Color::Dark {
                pixels[(y + 4) * side + x + 4] = Color32::BLACK;
            }
        }
    }
    Ok(egui::ColorImage::new([side, side], pixels))
}
#[cfg(test)]
mod tests {
    use super::*;
    use discord_core::{AccountId, Channel, Message, MessageId, Server};
    #[test]
    fn late_channel_history_cannot_replace_current_selection() {
        let ctx = egui::Context::default();
        let mut view = OnlineView {
            server: Some(ServerId::new(1)),
            channel: Some(ChannelId::new(2)),
            snapshot: Some(Snapshot {
                avatar: Default::default(),
                account: AccountId::new(1),
                profile: "Test".into(),
                servers: vec![Server {
                    icon: Default::default(),
                    id: ServerId::new(1),
                    name: "Test".into(),
                    initials: "T".into(),
                    channels: vec![Channel {
                        details: Default::default(),
                        id: ChannelId::new(2),
                        name: "chat".into(),
                        topic: String::new(),
                        messages: Vec::new(),
                    }],
                }],
            }),
            busy: true,
            ..Default::default()
        };
        view.apply(
            Event::History(
                ChannelId::new(99),
                vec![Message {
                    details: Default::default(),
                    id: MessageId::new(1),
                    author: "Other".into(),
                    text: "Wrong channel".into(),
                    time: String::new(),
                    reactions: 0,
                    reply_to: None,
                }],
            ),
            &ctx,
        );
        assert!(
            view.snapshot.as_ref().unwrap().servers[0].channels[0]
                .messages
                .is_empty()
        );
        assert!(view.busy);
        view.apply(Event::Channels(ServerId::new(99), Vec::new()), &ctx);
        assert_eq!(view.snapshot.as_ref().unwrap().servers[0].channels.len(), 1);
    }
    #[test]
    fn scanning_or_error_discards_login_qr() {
        let ctx = egui::Context::default();
        let mut view = OnlineView::default();
        view.apply(
            Event::Qr("https://discord.com/ra/synthetic-fingerprint".into()),
            &ctx,
        );
        assert!(view.qr.is_some());
        view.apply(Event::Scanned, &ctx);
        assert!(view.qr.is_none());
        assert!(view.busy);
        view.apply(Event::Error("expired"), &ctx);
        assert!(!view.busy);
        assert!(view.qr.is_none());
    }
}
