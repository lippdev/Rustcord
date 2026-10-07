//! Connected layout follows the same rail/sidebar/conversation language as the local UI.
use super::OnlineView;
use crate::{BLURPLE, CHAT, MUTED, RAIL, SELECTED, SIDEBAR, TEXT, author_color, avatar};
use discord_core::{ChannelId, ServerId};
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
        self.chat(root);
    }
    fn choose_server(&mut self, server: ServerId) {
        if self.server == Some(server) {
            return;
        }
        self.server = Some(server);
        self.channel = None;
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
        if let Some(snapshot) = &mut self.snapshot {
            for item in snapshot.servers.iter_mut().flat_map(|s| &mut s.channels) {
                item.messages.clear();
            }
        }
        self.request(Command::History(channel));
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
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    initials,
                                    egui::FontId::proportional(17.0),
                                    TEXT,
                                );
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
                            avatar(
                                ui,
                                &profile.chars().next().unwrap_or('?').to_string(),
                                32.0,
                                BLURPLE,
                                true,
                            );
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
                    ui.label(
                        RichText::new("CANAIS DE TEXTO")
                            .size(11.0)
                            .strong()
                            .color(MUTED),
                    );
                    ui.add_space(6.0);
                    if let Some(server) = self
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.servers.iter().find(|s| Some(s.id) == self.server))
                    {
                        egui::ScrollArea::vertical()
                            .id_salt(("live-channel-scroll", server.id))
                            .show_rows(ui, 32.0, server.channels.len(), |ui, rows| {
                                for index in rows {
                                    let channel = &server.channels[index];
                                    let active = self.channel == Some(channel.id);
                                    let label = egui::RichText::new(format!(
                                        "#   {}",
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
    fn chat(&mut self, root: &mut egui::Ui) {
        let mut refresh = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(CHAT)).show(root, |ui| {
            let channel = self.snapshot.as_ref().and_then(|s|s.servers.iter().flat_map(|s|&s.channels).find(|c|Some(c.id)==self.channel));
            egui::Panel::top("live-channel-header").exact_size(48.0).resizable(false)
                .frame(egui::Frame::new().fill(CHAT).inner_margin(12)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("#").size(23.0).color(MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            refresh = ui.add_enabled(channel.is_some() && !self.busy,egui::Button::new("Atualizar")).clicked();
                            if let Some(channel) = channel {
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(RichText::new(presentation_name(&channel.name)).size(16.0).strong()).truncate()).on_hover_text(&channel.name);
                                    if !channel.topic.is_empty() {
                                        ui.separator();
                                        ui.add(egui::Label::new(RichText::new(&channel.topic).size(12.0).color(MUTED)).truncate()).on_hover_text(&channel.topic);
                                    }
                                });
                            } else {
                                ui.label(RichText::new("Rustcord").size(16.0).strong());
                            }
                        });
                    });
                });
            egui::Panel::bottom("live-composer-status").exact_size(88.0).resizable(false)
                .frame(egui::Frame::new().fill(CHAT).inner_margin(16)).show(ui, |ui| {
                    egui::Frame::new().fill(egui::Color32::from_rgb(56,58,64)).corner_radius(8).inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Envio de mensagens em desenvolvimento").size(14.0).color(MUTED));
                    });
                    ui.horizontal(|ui| {
                        if self.busy { ui.spinner(); }
                        ui.add(egui::Label::new(RichText::new(self.status).size(11.0).color(MUTED)).truncate()).on_hover_text(self.status);
                    });
                });
            egui::ScrollArea::vertical().id_salt(("live-history",self.server,self.channel)).auto_shrink([false,false]).stick_to_bottom(true).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(channel) = channel {
                    if channel.messages.is_empty() && !self.busy {
                        egui::Frame::new().inner_margin(24).show(ui, |ui| {
                            ui.heading(format!("Bem-vindo a #{}",channel.name));
                            ui.label(RichText::new("Nenhuma mensagem carregada neste canal.").color(MUTED));
                        });
                    }
                    for (index,message) in channel.messages.iter().enumerate() {
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(16,10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                avatar(ui,&message.author.chars().next().unwrap_or('?').to_string(),40.0,author_color(index%4),false);
                                ui.vertical(|ui| {
                                    ui.set_max_width(ui.available_width());
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&message.author).size(15.0).strong());
                                        ui.label(RichText::new(display_time(&message.time)).size(11.0).color(MUTED)).on_hover_text(&message.time);
                                    });
                                    if message.text.is_empty() {
                                        ui.label(RichText::new("Mensagem sem texto; mídia ainda indisponível.").size(13.0).color(MUTED));
                                    } else {
                                        ui.add(egui::Label::new(RichText::new(&message.text).size(15.0)).wrap().selectable(true));
                                    }
                                });
                            });
                        });
                    }
                } else {
                    egui::Frame::new().inner_margin(32).show(ui, |ui| {
                        ui.add_space(20.0);
                        ui.heading("Seu Discord, em uma interface nativa");
                        ui.add_space(8.0);
                        ui.label(RichText::new("Escolha um servidor e um canal para ler suas conversas.").color(MUTED));
                    });
                }
            });
        });
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
