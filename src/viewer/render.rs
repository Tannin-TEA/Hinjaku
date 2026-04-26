use eframe::egui;
use crate::{painter, widgets, input};
use crate::types::WindowMode;
use super::App;

impl App {
    pub(super) fn draw_ui(&mut self, ctx: &egui::Context) {
        self.draw_windows(ctx);

        let mut menu_act = None;
        let mut tool_act = None;

        if self.view.window_mode != WindowMode::Standard {
            // ボーダレスモード：マウスホバーでオーバーレイ表示
            let mouse_pos = ctx.input(|i| i.pointer.hover_pos());
            let screen_rect = ctx.screen_rect();

            let in_menu_zone   = mouse_pos.is_some_and(|p| p.y < 40.0);
            let in_status_zone = mouse_pos.is_some_and(|p| p.y > screen_rect.height() - 40.0);

            // 前フレームのレイヤー情報からマウスがオーバーレイ（ドロップダウン含む）上にいるか検出
            let mouse_over_overlay = mouse_pos.is_some_and(|p| {
                ctx.layer_id_at(p).is_some_and(|id| id.order == egui::Order::Foreground)
            });

            // メニューが開いている（ポップアップがある）間も表示を維持する判定を追加
            let show_overlay = in_menu_zone || in_status_zone || mouse_over_overlay || ctx.memory(|m| m.any_popup_open());

            let show_menu   = show_overlay;
            let show_status = show_overlay;

            if show_menu {
                egui::Area::new(egui::Id::new("menu_overlay"))
                    .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0))
                    .order(egui::Order::Foreground)
                    .interactable(true)
                    .show(ctx, |ui| {
                        egui::Frame::menu(ui.style()).fill(ui.visuals().window_fill().linear_multiply(0.9)).show(ui, |ui| {
                            ui.set_width(screen_rect.width());
                            let act = widgets::main_menu_bar_inner(ui, &self.config, &self.manager, &self.view, self.ui.show_tree, self.ui.show_debug);
                            menu_act = act;
                        });
                    });
            }
            if show_status {
                egui::Area::new(egui::Id::new("status_overlay"))
                    .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(0.0, 0.0))
                    .order(egui::Order::Foreground)
                    .interactable(true)
                    .show(ctx, |ui| {
                        egui::Frame::menu(ui.style()).fill(ui.visuals().window_fill().linear_multiply(0.9)).show(ui, |ui| {
                            ui.set_width(screen_rect.width());
                            tool_act = widgets::bottom_toolbar_inner(ui, &self.manager, &self.config, &self.view, self.is_nav_locked(ctx));
                        });
                    });
            }
        } else {
            let act = widgets::main_menu_bar(ctx, &self.config, &self.manager, &self.view, self.ui.show_tree, self.ui.show_debug);
            menu_act = act;
            tool_act = widgets::bottom_toolbar(ctx, &self.manager, &self.config, &self.view, self.is_nav_locked(ctx));
        }

        let mut tree_req = None;
        if self.ui.show_tree {
            egui::SidePanel::left("tree")
                .resizable(true)
                .default_width(ctx.screen_rect().width() * 0.5)
                .max_width(ctx.screen_rect().width() * 0.5)
                .show(ctx, |ui| widgets::sidebar_ui(ui, &mut self.manager.tree, &self.manager.archive_path, ctx, &mut tree_req));
            self.manager.tree.scroll_to_selected = false;
        }
        if let Some(act) = menu_act { self.handle_action(ctx, act); }
        if let Some(act) = tool_act { self.handle_action(ctx, act); }
        if let Some(p) = tree_req { self.open_path(p, ctx); }

        if !self.ui.boss_mode {
            self.draw_main_panel(ctx);
        } else {
            egui::CentralPanel::default().show(ctx, |_ui| {});
            self.draw_boss_mode(ctx);
        }
        self.toasts.draw(ctx);

        // ボーダレスモード時に 1px の外枠を描画する
        if self.view.window_mode == WindowMode::Borderless {
            let color = ctx.style().visuals.text_color();
            ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("borderless_outline")))
                .rect_stroke(ctx.screen_rect().shrink(0.5), 0.0, egui::Stroke::new(1.0, color));
        }
    }

    pub(super) fn draw_windows(&mut self, ctx: &egui::Context) {
        if self.ui.show_settings
            && widgets::settings_window(ctx, &mut self.ui.show_settings, &mut self.config, &mut self.ui.settings_args_tmp) {
                self.save_config();
            }
        if self.ui.show_key_config {
            if let Some(id) = self.ui.capturing_key_for.clone() {
                if let Some(c) = input::detect_key_combination(ctx) {
                    self.config.keys.insert(id, c);
                    self.ui.capturing_key_for = None;
                    self.save_config();
                }
            }
            if widgets::key_config_window(ctx, &mut self.ui.show_key_config, &mut self.config, &mut self.ui.capturing_key_for) {
                self.save_config();
            }
        }
        if self.config.is_first_run {
            egui::Window::new("Hinjaku へようこそ")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .collapsible(false).resizable(false)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("設定ファイル (config.ini) を作成しました。").strong());
                        ui.add_space(8.0);
                        ui.label("吹けば飛ぶよな軽量ビューア");
                        ui.add_space(8.0);
                        ui.group(|ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.label(" [ 主な操作ショートカット ] ");
                                ui.label("・ < / > (P / N) : ページ移動");
                                ui.label("・ F : フィットモード切替");
                                ui.label("・ M / Space : マンガモード(見開き)切替");
                                ui.label("・ T : ディレクトリツリー表示");
                            });
                        });
                        ui.add_space(8.0);
                        ui.label("詳細な設定やキーの変更はメニューの「オプション」から行えます。");
                        ui.add_space(12.0);
                        if ui.button(egui::RichText::new("はじめる").size(18.0)).clicked() {
                            self.config.is_first_run = false;
                            self.save_config();
                        }
                    });
                });
        }
        if self.ui.show_sort_settings {
            widgets::sort_settings_window(ctx, &mut self.ui.show_sort_settings, &mut self.config, &mut self.ui.sort_focus_idx, false, ctx.input(|i| i.key_pressed(egui::Key::Space)));
            if !self.ui.show_sort_settings {
                self.manager.apply_sorting(&self.config);
                self.manager.clear_cache();
                let max_dim = self.get_effective_max_dim(ctx);
                self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
                self.save_config();
            }
        }
        if self.ui.show_debug { widgets::debug_window(ctx, &mut self.ui.show_debug, &self.manager); }
        if self.ui.show_about { widgets::dialogs::about_window(ctx, &mut self.ui.show_about); }
        if self.ui.show_jump_dialog {
            let total = self.manager.entries.len();
            let mut jumped = false;
            let mut closed = false;
            egui::Window::new("ページジャンプ")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let current = self.manager.current + 1;
                    let page_info = if self.view.manga_mode {
                        format!("現在: {}", current)
                    } else {
                        format!("現在: {} / {}", current, total)
                    };
                    ui.label(page_info);
                    ui.label(format!("ページ番号を入力 (1 – {})", total));
                    let enter = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    let resp = ui.text_edit_singleline(&mut self.ui.jump_input);
                    resp.request_focus();
                    if enter { jumped = true; }
                    ui.horizontal(|ui| {
                        if ui.button("ジャンプ").clicked() { jumped = true; }
                        if ui.button("キャンセル").clicked() { closed = true; }
                    });
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) { closed = true; }
                });
            if jumped {
                if let Ok(n) = self.ui.jump_input.trim().parse::<usize>() {
                    let idx = n.saturating_sub(1).min(total.saturating_sub(1));
                    self.seek(idx, ctx);
                }
                self.ui.show_jump_dialog = false;
            } else if closed {
                self.ui.show_jump_dialog = false;
            }
        }
        if self.ui.show_limiter_settings
            && widgets::limiter_settings_window(ctx, &mut self.ui.show_limiter_settings, &mut self.config) {
                self.save_config();
            }
        if self.ui.pdf_warning_open {
            egui::Window::new("PDF表示に関するお知らせ")
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
                .collapsible(false)
                .resizable(false)
                .frame(egui::Frame::window(&ctx.style()).inner_margin(12.0))
                .show(ctx, |ui| {
                    ui.label("PDFの閲覧は、画像に比べCPU負荷が高くなる場合があります。");
                    ui.add_space(4.0);
                    ui.checkbox(&mut self.config.show_pdf_warning, "以後、このメッセージを表示しない");
                    ui.add_space(8.0);
                    ui.vertical_centered_justified(|ui| {
                        if ui.button("了解").clicked() {
                            self.ui.pdf_warning_open = false;
                            self.save_config();
                        }
                    });
                });
        }
    }

    /// ⚠️ この関数の ScrollArea 設定・Sense・画像配置計算は描画性能に直結する。
    ///   変更前にユーザーへ確認すること。
    pub(super) fn draw_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            painter::paint_background(ui, ui.available_rect_before_wrap(), self.config.bg_mode);

            // 表示エリアとUIオーバーヘッドの自動計測
            let avail  = ui.available_size();
            let screen = ctx.screen_rect().size();
            let now        = ctx.input(|i| i.time);
            let in_resize  = self.last_resize_time > 0.0 && now - self.last_resize_time < 0.5;

            // Standard モードのときのみ overhead を更新
            // （非 Standard ではメニューがオーバーレイになり avail≈screen になるため）
            if self.view.window_mode == WindowMode::Standard {
                self.ui_width_overhead  = (screen.x - avail.x).max(0.0);
                self.ui_height_overhead = (screen.y - avail.y).max(0.0);
            }

            // リサイズ指示から 0.5 秒間は target を上書きしない（ウィンドウが実際に動くまで待つ）
            if !in_resize {
                match self.view.window_mode {
                    WindowMode::Standard   => { self.target_display_w = avail.x;   self.target_display_h = avail.y; }
                    WindowMode::Borderless => { self.target_display_w = screen.x;  self.target_display_h = screen.y; }
                    WindowMode::Fullscreen => {}
                }
            }

            if let Some(err) = self.error.clone() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new(format!("エラー: {err}")).color(egui::Color32::RED));
                });
                return;
            }
            if self.get_texture(self.manager.current).is_none() {
                self.draw_loading_screen(ui, ctx);
                return;
            }
            let is_at_end = self.manager.current >= self.manager.entries.len().saturating_sub(2);
            let sec_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
            let (resp, act, eff_zoom, scroll_off, vp_origin) = painter::draw_main_area(ui, &self.manager, &self.view, self.config.manga_rtl, ctx, is_at_end, sec_down, self.pending_scroll);

            // 画像エリアが直接クリックされた場合のみページ移動を実行
            if resp.clicked() {
                self.go_next(ctx);
            }
            if resp.secondary_clicked() && !self.is_mouse_gesture {
                self.go_prev(ctx);
            }

            self.view.effective_zoom = eff_zoom;
            self.scroll_offset = scroll_off;
            self.viewport_origin = vp_origin;
            if let Some(widgets::ViewerAction::NextDir) = act {
                // 自動めくり時と同様、リミッター設定（最後で止まる）を尊重する
                if !(self.config.limiter_mode && self.config.limiter_stop_at_end) {
                    self.go_next_dir(ctx);
                }
            }
        });
    }

    pub(super) fn draw_loading_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.centered_and_justified(|ui| {
            if self.manager.entries.is_empty() && !self.manager.is_listing {
                ui.vertical_centered(|ui| {
                    let faint = ui.visuals().weak_text_color().linear_multiply(0.1);
                    ui.label(egui::RichText::new("H").size(140.0).strong().color(faint));
                    ui.add_space(8.0);
                    ui.label("フォルダやアーカイブをドラッグ＆ドロップしてください。");
                    if let Some(p) = &self.manager.archive_path {
                        if let Some(parent) = p.parent() {
                            if ui.button("一つ上の階層へ").clicked() {
                                let c = p.clone();
                                self.move_to_dir(parent.to_path_buf(), Some(c), false, ctx);
                            }
                        }
                    }
                });
            } else {
                ui.label("読み込み中...");
            }
        });
    }

    pub(super) fn draw_boss_mode(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("boss_mode"))
            .order(egui::Order::TOP)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let painter = ui.painter();
                painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(200));

                let center = screen.center();
                ui.allocate_ui_at_rect(
                    egui::Rect::from_center_size(center, egui::vec2(200.0, 80.0)),
                    |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(40.0).color(egui::Color32::WHITE));
                            ui.add_space(12.0);
                            ui.label(egui::RichText::new("ロード中...").size(18.0).color(egui::Color32::WHITE).strong());
                        });
                    },
                );

                let resp = ui.allocate_rect(screen, egui::Sense::click());
                if resp.clicked() { self.ui.boss_mode = false; }
            });
        ctx.request_repaint(); // スピナーのアニメーションを維持
    }
}
