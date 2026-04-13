use crate::App;
use crate::config::{SortMode, SortOrder};
use eframe::egui;

impl App {
    pub fn ui_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("ファイル", |ui| {
                if ui.button("開く…").clicked() {
                    ui.close_menu();
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("アーカイブ・画像", &["zip", "7z", "jpg", "jpeg", "png", "gif", "bmp", "webp"])
                        .pick_file()
                    {
                        self.open_path(p, ctx);
                    }
                }
                if ui.button("フォルダを開く…").clicked() {
                    ui.close_menu();
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.open_path(p, ctx);
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.archive_path.is_some(),
                        egui::Button::new("エクスプローラーで表示 (BS)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    if let Some(path) = &self.archive_path {
                        let _ = std::process::Command::new("explorer")
                            .arg("/select,")
                            .arg(path)
                            .spawn();
                    }
                }
                if ui
                    .add_enabled(
                        self.archive_path.is_some(),
                        egui::Button::new("外部アプリで開く (E)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.open_external();
                }
                if ui.button("外部アプリ設定…").clicked() {
                    ui.close_menu();
                    self.settings_args_tmp = self.config.external_args.join(" ");
                    self.show_settings = true;
                }
                ui.separator();
                if ui.button("終了").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("表示", |ui| {
                if ui.selectable_label(self.fit, "フィット表示 (F)").clicked() {
                    self.fit = true;
                    ui.close_menu();
                }
                if ui.selectable_label(!self.fit, "等倍表示").clicked() {
                    self.fit = false;
                    self.zoom = 1.0;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("拡大 (+)").clicked() {
                    self.zoom = (self.zoom * 1.2).min(10.0);
                    self.fit = false;
                    ui.close_menu();
                }
                if ui.button("縮小 (-)").clicked() {
                    self.zoom = (self.zoom / 1.2).max(0.1);
                    self.fit = false;
                    ui.close_menu();
                }
                ui.separator();
                if ui.selectable_label(self.manga_mode, "マンガモード (M)").clicked() {
                    self.manga_mode = !self.manga_mode;
                    self.schedule_prefetch();
                    ctx.request_repaint();
                    ui.close_menu();
                }
                if ui.button("並べ替えの設定 (S)").clicked() {
                    self.show_sort_settings = true;
                    ui.close_menu();
                }
                if ui
                    .selectable_label(self.config.linear_filter, "画像の補正(スムージング) (I)")
                    .clicked()
                {
                    self.config.linear_filter = !self.config.linear_filter;
                    self.cache.clear();
                    self.cache_lru.clear();
                    self.pending.clear();
                    self.save_config();
                    self.schedule_prefetch();
                    ui.close_menu();
                }
                if ui
                    .checkbox(
                        &mut self.config.allow_multiple_instances,
                        "複数起動を許可",
                    )
                    .clicked()
                {
                    self.save_config();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("右回転 (R)").clicked() {
                    self.rotate_current(true, ctx);
                    ui.close_menu();
                }
                if ui.button("左回転 (Ctrl+R)").clicked() {
                    self.rotate_current(false, ctx);
                    ui.close_menu();
                }
            });
            ui.menu_button("フォルダ", |ui| {
                if ui.button("前のフォルダ (PgUp)").clicked() {
                    self.go_prev_dir(ctx);
                    ui.close_menu();
                }
                if ui.button("次のフォルダ (PgDn)").clicked() {
                    self.go_next_dir(ctx);
                    ui.close_menu();
                }
                ui.separator();
                ui.label("フォルダ移動時の設定:");
                ui.radio_value(&mut self.open_from_end, false, "先頭から開く");
                ui.radio_value(&mut self.open_from_end, true, "末尾から開く");
            });
        });
    }

    pub fn ui_settings_window(&mut self, ctx: &egui::Context) {
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
                            ui.text_edit_singleline(&mut self.config.external_app);
                            if ui.button("参照…").clicked() {
                                if let Some(p) = rfd::FileDialog::new().pick_file() {
                                    self.config.external_app = p.to_string_lossy().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("コマンド引数:");
                        ui.text_edit_singleline(&mut self.settings_args_tmp);
                        ui.end_row();
                    });
                ui.small("※ %P は表示中のファイルパスに置き換わります");
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if ui.button("設定を保存して閉じる").clicked() {
                        self.config.external_args = self
                            .settings_args_tmp
                            .split_whitespace()
                            .map(|s| s.to_string())
                            .collect();
                        self.save_config();
                        self.show_settings = false;
                    }
                    if ui.button("キャンセル").clicked() {
                        self.show_settings = false;
                    }
                });
            });
    }

    pub fn ui_sort_settings_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("並べ替えの設定 (S)")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let mut changed = false;
                let (arr_up, arr_dn, arr_left, arr_right, enter) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::ArrowLeft),
                        i.key_pressed(egui::Key::ArrowRight),
                        i.key_pressed(egui::Key::Enter),
                    )
                });

                if arr_up {
                    self.sort_focus_idx = (self.sort_focus_idx + 2) % 3;
                }
                if arr_dn {
                    self.sort_focus_idx = (self.sort_focus_idx + 1) % 3;
                }
                if enter {
                    self.show_sort_settings = false;
                }

                ui.label("矢印キーで選択 / Enterで戻る");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let active = self.sort_focus_idx == 0;
                    let label = if active {
                        egui::RichText::new("▶ 基準:").color(egui::Color32::YELLOW)
                    } else {
                        egui::RichText::new("  基準:")
                    };
                    ui.label(label);
                    changed |= ui
                        .radio_value(&mut self.config.sort_mode, SortMode::Name, "ファイル名")
                        .changed();
                    changed |= ui
                        .radio_value(&mut self.config.sort_mode, SortMode::Mtime, "更新日時")
                        .changed();
                    changed |= ui
                        .radio_value(&mut self.config.sort_mode, SortMode::Size, "サイズ")
                        .changed();

                    if active {
                        if arr_right {
                            self.config.sort_mode = match self.config.sort_mode {
                                SortMode::Name => SortMode::Mtime,
                                SortMode::Mtime => SortMode::Size,
                                SortMode::Size => SortMode::Name,
                            };
                            changed = true;
                        }
                        if arr_left {
                            self.config.sort_mode = match self.config.sort_mode {
                                SortMode::Name => SortMode::Size,
                                SortMode::Mtime => SortMode::Name,
                                SortMode::Size => SortMode::Mtime,
                            };
                            changed = true;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let active = self.sort_focus_idx == 1;
                    let label = if active {
                        egui::RichText::new("▶ 順序:").color(egui::Color32::YELLOW)
                    } else {
                        egui::RichText::new("  順序:")
                    };
                    ui.label(label);
                    changed |= ui
                        .radio_value(&mut self.config.sort_order, SortOrder::Ascending, "昇順")
                        .changed();
                    changed |= ui
                        .radio_value(&mut self.config.sort_order, SortOrder::Descending, "降順")
                        .changed();

                    if active && (arr_left || arr_right) {
                        self.config.sort_order = match self.config.sort_order {
                            SortOrder::Ascending => SortOrder::Descending,
                            SortOrder::Descending => SortOrder::Ascending,
                        };
                        changed = true;
                    }
                });

                ui.separator();

                let active = self.sort_focus_idx == 2;
                let check_text = if active {
                    egui::RichText::new("自然順（数字の大きさを考慮）")
                        .color(egui::Color32::YELLOW)
                } else {
                    egui::RichText::new("自然順（数字の大きさを考慮）")
                };
                if ui
                    .checkbox(&mut self.config.sort_natural, check_text)
                    .on_hover_text("1, 2, 10 の順に並べます。")
                    .changed()
                {
                    changed = true;
                }
                if active && (arr_left || arr_right) {
                    self.config.sort_natural = !self.config.sort_natural;
                    changed = true;
                }

                if changed {
                    self.apply_sorting();
                    self.save_config();
                }
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui.button("閉じる").clicked() {
                        self.show_sort_settings = false;
                    }
                });
            });
    }

    pub fn ui_loading_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("読み込み中")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("ファイルリストを取得しています...");
                });
            });
    }

    pub fn ui_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let has = !self.entries.is_empty();
            if ui.add_enabled(has, egui::Button::new("◀ (P)")).clicked() {
                self.go_prev(ctx);
            }

            if has {
                let max_idx = self.entries.len().saturating_sub(1);
                let mut slider_val = self.target_index;
                ui.style_mut().spacing.slider_width = 160.0;
                if ui
                    .add(egui::Slider::new(&mut slider_val, 0..=max_idx).show_value(false).trailing_fill(true))
                    .changed()
                {
                    self.target_index = slider_val;
                    self.schedule_prefetch();
                }
            }

            if ui.add_enabled(has, egui::Button::new("▶ (N)")).clicked() {
                self.go_next(ctx);
            }
            ui.separator();
            if ui
                .add_enabled(has, egui::Button::new("⟲"))
                .on_hover_text("左回転 Ctrl+R")
                .clicked()
            {
                self.rotate_current(false, ctx);
            }
            if ui
                .add_enabled(has, egui::Button::new("⟳"))
                .on_hover_text("右回転 R")
                .clicked()
            {
                self.rotate_current(true, ctx);
            }
            ui.separator();
            let ml = if self.manga_mode { "📖 2P" } else { "📄 1P" };
            if ui.button(ml).on_hover_text("マンガモード (M)").clicked() {
                self.manga_mode = !self.manga_mode;
                self.schedule_prefetch();
                ctx.request_repaint();
            }
            ui.separator();
            if has {
                let meta = &self.entries_meta[self.target_index];
                let short = std::path::Path::new(&meta.name)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(meta.name.as_str())
                    .to_string();
                let count = format!("{}/{}", self.target_index + 1, self.entries.len());
                let file_size = if meta.size >= 1024 * 1024 {
                    format!("{:.1} MB", meta.size as f64 / (1024.0 * 1024.0))
                } else if meta.size >= 1024 {
                    format!("{:.0} KB", meta.size as f64 / 1024.0)
                } else {
                    format!("{} B", meta.size)
                };
                let day_str = if meta.mtime > 0 {
                    chrono::Local
                        .timestamp_opt(meta.mtime as i64, 0)
                        .unwrap()
                        .format("%Y/%m/%d")
                        .to_string()
                } else {
                    "----/--/--".to_string()
                };
                let sort_label = match self.config.sort_mode {
                    SortMode::Name => "Name",
                    SortMode::Mtime => "Day",
                    SortMode::Size => "Size",
                };
                let sort_icon = if self.config.sort_order == SortOrder::Ascending {
                    "▲"
                } else {
                    "▼"
                };
                let loading = self.get_texture(self.target_index).is_none();
                let status = if loading { " ⏳" } else { "" };
                ui.label(format!(
                    "{}{} | {} | {} | {} | [{} {}]",
                    count, status, short, day_str, file_size, sort_label, sort_icon
                ));
                if !self.fit {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{:.0}%", self.zoom * 100.0));
                    });
                }
            } else {
                ui.label("ファイルをドラッグ＆ドロップ、またはメニューから開いてください");
            }
        });
    }

    pub fn ui_main_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, click_allowed: bool) {
        if let Some(err) = self.error.clone() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(format!("⚠ {err}"))
                        .color(egui::Color32::RED),
                );
            });
            return;
        }

        let tex1 = self
            .get_texture(self.current)
            .map(|t| (t.id(), t.size_vec2()));
        if tex1.is_none() {
            ui.centered_and_justified(|ui| {
                if self.entries.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "ここにファイル・フォルダをドロップ\nまたはメニュー → 開く",
                        )
                        .size(18.0)
                        .color(egui::Color32::GRAY),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("⏳ 読み込み中...")
                            .size(18.0)
                            .color(egui::Color32::GRAY),
                    );
                    ctx.request_repaint();
                }
            });
            return;
        }

        let (wheel, ctrl, secondary) = ctx.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.modifiers.ctrl,
                i.pointer.button_down(egui::PointerButton::Secondary),
            )
        });
        if wheel != 0.0 {
            if ctrl || secondary {
                self.zoom = (self.zoom * (1.0 + wheel * 0.002)).clamp(0.05, 10.0);
                self.fit = false;
            } else {
                self.wheel_accumulator += wheel;
                if self.wheel_accumulator.abs() >= 40.0 {
                    if self.wheel_accumulator > 0.0 {
                        self.go_prev(ctx);
                    } else {
                        self.go_next(ctx);
                    }
                    self.fit = true;
                    self.zoom = 1.0;
                    self.wheel_accumulator = 0.0;
                }
            }
        } else {
            self.wheel_accumulator = 0.0;
        }

        let avail = ui.available_size();
        let fit = self.fit;
        let zoom = self.zoom;
        let (tex1_id, tex1_size) = tex1.unwrap();
        let can_pair = (self.manga_shift || self.current > 0) && tex1_size.x <= tex1_size.y;
        let tex2 = if self.manga_mode && can_pair {
            self.get_texture(self.current + 1).and_then(|t| {
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
            if self.manga_mode {
                if let Some((tex2_id, tex2_size)) = tex2 {
                    crate::ui_draw::draw_manga_pair(
                        ui, ctx, click_allowed, self, tex1_id, tex1_size, tex2_id, tex2_size,
                        avail, fit, zoom,
                    );
                }
            } else {
                crate::ui_draw::draw_single_page(
                    ui, ctx, click_allowed, self, tex1_id, tex1_size, avail, fit, zoom,
                );
                if tex2.is_none() && self.manga_mode {
                    ctx.request_repaint();
                }
            }
        });
    }
}
