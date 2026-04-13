use super::render::draw_centered;
use super::{App, SortMode, SortOrder};
use chrono::{Local, TimeZone};
use eframe::egui;

/// `eframe::App::update` の実装。
/// App の状態変更ロジックは `mod.rs` に置き、ここは描画・入力処理のみとする。
pub fn update(app: &mut App, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let is_focused = ctx.input(|i| i.focused);
    // ウィンドウがフォーカスを得た瞬間のクリックは無視する
    let click_allowed = is_focused && app.was_focused;

    // ── 外部プロセスからのパス転送 ───────────────────────────────────────
    while let Ok(path) = app.path_rx.try_recv() {
        app.open_path(path, ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    // ── バックグラウンド結果を回収 ────────────────────────────────────────
    app.collect_results(ctx);

    // ── ページ同期（テクスチャ準備が整ったら current を更新） ────────────
    if app.is_loading_archive || app.current != app.target_index {
        if app.get_texture(app.target_index).is_some() {
            app.current = app.target_index;
            app.is_loading_archive = false;
            app.last_display_change_time = ctx.input(|i| i.time);
        }
    }

    // ── ドラッグ＆ドロップ ────────────────────────────────────────────────
    let dropped: Option<std::path::PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .first()
            .and_then(|f| f.path.as_ref().cloned())
    });
    if let Some(path) = dropped {
        app.open_path(path, ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    // ── キーボード入力 ────────────────────────────────────────────────────
    let keys = read_keys(ctx);
    let modal_open = app.show_sort_settings || app.show_settings;
    handle_keys(app, ctx, &keys, modal_open);

    // ── 設定ウィンドウ ────────────────────────────────────────────────────
    show_settings_window(app, ctx);
    show_sort_settings_window(app, ctx, keys.enter);

    // ── メニューバー ──────────────────────────────────────────────────────
    egui::TopBottomPanel::top("menu").show(ctx, |ui| {
        show_menu_bar(app, ctx, ui);
    });

    // ── ステータスバー（下部ツールバー） ──────────────────────────────────
    egui::TopBottomPanel::bottom("toolbar").show(ctx, |ui| {
        show_toolbar(app, ctx, ui);
    });

    // ── メイン表示エリア ──────────────────────────────────────────────────
    egui::CentralPanel::default().show(ctx, |ui| {
        show_main_area(app, ctx, ui, click_allowed);
    });

    app.was_focused = is_focused;
}

// ── キー入力 ─────────────────────────────────────────────────────────────────

struct Keys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    fit: bool,
    zoom_in: bool,
    zoom_out: bool,
    manga: bool,
    rot_cw: bool,
    rot_ccw: bool,
    pg_up: bool,
    pg_dn: bool,
    p_key: bool,
    n_key: bool,
    s_key: bool,
    home: bool,
    end: bool,
    bs: bool,
    e_key: bool,
    i_key: bool,
    enter: bool,
    alt: bool,
    esc: bool,
    y_key: bool,
}

fn read_keys(ctx: &egui::Context) -> Keys {
    ctx.input(|i| Keys {
        left: i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::A),
        right: i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D),
        up: i.key_pressed(egui::Key::ArrowUp),
        down: i.key_pressed(egui::Key::ArrowDown),
        fit: i.key_pressed(egui::Key::F),
        zoom_in: i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
        zoom_out: i.key_pressed(egui::Key::Minus),
        manga: i.key_pressed(egui::Key::M) || i.key_pressed(egui::Key::Space),
        rot_cw: i.key_pressed(egui::Key::R) && !i.modifiers.ctrl,
        rot_ccw: i.key_pressed(egui::Key::R) && i.modifiers.ctrl,
        pg_up: i.key_pressed(egui::Key::PageUp),
        pg_dn: i.key_pressed(egui::Key::PageDown),
        p_key: i.key_pressed(egui::Key::P),
        n_key: i.key_pressed(egui::Key::N),
        s_key: i.key_pressed(egui::Key::S),
        home: i.key_pressed(egui::Key::Home),
        end: i.key_pressed(egui::Key::End),
        bs: i.key_pressed(egui::Key::Backspace),
        e_key: i.key_pressed(egui::Key::E),
        i_key: i.key_pressed(egui::Key::I),
        enter: i.key_pressed(egui::Key::Enter),
        alt: i.modifiers.alt,
        esc: i.key_pressed(egui::Key::Escape),
        y_key: i.key_pressed(egui::Key::Y),
    })
}

