//! Session-only LRU textures: at most 64 thumbnails, one download outstanding.
use crate::{BLURPLE, TEXT, avatar};
use discord_network::{Command, Connection, ImageData};
use eframe::egui::{self, Vec2};
use std::collections::{HashMap, VecDeque};
#[derive(Default)]
pub(super) struct Images {
    entries: HashMap<String, Entry>,
    queue: VecDeque<String>,
    outstanding: Option<String>,
    clock: u64,
}
struct Entry {
    texture: Option<egui::TextureHandle>,
    touched: u64,
    failed: bool,
}
impl Images {
    fn get(&mut self, url: &str) -> Option<egui::TextureHandle> {
        self.clock = self.clock.saturating_add(1);
        if let Some(entry) = self.entries.get_mut(url) {
            entry.touched = self.clock;
            return entry.texture.clone();
        }
        if self.entries.len() >= 64 {
            let old = self
                .entries
                .iter()
                .filter(|(key, _)| self.outstanding.as_ref() != Some(*key))
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone());
            if let Some(old) = old {
                self.entries.remove(&old);
                self.queue.retain(|url| *url != old);
            }
        }
        self.entries.insert(
            url.to_owned(),
            Entry {
                texture: None,
                touched: self.clock,
                failed: false,
            },
        );
        self.queue.push_back(url.to_owned());
        None
    }
    pub(super) fn accept(&mut self, ctx: &egui::Context, url: String, image: Option<ImageData>) {
        if self.outstanding.as_ref() == Some(&url) {
            self.outstanding = None;
        }
        if let Some(entry) = self.entries.get_mut(&url) {
            entry.failed = image.is_none();
            entry.texture = image.map(|data| {
                ctx.load_texture(
                    "discord-media",
                    egui::ColorImage::from_rgba_unmultiplied([data.width, data.height], &data.rgba),
                    egui::TextureOptions::LINEAR,
                )
            });
        }
    }
    pub(super) fn flush(&mut self, connection: Option<&Connection>) {
        if self.outstanding.is_some() {
            return;
        }
        if let Some(url) = self.queue.front()
            && let Some(connection) = connection
            && connection.command(Command::Image(url.clone())).is_ok()
        {
            self.outstanding = self.queue.pop_front();
        }
    }
    pub(super) fn avatar(&mut self, ui: &mut egui::Ui, url: Option<&str>, name: &str, size: f32) {
        if !ui.is_rect_visible(egui::Rect::from_min_size(
            ui.next_widget_position(),
            Vec2::splat(size),
        )) {
            ui.allocate_space(Vec2::splat(size));
            return;
        }
        if let Some(texture) = url.and_then(|url| self.get(url)) {
            ui.add(
                egui::Image::new((texture.id(), Vec2::splat(size)))
                    .corner_radius((size / 2.0) as u8),
            );
        } else {
            avatar(
                ui,
                &name.chars().next().unwrap_or('?').to_string(),
                size,
                BLURPLE,
                false,
            );
        }
    }
    pub(super) fn thumbnail(&mut self, ui: &mut egui::Ui, url: &str) {
        if !ui.is_rect_visible(egui::Rect::from_min_size(
            ui.next_widget_position(),
            Vec2::new(ui.available_width().min(320.0), 180.0),
        )) {
            ui.allocate_space(Vec2::new(ui.available_width().min(320.0), 180.0));
            return;
        }
        if let Some(texture) = self.get(url) {
            let size = texture.size_vec2();
            let scale = (ui.available_width() / size.x).min(1.0);
            ui.add(egui::Image::new((texture.id(), size * scale)).corner_radius(6));
        } else {
            let failed = self.entries.get(url).is_some_and(|entry| entry.failed);
            ui.label(
                egui::RichText::new(if failed {
                    "Prévia indisponível; use o link do anexo."
                } else {
                    "Carregando imagem…"
                })
                .size(12.0)
                .color(TEXT),
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_and_pending_queue_are_bounded_and_repeated_draws_deduplicate() {
        let mut images = Images::default();
        for _ in 0..10 {
            images.get("same");
        }
        assert_eq!(images.queue.len(), 1);
        for i in 0..200 {
            images.get(&i.to_string());
        }
        assert_eq!(images.entries.len(), 64);
        assert_eq!(images.queue.len(), 64);
    }
}
