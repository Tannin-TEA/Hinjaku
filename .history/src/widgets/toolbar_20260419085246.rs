use eframe::egui::{self, Button, TopBottomPanel, Layout, Align, Slider};
use crate::config::{self, SortMode, SortOrder, FilterMode};
use crate::manager::Manager;
use crate::types::{DisplayMode, ViewState};
use crate::utils;
use super::ViewerAction;

pub fn bottom_toolbar(
    ctx: &egui::Context,
    manager: &Manager,
    config: &config::Config,
    view: &ViewState,
    is_nav_locked: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    TopBottomPanel::bottom("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let has = !manager.entries.is_empty();
            let accent = ui.visuals().selection.bg_fill;

            let mut left_btn = Button::new("<");
            if !config.manga_rtl { left_btn = left_btn.fill(accent); }
            if ui.add_enabled(has, left_btn).clicked() { action = Some(ViewerAction::PrevPage); }

            if has {
                let max_idx = manager.entries.len().saturating_sub(1);
                let mut slider_val = manager.target_index;
                ui.style_mut().spacing.slider_width = 160.0;
                let slider = Slider::new(&mut slider_val, 0..=max_idx).show_value(false).trailing_fill(true);
                if ui.add_enabled(!is_nav_locked, slider).changed() {
                    action = Some(ViewerAction::Seek(slider_val));
                }
                ui.label(format!("{}/{}", manager.target_index + 1, manager.entries.len()));
            }

            let mut right_btn = Button::new(">");
            if config.manga_rtl { right_btn = right_btn.fill(accent); }
            if ui.add_enabled(has, right_btn).clicked() { action = Some(ViewerAction::NextPage); }

            ui.separator();
            if ui.button(if view.manga_mode { "2P" } else { "1P" }).clicked() {
                action = Some(ViewerAction::ToggleManga);
            }
            ui.separator();

            if has {
                let meta = &manager.entries_meta[manager.target_index];
                let name = utils::get_display_name(std::path::Path::new(&meta.name));
                let size = utils::format_size(meta.size);
                let date = utils::format_timestamp(meta.mtime);
                let res  = if let Some(tex) = manager.get_first_tex(manager.target_index) {
                    let s = tex.size();
                    format!("{}x{}", s[0], s[1])
                } else {
                    "---x---".to_string()
                };
                let sort = format!("{} {}",
                    match config.sort_mode {
                        SortMode::Name  => "Name",
                        SortMode::Mtime => "Day",
                        SortMode::Size  => "Size",
                    },
                    if config.sort_order == SortOrder::Ascending { "Asc" } else { "Desc" }
                );
                let filter = match config.filter_mode {
                    FilterMode::Nearest  => "Nearest",
                    FilterMode::Bilinear => "Bilinear",
                    FilterMode::Bicubic  => "Bicubic",
                    FilterMode::Lanczos  => "Lanczos",
                };
                ui.label(format!("{} | {} | {} | {} | [{}] | {}", name, size, res, date, sort, filter));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if view.display_mode == DisplayMode::Manual { ui.label(format!("{:.0}%", view.zoom * 100.0)); }
                    ui.label(match view.display_mode {
                        DisplayMode::Fit       => "Fit",
                        DisplayMode::WindowFit => "WinFit",
                        DisplayMode::Manual    => "Manual",
                    });
                });
            } else {
                ui.label("待機中...");
            }
        });
    });
    action
}
