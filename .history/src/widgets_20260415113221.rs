use eframe::egui::{self, RichText, Color32, Button, TopBottomPanel, Layout, Align, menu, Slider};
use std::path::PathBuf;
use crate::config::{self, SortMode, SortOrder, BackgroundMode};
use crate::manager;
use crate::utils;
use crate::viewer::DisplayMode;
use crate::integrator;

/// ユーザーがUI操作を通じて要求したアクション
pub enum ViewerAction {
    OpenFolder,
    RevealInExplorer,
    OpenExternal,
    OpenExternalSettings,
    Exit,
    SetDisplayMode(DisplayMode),
    ZoomIn,
    ZoomOut,
    ToggleManga,
    ToggleMangaRtl,
    ToggleTree,
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
    Seek(usize),
}

/// ツリーのノードキャッシュをクリアするしきい値
pub const TREE_NODES_CACHE_LIMIT: usize = 1000;

pub fn settings_window(
    ctx: &egui::Context,
    show: &mut bool,
    config: &mut config::Config,
    settings_args_tmp: &mut String,
) -> bool {
    let mut saved = false;
    egui::Window::new("外部アプリ連携の設定")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Eキーを押した時に起動するソフトを設定します。");
            ui.add_space(8.0);

            egui::Grid::new("config_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                ui.label("アプリのパス:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut config.external_app);
                    if ui.button("参照…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            config.external_app = p.to_string_lossy().to_string();
                        }
                    }
                });
                ui.end_row();

                ui.label("コマンド引数:");
                ui.text_edit_singleline(settings_args_tmp);
                ui.end_row();
            });
            ui.small("※ %P は表示中のファイルパスに置き換わります");
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("設定を保存して閉じる").clicked() {
                    config.external_args = settings_args_tmp.split_whitespace().map(|s| s.to_string()).collect();
                    saved = true;
                    *show = false;
                }
                if ui.button("キャンセル").clicked() { *show = false; }
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
        .collapsible(false).resizable(false)
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

            ui.add_space(8.0);
            ui.vertical_centered(|ui| { if ui.button("閉じる").clicked() { *show = false; } });
        });
    changed
}

pub fn main_menu_bar(
    ctx: &egui::Context,
    config: &config::Config,
    manager: &manager::Manager,
    display_mode: DisplayMode,
    show_tree: bool,
    fit: bool, // 互換性のための残存フラグ（DisplayModeを使用する場合は不要）
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
                if ui.add_enabled(manager.archive_path.is_some(), Button::new("外部アプリで開く (E)")).clicked() {
                    ui.close_menu(); action = Some(ViewerAction::OpenExternal);
                }
                if ui.button("外部アプリ設定…").clicked() { ui.close_menu(); action = Some(ViewerAction::OpenExternalSettings); }
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
                if ui.selectable_label(config.linear_filter, "画像の補正 (I)").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleLinear); }
                if ui.checkbox(&mut false, "複数起動を許可").clicked() { ui.close_menu(); action = Some(ViewerAction::ToggleMultipleInstances); }
                ui.separator();
                if ui.button("右回転 (R)").clicked() { ui.close_menu(); action = Some(ViewerAction::Rotate(true)); }
                if ui.button("左回転 (Ctrl+R)").clicked() { ui.close_menu(); action = Some(ViewerAction::Rotate(false)); }
                ui.separator();
                ui.menu_button("背景色", |ui| {
                    for (m, label) in [(BackgroundMode::Theme, "アプリ既定"), (BackgroundMode::Checkerboard, "市松模様"), (BackgroundMode::Black, "黒")] {
                        if ui.button(label).clicked() { ui.close_menu(); action = Some(ViewerAction::SetBgMode(m)); }
                    }
                });
            });
            ui.menu_button("フォルダ", |ui| {
                if ui.button("前のフォルダ (PgUp)").clicked() { ui.close_menu(); action = Some(ViewerAction::GoPrevDir); }
                if ui.button("次のフォルダ (PgDn)").clicked() { ui.close_menu(); action = Some(ViewerAction::GoNextDir); }
                ui.separator();
                ui.label("フォルダ移動時の設定:");
                if ui.radio(false, "先頭から開く").clicked() { action = Some(ViewerAction::SetOpenFromEnd(false)); }
                if ui.radio(false, "末尾から開く").clicked() { action = Some(ViewerAction::SetOpenFromEnd(true)); }
            });
        });
    });
    action
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
            let ml = if manga_mode { "📖 2P" } else { "📄 1P" };
            if ui.button(ml).clicked() { action = Some(ViewerAction::ToggleManga); }
            ui.separator();
            
            if has {
                let meta = &manager.entries_meta[manager.target_index];
                let short = utils::get_display_name(std::path::Path::new(&meta.name));
                let day_str = integrator::format_timestamp(meta.mtime);
                let sort_label = match config.sort_mode {
                    SortMode::Name => "Name", SortMode::Mtime => "Day", SortMode::Size => "Size",
                };
                let sort_icon = if config.sort_order == SortOrder::Ascending { "▲" } else { "▼" };
                ui.label(format!("{} | {} | [{} {}]", short, day_str, sort_label, sort_icon));

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

pub fn ui_dir_tree(nav_tree: &mut manager::NavTree, current_path: &Option<PathBuf>, ui: &mut egui::Ui, path: PathBuf, ctx: &egui::Context, open_req: &mut Option<PathBuf>) {
    let filename = utils::get_display_name(&path);
    let kind = utils::detect_kind(&path);
    let is_archive = matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ);
    let icon = if is_archive { "📦 " } else { "📁 " };
    let is_current = current_path.as_ref() == Some(&path);
    let is_selected = nav_tree.selected.as_ref() == Some(&path);
    let text = RichText::new(format!("{}{}", icon, filename));
    let text = if is_current { text.color(Color32::YELLOW) } else { text };
    let text = if is_selected { text.background_color(ui.visuals().selection.bg_fill.linear_multiply(0.3)) } else { text };

    if is_archive {
        let resp = ui.selectable_label(is_selected, text);
        if is_selected { resp.scroll_to_me(Some(egui::Align::Center)); }
        if resp.clicked() { *open_req = Some(path); }
    } else {
        let is_expanded = nav_tree.expanded.contains(&path);
        let response = egui::CollapsingHeader::new(text).id_source(&path).open(Some(is_expanded)).show(ui, |ui| {
            let children = nav_tree.get_children(&path);
            for p in children { ui_dir_tree(nav_tree, current_path, ui, p, ctx, open_req); }
        });
        if is_selected { response.header_response.scroll_to_me(Some(egui::Align::Center)); }
        if response.header_response.clicked() {
            nav_tree.selected = Some(path.clone());
            if is_expanded { nav_tree.expanded.remove(&path); } else { nav_tree.expanded.insert(path); }
        }
    }
}