fn handle_keys(app: &mut App, ctx: &egui::Context, k: &Keys, modal_open: bool) {
    if !modal_open {
        if k.left || k.p_key {
            app.go_prev(ctx);
        }
        if k.right || k.n_key {
            app.go_next(ctx);
        }
        if k.up {
            if app.manga_mode {
                app.go_single_prev(ctx);
            } else {
                app.go_prev(ctx);
            }
        }
        if k.down {
            if app.manga_mode {
                app.go_single_next(ctx);
            } else {
                app.go_next(ctx);
            }
        }
        if k.bs {
            if let Some(path) = &app.archive_path {
                let _ = std::process::Command::new("explorer")
                    .arg("/select,")
                    .arg(path)
                    .spawn();
            }
        }
        if k.e_key {
            app.open_external();
        }
    }

    if k.home {
        app.go_first(ctx);
    }
    if k.end {
        app.go_last(ctx);
    }
    if k.fit {
        app.fit = !app.fit;
    }
    if k.zoom_in {
        app.zoom = (app.zoom * 1.2).min(10.0);
        app.fit = false;
    }
    if k.zoom_out {
        app.zoom = (app.zoom / 1.2).max(0.1);
        app.fit = false;
    }
    if k.rot_cw {
        app.rotate_current(true, ctx);
    }
    if k.rot_ccw {
        app.rotate_current(false, ctx);
    }
    if k.pg_up {
        app.go_prev_dir(ctx);
    }
    if k.pg_dn {
        app.go_next_dir(ctx);
    }
    if k.manga {
        app.manga_mode = !app.manga_mode;
        app.schedule_prefetch();
        ctx.request_repaint();
    }
    if k.s_key {
        app.show_sort_settings = !app.show_sort_settings;
        if app.show_sort_settings {
            app.sort_focus_idx = 0;
        }
    }
    if k.y_key {
        app.config.manga_rtl = !app.config.manga_rtl;
        app.save_config();
    }
    if k.i_key {
        app.config.linear_filter = !app.config.linear_filter;
        app.cache.clear();
        app.pending.clear();
        app.save_config();
        app.schedule_prefetch();
    }

    // 全画面 / ボーダレス切替
    if !modal_open && k.enter {
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        if k.alt {
            app.is_borderless = !app.is_borderless;
            app.is_fullscreen = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!app.is_borderless));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(app.is_borderless));
        } else {
            app.is_fullscreen = !app.is_fullscreen;
            app.is_borderless = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(app.is_fullscreen));
        }
    }
    if !modal_open && k.esc && (app.is_fullscreen || app.is_borderless) {
        app.is_fullscreen = false;
        app.is_borderless = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
    }
}

// ── 設定ウィンドウ ────────────────────────────────────────────────────────────

fn show_settings_window(app: &mut App, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }
    egui::Window::new("外部アプリ連携の設定")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Eキーを押した時に起動するソフトを設定します。");
            ui.add_space(8.0);

            egui::Grid::new("config_grid")
                .num_columns(2)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    ui.label("アプリのパス:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.config.external_app);
                        if ui.button("参照…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_file() {
                                app.config.external_app = p.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("コマンド引数:");
                    ui.text_edit_singleline(&mut app.settings_args_tmp);
                    ui.end_row();
                });
            ui.small("※ %P は表示中のファイルパスに置き換わります");
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("設定を保存して閉じる").clicked() {
                    app.config.external_args = app
                        .settings_args_tmp
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                    app.save_config();
                    app.show_settings = false;
                }
                if ui.button("キャンセル").clicked() {
                    app.show_settings = false;
                }
            });
        });
}

// ── ソート設定ウィンドウ ──────────────────────────────────────────────────────

