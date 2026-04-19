use eframe::egui::{self, RichText, Button, TopBottomPanel, menu};
use crate::config::{self, FilterMode, BackgroundMode};
use crate::manager::Manager;
use crate::types::ViewState;
use crate::utils;
use super::ViewerAction;

pub fn main_menu_bar(
    ctx: &egui::Context,
    config: &config::Config,
    manager: &Manager,
    view: &ViewState,
    show_tree: bool,
    show_debug: bool,
) -> Option<ViewerAction> {
    let mut action = None;
    TopBottomPanel::top("top_panel").show(ctx, |ui| {
        menu::bar(ui, |ui| {
            ui.menu_button("ファイル", |ui| {
                ui.set_min_width(220.0);
                if ui.button("フォルダを開く").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::OpenFolder);
                }
                ui.menu_button("最近開いたファイル", |ui| {
                    ui.set_min_width(300.0);
                    if config.recent_paths.is_empty() {
                        ui.label(RichText::new("（履歴なし）").weak());
                    } else {
                        for path_str in &config.recent_paths {
                            let path = std::path::Path::new(path_str);
                            let name = utils::get_display_name(path);
                            let max_len = 40;

                            let display_text = if name.chars().count() <= max_len {
                                name
                            } else {
                                let kind = utils::detect_kind(path);
                                if matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ | utils::ArchiveKind::Pdf) {
                                    let chars: Vec<char> = name.chars().collect();
                                    format!("...{}", chars[chars.len() - (max_len - 3)..].iter().collect::<String>())
                                } else {
                                    format!("{}...", name.chars().take(max_len - 3).collect::<String>())
                                }
                            };

                            if ui.button(display_text).clicked() {
                                ui.close_menu(); action = Some(ViewerAction::OpenRecent(path_str.clone()));
                            }
                        }
                    }
                });
                ui.separator();
                if ui.add_enabled(manager.archive_path.is_some(), Button::new("エクスプローラーで表示")).clicked() {
                    ui.close_menu(); action = Some(ViewerAction::RevealInExplorer);
                }
                ui.menu_button("送る", |ui| {
                    ui.set_min_width(200.0);
                    for (i, app) in config.external_apps.iter().enumerate() {
                        if ui.add_enabled(manager.archive_path.is_some() && !app.exe.is_empty(), Button::new(&app.name)).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::OpenExternal(i));
                        }
                    }
                    ui.separator();
                    if ui.button("外部アプリ設定...").clicked() {
                        ui.close_menu(); action = Some(ViewerAction::OpenExternalSettings);
                    }
                });
                ui.separator();
                if ui.button("終了").clicked() { action = Some(ViewerAction::Exit); }
            });

            ui.menu_button("表示", |ui| {
                ui.set_min_width(220.0);
                if ui.selectable_label(view.display_mode == crate::types::DisplayMode::Fit, "フィット表示 (F)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(crate::types::DisplayMode::Fit));
                }
                if ui.selectable_label(view.display_mode == crate::types::DisplayMode::WindowFit, "ウィンドウサイズに合わせる").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(crate::types::DisplayMode::WindowFit));
                }
                if ui.selectable_label(view.display_mode == crate::types::DisplayMode::Manual, "等倍表示").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::SetDisplayMode(crate::types::DisplayMode::Manual));
                }
                ui.separator();
                if ui.button("拡大 (+)").clicked() { action = Some(ViewerAction::ZoomIn); }
                if ui.button("縮小 (-)").clicked() { action = Some(ViewerAction::ZoomOut); }
                ui.separator();
                if ui.selectable_label(view.manga_mode, "マンガモード (M)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleManga);
                }
                if ui.selectable_label(config.manga_rtl, "右開き表示 (Y)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleMangaRtl);
                }
                if ui.selectable_label(show_tree, "ツリー表示 (T)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleTree);
                }
                if ui.button("ソートの設定 (S)...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::OpenSortSettings);
                }
                ui.separator();
                ui.menu_button("画像の補正 (I)", |ui| {
                    for (m, label) in [
                        (FilterMode::Nearest,  "なし (Nearest)"),
                        (FilterMode::Bilinear, "バイリニア (線形)"),
                        (FilterMode::Bicubic,  "バイキュービック (双三次)"),
                        (FilterMode::Lanczos,  "ランチョス (高品質)"),
                    ] {
                        if ui.selectable_label(config.filter_mode == m, label).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::ToggleLinear);
                        }
                    }
                });
                ui.menu_button("ウィンドウサイズ", |ui| {
                    for (w, h, label) in [
                        (640,  480,  "VGA (640x480)"),
                        (800,  600,  "SVGA (800x600)"),
                        (1024, 768,  "XGA (1024x768)"),
                        (1280, 960,  "Quad-VGA (1280x960)"),
                        (1400, 1050, "SXGA+ (1400x1050)"),
                        (1600, 1200, "UXGA (1600x1200)"),
                    ] {
                        if ui.button(label).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::ResizeWindow(w, h));
                        }
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
                if ui.button("右回転 (R)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::Rotate(true));
                }
                if ui.button("左回転 (Ctrl+R)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::Rotate(false));
                }
                ui.separator();
                ui.menu_button("背景色", |ui| {
                    for (m, label) in [
                        (BackgroundMode::Theme,        "既定"),
                        (BackgroundMode::Checkerboard, "市松模様"),
                        (BackgroundMode::Black,        "黒"),
                        (BackgroundMode::Gray,         "グレー"),
                        (BackgroundMode::White,        "白"),
                        (BackgroundMode::Green,        "緑"),
                    ] {
                        if ui.button(label).clicked() {
                            ui.close_menu(); action = Some(ViewerAction::SetBgMode(m));
                        }
                    }
                });
            });

            ui.menu_button("フォルダ", |ui| {
                ui.set_min_width(200.0);
                if ui.button("前のフォルダ (PgUp)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::GoPrevDir);
                }
                if ui.button("次のフォルダ (PgDn)").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::GoNextDir);
                }
                ui.separator();
                ui.label("フォルダ移動時の設定:");
                if ui.radio(!config.open_from_end, "先頭から開く").clicked() {
                    action = Some(ViewerAction::SetOpenFromEnd(false));
                }
                if ui.radio(config.open_from_end, "末尾から開く").clicked() {
                    action = Some(ViewerAction::SetOpenFromEnd(true));
                }
            });

            ui.menu_button("オプション", |ui| {
                ui.set_min_width(220.0);
                if ui.selectable_label(config.allow_multiple_instances, "複数起動を許可").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleMultipleInstances);
                }
                if ui.button("ウィンドウを中央に移動").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::MoveToCenter);
                }
                if ui.selectable_label(config.window_centered, "起動時に画面中央に配置").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleWindowCentered);
                }
                if ui.button("リミッター設定...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::OpenLimiterSettings);
                }
                if ui.button("キーコンフィグ...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::OpenKeyConfig);
                }
                if ui.selectable_label(show_debug, "デバッグ情報...").clicked() {
                    ui.close_menu(); action = Some(ViewerAction::ToggleDebug);
                }
                ui.separator();
                ui.label("レンダラー (再起動要):");
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
