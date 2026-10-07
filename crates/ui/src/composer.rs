//! Native multiline composer and keyboard/IME policy.
use super::{CHAT, MUTED, Rustcord, TEXT};
use eframe::egui::{self, Color32};
use input::AppAction;

#[derive(Default)]
pub(super) struct ComposerInput {
    composing: bool,
}
impl ComposerInput {
    fn consume_send(&mut self, ctx: &egui::Context, enabled: bool) -> bool {
        let popup_open = egui::Popup::is_any_open(ctx);
        ctx.input_mut(|input| {
            let mut ime_event = false;
            for event in &input.events {
                if let egui::Event::Ime(ime) = event {
                    ime_event = true;
                    match ime {
                        egui::ImeEvent::Preedit { text, .. } => self.composing = !text.is_empty(),
                        egui::ImeEvent::Commit(_) => self.composing = false,
                        #[expect(deprecated)]
                        egui::ImeEvent::Disabled => self.composing = false,
                        _ => {}
                    }
                }
            }
            // Enter can confirm an IME candidate. Never send on an IME event frame.
            if !enabled || popup_open || self.composing || ime_event {
                return false;
            }
            let mut send = false;
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = event
                    && modifiers.is_none()
                {
                    send |= !repeat;
                    return false;
                }
                true
            });
            send
        })
    }
}
impl Rustcord {
    pub(super) fn composer_id(&self) -> egui::Id {
        egui::Id::new(("composer", self.state.server, self.state.channel))
    }
    pub(super) fn composer_panel(&mut self, root: &mut egui::Ui) {
        let key = (self.state.server, self.state.channel);
        if key != self.draft_conversation {
            self.composer_input = ComposerInput::default();
            self.draft_conversation = key;
        }
        if let Some(draft) = self.state.draft() {
            self.draft.clone_from(&draft.text);
        } else {
            self.draft.clear();
        }
        let id = self.composer_id();
        let enabled = self.state.screen == app_state::Screen::Main
            && self.state.channels().get(self.state.channel).is_some();
        let focused = root.ctx().memory(|m| m.has_focus(id));
        let entered = self
            .composer_input
            .consume_send(root.ctx(), enabled && focused);
        let width = (root.available_width() - 128.0).max(40.0);
        let font = egui::TextStyle::Body.resolve(root.style());
        let line_height = root.text_style_height(&egui::TextStyle::Body);
        let rows = root
            .fonts_mut(|f| f.layout(self.draft.clone(), font, TEXT, width).rows.len())
            .clamp(1, 6);
        let editor_height = rows as f32 * line_height + 6.0;
        let reply = self.state.draft().and_then(|d| d.reply_to).map(|_| {
            self.state.reply_target().map_or_else(
                || "Original message unavailable".to_owned(),
                |m| {
                    format!(
                        "Replying to {} · {}",
                        m.author,
                        super::message_preview(&m.text)
                    )
                },
            )
        });
        egui::Panel::bottom("composer-panel")
            .exact_size(editor_height + 66.0 + if reply.is_some() { 28.0 } else { 0.0 })
            .resizable(false)
            .frame(egui::Frame::new().fill(CHAT).inner_margin(16))
            .show(root, |ui| {
                if let Some(reply) = reply {
                    ui.horizontal(|ui| {
                        if ui.small_button("Cancel reply").clicked() {
                            self.state.dispatch(AppAction::CancelReply);
                        }
                        ui.add(
                            egui::Label::new(egui::RichText::new(reply).size(12.0).color(MUTED))
                                .truncate(),
                        );
                    });
                    ui.add_space(4.0);
                }
                egui::Frame::new()
                    .fill(Color32::from_rgb(56, 58, 64))
                    .inner_margin(10)
                    .corner_radius(8)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(egui::RichText::new("+").size(24.0).color(MUTED));
                            let channel = self
                                .state
                                .channels()
                                .get(self.state.channel)
                                .map_or("channel", |c| c.name.as_str());
                            let editor = egui::ScrollArea::vertical()
                                .id_salt(("draft-scroll", key))
                                .max_width(width)
                                .max_height(editor_height)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.add_enabled(
                                        enabled,
                                        egui::TextEdit::multiline(&mut self.draft)
                                            .id(id)
                                            .hint_text(format!("Message #{channel}"))
                                            .frame(egui::Frame::NONE)
                                            .desired_rows(1)
                                            .return_key(egui::KeyboardShortcut::new(
                                                egui::Modifiers::SHIFT,
                                                egui::Key::Enter,
                                            ))
                                            .char_limit(2000)
                                            .desired_width(width),
                                    )
                                })
                                .inner;
                            if editor.changed() {
                                self.state
                                    .dispatch(AppAction::UpdateDraft(self.draft.clone()));
                            }
                            let can_send = enabled
                                && !self.draft.trim().is_empty()
                                && !self.composer_input.composing;
                            let clicked = ui
                                .add_enabled(can_send, egui::Button::new("Send"))
                                .clicked();
                            if can_send && (entered || clicked) {
                                self.state.dispatch(AppAction::SendDraft);
                                self.draft.clear();
                                editor.request_focus();
                            }
                            if self.focus_composer {
                                editor.request_focus();
                                self.focus_composer = false;
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Enter to send · Shift+Enter for a new line · Local mock",
                        )
                        .size(10.0)
                        .color(MUTED),
                    );
                    if self.draft.chars().count() >= 1800 {
                        ui.label(
                            egui::RichText::new(format!("{}/2000", self.draft.chars().count()))
                                .size(10.0)
                                .color(MUTED),
                        );
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(input: &mut ComposerInput, events: Vec<egui::Event>, enabled: bool) -> bool {
        let ctx = egui::Context::default();
        frame_with_context(&ctx, input, events, enabled)
    }
    fn frame_with_context(
        ctx: &egui::Context,
        input: &mut ComposerInput,
        events: Vec<egui::Event>,
        enabled: bool,
    ) -> bool {
        let mut send = false;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                send = input.consume_send(ui.ctx(), enabled);
            },
        );
        out.textures_delta.clear();
        send
    }
    fn enter(modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }
    fn app_frame(
        app: &mut Rustcord,
        ctx: &egui::Context,
        mut events: Vec<egui::Event>,
    ) -> egui::PlatformOutput {
        // Model physical key taps, including key-up; egui derives repeat from held keys.
        let releases: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(egui::Event::Key {
                    key: *key,
                    physical_key: *physical_key,
                    pressed: false,
                    repeat: false,
                    modifiers: *modifiers,
                }),
                _ => None,
            })
            .collect();
        events.extend(releases);
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
        );
        out.textures_delta.clear();
        out.platform_output
    }
    fn native_test_app() -> (Rustcord, egui::Context) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = Rustcord::new(&cc, std::time::Instant::now(), false, false);
        app_frame(&mut app, &ctx, vec![]);
        (app, ctx)
    }
    #[test]
    fn keyboard_reply_focuses_editor_and_escape_preserves_written_draft() {
        let (mut app, ctx) = native_test_app();
        app.state
            .dispatch(AppAction::Select(input::Region::Conversation, 1));
        let target = app.state.messages()[1].id;
        let mut f3 = enter(egui::Modifiers::NONE, false);
        if let egui::Event::Key { key, .. } = &mut f3 {
            *key = egui::Key::F3;
        }
        app_frame(&mut app, &ctx, vec![f3.clone()]);
        assert!(ctx.memory(|m| m.has_focus(app.composer_id())));
        assert_eq!(app.state.reply_target().unwrap().id, target);
        app_frame(&mut app, &ctx, vec![egui::Event::Text("my reply".into())]);
        let mut escape = enter(egui::Modifiers::NONE, false);
        if let egui::Event::Key { key, .. } = &mut escape {
            *key = egui::Key::Escape;
        }
        app_frame(&mut app, &ctx, vec![escape]);
        assert!(app.state.reply_target().is_none());
        assert_eq!(app.state.draft().unwrap().text, "my reply");
        app_frame(&mut app, &ctx, vec![f3]);
        app_frame(&mut app, &ctx, vec![enter(egui::Modifiers::NONE, false)]);
        assert_eq!(app.state.messages().last().unwrap().reply_to, Some(target));
    }
    #[test]
    fn context_copy_emits_full_text_without_sending_a_message() {
        let (mut app, ctx) = native_test_app();
        app.state
            .dispatch(AppAction::Select(input::Region::Conversation, 1));
        let text = app.state.messages()[1].text.clone();
        let count = app.state.messages().len();
        app.state.dispatch(AppAction::Context);
        app.state.dispatch(AppAction::SelectMenu(2));
        let out = app_frame(&mut app, &ctx, vec![enter(egui::Modifiers::NONE, false)]);
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if *t == text))
        );
        assert_eq!(app.state.messages().len(), count);
        assert_eq!(app.state.screen, app_state::Screen::Main);
    }
    #[test]
    fn native_editor_shift_enter_then_enter_sends_exactly_one_multiline_message() {
        let (mut app, ctx) = native_test_app();
        let count = app.state.messages().len();
        ctx.memory_mut(|m| m.request_focus(app.composer_id()));
        app_frame(&mut app, &ctx, vec![egui::Event::Text("first".into())]);
        app_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                enter(egui::Modifiers::SHIFT, false),
            ],
        );
        app_frame(&mut app, &ctx, vec![egui::Event::Text("second".into())]);
        assert_eq!(app.state.messages().len(), count);
        assert_eq!(app.state.draft().unwrap().text, "first\nsecond");
        app_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                enter(egui::Modifiers::NONE, false),
            ],
        );
        assert_eq!(app.state.messages().len(), count + 1);
        assert_eq!(app.state.messages().last().unwrap().text, "first\nsecond");
        assert!(app.state.draft().is_none());
        assert!(ctx.memory(|m| m.has_focus(app.composer_id())));
    }
    #[test]
    fn switching_channels_preserves_editor_text_and_isolates_undo() {
        let (mut app, ctx) = native_test_app();
        ctx.memory_mut(|m| m.request_focus(app.composer_id()));
        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("first channel draft".into())],
        );
        let first = app.composer_id();
        app.state.dispatch(AppAction::SwitchChannel(1));
        app_frame(&mut app, &ctx, vec![]);
        let second = app.composer_id();
        assert_ne!(first, second);
        ctx.memory_mut(|m| m.request_focus(second));
        app_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
        );
        assert!(app.state.draft().is_none());
        app.state.dispatch(AppAction::SwitchChannel(-1));
        app_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.draft, "first channel draft");
    }
    #[test]
    fn only_plain_non_repeated_enter_sends_from_an_enabled_editor() {
        let mut input = ComposerInput::default();
        assert!(frame(
            &mut input,
            vec![enter(egui::Modifiers::NONE, false)],
            true
        ));
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
        ] {
            assert!(!frame(&mut input, vec![enter(modifiers, false)], true));
        }

        assert!(!frame(
            &mut input,
            vec![enter(egui::Modifiers::NONE, false)],
            false
        ));
    }
    #[test]
    fn holding_enter_does_not_send_repeatedly() {
        let ctx = egui::Context::default();
        let mut input = ComposerInput::default();
        assert!(frame_with_context(
            &ctx,
            &mut input,
            vec![enter(egui::Modifiers::NONE, false)],
            true
        ));
        assert!(!frame_with_context(
            &ctx,
            &mut input,
            vec![enter(egui::Modifiers::NONE, true)],
            true
        ));
    }
    #[test]
    fn ime_confirmation_does_not_send_until_a_later_enter() {
        let mut input = ComposerInput::default();
        assert!(!frame(
            &mut input,
            vec![
                egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "候補".into(),
                    active_range_chars: None
                }),
                enter(egui::Modifiers::NONE, false)
            ],
            true
        ));
        assert!(!frame(
            &mut input,
            vec![enter(egui::Modifiers::NONE, false)],
            true
        ));
        assert!(!frame(
            &mut input,
            vec![
                egui::Event::Ime(egui::ImeEvent::Commit("確定".into())),
                enter(egui::Modifiers::NONE, false)
            ],
            true
        ));
        assert!(frame(
            &mut input,
            vec![enter(egui::Modifiers::NONE, false)],
            true
        ));
    }
}