fn show_sort_settings_window(app: &mut App, ctx: &egui::Context, enter_pressed: bool) {
    if !app.show_sort_settings {
        return;
    }
    egui::Window::new("並べ替えの設定 (S)")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let mut changed = false;

            let (arr_up, arr_dn, arr_left, arr_right) = ctx.input(|i| (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            ));

            if arr_up {
                app.sort_focus_idx = (app.sort_focus_idx + 2) % 3;
            }
            if arr_dn {
                app.sort_focus_idx = (app.sort_focus_idx + 1) % 3;
            }
            if enter_pressed {
                app.show_sort_settings = false;
            }

            ui.label("矢印キーで選択 / Enterで戻る");
            ui.add_space(8.0);

            // 基準
            ui.horizontal(|ui| {
                let active = app.sort_focus_idx == 0;
                let label = if active {
                    egui::RichText::new("▶ 基準:").color(egui::Color32::YELLOW)
                } else {
                    egui::RichText::new("  基準:")
                };
                ui.label(label);
                changed |= ui
                    .radio_value(&mut app.config.sort_mode, SortMode::Name, "ファイル名")
                    .changed();
                changed |= ui
                    .radio_value(&mut app.config.sort_mode, SortMode::Mtime, "更新日時")
                    .changed();
                changed |= ui
                    .radio_value(&mut app.config.sort_mode, SortMode::Size, "サイズ")
                    .changed();

                if active {
                    if arr_right {
                        app.config.sort_mode = match app.config.sort_mode {
                            SortMode::Name => SortMode::Mtime,
                            SortMode::Mtime => SortMode::Size,
                            SortMode::Size => SortMode::Name,
                        };
                        changed = true;
                    }
                    if arr_left {
                        app.config.sort_mode = match app.config.sort_mode {
                            SortMode::Name => SortMode::Size,
                            SortMode::Mtime => SortMode::Name,
                            SortMode::Size => SortMode::Mtime,
                        };
                        changed = true;
                    }
                }
            });

            // 順序
            ui.horizontal(|ui| {
                let active = app.sort_focus_idx == 1;
                let label = if active {
                    egui::RichText::new("▶ 順序:").color(egui::Color32::YELLOW)
                } else {
                    egui::RichText::new("  順序:")
                };
                ui.label(label);
                changed |= ui
                    .radio_value(&mut app.config.sort_order, SortOrder::Ascending, "昇順")
                    .changed();
                changed |= ui
                    .radio_value(&mut app.config.sort_order, SortOrder::Descending, "降順")
                    .changed();

                if active && (arr_left || arr_right) {
                    app.config.sort_order = match app.config.sort_order {
                        SortOrder::Ascending => SortOrder::Descending,
                        SortOrder::Descending => SortOrder::Ascending,
                    };
                    changed = true;
                }
            });

            ui.separator();

            // 自然順
            let active = app.sort_focus_idx == 2;
            let check_text = if active {
                egui::RichText::new("自然順（数字の大きさを考慮）")
                    .color(egui::Color32::YELLOW)
            } else {
                egui::RichText::new("自然順（数字の大きさを考慮）")
            };
            if ui
                .checkbox(&mut app.config.sort_natural, check_text)
                .on_hover_text("1, 2, 10 の順に並べます。")
                .changed()
            {
                changed = true;
            }
            if active && (arr_left || arr_right) {
                app.config.sort_natural = !app.config.sort_natural;
                changed = true;
            }

            if changed {
                app.apply_sorting();
                app.save_config();
            }

            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                if ui.button("閉じる").clicked() {
                    app.show_sort_settings = false;
                }
            });
        });
}

// ── メニューバー ──────────────────────────────────────────────────────────────

