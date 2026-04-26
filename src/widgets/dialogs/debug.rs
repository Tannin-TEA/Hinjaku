use eframe::egui;
use crate::manager;
use crate::integrator;

pub fn debug_window(
    ctx: &egui::Context,
    show: &mut bool,
    manager: &manager::Manager,
) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }

    egui::Window::new("デバッグ情報")
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size([400.0, 450.0])
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(format!("メモリ使用量: {}", integrator::get_memory_usage_str()));
                ui.label(format!("キャッシュ使用数: {} / {}", manager.cache_len(), crate::constants::cache::CACHE_MAX));
                let cache_kb = manager.total_cache_size_bytes() / 1024;
                ui.label(format!("キャッシュサイズ (推定): {} KB", cache_kb));
                ui.label(format!("リスティング中: {}", if manager.is_listing { "はい" } else { "いいえ" }));
                ui.separator();
                ui.label(format!("現在のページ (current): {}", manager.current + 1));
                ui.label(format!("ターゲット (target_index): {}", manager.target_index + 1));
                ui.label(format!("全エントリ数: {}", manager.entries.len()));
                if let Some(path) = &manager.archive_path {
                    ui.label(format!("パス: {}", path.display()));
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
}
