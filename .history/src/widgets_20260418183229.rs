use eframe::egui::{self, RichText, Color32, Button, TopBottomPanel, Layout, Align, menu, Slider, ScrollArea};
use std::path::PathBuf;
use std::collections::HashSet;
use crate::config::{self, SortMode, SortOrder, BackgroundMode, FilterMode};
use crate::manager;
use crate::utils;
use crate::viewer::DisplayMode;
use crate::integrator;

/// ユーザーがUI操作を通じて要求したアクション
pub enum ViewerAction {
    OpenRecent(String),
    OpenFolder,
    RevealInExplorer,
    OpenExternal(usize),
    OpenExternalSettings,
    OpenKeyConfig,
    Exit,
    SetDisplayMode(DisplayMode),
    ZoomIn,
    ZoomOut,
    ToggleManga,
    ToggleMangaRtl,
    ToggleTree,
    OpenSortSettings,
    ToggleAlwaysOnTop,
    ToggleLinear,
    ToggleMultipleInstances,
    Rotate(bool), // true = CW, false = CCW
    GoPrevDir,
    GoNextDir,
    SetOpenFromEnd(bool),
    SetBgMode(BackgroundMode),
    // ツールバーアクション
    PrevPage,
    NextPage,
    NextDir,
    Seek(usize),
    ToggleDebug,
    SetRenderer(config::RendererMode),
    ToggleWindowResizable,
    ResizeWindow(u32, u32),
    About,
    SetMouseAction(u8, String),
}

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

            // フッターの高さを確保したスクロールエリア
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

            // フッターを下部に配置
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("適用").clicked() {
                        for i in 0..5 {
                            config.external_apps[i].args = settings_args_tmp[i].split_whitespace().map(|s| s.to_string()).collect();
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
        .collapsible(false).resizable(true)
        .show(ctx, |ui| {
            let (arr_up, arr_dn, arr_left, arr_right) = ctx.input(|i| (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            ));

            if arr_up { *focus_idx = (*focus_idx + 2) % 3; }
            if arr_dn { *focus_idx = (*focus_idx + 1) % 3; }
            if enter_key { *show = false; }

            // Spaceキーで現在のフォーカス行を操作
            if space_key {
                match *focus_idx {
                    0 => { // 基準をサイクル切替
                        config.sort_mode = match config.sort_mode {
                            SortMode::Name => SortMode::Mtime,
                            SortMode::Mtime => SortMode::Size,
                            SortMode::Size => SortMode::Name,
                        };
                        changed = true;
                    }
                    1 => { // 順序を反転
                        config.sort_order = if config.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                        changed = true;
                    }
                    2 => { // 自然順をトグル
                        config.sort_natural = !config.sort_natural;
                        changed = true;
                    }
                    _ => {}
                }
            }

            ui.label("矢印キーで選択 / Enterで戻る");
            ui.add_space(8.0);
            
            let scroll_height = ui.available_height() - 60.0;
            egui::ScrollArea::vertical().max_height(scroll_height).show(ui, |ui| {
            
            ui.horizontal(|ui| {
                let active = *focus_idx == 0;
                let label = if active { egui::RichText::new("> 基準:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  基準:") };
                ui.label(label);
                changed |= ui.radio_value(&mut config.sort_mode, SortMode::Name, "ファイル名").changed();
                changed |= ui.radio_value(&mut config.sort_mode, SortMode::Mtime, "更新日時").changed();
                changed |= ui.radio_value(&mut config.sort_mode, SortMode::Size, "サイズ").changed();
                if active {
                    if arr_right {
                        config.sort_mode = match config.sort_mode {
                            SortMode::Name => SortMode::Mtime, SortMode::Mtime => SortMode::Size, SortMode::Size => SortMode::Name,
                        };
                        changed = true;
                    }
                    if arr_left {
                        config.sort_mode = match config.sort_mode {
                            SortMode::Name => SortMode::Size, SortMode::Mtime => SortMode::Name, SortMode::Size => SortMode::Mtime,
                        };
                        changed = true;
                    }
                }
            });

            ui.horizontal(|ui| {
                let active = *focus_idx == 1;
                let label = if active { egui::RichText::new("> 順序:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  順序:") };
                ui.label(label);
                changed |= ui.radio_value(&mut config.sort_order, SortOrder::Ascending, "昇順").changed();
                changed |= ui.radio_value(&mut config.sort_order, SortOrder::Descending, "降順").changed();
                if active && (arr_left || arr_right) {
                    config.sort_order = if config.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                    changed = true;
                }
            });

            ui.separator();
            let active = *focus_idx == 2;
            let check_text = if active { egui::RichText::new("> 自然順（数字の大きさを考慮）").color(egui::Color32::YELLOW) } else { egui::RichText::new("  自然順（数字の大きさを考慮）") };
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

/// デバッグ情報を表示するウィンドウ
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

/// キーコンフィグ画面で表示するアクション名の日本語翻訳マップ
fn get_action_label(id: &str) -> &str {
    match id {
        "PrevPage" => "前のページを表示",
        "NextPage" => "次のページを表示",
        "PrevPageSingle" => "前のページを表示 (1枚送り)",
        "NextPageSingle" => "次のページを表示 (1枚送り)",
        "Left" => "左 (移動/ツリー操作)",
        "Right" => "右 (移動/ツリー操作)",
        "Up" => "上 (移動/ツリー操作)",
        "Down" => "下 (移動/ツリー操作)",
        "Enter" => "決定 (ツリー選択/ダイアログ)",
        "OpenKeyConfig" => "キーコンフィグ画面を開く",
        "ToggleFullscreen" => "全画面表示の切替",
        "ToggleBorderless" => "ボーダレス全画面の切替",
        "Escape" => "閉じる/解除/終了",
        "ToggleTree" => "ディレクトリツリーの表示切替",
        "ToggleFit" => "画像フィットモードの切替",
        "ZoomIn" => "拡大",
        "ZoomOut" => "縮小",
        "ToggleManga" => "マンガモード(見開き)の切替",
        "RotateCW" => "画像を右に回転",
        "RotateCCW" => "画像を左に回転",
        "PrevDir" => "前のフォルダ/アーカイブへ",
        "NextDir" => "次のフォルダ/アーカイブへ",
        "SortSettings" => "ソート設定ウィンドウを開く",
        "FirstPage" => "最初のページへ移動",
        "LastPage" => "最後のページへ移動",
        "RevealExplorer" => "エクスプローラーで表示",
        "OpenExternal1" => "外部アプリ1で開く",
        "OpenExternal2" => "外部アプリ2で開く",
        "OpenExternal3" => "外部アプリ3で開く",
        "OpenExternal4" => "外部アプリ4で開く",
        "OpenExternal5" => "外部アプリ5で開く",
        "ToggleLinear" => "画像補正(スムージング)の切替",
        "ToggleMangaRtl" => "右開き/左開きの切替",
        "Quit" => "アプリを終了",
        "ToggleBg" => "背景色の切替",
        "ToggleDebug" => "デバッグ情報の表示切替",
        _ => id,
    }
}

pub fn key_config_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    capturing_for: &mut Option<String>,
) -> bool {
    // キー入力待ち状態（録画中）でない場合のみ、ESCで閉じる
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
                let conflicts = {
                    // ツリー操作用のモーダルなキー設定を分離して重複チェックを行う
                    let tree_modal_actions: HashSet<&str> = ["Up", "Down", "Left", "Right", "Enter"].into_iter().collect();
                    let mut key_to_actions_normal: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
                    let mut key_to_actions_tree: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

                    for (action_id, binding) in &config.keys {
                        let target_map = if tree_modal_actions.contains(action_id.as_str()) { &mut key_to_actions_tree } else { &mut key_to_actions_normal };
                        for k in binding.split(',') {
                            let k = k.trim();
                            if !k.is_empty() {
                                target_map.entry(k.to_string()).or_default().push(action_id.clone());
                            }
                        }
                    }
                    let mut set = HashSet::new();
                    // 通常操作グループとツリー操作グループ、それぞれの内部でのみ重複をチェックする
                    for map in [key_to_actions_normal, key_to_actions_tree] {
                        for (_key_name, actions) in map {
                            if actions.len() > 1 {
                                for action_id in actions {
                                    set.insert(action_id);
                                }
                            }
                        }
                    }
                    set
                };

                let mut shown = HashSet::new();
                let categories = [
                    ("移動・ページ送り", vec!["PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle", "FirstPage", "LastPage", "PrevDir", "NextDir"]),
                    ("画像操作", vec!["ToggleFit", "ZoomIn", "ZoomOut", "ToggleManga", "ToggleMangaRtl", "ToggleLinear", "ToggleBg", "RotateCW", "RotateCCW"]),
                    ("フォルダ・ツリー操作", vec!["Up", "Down", "Left", "Right", "Enter", "ToggleTree", "RevealExplorer"]),
                    ("システム・その他", vec!["ToggleFullscreen", "ToggleBorderless", "Escape", "SortSettings", "OpenKeyConfig", "OpenExternal1", "OpenExternal2", "OpenExternal3", "OpenExternal4", "OpenExternal5", "Quit"]),
                ];

                for (cat_name, key_ids) in categories {
                    ui.add_space(4.0);
                    ui.heading(RichText::new(cat_name).size(14.0).strong());
                    ui.separator();

                    // カテゴリ内を2列に分割して表示（縦の長さを節約）
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

                ui.add_space(10.0);
                ui.heading(RichText::new("マウスボタンの割り当て").size(14.0).strong());
                ui.separator();
                egui::Grid::new("mouse_buttons_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let mouse_actions = [
                        "PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle", "PrevDir", "NextDir"
                    ];

                    ui.label("戻るボタン (Mouse4):");
                    egui::ComboBox::from_id_source("mouse4_combo")
                        .selected_text(get_action_label(&config.mouse4_action))
                        .show_ui(ui, |ui| {
                            for act in mouse_actions {
                                if ui.selectable_label(config.mouse4_action == act, get_action_label(act)).clicked() {
                                    config.mouse4_action = act.to_string();
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("進むボタン (Mouse5):");
                    egui::ComboBox::from_id_source("mouse5_combo")
                        .selected_text(get_action_label(&config.mouse5_action))
                        .show_ui(ui, |ui| {
                            for act in mouse_actions {
                                if ui.selectable_label(config.mouse5_action == act, get_action_label(act)).clicked() {
                                    config.mouse5_action = act.to_string();
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });

                // 未知のキー（INI直接編集時など）
                let mut remaining_keys: Vec<_> = config.keys.keys()
                    .filter(|k| !shown.contains(*k))
                    .cloned().collect();
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

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
    changed
}

pub fn main_menu_bar(
    ctx: &egui::Context,
    config: &config::Config,
    manager: &manager::Manager,
    display_mode: DisplayMode,
    manga_mode: bool,
    show_tree: bool,
    show_debug: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    TopBottomPanel::top("top_panel").show(ctx, |ui| {
        menu::bar(ui, |ui| {
            ui.menu_button("ファイル", |ui| {
                if ui.button("フォルダを開く...").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenFolder); }
                ui.separator();
                if ui.add_enabled(manager.archive_path.is_some(), Button::new("エクスプローラーで表示 (BS)")).clicked() {
                    ui.close_menu(); action = Some(ViewerAction::RevealInExplorer);
                }
                ui.menu_button("最近開いたファイル", |ui| {
                    if config.recent_paths.is_empty() {
                        ui.label(RichText::new("（履歴なし）").weak());
                    } else {
                        for path in &config.recent_paths {
                            if ui.button(utils::get_display_name(std::path::Path::new(path))).clicked() {
                                ui.close_menu(); action = Some(ViewerAction::OpenRecent(path.clone()));
                            }
                        }
                    }
                });
                ui.menu_button("送る", |ui| {
                    for (i, app) in config.external_apps.iter().enumerate() {
                        if ui.add_enabled(manager.archive_path.is_some() && !app.exe.is_empty(), Button::new(&app.name)).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::OpenExternal(i));
                        }
                    }
                    ui.separator();
                    if ui.button("外部アプリ設定...").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenExternalSettings); }
                });
                ui.separator();
                if ui.button("終了").clicked() { action = Some(ViewerAction::Exit); }
            });
            ui.menu_button("表示", |ui| {
                if ui.selectable_label(display_mode == DisplayMode::Fit, "フィット表示 (F)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(DisplayMode::Fit));
                }
                if ui.selectable_label(display_mode == DisplayMode::WindowFit, "ウィンドウサイズに合わせる").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(DisplayMode::WindowFit));
                }
                if ui.selectable_label(display_mode == DisplayMode::Manual, "等倍表示").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(DisplayMode::Manual));
                }
                ui.separator();
                if ui.button("拡大 (+)").clicked() { action = Some(ViewerAction::ZoomIn); }
                if ui.button("縮小 (-)").clicked() { action = Some(ViewerAction::ZoomOut); }
                ui.separator();
                if ui.selectable_label(manga_mode, "マンガモード (M)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleManga); }
                if ui.selectable_label(config.manga_rtl, "右開き表示 (Y)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleMangaRtl); }
                if ui.selectable_label(show_tree, "ツリー表示 (T)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleTree); }
                if ui.button("ソートの設定 (S)...").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenSortSettings); }
                ui.separator();
                ui.menu_button("画像の補正 (I)", |ui| {
                    for (m, label) in [
                        (FilterMode::Nearest, "なし (Nearest)"),
                        (FilterMode::Bilinear, "バイリニア (線形)"),
                        (FilterMode::Bicubic, "バイキュービック (双三次)"),
                        (FilterMode::Lanczos, "ランチョス (高品質)"),
                    ] {
                        if ui.selectable_label(config.filter_mode == m, label).clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleLinear); }
                    }
                });
                ui.menu_button("ウィンドウサイズ", |ui| {
                    for (w, h, label) in [
                        (640, 480, "VGA (640x480)"),
                        (800, 600, "SVGA (800x600)"),
                        (1024, 768, "XGA (1024x768)"),
                        (1280, 960, "Quad-VGA (1280x960)"),
                        (1400, 1050, "SXGA+ (1400x1050)"),
                    ] {
                        if ui.button(label).clicked() { ui.close_menu(); action = Some(ViewerAction::ResizeWindow(w, h)); }
                    }
                    ui.separator();
                    if ui.selectable_label(!config.window_resizable, "ウィンドロック").clicked() {
                        ui.close_menu(); action = Some(ViewerAction::ToggleWindowResizable);
                    }
                });
                if ui.selectable_label(config.always_on_top, "常に手前に表示").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleAlwaysOnTop);
                }
                ui.separator();
                if ui.button("右回転 (R)").clicked() { ui.close_menu(); action = Some(ViewerAction::Rotate(true)); }
                if ui.button("左回転 (Ctrl+R)").clicked() { ui.close_menu(); action = Some(ViewerAction::Rotate(false)); }
                ui.separator();
                ui.menu_button("背景色", |ui| {
                    let bg_options = [
                        (BackgroundMode::Theme, "既定"),
                        (BackgroundMode::Checkerboard, "市松模様"),
                        (BackgroundMode::Black, "黒"),
                        (BackgroundMode::Gray, "グレー"),
                        (BackgroundMode::White, "白"),
                        (BackgroundMode::Green, "緑"),
                    ];
                    for (m, label) in bg_options {
                        if ui.button(label).clicked() { ui.close_menu(); action = Some(ViewerAction::SetBgMode(m)); }
                    }
                });
            });
            ui.menu_button("フォルダ", |ui| {
                if ui.button("前のフォルダ (PgUp)").clicked() { ui.close_menu(); action = Some(ViewerAction::GoPrevDir); }
                if ui.button("次のフォルダ (PgDn)").clicked() { ui.close_menu(); action = Some(ViewerAction::GoNextDir); }
                ui.separator();
                ui.label("フォルダ移動時の設定:");
                if ui.radio(!config.open_from_end, "先頭から開く").clicked() { action = Some(ViewerAction::SetOpenFromEnd(false)); }
                if ui.radio(config.open_from_end, "末尾から開く").clicked() { action = Some(ViewerAction::SetOpenFromEnd(true)); }
            });
            ui.menu_button("オプション", |ui| {
                if ui.selectable_label(config.allow_multiple_instances, "複数起動を許可").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleMultipleInstances);
                }
                if ui.button("キーコンフィグ...").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenKeyConfig); }
                if ui.selectable_label(show_debug, "デバッグ情報...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleDebug);
                }
                ui.menu_button("マウスボタン割り当て", |ui| {
                    ui.menu_button("戻るボタン (Mouse4)", |ui| {
                        if ui.selectable_label(config.mouse4_action == "PrevPage", "前のページ").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(4, "PrevPage".into())); }
                        if ui.selectable_label(config.mouse4_action == "PrevPageSingle", "前のページ (1枚送り)").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(4, "PrevPageSingle".into())); }
                        if ui.selectable_label(config.mouse4_action == "PrevDir", "前のフォルダ").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(4, "PrevDir".into())); }
                    });
                    ui.menu_button("進むボタン (Mouse5)", |ui| {
                        if ui.selectable_label(config.mouse5_action == "NextPage", "次のページ").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(5, "NextPage".into())); }
                        if ui.selectable_label(config.mouse5_action == "NextPageSingle", "次のページ (1枚送り)").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(5, "NextPageSingle".into())); }
                        if ui.selectable_label(config.mouse5_action == "NextDir", "次のフォルダ").clicked() { ui.close_menu(); action = Some(ViewerAction::SetMouseAction(5, "NextDir".into())); }
                    });
                });
                ui.separator();
                ui.label("レンダラー (再起動後に反映):");
                if ui.selectable_label(config.renderer == config::RendererMode::Glow, "OpenGL (軽量)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetRenderer(config::RendererMode::Glow));
                }
                if ui.selectable_label(config.renderer == config::RendererMode::Wgpu, "WGPU (互換性)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetRenderer(config::RendererMode::Wgpu));
                }
                ui.separator();
                if ui.button("このソフトについて...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::About);
                }
            });
        });
    });
    action
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
                    ("image", "MIT", "https://github.com/image-rs/image"),
                    ("zip-rs", "MIT", "https://github.com/zip-rs/zip"),
                    ("sevenz-rust", "Apache-2.0", "https://github.com/mcmunder/sevenz-rust"),
                    ("rust-ini", "MIT", "https://github.com/amrayn/rust-ini"),
                    ("windows-sys", "MIT / Apache-2.0", "https://github.com/microsoft/windows-rs"),
                    ("rfd", "MIT", "https://github.com/PolyMeilex/rfd"),
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

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
}