fn show_menu_bar(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    egui::menu::bar(ui, |ui| {
        // ファイルメニュー
        ui.menu_button("ファイル", |ui| {
            if ui.button("開く…").clicked() {
                ui.close_menu();
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "アーカイブ・画像",
                        &[
                            "zip", "7z", "jpg", "jpeg", "png", "gif", "bmp", "webp",
                            "tiff", "tif",
                        ],
                    )
                    .pick_file()
                {
                    app.open_path(p, ctx);
                }
            }
            if ui.button("フォルダを開く…").clicked() {
                ui.close_menu();
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    app.open_path(p, ctx);
                }
            }
            ui.separator();
            if ui
                .add_enabled(
                    app.archive_path.is_some(),
                    egui::Button::new("エクスプローラーで表示 (BS)"),
                )
                .clicked()
            {
                ui.close_menu();
                if let Some(path) = &app.archive_path {
                    let _ = std::process::Command::new("explorer")
                        .arg("/select,")
                        .arg(path)
                        .spawn();
                }
            }
            if ui
                .add_enabled(
                    app.archive_path.is_some(),
                    egui::Button::new("外部アプリで開く (E)"),
                )
                .clicked()
            {
                ui.close_menu();
                app.open_external();
            }
            if ui.button("外部アプリ設定…").clicked() {
                ui.close_menu();
                app.settings_args_tmp = app.config.external_args.join(" ");
                app.show_settings = true;
            }
            ui.separator();
            if ui.button("終了").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        // 表示メニュー
        ui.menu_button("表示", |ui| {
            if ui.selectable_label(app.fit, "フィット表示 (F)").clicked() {
                app.fit = true;
                ui.close_menu();
            }
            if ui.selectable_label(!app.fit, "等倍表示").clicked() {
                app.fit = false;
                app.zoom = 1.0;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("拡大 (+)").clicked() {
                app.zoom = (app.zoom * 1.2).min(10.0);
                app.fit = false;
                ui.close_menu();
            }
            if ui.button("縮小 (-)").clicked() {
                app.zoom = (app.zoom / 1.2).max(0.1);
                app.fit = false;
                ui.close_menu();
            }
            ui.separator();
            if ui
                .selectable_label(app.manga_mode, "マンガモード (M)")
                .clicked()
            {
                app.manga_mode = !app.manga_mode;
                app.schedule_prefetch();
                ctx.request_repaint();
                ui.close_menu();
            }
            if ui
                .selectable_label(app.config.manga_rtl, "右開き表示 (Y)")
                .clicked()
            {
                app.config.manga_rtl = !app.config.manga_rtl;
                app.save_config();
                ui.close_menu();
            }
            if ui.button("並べ替えの設定 (S)").clicked() {
                app.show_sort_settings = true;
                ui.close_menu();
            }
            if ui
                .selectable_label(
                    app.config.linear_filter,
                    "画像の補正(スムージング) (I)",
                )
                .clicked()
            {
                app.config.linear_filter = !app.config.linear_filter;
                app.cache.clear();
                app.pending.clear();
                app.save_config();
                app.schedule_prefetch();
                ui.close_menu();
            }
            if ui
                .checkbox(&mut app.config.allow_multiple_instances, "複数起動を許可")
                .clicked()
            {
                app.save_config();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("右回転 (R)").clicked() {
                app.rotate_current(true, ctx);
                ui.close_menu();
            }
            if ui.button("左回転 (Ctrl+R)").clicked() {
                app.rotate_current(false, ctx);
                ui.close_menu();
            }
        });

        // フォルダメニュー
        ui.menu_button("フォルダ", |ui| {
            if ui.button("前のフォルダ (PgUp)").clicked() {
                app.go_prev_dir(ctx);
                ui.close_menu();
            }
            if ui.button("次のフォルダ (PgDn)").clicked() {
                app.go_next_dir(ctx);
                ui.close_menu();
            }
            ui.separator();
            ui.label("フォルダ移動時の設定:");
            ui.radio_value(&mut app.open_from_end, false, "先頭から開く");
            ui.radio_value(&mut app.open_from_end, true, "末尾から開く");
        });
    });
}

// ── ツールバー ────────────────────────────────────────────────────────────────

fn show_toolbar(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let has = !app.entries.is_empty();

        if ui
            .add_enabled(has, egui::Button::new("◀ (P)"))
            .clicked()
        {
            app.go_prev(ctx);
        }

        // ページシークスライダー
        if has {
            let max_idx = app.entries.len().saturating_sub(1);
            let mut slider_val = app.target_index;
            ui.style_mut().spacing.slider_width = 160.0;
            if ui
                .add(
                    egui::Slider::new(&mut slider_val, 0..=max_idx)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                app.target_index = slider_val;
                app.schedule_prefetch();
            }
        }

        if ui
            .add_enabled(has, egui::Button::new("▶ (N)"))
            .clicked()
        {
            app.go_next(ctx);
        }
        ui.separator();
        if ui
            .add_enabled(has, egui::Button::new("⟲"))
            .on_hover_text("左回転 Ctrl+R")
            .clicked()
        {
            app.rotate_current(false, ctx);
        }
        if ui
            .add_enabled(has, egui::Button::new("⟳"))
            .on_hover_text("右回転 R")
            .clicked()
        {
            app.rotate_current(true, ctx);
        }
        ui.separator();
        let ml = if app.manga_mode { "📖 2P" } else { "📄 1P" };
        if ui
            .button(ml)
            .on_hover_text("マンガモード (M)")
            .clicked()
        {
            app.manga_mode = !app.manga_mode;
            app.schedule_prefetch();
            ctx.request_repaint();
        }
        ui.separator();

        if has {
            // target_index と entries_meta の長さは常に一致するが念のため bounds チェック
            if let Some(meta) = app.entries_meta.get(app.target_index) {
                let short = std::path::Path::new(&meta.name)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(meta.name.as_str());

                let count = format!("{}/{}", app.target_index + 1, app.entries.len());

                let file_size = format_file_size(meta.size);

                let day_str = if meta.mtime > 0 {
                    Local
                        .timestamp_opt(meta.mtime as i64, 0)
                        .unwrap()
                        .format("%Y/%m/%d")
                        .to_string()
                } else {
                    "----/--/--".to_string()
                };

                let sort_label = match app.config.sort_mode {
                    SortMode::Name => "Name",
                    SortMode::Mtime => "Day",
                    SortMode::Size => "Size",
                };
                let sort_icon = if app.config.sort_order == SortOrder::Ascending {
                    "▲"
                } else {
                    "▼"
                };

                let loading = app.get_texture(app.target_index).is_none();
                let status = if loading { " ⏳" } else { "" };
                ui.label(format!(
                    "{}{} | {} | {} | {} | [{} {}]",
                    count, status, short, day_str, file_size, sort_label, sort_icon
                ));

                if !app.fit {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{:.0}%", app.zoom * 100.0));
                    });
                }
            }
        } else {
            ui.label("ファイルをドラッグ＆ドロップ、またはメニューから開いてください");
        }
    });
}

