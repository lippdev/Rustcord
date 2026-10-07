//! Discord-style native desktop frontend. No webview or controller runtime.
mod composer;
use app_state::{AppState, ConversationId, Screen};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use input::{AppAction, Key, Region};
use platform::Metrics;
use std::time::{Duration, Instant};
const RAIL: Color32 = Color32::from_rgb(30, 31, 34);
const SIDEBAR: Color32 = Color32::from_rgb(43, 45, 49);
const CHAT: Color32 = Color32::from_rgb(49, 51, 56);
const SELECTED: Color32 = Color32::from_rgb(64, 66, 73);
const TEXT: Color32 = Color32::from_rgb(242, 243, 245);
const MUTED: Color32 = Color32::from_rgb(148, 155, 164);
const BLURPLE: Color32 = Color32::from_rgb(88, 101, 242);
const GREEN: Color32 = Color32::from_rgb(35, 165, 90);

pub struct Rustcord {
    state: AppState,
    metrics: Metrics,
    started: Instant,
    first_frame: bool,
    draft: String,
    draft_conversation: Option<ConversationId>,
    composer_input: composer::ComposerInput,
    focus_composer: bool,
    context_anchor: egui::Pos2,
    smoke: bool,
    smoke_step: usize,
    smoke_done: bool,
    last_conversation: Option<ConversationId>,
    last_message_count: usize,
}
impl Rustcord {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        started: Instant,
        smoke: bool,
        metrics: bool,
    ) -> Self {
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = CHAT;
        style.visuals.window_fill = SIDEBAR;
        style.visuals.extreme_bg_color = RAIL;
        style.visuals.selection.bg_fill = BLURPLE;
        style.visuals.widgets.inactive.bg_fill = SELECTED;
        style.visuals.widgets.hovered.bg_fill = SELECTED;
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let mut state = AppState::default();
        if metrics {
            state.metrics = true;
        }
        Self {
            state,
            metrics: Metrics::new(started),
            started,
            first_frame: true,
            draft: String::new(),
            draft_conversation: None,
            composer_input: composer::ComposerInput::default(),
            focus_composer: false,
            context_anchor: egui::pos2(520.0, 240.0),
            smoke,
            smoke_step: 0,
            smoke_done: false,
            last_conversation: None,
            last_message_count: 0,
        }
    }
    fn actions(&mut self, ctx: &egui::Context) {
        let typing =
            self.state.screen == Screen::Main && ctx.memory(|m| m.has_focus(self.composer_id()));
        for action in keyboard_actions(ctx, typing) {
            if action == AppAction::Confirm && self.state.screen == Screen::Context {
                self.confirm_context(ctx);
            } else {
                let reply = action == AppAction::Reply && self.state.screen == Screen::Main;
                self.state.dispatch(action);
                if reply && self.state.reply_target().is_some() {
                    self.focus_composer = true;
                }
            }
        }
        if self.smoke {
            let script = [
                AppAction::Select(Region::Servers, 1),
                AppAction::Select(Region::Channels, 1),
                AppAction::Submit("Hello from Rustcord. This message is stored locally.".into()),
                AppAction::Context,
                AppAction::SelectMenu(1),
                AppAction::Confirm,
                AppAction::Menu,
                AppAction::Back,
            ];
            if self.smoke_step < script.len()
                && self.started.elapsed().as_millis() > 400 + self.smoke_step as u128 * 250
            {
                self.state.dispatch(script[self.smoke_step].clone());
                self.smoke_step += 1;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            if self.started.elapsed() > Duration::from_secs(4) && !self.smoke_done {
                assert_eq!(self.state.screen, Screen::Main);
                assert_eq!(self.state.messages().last().map(|m| m.reactions), Some(1));
                println!(
                    "SMOKE OK startup_ms={:.2} rss_bytes={:?} ui_cpu_ms={:.3}",
                    self.metrics.startup_ms,
                    platform::memory_bytes(),
                    self.metrics.frame_ms
                );
                self.smoke_done = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn rail(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("guild-rail")
            .exact_size(72.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(RAIL).inner_margin(12))
            .show(root, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 16, BLURPLE);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "RC",
                    egui::FontId::proportional(18.0),
                    TEXT,
                );
                response.on_hover_text("Rustcord · local mock");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                let mut selected = None;
                for (i, server) in self.state.data.servers.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::click());
                    let active = self.state.server == i;
                    ui.painter().rect_filled(
                        rect,
                        if active || response.hovered() { 16 } else { 24 },
                        if active { BLURPLE } else { CHAT },
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &server.initials,
                        egui::FontId::proportional(17.0),
                        TEXT,
                    );
                    if active {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.min - Vec2::new(12.0, -8.0),
                                Vec2::new(4.0, 32.0),
                            ),
                            2,
                            TEXT,
                        );
                    }
                    if response.clicked() {
                        selected = Some(i);
                    }
                    response.on_hover_text(&server.name);
                    ui.add_space(8.0);
                }
                if let Some(i) = selected {
                    self.state.dispatch(AppAction::Select(Region::Servers, i));
                }
            });
    }
    fn channels(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("channel-sidebar")
            .exact_size(240.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR))
            .show(root, |ui| {
                egui::Panel::bottom("profile")
                    .exact_size(56.0)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(Color32::from_rgb(35, 36, 40))
                            .inner_margin(8),
                    )
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            avatar(ui, "P", 32.0, BLURPLE, true);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&self.state.data.profile).size(13.0).strong(),
                                );
                                ui.label(RichText::new("Online").size(11.0).color(MUTED));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("...").on_hover_text("User settings").clicked() {
                                        self.state.dispatch(AppAction::Menu);
                                    }
                                },
                            );
                        });
                    });
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(
                            self.state
                                .data
                                .servers
                                .get(self.state.server)
                                .map_or("Servers", |s| s.name.as_str()),
                        )
                        .size(16.0)
                        .strong(),
                    );
                });
                ui.separator();
                ui.add_space(16.0);
                egui::Frame::new().inner_margin(8).show(ui, |ui| {
                    ui.label(
                        RichText::new("v  TEXT CHANNELS")
                            .size(11.0)
                            .strong()
                            .color(MUTED),
                    );
                    ui.add_space(6.0);
                    let mut clicked = None;
                    for (i, channel) in self.state.channels().iter().enumerate() {
                        let active = self.state.channel == i;
                        let button = egui::Button::new("")
                            .left_text(
                                RichText::new(format!("#   {}", channel.name))
                                    .size(15.0)
                                    .color(if active { TEXT } else { MUTED }),
                            )
                            .fill(if active {
                                SELECTED
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(Stroke::NONE)
                            .corner_radius(4);
                        if ui.add_sized([ui.available_width(), 32.0], button).clicked() {
                            clicked = Some(i);
                        }
                    }
                    if let Some(i) = clicked {
                        self.state.dispatch(AppAction::Select(Region::Channels, i));
                    }
                    ui.add_space(24.0);
                    ui.label(RichText::new("MOCK DATA").size(10.0).strong().color(MUTED));
                    ui.label(
                        RichText::new("No Discord connection")
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
    }
    fn members(&self, root: &mut egui::Ui) {
        if root.available_width() < 850.0 {
            return;
        }
        egui::Panel::right("members")
            .exact_size(200.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(16))
            .show(root, |ui| {
                ui.add_space(44.0);
                ui.label(RichText::new("ONLINE — 4").size(11.0).strong().color(MUTED));
                ui.add_space(8.0);
                for (i, name) in ["Maya", "Theo", "Lena", "Player One"].iter().enumerate() {
                    ui.horizontal(|ui| {
                        avatar(ui, &name[..1], 32.0, author_color(i), true);
                        ui.label(RichText::new(*name).color(MUTED));
                    });
                    ui.add_space(8.0);
                }
            });
    }
    fn conversation(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CHAT))
            .show(root, |ui| {
                egui::Panel::top("channel-header")
                    .exact_size(48.0)
                    .resizable(false)
                    .frame(egui::Frame::new().fill(CHAT).inner_margin(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(channel) = self.state.channels().get(self.state.channel) {
                                ui.label(RichText::new("#").size(23.0).color(MUTED));
                                ui.label(RichText::new(&channel.name).size(16.0).strong());
                                ui.separator();
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&channel.topic).size(12.0).color(MUTED),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    });
                self.composer_panel(ui);
                let conversation = self.state.conversation();
                let scroll_bottom = conversation != self.last_conversation
                    || self.state.messages().len() != self.last_message_count;
                self.last_conversation = conversation;
                self.last_message_count = self.state.messages().len();
                egui::ScrollArea::vertical()
                    .id_salt(("history", conversation))
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add_space(20.0);
                        egui::Frame::new().inner_margin(16).show(ui, |ui| {
                            let name = self
                                .state
                                .channels()
                                .get(self.state.channel)
                                .map_or("channel", |c| c.name.as_str());
                            ui.heading(format!("Welcome to #{name}!"));
                            ui.label(
                                RichText::new(format!(
                                    "This is the beginning of the #{name} channel."
                                ))
                                .size(14.0)
                                .color(MUTED),
                            );
                        });
                        ui.add_space(16.0);
                        let mut clicked = None;
                        let mut context = None;
                        let mut react = None;
                        for (i, message) in self.state.messages().iter().enumerate() {
                            let frame =
                                egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10));
                            let response = frame
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal_top(|ui| {
                                        avatar(
                                            ui,
                                            &message
                                                .author
                                                .chars()
                                                .next()
                                                .unwrap_or('?')
                                                .to_string(),
                                            40.0,
                                            author_color(i % 4),
                                            false,
                                        );
                                        ui.vertical(|ui| {
                                            if let Some(id) = message.reply_to {
                                                let preview = self
                                                    .state
                                                    .messages()
                                                    .iter()
                                                    .find(|m| m.id == id)
                                                    .map_or_else(
                                                        || {
                                                            "Original message unavailable"
                                                                .to_owned()
                                                        },
                                                        |m| {
                                                            format!(
                                                                "Reply to {} · {}",
                                                                m.author,
                                                                message_preview(&m.text)
                                                            )
                                                        },
                                                    );
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(preview)
                                                            .size(12.0)
                                                            .color(MUTED),
                                                    )
                                                    .truncate(),
                                                );
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(&message.author)
                                                        .size(15.0)
                                                        .strong(),
                                                );
                                                ui.label(
                                                    RichText::new(format!(
                                                        "Today at {}",
                                                        message.time
                                                    ))
                                                    .size(11.0)
                                                    .color(MUTED),
                                                );
                                            });
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&message.text)
                                                        .size(15.0)
                                                        .color(Color32::from_rgb(219, 222, 225)),
                                                )
                                                .wrap()
                                                .selectable(true),
                                            );
                                            if message.reactions > 0
                                                && ui.small_button("+1  1").clicked()
                                            {
                                                react = Some(i);
                                            }
                                        });
                                    });
                                })
                                .response;
                            let pointer = ui.input(|input| input.pointer.interact_pos());
                            if pointer.is_some_and(|p| response.rect.contains(p)) {
                                if ui.input(|input| {
                                    input.pointer.button_clicked(egui::PointerButton::Primary)
                                }) {
                                    clicked = Some(i);
                                }
                                if ui.input(|input| {
                                    input.pointer.button_clicked(egui::PointerButton::Secondary)
                                }) {
                                    context = Some((i, pointer.unwrap()));
                                }
                            }
                            if scroll_bottom && i + 1 == self.state.messages().len() {
                                response.scroll_to_me(Some(egui::Align::BOTTOM));
                            }
                        }
                        if let Some(i) = clicked {
                            self.state
                                .dispatch(AppAction::Select(Region::Conversation, i));
                        }
                        if let Some((i, anchor)) = context {
                            self.context_anchor = anchor;
                            if self.state.screen == Screen::Context {
                                self.state.dispatch(AppAction::Back);
                            }
                            self.state
                                .dispatch(AppAction::Select(Region::Conversation, i));
                            self.state.dispatch(AppAction::Context);
                        }
                        if let Some(i) = react {
                            self.state
                                .dispatch(AppAction::Select(Region::Conversation, i));
                            self.state.dispatch(AppAction::React);
                        }
                    });
            });
    }
    fn confirm_context(&mut self, ctx: &egui::Context) {
        if self.state.menu_item == 2
            && let Some(message) = self.state.messages().get(self.state.message)
        {
            ctx.copy_text(message.text.clone());
        }
        let reply = self.state.menu_item == 0;
        self.state.dispatch(AppAction::Confirm);
        if reply && self.state.reply_target().is_some() {
            self.focus_composer = true;
        }
    }
    fn modal(&mut self, ctx: &egui::Context) {
        if self.state.screen == Screen::Context {
            let mut open = true;
            egui::Popup::new(
                egui::Id::new("message-context"),
                ctx.clone(),
                self.context_anchor,
                egui::LayerId::background(),
            )
            .kind(egui::PopupKind::Menu)
            .open_bool(&mut open)
            .width(210.0)
            .show(|ui| {
                for (i, label) in ["Reply", "Toggle local reaction", "Copy text", "Close"]
                    .iter()
                    .enumerate()
                {
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::new(*label).selected(self.state.menu_item == i),
                        )
                        .clicked()
                    {
                        self.state.dispatch(AppAction::SelectMenu(i));
                        self.confirm_context(ctx);
                    }
                }
            });
            if !open && self.state.screen == Screen::Context {
                self.state.dispatch(AppAction::Back);
            }
            return;
        }
        if self.state.screen != Screen::Settings {
            return;
        }
        egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.heading("User Settings");
            ui.add_space(12.0);
            let labels = [
                format!(
                    "Performance metrics: {}",
                    if self.state.metrics { "On" } else { "Off" }
                ),
                "Close".into(),
            ];
            for (i, label) in labels.iter().enumerate() {
                if ui
                    .add_sized([ui.available_width(), 34.0], egui::Button::new(label))
                    .clicked()
                {
                    self.state.dispatch(AppAction::SelectMenu(i));
                    self.state.dispatch(AppAction::Confirm);
                }
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Rustcord · native Rust frontend\nMock backend · no account signed in",
                )
                .size(12.0)
                .color(MUTED),
            );
        });
    }
}
impl eframe::App for Rustcord {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let start = Instant::now();
        let ctx = root.ctx().clone();
        self.actions(&ctx);
        if self.first_frame {
            self.metrics.startup_ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.first_frame = false;
        }
        if self.state.metrics {
            egui::Panel::bottom("debug-metrics")
                .frame(egui::Frame::new().fill(RAIL).inner_margin(4))
                .show(root, |ui| {
                    let memory = self
                        .metrics
                        .memory
                        .map_or("n/a".into(), |m| format!("{:.1} MiB", m as f64 / 1048576.0));
                    ui.label(
                        RichText::new(format!(
                            "RAM {memory} | {:.0} rendered FPS | UI {:.2} ms | startup {:.1} ms",
                            self.metrics.fps, self.metrics.frame_ms, self.metrics.startup_ms
                        ))
                        .size(11.0)
                        .color(MUTED),
                    );
                });
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        self.rail(root);
        self.channels(root);
        self.members(root);
        self.conversation(root);
        self.modal(&ctx);
        self.metrics.frame(start.elapsed());
    }
}
fn message_preview(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .take(90)
        .collect()
}
fn author_color(i: usize) -> Color32 {
    [
        BLURPLE,
        Color32::from_rgb(187, 121, 196),
        Color32::from_rgb(83, 156, 142),
        Color32::from_rgb(195, 143, 91),
    ][i % 4]
}
fn avatar(ui: &mut egui::Ui, letter: &str, size: f32, color: Color32, online: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::proportional(size * 0.42),
        TEXT,
    );
    if online {
        let center = rect.right_bottom() - Vec2::splat(3.0);
        ui.painter().circle_filled(center, 6.0, SIDEBAR);
        ui.painter().circle_filled(center, 4.0, GREEN);
    }
}
fn keyboard_actions(ctx: &egui::Context, typing: bool) -> Vec<AppAction> {
    ctx.input_mut(|input| {
        let mut out = Vec::new();
        if !typing {
            for (key, logical) in [
                (egui::Key::ArrowUp, Key::Up),
                (egui::Key::ArrowDown, Key::Down),
                (egui::Key::ArrowLeft, Key::Left),
                (egui::Key::ArrowRight, Key::Right),
                (egui::Key::Enter, Key::Enter),
            ] {
                if input.consume_key(egui::Modifiers::NONE, key) {
                    out.push(input::keyboard(logical, false));
                }
            }
            if input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                out.push(AppAction::PreviousRegion);
            } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                out.push(AppAction::NextRegion);
            }
        }
        for (key, action) in [
            (egui::Key::Escape, AppAction::Back),
            (egui::Key::F1, AppAction::Menu),
            (egui::Key::F2, AppAction::Context),
            (egui::Key::F3, AppAction::Reply),
        ] {
            if input.consume_key(egui::Modifiers::NONE, key) {
                out.push(action);
            }
        }
        for (key, action) in [
            (egui::Key::PageUp, AppAction::SwitchServer(-1)),
            (egui::Key::PageDown, AppAction::SwitchServer(1)),
            (egui::Key::ArrowUp, AppAction::SwitchChannel(-1)),
            (egui::Key::ArrowDown, AppAction::SwitchChannel(1)),
        ] {
            if input.consume_key(egui::Modifiers::CTRL, key) {
                out.push(action);
            }
        }
        out
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key_action(key: egui::Key, modifiers: egui::Modifiers, typing: bool) -> Vec<AppAction> {
        let ctx = egui::Context::default();
        let mut actions = vec![];
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                actions = keyboard_actions(ui.ctx(), typing);
            },
        );
        output.textures_delta.clear();
        actions
    }
    #[test]
    fn editor_keeps_arrows_and_enter() {
        assert!(key_action(egui::Key::ArrowLeft, egui::Modifiers::NONE, true).is_empty());
        assert!(key_action(egui::Key::Enter, egui::Modifiers::NONE, true).is_empty());
        assert_eq!(
            key_action(egui::Key::Escape, egui::Modifiers::NONE, true),
            vec![AppAction::Back]
        );
    }
    #[test]
    fn keyboard_shortcuts() {
        assert_eq!(
            key_action(egui::Key::Tab, egui::Modifiers::SHIFT, false),
            vec![AppAction::PreviousRegion]
        );
        assert_eq!(
            key_action(egui::Key::ArrowDown, egui::Modifiers::CTRL, false),
            vec![AppAction::SwitchChannel(1)]
        );
        assert_eq!(
            key_action(egui::Key::Enter, egui::Modifiers::NONE, false),
            vec![AppAction::Confirm]
        );
    }
}
