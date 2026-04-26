use eframe::egui;
use crate::config;

pub fn limiter_settings_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
) -> bool {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }
    let mut is_open = *show;
    let mut changed = false;
    let mut close_clicked = false;

    egui::Window::new("リミッター設定")
        .open(&mut is_open)
        .collapsible(false).resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            changed |= ui.checkbox(&mut config.limiter_mode, "リミッターモードを有効にする").changed();
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label("動作設定");
                changed |= ui.checkbox(&mut config.limiter_stop_at_start, "フォルダの最初で止まる").changed();
                changed |= ui.checkbox(&mut config.limiter_stop_at_end, "フォルダの最後で止まる").changed();
            });
            ui.group(|ui| {
                ui.label("ページ送り待機時間（秒）");
                changed |= ui.add(egui::Slider::new(&mut config.limiter_page_duration, 0.0..=1.0).step_by(0.01)).changed();
                ui.add_space(4.0);
                ui.label("フォルダ/アーカイブ移動待機時間（秒）");
                changed |= ui.add(egui::Slider::new(&mut config.limiter_folder_duration, 0.0..=2.0).step_by(0.01)).changed();
            });
            ui.add_space(12.0);
            if ui.button("閉じる").clicked() { close_clicked = true; }
        });

    if close_clicked { is_open = false; }
    *show = is_open;
    changed
}