// ── メイン表示エリア ──────────────────────────────────────────────────────────

fn show_main_area(
    app: &mut App,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    click_allowed: bool,
) {
    // エラー表示
    if let Some(err) = app.error.clone() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(format!("⚠ {err}"))
                    .color(egui::Color32::RED),
            );
        });
        return;
    }

    // テクスチャ未準備の場合
    let tex1 = app.get_texture(app.current).map(|t| (t.id(), t.size_vec2()));
    if tex1.is_none() {
        show_loading_or_empty(app, ctx, ui);
        return;
    }

    // マウスホイール処理
    let (wheel, ctrl, secondary) = ctx.input(|i| {
        (
            i.smooth_scroll_delta.y,
            i.modifiers.ctrl,
            i.pointer.button_down(egui::PointerButton::Secondary),
        )
    });

    if wheel != 0.0 {
        if ctrl || secondary {
            app.zoom = (app.zoom * (1.0 + wheel * 0.002)).clamp(0.05, 10.0);
            app.fit = false;
        } else {
            // 蓄積バッファがしきい値(40.0)を超えた時だけページ送り
            app.wheel_accumulator += wheel;
            if app.wheel_accumulator.abs() >= 40.0 {
                if app.wheel_accumulator > 0.0 {
                    app.go_prev(ctx);
                } else {
                    app.go_next(ctx);
                }
                app.fit = true;
                app.zoom = 1.0;
                app.wheel_accumulator = 0.0;
            }
        }
    } else {
        app.wheel_accumulator = 0.0;
    }

    let avail = ui.available_size();
    let (tex1_id, tex1_size) = tex1.unwrap();

    // 2 枚目の取得判定（マンガモード）
    let can_pair = (app.manga_shift || app.current > 0) && tex1_size.x <= tex1_size.y;
    let tex2 = if app.manga_mode && can_pair {
        app.get_texture(app.current + 1).and_then(|t| {
            let s = t.size_vec2();
            if s.x <= s.y {
                Some((t.id(), s))
            } else {
                None
            }
        })
    } else {
        None
    };

    egui::ScrollArea::both().show(ui, |ui| {
        if app.manga_mode {
            if let Some((tex2_id, tex2_size)) = tex2 {
                show_double_page(
                    app,
                    ctx,
                    ui,
                    avail,
                    tex1_id,
                    tex1_size,
                    tex2_id,
                    tex2_size,
                    click_allowed,
                );
            } else {
                // 2 枚目ロード待ち：1 枚で表示
                let resp = draw_centered(ui, tex1_id, tex1_size, avail, app.fit, app.zoom);
                handle_click(app, ctx, &resp, click_allowed, false);
                ctx.request_repaint();
            }
        } else {
            let resp = draw_centered(ui, tex1_id, tex1_size, avail, app.fit, app.zoom);
            handle_click(app, ctx, &resp, click_allowed, false);
        }

        // 末尾に「次のフォルダへ」ボタン
        let is_at_end = if tex2.is_some() {
            app.current + 1 >= app.entries.len().saturating_sub(1)
        } else {
            app.current >= app.entries.len().saturating_sub(1)
        };
        if is_at_end {
            if let Some((siblings, idx)) = app.sibling_dirs() {
                if idx + 1 < siblings.len() {
                    let next_path = &siblings[idx + 1];
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        let btn_text = format!(
                            "次のフォルダへ: {} ➡",
                            next_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        );
                        if ui
                            .button(egui::RichText::new(btn_text).size(20.0).strong())
                            .clicked()
                        {
                            app.go_next_dir(ctx);
                        }
                    });
                    ui.add_space(48.0);
                }
            }
        }
    });
}

