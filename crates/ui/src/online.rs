//! Native login/read-only view. The local composer is never mounted for a real session.
use super::{CHAT, MUTED, RAIL, SIDEBAR, TEXT};
use discord_core::{ChannelId, ServerId, Snapshot};
use discord_network::{Command, Connection, Event};
use eframe::egui::{self, Color32, RichText};

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
        egui::Panel::top("connection-status")
            .frame(egui::Frame::new().fill(RAIL).inner_margin(12))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Rustcord").strong());
                    if let Some(snapshot) = &self.snapshot {
                        ui.label(&snapshot.profile);
                    }
                    ui.label(RichText::new(self.status).color(MUTED));
                    if ui.button("Sair").clicked() {
                        *self = Self::default();
                    }
                });
            });
        if self.snapshot.is_none() {
            return false;
        }
        let mut selected_server = None;
        egui::Panel::left("live-servers")
            .exact_size(200.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(RAIL).inner_margin(10))
            .show(root, |ui| {
                ui.label(RichText::new("SERVIDORES").strong().color(MUTED));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy, |ui| {
                        for server in &self.snapshot.as_ref().unwrap().servers {
                            if ui
                                .selectable_label(self.server == Some(server.id), &server.name)
                                .clicked()
                            {
                                selected_server = Some(server.id);
                            }
                        }
                    });
                });
            });
        if let Some(server) = selected_server {
            self.server = Some(server);
            self.channel = None;
            for item in &mut self.snapshot.as_mut().unwrap().servers {
                item.channels.clear();
            }
            self.request(Command::Channels(server));
        }
        let mut selected_channel = None;
        egui::Panel::left("live-channels")
            .exact_size(220.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(10))
            .show(root, |ui| {
                ui.label(RichText::new("CANAIS DE TEXTO").strong().color(MUTED));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy, |ui| {
                        if let Some(server) = self
                            .snapshot
                            .as_ref()
                            .unwrap()
                            .servers
                            .iter()
                            .find(|s| Some(s.id) == self.server)
                        {
                            for channel in &server.channels {
                                if ui
                                    .selectable_label(
                                        self.channel == Some(channel.id),
                                        format!("# {}", channel.name),
                                    )
                                    .clicked()
                                {
                                    selected_channel = Some(channel.id);
                                }
                            }
                        }
                    });
                });
            });
        if let Some(channel) = selected_channel {
            self.channel = Some(channel);
            for channel in self
                .snapshot
                .as_mut()
                .unwrap()
                .servers
                .iter_mut()
                .flat_map(|s| &mut s.channels)
            {
                channel.messages.clear();
            }
            self.request(Command::History(channel));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(CHAT).inner_margin(16)).show(root, |ui| {
            let channel = self.snapshot.as_ref().unwrap().servers.iter().flat_map(|s| &s.channels).find(|c| Some(c.id) == self.channel);
            let mut refresh = false;
            if let Some(channel) = channel {
                ui.horizontal(|ui| {
                    ui.heading(format!("# {}",channel.name));
                    refresh = ui.add_enabled(!self.busy,egui::Button::new("Atualizar")).clicked();
                });
                if !channel.topic.is_empty() { ui.label(RichText::new(&channel.topic).color(MUTED)); }
                ui.separator();
                egui::Panel::bottom("live-read-only").show(ui, |ui| {
                    ui.label(RichText::new("Leitura de até 50 mensagens • envio e atualizações em tempo real ainda em desenvolvimento").color(MUTED));
                });
                egui::ScrollArea::vertical().id_salt(("live-history",self.channel)).stick_to_bottom(true).show(ui, |ui| {
                    if channel.messages.is_empty() && !self.busy { ui.label("Nenhuma mensagem carregada."); }
                    for message in &channel.messages {
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&message.author).strong().color(TEXT));
                            ui.label(RichText::new(&message.time).small().color(MUTED));
                        });
                        if !message.text.is_empty() { ui.label(&message.text); }
                        else { ui.label(RichText::new("Mensagem sem texto; anexos ainda não são exibidos.").color(MUTED)); }
                    }
                });
            } else {
                ui.heading("Bem-vindo ao Rustcord");
                ui.label("Selecione um servidor e um canal para carregar mensagens reais.");
            }
            if refresh && let Some(channel) = self.channel { self.request(Command::History(channel)); }
            if self.busy { ui.spinner(); }
        });
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
                account: AccountId::new(1),
                profile: "Test".into(),
                servers: vec![Server {
                    id: ServerId::new(1),
                    name: "Test".into(),
                    initials: "T".into(),
                    channels: vec![Channel {
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