pub fn bottom_toolbar(
    ctx: &egui::Context,
    manager: &manager::Manager,
    config: &config::Config,
    display_mode: DisplayMode,
    zoom: f32,
    manga_mode: bool,
    is_nav_locked: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    TopBottomPanel::bottom("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let has = !manager.entries.is_empty();
            
            let accent = ui.visuals().selection.bg_fill;

            let mut left_btn = Button::new("<");
            if !config.manga_rtl {
                left_btn = left_btn.fill(accent);
            }
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
            if config.manga_rtl {
                right_btn = right_btn.fill(accent);
            }
            if ui.add_enabled(has, right_btn).clicked() { action = Some(ViewerAction::NextPage); }

            ui.separator();
            let ml = if manga_mode { "2P" } else { "1P" };
            if ui.button(ml).clicked() { action = Some(ViewerAction::ToggleManga); }
            ui.separator();
            
            if has {
                let meta = &manager.entries_meta[manager.target_index];
                let name = utils::get_display_name(std::path::Path::new(&meta.name));
                let size = utils::format_size(meta.size);
                let date = utils::format_timestamp(meta.mtime);
                
                let res = if let Some(tex) = manager.get_first_tex(manager.target_index) {
                    let s = tex.size();
                    format!("{}x{}", s[0], s[1])
                } else { "---x---".to_string() };

                let sort = format!("{} {}", 
                    match config.sort_mode { SortMode::Name => "Name", SortMode::Mtime => "Day", SortMode::Size => "Size" },
                    if config.sort_order == SortOrder::Ascending { "Asc" } else { "Desc" }
                );

                let filter = match config.filter_mode {
                    FilterMode::Nearest => "Nearest",
                    FilterMode::Bilinear => "Bilinear",
                    FilterMode::Bicubic => "Bicubic",
                    FilterMode::Lanczos => "Lanczos",
                };

                ui.label(format!("{} | {} | {} | {} | [{}] | {}", name, size, res, date, sort, filter));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if display_mode == DisplayMode::Manual { ui.label(format!("{:.0}%", zoom * 100.0)); }
                    ui.label(match display_mode {
                        DisplayMode::Fit => "Fit", DisplayMode::WindowFit => "WinFit", DisplayMode::Manual => "Manual",
                    });
                });
            } else {
                ui.label("待機中...");
            }
        });
    });
    action
}

