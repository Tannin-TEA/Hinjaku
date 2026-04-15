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
    ToggleExif,
    OpenSortSettings,
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
}

/// ツリーのノードキャッシュをクリアするしきい値
pub const TREE_NODES_CACHE_LIMIT: usize = 1000;

pub fn settings_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    settings_args_tmp: &mut [String],
) -> bool {
    let mut saved = false;
    egui::Window::new("外部アプリ連携の設定 (送る)")
        .fixed_size([500.0, 550.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("ショートカットキーやメニューから起動するソフトを5つまで設定できます。");
            ui.add_space(8.0);

            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
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
                                if ui.button("参照…").clicked() {
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
            ui.small("※ %P はフルパス、%A はフォルダ/アーカイブのパスに置換されます。必要に応じて \"%P\" のように引用符で囲んでください。");

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0); // ウィンドウ最下部の余白
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space((w - 120.0) / 2.0); // ボタン2つ分のセンタリング調整
                    if ui.button("適用").clicked() {
                        for i in 0..5 {
                            config.external_apps[i].args = settings_args_tmp[i].split_whitespace().map(|s| s.to_string()).collect();
                        }
                        saved = true;
                    }
                    if ui.button("閉じる").clicked() { *show = false; }
                });
                ui.add_space(12.0); // ボタン上の余白
                ui.separator();    // コンテンツとの区切り線
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
) -> bool {
    let mut changed = false;
    egui::Window::new("並べ替えの設定 (S)")
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

            ui.label("矢印キーで選択 / Enterで戻る");
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                let active = *focus_idx == 0;
                let label = if active { egui::RichText::new("▶ 基準:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  基準:") };
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
                let label = if active { egui::RichText::new("▶ 順序:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  順序:") };
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
            let check_text = if active { egui::RichText::new("自然順（数字の大きさを考慮）").color(egui::Color32::YELLOW) } else { egui::RichText::new("自然順（数字の大きさを考慮）") };
            if ui.checkbox(&mut config.sort_natural, check_text).changed() { changed = true; }
            if active && (arr_left || arr_right) { config.sort_natural = !config.sort_natural; changed = true; }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.button("閉じる").clicked() { *show = false; }
                ui.add_space(12.0);
                ui.separator();
            });
        });
    changed
}

/// キーコンフィグ画面で表示するアクション名の日本語翻訳マップ
fn get_action_label(id: &str) -> &str {
    match id {
        "PrevPage" => "前のページを表示",
        "NextPage" => "次のページを表示",
        "PrevPageSingle" => "前のページを表示 (1枚送り)",
        "NextPageSingle" => "次のページを表示 (1枚送り)",
        "Left" => "※左 (移動/ツリー操作)",
        "Right" => "※右 (移動/ツリー操作)",
        "Up" => "※上 (移動/ツリー操作)",
        "Down" => "※下 (移動/ツリー操作)",
        "Enter" => "※決定 (ツリー選択/ダイアログ)",
        "OpenKeyConfig" => "キーコンフィグ画面を開く",
        "ToggleFullscreen" => "全画面表示の切替",
        "ToggleBorderless" => "ボーダレス全画面の切替",
        "Escape" => "閉じる/解除/終了",
        "ToggleTree" => "※ディレクトリツリーの表示切替",
        "ToggleFit" => "画像フィットモードの切替",
        "ToggleExif" => "Exifパネルの表示切替",
        "ZoomIn" => "拡大",
        "ZoomOut" => "縮小",
        "ToggleManga" => "マンガモード(見開き)の切替",
        "RotateCW" => "画像を右に回転",
        "RotateCCW" => "画像を左に回転",
        "PrevDir" => "前のフォルダ/アーカイブへ",
        "NextDir" => "次のフォルダ/アーカイブへ",
        "SortSettings" => "並べ替え設定ウィンドウを開く",
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
        _ => id,
    }
}

