//! Native login/read-only view. The local composer is never mounted for a real session.
use discord_core::{ChannelId, ServerId, Snapshot};
use discord_network::{Command, Connection, Event};
use eframe::egui::{self, Color32};
mod connected;
mod images;
use discord_core::{Member, MessageId};
use images::Images;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct OnlineView {
    connection: Option<Connection>,
    snapshot: Option<Snapshot>,
    qr: Option<egui::TextureHandle>,
    status: &'static str,
    server: Option<ServerId>,
    channel: Option<ChannelId>,
    busy: bool,
    images: Images,
    members: Vec<Member>,
    members_loaded: bool,
    members_started: Option<Instant>,
    voice: Vec<(u64, ChannelId)>,
    realtime: Option<bool>,
    hide_members: bool,
    refresh_due: Option<Instant>,
    deleted: HashSet<MessageId>,
    history_error: Option<&'static str>,
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
                    let newest = messages.last().map(|m| m.id);
                    let mut merged = messages;
                    for message in &item.messages {
                        if newest.is_none_or(|id| message.id > id)
                            && !merged.iter().any(|m| m.id == message.id)
                        {
                            merged.push(message.clone());
                        }
                    }
                    merged.retain(|m| !self.deleted.contains(&m.id));
                    merged.sort_by_key(|m| m.id);
                    if merged.len() > 50 {
                        merged.drain(..merged.len() - 50);
                    }
                    item.messages = merged;
                }
                self.busy = false;
                self.history_error = None;
                self.status = "Histórico carregado.";
            }
            Event::Image(url, image) => self.images.accept(ctx, url, image),
            Event::Realtime(connected) => {
                self.realtime = Some(connected);
                if connected
                    && self.snapshot.as_ref().is_some_and(|s| {
                        s.servers.iter().flat_map(|s| &s.channels).any(|c| {
                            Some(c.id) == self.channel
                                && c.details.kind == discord_core::ChannelKind::Text
                        })
                    })
                {
                    self.refresh_due
                        .get_or_insert(Instant::now() + Duration::from_millis(750));
                }
            }
            Event::Members(server, channel, members)
                if self.server == Some(server) && self.channel == Some(channel) =>
            {
                self.members = members;
                self.members_loaded = true;
                self.members_started = None;
            }
            Event::Voice(server, voice) if self.server == Some(server) => self.voice = voice,
            Event::Message(channel, message) if self.channel == Some(channel) => {
                if let Some(item) = self.selected_channel_mut() {
                    if let Some(existing) = item.messages.iter_mut().find(|m| m.id == message.id) {
                        *existing = message;
                    } else {
                        item.messages.push(message);
                        item.messages.sort_by_key(|m| m.id);
                        if item.messages.len() > 50 {
                            item.messages.remove(0);
                        }
                    }
                }
            }
            Event::Deleted(channel, ids) if self.channel == Some(channel) => {
                if let Some(item) = self.selected_channel_mut() {
                    item.messages.retain(|m| !ids.contains(&m.id));
                }
                if self.deleted.len() + ids.len() > 200 {
                    self.deleted.clear();
                }
                self.deleted.extend(ids);
            }
            Event::Refresh(channel) if self.channel == Some(channel) => {
                self.refresh_due
                    .get_or_insert(Instant::now() + Duration::from_millis(750));
            }
            Event::Error(error) => {
                if error == "Sessão expirada. Entre novamente." {
                    *self = Self::default();
                    self.status = error;
                    return;
                }
                self.qr = None;
                self.busy = false;
                self.status = error;
                self.history_error = Some(error);
            }
            _ => {} // Responses never change a different selected conversation.
        }
    }
    fn selected_channel_mut(&mut self) -> Option<&mut discord_core::Channel> {
        self.snapshot
            .as_mut()?
            .servers
            .iter_mut()
            .flat_map(|s| &mut s.channels)
            .find(|c| Some(c.id) == self.channel)
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
        if self.refresh_due.is_some_and(|due| Instant::now() >= due) && !self.busy {
            self.refresh_due = None;
            if let Some(channel) = self.channel {
                self.request(Command::History(channel));
            }
        }
        if self.refresh_due.is_some() {
            ctx.request_repaint_after(Duration::from_millis(750));
        }
        if self.members_started.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        self.connected_ui(root);
        self.images.flush(self.connection.as_ref());

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
    fn live_messages_and_deletions_survive_an_in_flight_history_response() {
        let ctx = egui::Context::default();
        let snapshot = discord_core::MockBackend::snapshot();
        let server = snapshot.servers[0].id;
        let channel = snapshot.servers[0].channels[0].id;
        let mut view = OnlineView {
            snapshot: Some(snapshot),
            server: Some(server),
            channel: Some(channel),
            ..Default::default()
        };
        view.selected_channel_mut().unwrap().messages.clear();
        let message = |id| discord_core::Message {
            id: MessageId::new(id),
            author: "Test".into(),
            text: "Live".into(),
            time: String::new(),
            reactions: 0,
            reply_to: None,
            details: Default::default(),
        };
        view.apply(Event::Realtime(true), &ctx);
        assert!(view.refresh_due.is_some());
        view.apply(Event::Message(channel, message(12)), &ctx);
        view.apply(Event::Deleted(channel, vec![MessageId::new(10)]), &ctx);
        view.apply(
            Event::History(channel, vec![message(10), message(11)]),
            &ctx,
        );
        let ids: Vec<_> = view
            .selected_channel_mut()
            .unwrap()
            .messages
            .iter()
            .map(|m| m.id.get())
            .collect();
        assert_eq!(ids, vec![11, 12]);
        for id in 13..100 {
            view.apply(Event::Message(channel, message(id)), &ctx);
        }
        assert_eq!(view.selected_channel_mut().unwrap().messages.len(), 50);
        view.apply(
            Event::Members(
                server,
                ChannelId::new(999),
                vec![Member {
                    id: 1,
                    name: "Other".into(),
                    avatar: None,
                    status: "online".into(),
                }],
            ),
            &ctx,
        );
        assert!(view.members.is_empty());
    }
    #[test]
    fn connected_content_renders_voice_members_and_media_without_network() {
        let ctx = egui::Context::default();
        let mut snapshot = discord_core::MockBackend::snapshot();
        let server = snapshot.servers[0].id;
        let channel = snapshot.servers[0].channels[0].id;
        let mut voice = snapshot.servers[0].channels[1].clone();
        voice.details.kind = discord_core::ChannelKind::Voice;
        voice.name = "Voice room".into();
        snapshot.servers[0].channels[1] = voice;
        snapshot.servers[0].channels[0].messages[0]
            .details
            .attachments
            .push(discord_core::Attachment {
                name: "photo.png".into(),
                url: "https://cdn.discordapp.com/test.png".into(),
                image: true,
            });
        let mut view = OnlineView {
            snapshot: Some(snapshot),
            server: Some(server),
            channel: Some(channel),
            members: vec![Member {
                id: 1,
                name: "Member test".into(),
                avatar: None,
                status: "online".into(),
            }],
            members_loaded: true,
            realtime: Some(true),
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| view.connected_ui(ui),
        );
        let mut texts = String::new();
        fn collect(shape: &egui::epaint::Shape, texts: &mut String) {
            match shape {
                egui::epaint::Shape::Text(text) => texts.push_str(&text.galley.job.text),
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, texts)
                    }
                }
                _ => {}
            }
        }
        output.textures_delta.clear();
        for shape in output.shapes {
            collect(&shape.shape, &mut texts);
        }
        assert!(texts.contains("Member test"));
        assert!(texts.contains("Voice room"));
        assert!(texts.contains("photo.png"));
        assert!(texts.contains("Welcome in"));
        assert!(view.connection.is_none());
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
