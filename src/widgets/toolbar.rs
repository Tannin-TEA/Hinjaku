use eframe::egui::{self, Button, Layout, Align, Slider};
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
    // ウィンドウ最下部に固定され、横幅いっぱいに広がるパネル
    egui::TopBottomPanel::bottom("status_bar")
        .resizable(false)
        .min_height(22.0)
        .show(ctx, |ui| {
            bottom_toolbar_inner(ui, manager, config, view, is_nav_locked)
        }).inner
}

pub fn bottom_toolbar_inner(
    ui: &mut egui::Ui,
    manager: &Manager,
    config: &config::Config,
    view: &ViewState,
    is_nav_locked: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    ui.add_space(1.0);
    ui.horizontal(|ui| {
        let has = !manager.entries.is_empty();
            let accent = ui.visuals().selection.bg_fill;

            let mut left_btn = Button::new("<");
            if !config.manga_rtl { left_btn = left_btn.fill(accent); }
            if ui.add_enabled(has, left_btn).clicked() { action = Some(ViewerAction::PrevPage); }

            {
                let (max_idx, mut slider_val) = if has {
                    (manager.entries.len().saturating_sub(1), manager.target_index)
                } else {
                    (1, 0) // 0..=1, value=0 で確実に左端表示
                };
                ui.style_mut().spacing.slider_width = 160.0;
                let slider = Slider::new(&mut slider_val, 0..=max_idx).show_value(false).trailing_fill(true);
                if ui.add_enabled(has && !is_nav_locked, slider).changed() {
                    action = Some(ViewerAction::Seek(slider_val));
                }
            }

            let mut right_btn = Button::new(">");
            if config.manga_rtl { right_btn = right_btn.fill(accent); }
            if ui.add_enabled(has, right_btn).clicked() { action = Some(ViewerAction::NextPage); }

            {
                let count_text = if has {
                    format!("{}/{}", manager.target_index + 1, manager.entries.len())
                } else {
                    "0/0".to_string()
                };
                // "0000/0000" 相当の幅を固定確保し、直接描画でガタつきとCPU負荷を回避
                let (rect, _) = ui.allocate_exact_size(egui::vec2(65.0, ui.available_height()), egui::Sense::hover());
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, count_text,
                    egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color());
            }

            ui.separator();
            if ui.button(if view.manga_mode { "2P" } else { "1P" }).clicked() {
                action = Some(ViewerAction::ToggleManga);
            }
            ui.separator();

            if let Some(meta) = manager.entries_meta.get(manager.target_index) {
                let name = utils::get_display_name(std::path::Path::new(&meta.name));
                let size = utils::format_size(meta.size);
                let date = utils::format_timestamp(meta.mtime);
                let res  = if let Some(tex) = manager.get_first_tex(manager.target_index) {
                    let s = tex.size();
                    format!("{}x{}", s[0], s[1])
                } else {
                    "0x0".to_string()
                };
                let sort = format!("{} {}",
                    match config.sort_mode {
                        SortMode::Name  => "名前",
                        SortMode::Mtime => "日付",
                        SortMode::Size  => "サイズ",
                    },
                    if config.sort_order == SortOrder::Ascending { "▲" } else { "▼" }
                );
                let filter = match config.filter_mode {
                    FilterMode::Nearest  => "Nearest",
                    FilterMode::Bilinear => "Bilinear",
                    FilterMode::Bicubic  => "Bicubic",
                    FilterMode::Lanczos  => "Lanczos",
                };

                ui.separator();
                // 左寄せ: ファイル名 | ファイルサイズ | 解像度 | 更新日 | ソート
                ui.label(format!("{} | {} | {} | {} | {}", name, size, res, date, sort));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // 右寄せ: Image補正 | <Fit(xXXX)>
                    let zoom_pct = format!("{:.0}%", view.effective_zoom * 100.0);
                    let mode_label = match view.display_mode {
                        DisplayMode::Fit => format!("Fit({})", zoom_pct),
                        DisplayMode::WindowFit => format!("WinFit({})", zoom_pct),
                        DisplayMode::Manual => zoom_pct,
                    };
                    ui.label(mode_label);
                    ui.separator();
                    ui.label(filter);
                });
            } else {
                ui.label("待機中...");
            }
        });
    ui.add_space(1.0);
    action
}
