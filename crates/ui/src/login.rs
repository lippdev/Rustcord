//! Discord-inspired login geometry rendered natively; no web assets or browser runtime.
use super::{BLURPLE, TEXT};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2};
const CARD: Color32 = Color32::from_rgb(35, 36, 41);
const SECONDARY: Color32 = Color32::from_rgb(180, 181, 190);
const INPUT: Color32 = Color32::from_rgb(31, 32, 36);

pub(super) enum Action {
    None,
    Start,
    Cancel,
    Demo,
}
pub(super) fn draw(
    root: &mut egui::Ui,
    qr: Option<&egui::TextureHandle>,
    status: &str,
    busy: bool,
    active: bool,
) -> Action {
    let mut action = Action::None;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(root, |ui| {
            let canvas = ui.max_rect();
            background(ui, canvas);
            ui.painter().text(
                canvas.min + Vec2::new(48.0, 48.0),
                egui::Align2::LEFT_TOP,
                "Rustcord",
                egui::FontId::proportional(24.0),
                Color32::WHITE,
            );
            let card = Rect::from_center_size(canvas.center(), Vec2::new(784.0, 412.0));
            ui.painter().rect_filled(
                card.translate(Vec2::new(0.0, 6.0)).expand(4.0),
                12,
                Color32::from_black_alpha(35),
            );
            ui.painter().rect_filled(card, 8, CARD);
            let origin = card.min;
            let left = Rect::from_min_size(origin + Vec2::new(32.0, 32.0), Vec2::new(416.0, 348.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Boas-vindas de volta!")
                            .size(24.0)
                            .strong()
                            .color(TEXT),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Que bom ver você por aqui.")
                            .size(16.0)
                            .color(SECONDARY),
                    );
                });
            });
            for (name, label_y, field_y) in [
                ("E-mail ou número de telefone", 112.0, 138.0),
                ("Senha", 204.0, 230.0),
            ] {
                let label =
                    Rect::from_min_size(origin + Vec2::new(32.0, label_y), Vec2::new(416.0, 20.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(label), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).size(14.0).strong().color(TEXT));
                        ui.label(RichText::new("*").color(Color32::from_rgb(242, 117, 117)));
                    });
                });
                let field =
                    Rect::from_min_size(origin + Vec2::new(32.0, field_y), Vec2::new(416.0, 44.0));
                ui.painter().rect_filled(field, 8, INPUT);
                ui.painter().rect_stroke(
                    field,
                    8,
                    Stroke::new(1.0, Color32::from_rgb(52, 53, 60)),
                    egui::StrokeKind::Inside,
                );
                // Disabled semantic controls: never collect a password for an unsupported flow.
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(12.0, 8.0))),
                    |ui| {
                        let mut empty = String::new();
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut empty)
                                .id_salt(name)
                                .frame(egui::Frame::NONE)
                                .hint_text("Login por senha indisponível")
                                .desired_width(392.0)
                                .password(name == "Senha"),
                        );
                    },
                );
            }
            at(
                ui,
                origin + Vec2::new(32.0, 282.0),
                Vec2::new(416.0, 20.0),
                |ui| {
                    ui.label(
                        RichText::new("Use o código QR ao lado para entrar.")
                            .size(13.0)
                            .color(SECONDARY),
                    );
                },
            );
            at(
                ui,
                origin + Vec2::new(32.0, 314.0),
                Vec2::new(416.0, 40.0),
                |ui| {
                    let label = if active {
                        "Cancelar login"
                    } else {
                        "Entrar com QR"
                    };
                    if ui
                        .add_sized(
                            [416.0, 40.0],
                            egui::Button::new(RichText::new(label).size(16.0).strong())
                                .fill(BLURPLE)
                                .corner_radius(8),
                        )
                        .clicked()
                    {
                        action = if active {
                            Action::Cancel
                        } else {
                            Action::Start
                        };
                    }
                },
            );
            at(
                ui,
                origin + Vec2::new(32.0, 362.0),
                Vec2::new(416.0, 24.0),
                |ui| {
                    if ui
                        .add_enabled(
                            !active,
                            egui::Button::new(
                                RichText::new("Abrir demonstração local")
                                    .size(13.0)
                                    .color(Color32::from_rgb(139, 153, 255)),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        action = Action::Demo;
                    }
                },
            );
            let qr_rect = Rect::from_min_size(origin + Vec2::new(544.0, 36.0), Vec2::splat(176.0));
            ui.painter().rect_filled(qr_rect, 4, Color32::WHITE);
            if let Some(texture) = qr {
                ui.painter().image(
                    texture.id(),
                    qr_rect.shrink(6.0),
                    Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                at(ui, qr_rect.min, qr_rect.size(), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        if busy {
                            ui.spinner();
                        } else {
                            ui.label(
                                RichText::new("QR de login")
                                    .size(16.0)
                                    .color(Color32::from_rgb(77, 80, 91)),
                            );
                        }
                        ui.label(
                            RichText::new(if busy {
                                "Aguarde…"
                            } else {
                                "Clique em Entrar com QR"
                            })
                            .size(12.0)
                            .color(Color32::from_rgb(77, 80, 91)),
                        );
                    });
                });
            }
            at(
                ui,
                origin + Vec2::new(512.0, 244.0),
                Vec2::new(240.0, 120.0),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Entrar com código QR")
                                .size(23.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "Escaneie com o app Discord no celular e confirme o login.",
                            )
                            .size(16.0)
                            .color(SECONDARY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Sessão somente em memória")
                                .size(12.0)
                                .color(SECONDARY),
                        );
                    });
                },
            );
            if !status.is_empty() {
                at(
                    ui,
                    egui::pos2(card.left(), card.bottom() + 18.0),
                    Vec2::new(card.width(), 48.0),
                    |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(status).size(14.0).color(Color32::WHITE));
                        });
                    },
                );
            }
        });
    action
}
fn at(ui: &mut egui::Ui, position: Pos2, size: Vec2, content: impl FnOnce(&mut egui::Ui)) {
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(Rect::from_min_size(position, size)),
        content,
    );
}
fn background(ui: &egui::Ui, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [
        (rect.left_top(), Color32::from_rgb(24, 4, 70)),
        (rect.right_top(), Color32::from_rgb(30, 84, 217)),
        (rect.right_bottom(), Color32::from_rgb(10, 154, 247)),
        (rect.left_bottom(), Color32::from_rgb(22, 3, 66)),
    ] {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    ui.painter().add(egui::Shape::mesh(mesh));
    // Static lightweight particles; background needs no timers or texture assets.
    for i in 0..46 {
        let x = rect.left() + ((i * 197 + 39) % 997) as f32 / 997.0 * rect.width();
        let y = rect.top() + ((i * 283 + 17) % 991) as f32 / 991.0 * rect.height();
        ui.painter().circle_filled(
            egui::pos2(x, y),
            if i % 5 == 0 { 2.8 } else { 1.4 },
            Color32::from_white_alpha(if i % 3 == 0 { 55 } else { 28 }),
        );
    }
}
