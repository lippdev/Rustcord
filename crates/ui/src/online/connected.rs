//! Connected layout follows the same rail/sidebar/conversation language as the local UI.
use super::OnlineView;
use crate::{BLURPLE, CHAT, MUTED, RAIL, SELECTED, SIDEBAR, TEXT};
use discord_core::{ChannelId, ChannelKind, ServerId};
use discord_network::Command;
use eframe::egui::{self, RichText, Stroke, Vec2};
use std::borrow::Cow;

impl OnlineView {
    pub(super) fn connected_ui(&mut self, root: &mut egui::Ui) {
        self.server_rail(root);
        self.channel_sidebar(root);
        if self.snapshot.is_none() {
            return;
        }
        if !self.hide_members {
            self.member_sidebar(root);
        }
        self.chat(root);
    }
    fn choose_server(&mut self, server: ServerId) {
        if self.server == Some(server) {
            return;
        }
        self.server = Some(server);
        self.channel = None;
        self.members.clear();
        self.members_loaded = false;
        self.members_started = None;
        self.voice.clear();
        self.deleted.clear();
        self.history_error = None;
        self.refresh_due = None;
        if let Some(snapshot) = &mut self.snapshot {
            for item in &mut snapshot.servers {
                item.channels.clear();
            }
        }
        self.request(Command::Channels(server));
    }
    fn choose_channel(&mut self, channel: ChannelId) {
        if self.channel == Some(channel) {
            return;
        }
        self.channel = Some(channel);
        self.members.clear();
        self.members_loaded = false;
        self.members_started = Some(std::time::Instant::now());
        self.deleted.clear();
        self.history_error = None;
        self.refresh_due = None;
        if let Some(server) = self.server
            && let Some(connection) = &self.connection
        {
            let _ = connection.command(Command::Select(server, channel));
        }
        if let Some(snapshot) = &mut self.snapshot {
            for item in snapshot.servers.iter_mut().flat_map(|s| &mut s.channels) {
                item.messages.clear();
            }
        }
        let kind = self
            .snapshot
            .as_ref()
            .and_then(|s| {
                s.servers
                    .iter()
                    .flat_map(|s| &s.channels)
                    .find(|c| c.id == channel)
            })
            .map(|c| c.details.kind);
        if kind == Some(ChannelKind::Text) {
            self.request(Command::History(channel));
        } else {
            self.status = "Canal selecionado.";
        }
    }
    fn server_rail(&mut self, root: &mut egui::Ui) {
        let mut selected = None;
        egui::Panel::left("live-guild-rail")
            .exact_size(72.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(RAIL).inner_margin(12))
            .show(root, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 16, BLURPLE);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "RC",
                    egui::FontId::proportional(18.0),
                    TEXT,
                );
                response.on_hover_text("Rustcord · conectado");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                if let Some(snapshot) = &self.snapshot {
                    egui::ScrollArea::vertical()
                        .id_salt("live-guild-scroll")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show_rows(ui, 48.0, snapshot.servers.len(), |ui, rows| {
                            for index in rows {
                                let server = &snapshot.servers[index];
                                let active = self.server == Some(server.id);
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::click());
                                ui.painter().rect_filled(
                                    rect,
                                    if active || response.hovered() { 16 } else { 24 },
                                    if active { BLURPLE } else { CHAT },
                                );
                                let initials = if server.initials.is_empty() {
                                    "?"
                                } else {
                                    &server.initials
                                };
                                if let Some(url) = server.icon.as_deref() {
                                    let mut child =
                                        ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                    self.images.avatar(&mut child, Some(url), initials, 48.0);
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        initials,
                                        egui::FontId::proportional(17.0),
                                        TEXT,
                                    );
                                }
                                if active || response.hovered() {
                                    let height = if active { 32.0 } else { 16.0 };
                                    let marker = egui::Rect::from_center_size(
                                        egui::pos2(rect.left() - 10.0, rect.center().y),
                                        Vec2::new(4.0, height),
                                    );
                                    ui.painter().rect_filled(marker, 2, TEXT);
                                }
                                if response.clicked() && !self.busy {
                                    selected = Some(server.id);
                                }
                                response.on_hover_text(&server.name);
                            }
                        });
                }
            });
        if let Some(server) = selected {
            self.choose_server(server);
        }
    }
    fn channel_sidebar(&mut self, root: &mut egui::Ui) {
        let mut selected = None;
        let mut logout = false;
        egui::Panel::left("live-channel-sidebar")
            .exact_size(240.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR))
            .show(root, |ui| {
                egui::Panel::bottom("live-profile")
                    .exact_size(56.0)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(35, 36, 40))
                            .inner_margin(8),
                    )
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let profile = self.snapshot.as_ref().map_or("", |s| s.profile.as_str());
                            let image = self.snapshot.as_ref().and_then(|s| s.avatar.as_deref());
                            self.images.avatar(ui, image, profile, 32.0);
                            ui.vertical(|ui| {
                                ui.set_width(126.0);
                                ui.add(
                                    egui::Label::new(RichText::new(profile).size(13.0).strong())
                                        .truncate(),
                                )
                                .on_hover_text(profile);
                                ui.label(RichText::new("Conectado").size(11.0).color(MUTED));
                            });
                            logout = ui.small_button("Sair").clicked();
                        });
                    });
                egui::Panel::top("live-server-heading")
                    .exact_size(48.0)
                    .resizable(false)
                    .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(12))
                    .show(ui, |ui| {
                        let name = self
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.servers.iter().find(|s| Some(s.id) == self.server))
                            .map_or("Seus servidores", |s| s.name.as_str());
                        ui.add(
                            egui::Label::new(
                                RichText::new(presentation_name(name)).size(16.0).strong(),
                            )
                            .truncate(),
                        )
                        .on_hover_text(name);
                    });
                egui::Frame::new().inner_margin(8).show(ui, |ui| {
                    ui.add_space(12.0);
                    if let Some(server) = self
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.servers.iter().find(|s| Some(s.id) == self.server))
                    {
                        egui::ScrollArea::vertical()
                            .id_salt(("live-channel-scroll", server.id))
                            .show(ui, |ui| {
                                let mut ordered = Vec::new();
                                for channel in server.channels.iter().filter(|c| {
                                    c.details.parent.is_none()
                                        && c.details.kind != ChannelKind::Category
                                }) {
                                    ordered.push(channel);
                                }
                                for category in server
                                    .channels
                                    .iter()
                                    .filter(|c| c.details.kind == ChannelKind::Category)
                                {
                                    ordered.push(category);
                                    ordered.extend(
                                        server
                                            .channels
                                            .iter()
                                            .filter(|c| c.details.parent == Some(category.id)),
                                    );
                                }
                                ordered.extend(server.channels.iter().filter(|c| {
                                    c.details.parent.is_some_and(|parent| {
                                        !server.channels.iter().any(|p| p.id == parent)
                                    })
                                }));
                                for channel in ordered {
                                    if channel.details.kind == ChannelKind::Category {
                                        ui.add_space(14.0);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "⌄ {}",
                                                    presentation_name(&channel.name)
                                                ))
                                                .size(11.0)
                                                .strong()
                                                .color(MUTED),
                                            )
                                            .truncate(),
                                        );
                                        continue;
                                    }
                                    let symbol = match channel.details.kind {
                                        ChannelKind::Voice | ChannelKind::Stage => "◖",
                                        ChannelKind::Forum => "▤",
                                        _ => "#",
                                    };
                                    let active = self.channel == Some(channel.id);
                                    let label = RichText::new(format!(
                                        "{symbol}   {}",
                                        presentation_name(&channel.name)
                                    ))
                                    .size(15.0)
                                    .color(if active { TEXT } else { MUTED });
                                    let button = egui::Button::new("")
                                        .left_text(label)
                                        .wrap_mode(egui::TextWrapMode::Truncate)
                                        .fill(if active {
                                            SELECTED
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .stroke(Stroke::NONE)
                                        .corner_radius(4);
                                    let response = ui.add_enabled(
                                        !self.busy,
                                        button.min_size(Vec2::new(ui.available_width(), 32.0)),
                                    );
                                    if response.clicked() {
                                        selected = Some(channel.id);
                                    }
                                    response.on_hover_text(&channel.name);
                                    if matches!(
                                        channel.details.kind,
                                        ChannelKind::Voice | ChannelKind::Stage
                                    ) {
                                        for (user, _) in
                                            self.voice.iter().filter(|(_, id)| *id == channel.id)
                                        {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                let member =
                                                    self.members.iter().find(|m| m.id == *user);
                                                self.images.avatar(
                                                    ui,
                                                    member.and_then(|m| m.avatar.as_deref()),
                                                    member.map_or("?", |m| &m.name),
                                                    24.0,
                                                );
                                                ui.label(
                                                    RichText::new(member.map_or_else(
                                                        || format!("Usuário {user}"),
                                                        |m| m.name.clone(),
                                                    ))
                                                    .size(12.0)
                                                    .color(MUTED),
                                                );
                                            });
                                        }
                                    }
                                }
                            });
                    } else {
                        ui.label(
                            RichText::new("Escolha um servidor na barra ao lado.")
                                .size(13.0)
                                .color(MUTED),
                        );
                    }
                });
            });
        if logout {
            *self = Self::default();
            return;
        }
        if let Some(channel) = selected {
            self.choose_channel(channel);
        }
    }
    fn member_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("live-members")
            .exact_size(224.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(12))
            .show(root, |ui| {
                ui.add_space(48.0);
                ui.label(
                    RichText::new(format!("MEMBROS — {} CARREGADOS", self.members.len()))
                        .size(11.0)
                        .strong()
                        .color(MUTED),
                );
                ui.add_space(12.0);
                if self.channel.is_none() {
                    ui.label(RichText::new("Selecione um canal.").color(MUTED));
                } else if self.members.is_empty() {
                    let text = if self.members_loaded {
                        "Nenhum membro retornado para este canal."
                    } else if self.realtime == Some(true)
                        && self.members_started.is_some_and(|started| {
                            started.elapsed() < std::time::Duration::from_secs(20)
                        })
                    {
                        "Carregando membros do canal…"
                    } else {
                        "Lista de membros indisponível. Reconecte para tentar novamente."
                    };
                    ui.label(RichText::new(text).size(13.0).color(MUTED));
                }
                egui::ScrollArea::vertical()
                    .id_salt(("live-members-scroll", self.channel))
                    .show_rows(ui, 44.0, self.members.len(), |ui, rows| {
                        for index in rows {
                            let member = &self.members[index];
                            ui.horizontal(|ui| {
                                self.images.avatar(
                                    ui,
                                    member.avatar.as_deref(),
                                    &member.name,
                                    32.0,
                                );
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&member.name).size(14.0))
                                            .truncate(),
                                    )
                                    .on_hover_text(&member.name);
                                    let status = match member.status.as_str() {
                                        "online" => "Online",
                                        "idle" => "Ausente",
                                        "dnd" => "Não perturbe",
                                        "offline" => "Offline",
                                        _ => "",
                                    };
                                    if !status.is_empty() {
                                        ui.label(RichText::new(status).size(11.0).color(MUTED));
                                    }
                                });
                            });
                        }
                    });
                ui.label(
                    RichText::new("Até 200 posições da lista do canal.")
                        .size(10.0)
                        .color(MUTED),
                );
            });
    }
    fn chat(&mut self, root: &mut egui::Ui) {
        let mut refresh = false;
        let mut toggle = false;
        let images = &mut self.images;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(CHAT)).show(root,|ui| {
            let channel=self.snapshot.as_ref().and_then(|s|s.servers.iter().flat_map(|s|&s.channels).find(|c|Some(c.id)==self.channel));
            let text_channel=channel.is_some_and(|c|c.details.kind==ChannelKind::Text);
            egui::Panel::top("live-channel-header").exact_size(48.0).resizable(false).frame(egui::Frame::new().fill(CHAT).inner_margin(12)).show(ui,|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if text_channel {"#"}else{"◖"}).size(23.0).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                        toggle=ui.button("Membros").clicked();
                        refresh=ui.add_enabled(text_channel&&!self.busy,egui::Button::new("Atualizar")).clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center),|ui| {
                            ui.add(egui::Label::new(RichText::new(channel.map_or("Rustcord",|c|&c.name)).size(16.0).strong()).truncate());
                            if let Some(channel)=channel && !channel.topic.is_empty() {
                                ui.separator(); ui.add(egui::Label::new(RichText::new(&channel.topic).size(12.0).color(MUTED)).truncate()).on_hover_text(&channel.topic);
                            }
                        });
                    });
                });
            });
            egui::Panel::bottom("live-composer-status").exact_size(88.0).resizable(false).frame(egui::Frame::new().fill(CHAT).inner_margin(16)).show(ui,|ui| {
                egui::Frame::new().fill(egui::Color32::from_rgb(56,58,64)).corner_radius(8).inner_margin(12).show(ui,|ui| {
                    ui.set_width(ui.available_width()); ui.label(RichText::new("Envio de mensagens em desenvolvimento").size(14.0).color(MUTED));
                });
                ui.horizontal(|ui| {
                    if self.busy {ui.spinner();}
                    let status=match self.realtime {Some(true)=>"● Atualização ao vivo",Some(false)=>"Atualização ao vivo desconectada · use Atualizar",None=>"Conectando atualização ao vivo…"};
                    ui.add(egui::Label::new(RichText::new(format!("{status} · {}",self.status)).size(11.0).color(MUTED)).truncate()).on_hover_text(self.status);
                });
            });
            egui::ScrollArea::vertical().id_salt(("live-history",self.server,self.channel)).auto_shrink([false,false]).stick_to_bottom(true).show(ui,|ui| {
                ui.set_width(ui.available_width());
                if let Some(channel)=channel {
                    if !text_channel {
                        egui::Frame::new().inner_margin(24).show(ui,|ui| {
                            ui.heading(&channel.name);
                            ui.label(RichText::new(if matches!(channel.details.kind,ChannelKind::Voice|ChannelKind::Stage) {"Canal de voz. A conexão de áudio ainda está em desenvolvimento."}else{"Canal de fórum. A lista de publicações ainda está em desenvolvimento."}).color(MUTED));
                        }); return;
                    }
                    if let Some(error)=self.history_error {
                        egui::Frame::new().inner_margin(16).show(ui,|ui| {ui.colored_label(egui::Color32::from_rgb(240,120,120),error);});
                    }
                    if channel.messages.is_empty() && !self.busy && self.history_error.is_none() {
                        egui::Frame::new().inner_margin(24).show(ui,|ui| {ui.heading(format!("Bem-vindo a #{}",channel.name));ui.label(RichText::new("Este canal não retornou mensagens.").color(MUTED));});
                    }
                    for message in &channel.messages {
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(16,10)).show(ui,|ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                images.avatar(ui,message.details.avatar.as_deref(),&message.author,40.0);
                                ui.vertical(|ui| {
                                    ui.set_max_width(ui.available_width());
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&message.author).size(15.0).strong());
                                        ui.label(RichText::new(display_time(&message.time)).size(11.0).color(MUTED)).on_hover_text(&message.time);
                                        if message.details.edited {ui.label(RichText::new("(editada)").size(10.0).color(MUTED));}
                                    });
                                    if let Some(reply)=message.reply_to {
                                        let referenced=channel.messages.iter().find(|m|m.id==reply);
                                        ui.add(egui::Label::new(RichText::new(referenced.map_or_else(||"↪ Respondendo a uma mensagem anterior".to_owned(),|m|format!("↪ {}: {}",m.author,m.text.chars().take(100).collect::<String>()))).size(12.0).color(MUTED)).truncate());
                                    }
                                    if !message.text.is_empty() {ui.add(egui::Label::new(RichText::new(&message.text).size(15.0)).wrap().selectable(true));}
                                    for attachment in &message.details.attachments {
                                        if attachment.image {images.thumbnail(ui,&attachment.url);}
                                        ui.hyperlink_to(if attachment.name.is_empty() {"Abrir anexo"}else{&attachment.name},&attachment.url);
                                    }
                                    for embed in &message.details.embeds {
                                        egui::Frame::new().fill(RAIL).corner_radius(4).inner_margin(12).show(ui,|ui| {
                                            ui.set_max_width(ui.available_width().min(400.0));
                                            if !embed.title.is_empty() {
                                                if let Some(url)=&embed.url {ui.hyperlink_to(&embed.title,url);}else{ui.label(RichText::new(&embed.title).strong());}
                                            }
                                            if !embed.description.is_empty() {ui.add(egui::Label::new(&embed.description).wrap().selectable(true));}
                                            if let Some(image)=&embed.image {images.thumbnail(ui,image);}
                                        });
                                    }
                                    if message.text.is_empty() && message.details.attachments.is_empty() && message.details.embeds.is_empty() {ui.label(RichText::new("Mensagem sem texto ou mídia compatível.").size(12.0).color(MUTED));}
                                });
                            });
                        });
                    }
                } else {egui::Frame::new().inner_margin(32).show(ui,|ui| {ui.add_space(20.0);ui.heading("Seu Discord, em uma interface nativa");ui.label(RichText::new("Escolha um servidor e um canal para ler suas conversas.").color(MUTED));});}
            });
        });
        if toggle {
            self.hide_members = !self.hide_members;
        }
        if refresh && let Some(channel) = self.channel {
            self.request(Command::History(channel));
        }
    }
}
/// Keep the returned timezone explicit; this prototype does not convert to local time.
fn display_time(timestamp: &str) -> String {
    if timestamp.is_ascii() && timestamp.len() >= 20 && timestamp.as_bytes()[10] == b'T' {
        let zone = if timestamp.ends_with('Z') || timestamp.ends_with("+00:00") {
            " UTC"
        } else {
            ""
        };
        format!("{} · {}{zone}", &timestamp[..10], &timestamp[11..16])
    } else {
        timestamp.to_owned()
    }
}

// Common decorative CJK vertical bars have no glyph in the minimal font set.
// Normalize their presentation only; domain names and IDs remain untouched.
fn presentation_name(name: &str) -> Cow<'_, str> {
    let decorative = |c| matches!(c, '\u{fe31}' | '\u{fe32}' | '\u{ff5c}');
    if name.chars().any(decorative) {
        Cow::Owned(
            name.chars()
                .map(|c| if decorative(c) { '│' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(name)
    }
}
