use eframe::egui;
use crate::config;

pub fn settings_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    settings_args_tmp: &mut [String],
) -> bool {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }

    let mut saved = false;
    egui::Window::new("外部アプリ連携の設定 (送る)")
        .fixed_size([500.0, 550.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("ショートカットキーやメニューから起動するソフトを5つまで設定できます。");
            ui.label("%P(%F):内部パスまで含む, %A(%D):実在パス(画像/書庫)");
            ui.add_space(8.0);

            let scroll_height = ui.available_height() - 100.0;
            egui::ScrollArea::vertical().max_height(scroll_height).show(ui, |ui| {
                for (i, args_tmp) in settings_args_tmp.iter_mut().enumerate().take(9) {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("送る {}", i + 1)).strong());
                            ui.add(egui::TextEdit::singleline(&mut config.external_apps[i].name).hint_text("メニューに表示される名前"));
                            ui.checkbox(&mut config.external_apps[i].close_after_launch, "起動後に終了");
                        });
                        egui::Grid::new(format!("grid_{}", i)).num_columns(2).show(ui, |ui| {
                            ui.label("実行パス:");
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut config.external_apps[i].exe);
                                if ui.button("参照 >").clicked() {
                                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                                        config.external_apps[i].exe = p.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label("引数:");
                            ui.text_edit_singleline(args_tmp);
                            ui.end_row();
                        });
                    });
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                // 固定幅コンテナ → 親の bottom_up(Center) が水平中央に配置する
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if ui.button("適用").clicked() {
                            for (i, args_tmp) in settings_args_tmp.iter().enumerate().take(9) {
                                config.external_apps[i].args = args_tmp
                                    .split_whitespace()
                                    .map(|s| s.to_string())
                                    .collect();
                            }
                            saved = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("閉じる").clicked() { *show = false; }
                    },
                );
            });
        });
    saved
}