pub fn sidebar_ui(
    ui: &mut egui::Ui,
    nav_tree: &mut manager::NavTree,
    archive_path: &Option<PathBuf>,
    ctx: &egui::Context,
    open_req: &mut Option<PathBuf>,
) {
    ui.vertical(|ui| {
        ui.set_min_width(ui.available_width());
        ui.heading("ディレクトリツリー");
        ui.separator();

        let scroll_height = ui.available_height() - 40.0;
        ScrollArea::vertical().max_height(scroll_height).auto_shrink([false; 2]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let roots = nav_tree.get_roots();
            for root in roots {
                ui_dir_tree(nav_tree, archive_path, ui, root, ctx, open_req);
            }
        });

        ui.separator();
        if let Some(sel) = nav_tree.selected.clone() {
            let count = nav_tree.get_image_count(&sel);
            ui.label(format!("選択中: {} ({}枚)", utils::get_display_name(&sel), count));
        }
    });
}

pub fn ui_dir_tree(nav_tree: &mut manager::NavTree, current_path: &Option<PathBuf>, ui: &mut egui::Ui, path: PathBuf, ctx: &egui::Context, open_req: &mut Option<PathBuf>) {
    // システム属性または隠し属性を持つパスは一切表示しない（リセット対応）
    if utils::is_system(&path) || utils::is_hidden(&path) {
        return;
    }

    let filename = if path.parent().is_none() { path.to_string_lossy().to_string() } else { utils::get_display_name(&path) };

    let kind = utils::detect_kind(&path);
    let is_archive = matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ);
    let is_current = current_path.as_ref() == Some(&path);
    let is_selected = nav_tree.selected.as_ref() == Some(&path);
    let text = RichText::new(filename);
    let text = if is_current { text.color(Color32::YELLOW) } else { text };
    let text = if is_selected { text.background_color(ui.visuals().selection.bg_fill.linear_multiply(0.3)) } else { text };

    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
        if is_archive {
            let resp = ui.selectable_label(is_selected, text);
            if is_selected && nav_tree.scroll_to_selected { resp.scroll_to_me(Some(egui::Align::Center)); }
            if resp.clicked() { *open_req = Some(path); }
        } else {
            let is_expanded = nav_tree.expanded.contains(&path);
            let response = egui::CollapsingHeader::new(text).id_source(&path).open(Some(is_expanded)).show(ui, |ui| {
                let children = nav_tree.get_children(&path);
                for p in children { ui_dir_tree(nav_tree, current_path, ui, p, ctx, open_req); }
            });
            if is_selected && nav_tree.scroll_to_selected { response.header_response.scroll_to_me(Some(egui::Align::Center)); }
            if response.header_response.clicked() {
                nav_tree.selected = Some(path.clone());
                if is_expanded { nav_tree.expanded.remove(&path); } else { nav_tree.expanded.insert(path); }
            }
        }
    });
}