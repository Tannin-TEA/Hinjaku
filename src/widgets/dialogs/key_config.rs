use eframe::egui::{self, RichText, Color32, Layout, Align};
use std::collections::HashSet;
use crate::config;
use super::super::get_action_label;

pub fn key_config_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    capturing_for: &mut Option<String>,
) -> bool {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && capturing_for.is_none() { *show = false; }

    let mut changed = false;
    egui::Window::new("キーコンフィグの設定")
        .fixed_size([500.0, 550.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("各アクションに割り当てるキーを設定します。");
            ui.label("・カンマ区切りで複数指定可能 (例: A, Space)");
            ui.label("・修飾キーは + で連結 (例: Ctrl+R, Alt+Enter)");
            ui.add_space(8.0);

            let scroll_height = ui.available_height() - 60.0;
            egui::ScrollArea::vertical().max_height(scroll_height).show(ui, |ui| {
                let conflicts = build_conflict_set(config);

                // キーコンフィグ画面に表示しないキー
                let mut shown: HashSet<String> = ["ToggleLimiter"].iter().map(|s| s.to_string()).collect();
                let categories = [
                    ("移動・ページ送り", vec![
                        "PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle",
                        "FirstPage", "LastPage", "JumpPage", "PrevDir", "NextDir",
                    ]),
                    ("画像操作", vec![
                        "ToggleFit", "ZoomIn", "ZoomOut", "ZoomReset",
                        "ToggleManga", "ToggleMangaRtl", "RotateCW", "RotateCCW",
                        "ToggleLinear", "ToggleBg",
                    ]),
                    ("ツリー・フォルダ操作", vec![
                        "Up", "Down", "Left", "Right", "Enter",
                        "ToggleTree", "RevealExplorer",
                    ]),
                    ("システム・その他", vec![
                        "ToggleMaximized", "ToggleFullscreen", "ToggleBorderless", "ToggleSmallBorderless",
                        "Escape", "SortSettings", "OpenKeyConfig", "ToggleDebug", "Quit",
                        "OpenExternal1", "OpenExternal2", "OpenExternal3",
                        "OpenExternal4", "OpenExternal5", "OpenExternal6",
                        "OpenExternal7", "OpenExternal8", "OpenExternal9",
                    ]),
                ];

                for (cat_name, key_ids) in categories {
                    ui.add_space(4.0);
                    ui.heading(RichText::new(cat_name).size(14.0).strong());
                    ui.separator();

                    let chunk_size = key_ids.len().div_ceil(2);
                    ui.columns(2, |cols| {
                        for (col_idx, chunk) in key_ids.chunks(chunk_size).enumerate() {
                            let col_ui = &mut cols[col_idx];
                            egui::Grid::new(egui::Id::new(cat_name).with(col_idx))
                                .num_columns(2)
                                .spacing([8.0, 4.0])
                                .show(col_ui, |ui| {
                                    for &key_id in chunk {
                                        if let Some(binding_val) = config.keys.get(key_id) {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 2.0;
                                                ui.label(RichText::new(get_action_label(key_id)).small());
                                                if conflicts.contains(key_id) {
                                                    ui.label(RichText::new("!").color(Color32::RED))
                                                        .on_hover_text("他のアクションとキーが重複しています");
                                                }
                                            });
                                            let mut binding = binding_val.clone();
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 2.0;
                                                if ui.add(egui::TextEdit::singleline(&mut binding).desired_width(80.0)).changed() {
                                                    config.keys.insert(key_id.to_string(), binding);
                                                    changed = true;
                                                }
                                                let is_capturing = capturing_for.as_deref() == Some(key_id);
                                                let btn_text = if is_capturing { "入力待ち..." } else { "設定" };
                                                if ui.selectable_label(is_capturing, btn_text).clicked() {
                                                    if is_capturing { *capturing_for = None; }
                                                    else { *capturing_for = Some(key_id.to_string()); }
                                                }
                                            });
                                            ui.end_row();
                                            shown.insert(key_id.to_string());
                                        }
                                    }
                                });
                        }
                    });
                }

                // マウスボタン割り当て
                ui.add_space(10.0);
                ui.heading(RichText::new("マウスボタンの割り当て").size(14.0).strong());
                ui.separator();
                egui::Grid::new("mouse_buttons_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let nav_actions: &[&str] = &[
                        "None", "PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle", "PrevDir", "NextDir",
                    ];
                    let mid_actions: &[&str] = &[
                        "None", "ToggleFit", "ToggleManga", "ToggleMangaRtl", "WindSizeLock",
                    ];
                    for i in 0..3usize {
                        let (label, current) = match i {
                            0 => ("戻るボタン (Mouse4):",   config.mouse4_action.clone()),
                            1 => ("中ボタン (WheelClick):", config.mouse_middle_action.clone()),
                            _ => ("進むボタン (Mouse5):",   config.mouse5_action.clone()),
                        };
                        let actions = if i == 1 { mid_actions } else { nav_actions };
                        ui.label(label);
                        egui::ComboBox::from_id_source(label)
                            .selected_text(get_action_label(&current))
                            .show_ui(ui, |ui| {
                                for &act in actions {
                                    if ui.selectable_label(current == act, get_action_label(act)).clicked() {
                                        match i {
                                            0 => config.mouse4_action = act.to_string(),
                                            1 => config.mouse_middle_action = act.to_string(),
                                            _ => config.mouse5_action = act.to_string(),
                                        }
                                        changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    }
                });

                // 未知のキー（INI直接編集分）
                let mut remaining_keys: Vec<_> = config.keys.keys()
                    .filter(|k| !shown.contains(*k))
                    .cloned()
                    .collect();
                if !remaining_keys.is_empty() {
                    ui.add_space(4.0);
                    ui.heading("その他（カスタム）");
                    ui.separator();
                    remaining_keys.sort();
                    egui::Grid::new("grid_remaining").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        for key in remaining_keys {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.label(RichText::new(&key).small());
                                if conflicts.contains(&key) {
                                    ui.label(RichText::new("!").color(Color32::RED))
                                        .on_hover_text("他のアクションとキーが重複しています");
                                }
                            });
                            let mut binding = config.keys.get(&key).cloned().unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if ui.add(egui::TextEdit::singleline(&mut binding).desired_width(80.0)).changed() {
                                    config.keys.insert(key.clone(), binding);
                                    changed = true;
                                }
                                let is_capturing = capturing_for.as_deref() == Some(&key);
                                let btn_text = if is_capturing { "入力待ち..." } else { "設定" };
                                if ui.selectable_label(is_capturing, btn_text).clicked() {
                                    if is_capturing { *capturing_for = None; }
                                    else { *capturing_for = Some(key.clone()); }
                                }
                            });
                            ui.end_row();
                        }
                    });
                }
            });

            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
    changed
}

fn build_conflict_set(config: &config::Config) -> HashSet<String> {
    let tree_modal: HashSet<&str> = ["Up", "Down", "Left", "Right", "Enter"].into_iter().collect();
    let mut normal_map: std::collections::HashMap<String, Vec<String>> = Default::default();
    let mut tree_map:   std::collections::HashMap<String, Vec<String>> = Default::default();

    for (action_id, binding) in &config.keys {
        let target = if tree_modal.contains(action_id.as_str()) { &mut tree_map } else { &mut normal_map };
        for k in binding.split(',') {
            let k = k.trim();
            if !k.is_empty() {
                target.entry(k.to_string()).or_default().push(action_id.clone());
            }
        }
    }

    let mut set = HashSet::new();
    for map in [normal_map, tree_map] {
        for (_key, actions) in map {
            if actions.len() > 1 {
                for id in actions { set.insert(id); }
            }
        }
    }
    set
}
