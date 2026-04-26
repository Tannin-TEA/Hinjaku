use eframe::egui;
use crate::config::{self, SortMode, SortOrder};

pub fn sort_settings_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    focus_idx: &mut usize,
    enter_key: bool,
    space_key: bool,
) -> bool {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }

    let mut changed = false;
    egui::Window::new("ソートの設定 (S)")
        .fixed_size([500.0, 550.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            let (arr_up, arr_dn, arr_left, arr_right) = ctx.input(|i| (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            ));

            if arr_up   { *focus_idx = (*focus_idx + 2) % 3; }
            if arr_dn   { *focus_idx = (*focus_idx + 1) % 3; }
            if enter_key { *show = false; }

            if space_key {
                match *focus_idx {
                    0 => {
                        config.sort_mode = match config.sort_mode {
                            SortMode::Name  => SortMode::Mtime,
                            SortMode::Mtime => SortMode::Size,
                            SortMode::Size  => SortMode::Name,
                        };
                        changed = true;
                    }
                    1 => {
                        config.sort_order = if config.sort_order == SortOrder::Ascending {
                            SortOrder::Descending
                        } else {
                            SortOrder::Ascending
                        };
                        changed = true;
                    }
                    2 => { config.sort_natural = !config.sort_natural; changed = true; }
                    _ => {}
                }
            }

            ui.label("矢印キーで選択 / Enterで戻る");
            ui.add_space(8.0);

            let scroll_height = ui.available_height() - 60.0;
            egui::ScrollArea::vertical().max_height(scroll_height).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let active = *focus_idx == 0;
                    let label = if active {
                        egui::RichText::new("> 基準:").color(egui::Color32::YELLOW)
                    } else {
                        egui::RichText::new("  基準:")
                    };
                    ui.label(label);
                    changed |= ui.radio_value(&mut config.sort_mode, SortMode::Name,  "ファイル名").changed();
                    changed |= ui.radio_value(&mut config.sort_mode, SortMode::Mtime, "更新日時").changed();
                    changed |= ui.radio_value(&mut config.sort_mode, SortMode::Size,  "サイズ").changed();
                    if active {
                        if arr_right {
                            config.sort_mode = match config.sort_mode {
                                SortMode::Name  => SortMode::Mtime,
                                SortMode::Mtime => SortMode::Size,
                                SortMode::Size  => SortMode::Name,
                            };
                            changed = true;
                        }
                        if arr_left {
                            config.sort_mode = match config.sort_mode {
                                SortMode::Name  => SortMode::Size,
                                SortMode::Mtime => SortMode::Name,
                                SortMode::Size  => SortMode::Mtime,
                            };
                            changed = true;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let active = *focus_idx == 1;
                    let label = if active {
                        egui::RichText::new("> 順序:").color(egui::Color32::YELLOW)
                    } else {
                        egui::RichText::new("  順序:")
                    };
                    ui.label(label);
                    changed |= ui.radio_value(&mut config.sort_order, SortOrder::Ascending,  "昇順").changed();
                    changed |= ui.radio_value(&mut config.sort_order, SortOrder::Descending, "降順").changed();
                    if active && (arr_left || arr_right) {
                        config.sort_order = if config.sort_order == SortOrder::Ascending {
                            SortOrder::Descending
                        } else {
                            SortOrder::Ascending
                        };
                        changed = true;
                    }
                });

                ui.separator();
                let active = *focus_idx == 2;
                let check_text = if active {
                    egui::RichText::new("> 自然順（数字の大きさを考慮）").color(egui::Color32::YELLOW)
                } else {
                    egui::RichText::new("  自然順（数字の大きさを考慮）")
                };
                if ui.checkbox(&mut config.sort_natural, check_text).changed() { changed = true; }
                if active && (arr_left || arr_right) { config.sort_natural = !config.sort_natural; changed = true; }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
    changed
}
