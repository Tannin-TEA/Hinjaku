use eframe::egui;
use crate::config;
use crate::types::DisplayMode;
use crate::constants::*;
use super::App;

impl App {
    pub(super) fn zoom_in(&mut self) {
        self.view.zoom = (self.view.zoom * ui::ZOOM_STEP).min(ui::MAX_ZOOM);
    }

    pub(super) fn zoom_out(&mut self) {
        self.view.zoom = (self.view.zoom / ui::ZOOM_STEP).max(ui::MIN_ZOOM);
    }

    pub(super) fn zoom_reset(&mut self) {
        self.view.zoom = 1.0;
    }

    pub(super) fn set_display_mode(&mut self, m: DisplayMode) {
        self.view.display_mode = m;
        self.config.display_mode = m;
        self.save_config();
        if m == DisplayMode::Manual { self.view.zoom = 1.0; }
    }

    pub(super) fn toggle_manga(&mut self, ctx: &egui::Context) {
        self.view.manga_mode = !self.view.manga_mode;
        self.config.manga_mode = self.view.manga_mode;
        self.save_config();
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    pub(super) fn toggle_filter_mode(&mut self, ctx: &egui::Context) {
        self.config.filter_mode = match self.config.filter_mode {
            config::FilterMode::Nearest  => config::FilterMode::Bilinear,
            config::FilterMode::Bilinear => config::FilterMode::Bicubic,
            config::FilterMode::Bicubic  => config::FilterMode::Lanczos,
            config::FilterMode::Lanczos  => config::FilterMode::Nearest,
        };
        self.manager.clear_cache();
        self.save_config();
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }
}