/// テクスチャ未準備時の表示（スピナー or 空フォルダナビ）
fn show_loading_or_empty(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        if app.entries.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("画像が見つかりませんでした")
                        .size(20.0)
                        .strong(),
                );
                ui.add_space(10.0);

                if let Some(p) = &app.archive_path.clone() {
                    if let Some(parent) = p.parent() {
                        if ui
                            .button(format!("⤴ 親フォルダへ: {}", parent.display()))
                            .clicked()
                        {
                            app.open_path(parent.to_path_buf(), ctx);
                        }
                    }
                }

                ui.add_space(10.0);
                ui.label("移動候補:");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for path in app.nav_items.clone() {
                        let icon = if path.is_dir() { "📁" } else { "📦" };
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if ui.button(format!("{icon} {name}")).clicked() {
                            app.open_path(path, ctx);
                        }
                    }
                });
            });
        } else {
            ui.label(
                egui::RichText::new("⏳ 読み込み中...")
                    .size(18.0)
                    .color(egui::Color32::GRAY),
            );
        }
    });
}

/// 2 枚並べ（マンガモード）を描画する
#[allow(clippy::too_many_arguments)]
fn show_double_page(
    app: &mut App,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    avail: egui::Vec2,
    tex1_id: egui::TextureId,
    tex1_size: egui::Vec2,
    tex2_id: egui::TextureId,
    tex2_size: egui::Vec2,
    click_allowed: bool,
) {
    let half = egui::vec2(avail.x / 2.0, avail.y);
    let s1 = if app.fit {
        (half.x / tex1_size.x)
            .min(half.y / tex1_size.y)
            .min(1.0)
    } else {
        app.zoom
    };
    let s2 = if app.fit {
        (half.x / tex2_size.x)
            .min(half.y / tex2_size.y)
            .min(1.0)
    } else {
        app.zoom
    };
    let ds1 = tex1_size * s1;
    let ds2 = tex2_size * s2;
    let total_w = (ds1.x + ds2.x).max(avail.x);
    let total_h = ds1.y.max(ds2.y).max(avail.y);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::click());
    let cx = rect.min.x + total_w / 2.0;
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    if app.config.manga_rtl {
        // 右開き：右に1枚目(n)、左に2枚目(n+1)
        ui.painter().image(
            tex1_id,
            egui::Rect::from_min_size(
                egui::pos2(cx, rect.min.y + (total_h - ds1.y) / 2.0),
                ds1,
            ),
            uv,
            egui::Color32::WHITE,
        );
        ui.painter().image(
            tex2_id,
            egui::Rect::from_min_size(
                egui::pos2(cx - ds2.x, rect.min.y + (total_h - ds2.y) / 2.0),
                ds2,
            ),
            uv,
            egui::Color32::WHITE,
        );
    } else {
        // 左開き：左に1枚目(n)、右に2枚目(n+1)
        ui.painter().image(
            tex1_id,
            egui::Rect::from_min_size(
                egui::pos2(cx - ds1.x, rect.min.y + (total_h - ds1.y) / 2.0),
                ds1,
            ),
            uv,
            egui::Color32::WHITE,
        );
        ui.painter().image(
            tex2_id,
            egui::Rect::from_min_size(
                egui::pos2(cx, rect.min.y + (total_h - ds2.y) / 2.0),
                ds2,
            ),
            uv,
            egui::Color32::WHITE,
        );
    }

    handle_click(app, ctx, &resp, click_allowed, true);
}

/// クリック操作の処理
fn handle_click(
    app: &mut App,
    ctx: &egui::Context,
    resp: &egui::Response,
    click_allowed: bool,
    _is_double: bool,
) {
    if !click_allowed {
        return;
    }
    if resp.secondary_clicked() {
        app.go_prev(ctx);
    } else if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.x < resp.rect.center().x {
                app.go_prev(ctx);
            } else {
                app.go_next(ctx);
            }
        }
    }
}

// ── ユーティリティ ────────────────────────────────────────────────────────────

fn format_file_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.0} KB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
}