pub fn key_config_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    capturing_for: &mut Option<String>,
) -> bool {
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

            egui::ScrollArea::vertical().show(ui, |ui| {
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
                    ("📖 移動・ページ送り", vec!["PrevPage", "NextPage", "PrevPageSingle", "NextPageSingle", "FirstPage", "LastPage", "PrevDir", "NextDir"]),
                    ("🎨 画像操作", vec!["ToggleFit", "ZoomIn", "ZoomOut", "ToggleManga", "ToggleMangaRtl", "ToggleLinear", "ToggleBg", "RotateCW", "RotateCCW"]),
                    ("📁 フォルダ・ツリー操作 (※)", vec!["Up", "Down", "Left", "Right", "Enter", "ToggleTree", "RevealExplorer"]),
                    ("⚙ システム・その他", vec!["ToggleFullscreen", "ToggleBorderless", "Escape", "SortSettings", "OpenKeyConfig", "OpenExternal1", "OpenExternal2", "OpenExternal3", "OpenExternal4", "OpenExternal5", "Quit"]),
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
                                                    ui.label(RichText::new("⚠").color(Color32::RED))
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
                                                let btn_text = if is_capturing { "⏺…" } else { "⏺" };
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
                                    ui.label(RichText::new("⚠").color(Color32::RED))
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
                                if ui.selectable_label(is_capturing, "⏺").clicked() {
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
    show_tree: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    TopBottomPanel::top("top_panel").show(ctx, |ui| {
        menu::bar(ui, |ui| {
            ui.menu_button("ファイル", |ui| {
                if ui.button("フォルダを開く…").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenFolder); }
                ui.separator();
                if ui.add_enabled(manager.archive_path.is_some(), Button::new("エクスプローラーで表示 (BS)")).clicked() {
                    ui.close_menu(); action = Some(ViewerAction::RevealInExplorer);
                }
                ui.menu_button("最近開いたファイル", |ui| {
                    // ここに将来的に履歴を表示する
                    ui.label(RichText::new("（履歴なし）").weak());
                });
                ui.menu_button("送る...", |ui| {
                    for (i, app) in config.external_apps.iter().enumerate() {
                        if ui.add_enabled(manager.archive_path.is_some() && !app.exe.is_empty(), Button::new(&app.name)).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::OpenExternal(i));
                        }
                    }
                });
                if ui.button("外部アプリ設定…").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenExternalSettings); }
                if ui.button("キーコンフィグ…").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenKeyConfig); }
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
                if ui.selectable_label(false, "マンガモード (M)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleManga); }
                if ui.selectable_label(config.manga_rtl, "右開き表示 (Y)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleMangaRtl); }
                if ui.selectable_label(show_tree, "ツリー表示 (T)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleTree); }
                if ui.button("並べ替えの設定 (S)").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenSortSettings); }
                ui.separator();
                ui.menu_button("画像調整 (試験的)", |ui| {
                    ui.label("明るさ");
                    ui.add(Slider::new(&mut 1.0, 0.5..=1.5));
                    ui.label(RichText::new("※現バージョンでは表示のみ").small().weak());
                });
                ui.menu_button("画像の補正 (I)", |ui| {
                    for (m, label) in [
                        (FilterMode::Nearest, "なし (Nearest)"),
                        (FilterMode::Bilinear, "バイリニア (線形)"),
                        (FilterMode::Bicubic, "※バイキュービック (双三次)"),
                        (FilterMode::Lanczos, "※ランチョス (高品質)"),
                    ] {
                        if ui.selectable_label(config.filter_mode == m, label).clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleLinear); }
                    }
                });
                if ui.checkbox(&mut false, "複数起動を許可").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleMultipleInstances); }
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
        });
    });
    action
}

pub fn exif_panel_ui(ui: &mut egui::Ui, exif_str: Option<&str>) {
    ScrollArea::vertical().show(ui, |ui| {
        if let Some(info) = exif_str {
            ui.add_space(4.0);
            ui.label(RichText::new(info).line_height(Some(20.0)));
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Exifデータなし").weak());
            });
        }
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
            if ui.add_enabled(has, Button::new("◀")).clicked() { action = Some(ViewerAction::PrevPage); }
            
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

            if ui.add_enabled(has, Button::new("▶")).clicked() { action = Some(ViewerAction::NextPage); }
            ui.separator();
            if ui.add_enabled(has, Button::new("📷")).on_hover_text("Exifパネル表示").clicked() {
                action = Some(ViewerAction::ToggleExif);
            }
            ui.separator();
            let ml = if manga_mode { "📖 2P" } else { "📄 1P" };
            if ui.button(ml).clicked() { action = Some(ViewerAction::ToggleManga); }
            ui.separator();
            
            if has {
                let meta = &manager.entries_meta[manager.target_index];
                let short = utils::get_display_name(std::path::Path::new(&meta.name));
                
                let file_size = if meta.size >= 1024 * 1024 {
                    format!("{:.1} MB", meta.size as f64 / (1024.0 * 1024.0))
                } else if meta.size >= 1024 {
                    format!("{:.0} KB", meta.size as f64 / 1024.0)
                } else {
                    format!("{} B", meta.size)
                };

                let res_str = if let Some(tex) = manager.get_first_tex(manager.target_index) {
                    let s = tex.size();
                    format!("{}x{}", s[0], s[1])
                } else {
                    "---x---".to_string()
                };

                let day_str = integrator::format_timestamp(meta.mtime);
                let sort_label = match config.sort_mode {
                    SortMode::Name => "Name", SortMode::Mtime => "Day", SortMode::Size => "Size",
                };
                let sort_icon = if config.sort_order == SortOrder::Ascending { "▲" } else { "▼" };
            let filter_label = match config.filter_mode {
                FilterMode::Nearest => "Nearest",
                FilterMode::Bilinear => "Bilinear",
                FilterMode::Bicubic => "※Bicubic",
                FilterMode::Lanczos => "※Lanczos",
            };
            ui.label(format!("{} | {} | {} | {} | [{} {}] | {}", short, file_size, res_str, day_str, sort_label, sort_icon, filter_label));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if display_mode == DisplayMode::Manual { ui.label(format!("{:.0}%", zoom * 100.0)); }
                    let mode_str = match display_mode {
                        DisplayMode::Fit => "Fit", DisplayMode::WindowFit => "WinFit", DisplayMode::Manual => "Manual",
                    };
                    ui.label(mode_str);
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
            ui.label(RichText::new(format!("選択: {} ({} 枚)", utils::get_display_name(&sel), count)).small());
        }
    });
}

pub fn ui_dir_tree(nav_tree: &mut manager::NavTree, current_path: &Option<PathBuf>, ui: &mut egui::Ui, path: PathBuf, ctx: &egui::Context, open_req: &mut Option<PathBuf>) {
    let filename = if path.parent().is_none() { path.to_string_lossy().to_string() } else { utils::get_display_name(&path) };
    let kind = utils::detect_kind(&path);
    let is_archive = matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ);
    let icon = if is_archive { "📦 " } else { "📁 " };
    let is_current = current_path.as_ref() == Some(&path);
    let is_selected = nav_tree.selected.as_ref() == Some(&path);
    let text = RichText::new(format!("{}{}", icon, filename));
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