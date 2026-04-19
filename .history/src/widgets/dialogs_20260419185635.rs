use eframe::egui::{self, RichText, Color32, Layout, Align};
use std::collections::HashSet;
use crate::config::{self, SortMode, SortOrder};
use crate::manager;
use crate::integrator;
use super::get_action_label;

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
                for i in 0..5 {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("アプリ {}", i + 1)).strong());
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
                            ui.text_edit_singleline(&mut settings_args_tmp[i]);
                            ui.end_row();
                        });
                    });
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("適用").clicked() {
                        for i in 0..5 {
                            config.external_apps[i].args = settings_args_tmp[i]
                                .split_whitespace()
                                .map(|s| s.to_string())
                                .collect();
                        }
                        saved = true;
                    }
                    if ui.button("閉じる").clicked() { *show = false; }
                });
            });
        });
    saved
}

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

                let mut shown = HashSet::new();
                let categories = [
                    ("移動・ページ送り", vec!["PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle", "FirstPage", "LastPage", "PrevDir", "NextDir"]),
                    ("画像操作",         vec!["ToggleFit", "ZoomIn", "ZoomOut", "ToggleManga", "ToggleMangaRtl", "ToggleLinear", "ToggleBg", "RotateCW", "RotateCCW"]),
                    ("フォルダ・ツリー操作", vec!["Up", "Down", "Left", "Right", "Enter", "ToggleTree", "RevealExplorer"]),
                    ("システム・その他",  vec!["ToggleFullscreen", "ToggleBorderless", "Escape", "SortSettings", "OpenKeyConfig", "OpenExternal1", "OpenExternal2", "OpenExternal3", "OpenExternal4", "OpenExternal5", "Quit"]),
                ];

                for (cat_name, key_ids) in categories {
                    ui.add_space(4.0);
                    ui.heading(RichText::new(cat_name).size(14.0).strong());
                    ui.separator();

                    let chunk_size = (key_ids.len() + 1) / 2;
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
                    let mouse_actions = [
                        "None", "PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle",
                        "PrevDir", "NextDir", "ToggleFit", "ToggleManga",
                    ];
                    for i in 0..3usize {
                        let (label, current) = match i {
                            0 => ("戻るボタン (Mouse4):",   config.mouse4_action.clone()),
                            1 => ("中ボタン (WheelClick):", config.mouse_middle_action.clone()),
                            _ => ("進むボタン (Mouse5):",   config.mouse5_action.clone()),
                        };
                        ui.label(label);
                        egui::ComboBox::from_id_source(label)
                            .selected_text(get_action_label(&current))
                            .show_ui(ui, |ui| {
                                for act in mouse_actions {
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

pub fn about_window(ctx: &egui::Context, show: &mut bool) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }
    egui::Window::new("Hinjaku について")
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size([400.0, 450.0])
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Hinjaku");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("吹けば飛ぶよな軽量ビューア");
            });
            ui.separator();
            ui.label("このソフトウェアは以下のオープンソースライブラリを使用しています:");
            ui.add_space(4.0);

            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                let licenses = [
                    ("eframe / egui", "MIT / Apache-2.0", "https://github.com/emilk/egui"),
                    ("image",         "MIT",               "https://github.com/image-rs/image"),
                    ("zip-rs",        "MIT",               "https://github.com/zip-rs/zip"),
                    ("sevenz-rust",   "Apache-2.0",        "https://github.com/mcmunder/sevenz-rust"),
                    ("rust-ini",      "MIT",               "https://github.com/amrayn/rust-ini"),
                    ("windows-sys",   "MIT / Apache-2.0",  "https://github.com/microsoft/windows-rs"),
                    ("rfd",           "MIT",               "https://github.com/PolyMeilex/rfd"),
                    ("Pdfium",        "BSD 3-Clause",      "https://opensource.google/projects/pdfium"),
                ];
                for (name, license, url) in licenses {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(license).small());
                            });
                        });
                        ui.label(RichText::new(url).small().weak());
                        ui.add_space(4.0);
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
}

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

/// キーの重複しているアクションIDのセットを生成する
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
