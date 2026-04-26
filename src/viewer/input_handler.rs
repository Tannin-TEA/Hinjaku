use eframe::egui;
use crate::{input, widgets, shell, startup, integrator, config};
use crate::types::{DisplayMode, WindowMode};
use crate::constants::*;
use super::App;

impl App {
    pub(super) fn handle_input(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        let is_typing    = ctx.wants_keyboard_input();
        let is_capturing = self.ui.capturing_key_for.is_some();
        let modal_open   = self.ui.show_settings || self.ui.show_key_config
                        || self.config.is_first_run || self.ui.show_sort_settings || self.ui.show_limiter_settings
                        || self.ui.show_jump_dialog;

        // モーダル表示中であっても、トグルキーによるクローズ処理はここで行う
        if !is_typing && !is_capturing && self.ui.show_sort_settings && k.sort_settings {
            self.manager.apply_sorting(&self.config);
            self.manager.clear_cache();
            let max_dim = self.get_effective_max_dim(ctx);
            self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
            self.save_config();
            self.ui.show_sort_settings = false;
        }

        if !modal_open && !is_typing && !is_capturing {
            // グローバルアクション (ツリー表示中でも常に有効)
            if k.quit { self.save_config(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }

            if self.ui.show_tree {
                self.handle_tree_navigation(ctx, k);
            } else {
                if k.toggle_maximized { self.toggle_maximized(ctx); }
                if k.toggle_fullscreen { self.toggle_fullscreen(ctx); }
                if k.toggle_borderless { self.toggle_borderless(ctx); }
                self.handle_viewer_keys(ctx, k);
            }
        }
        if !modal_open && k.esc { self.exit_to_base(ctx); }

        // イースターエッグ: Ctrl+Shift+F12 固定（キーコンフィグ非公開）
        if ctx.input(|i| i.key_pressed(egui::Key::F12) && i.modifiers.ctrl && i.modifiers.shift) {
            self.ui.boss_mode = !self.ui.boss_mode;
            if self.ui.boss_mode {
                let config_name = self.config_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy());
                let mut title = startup::build_window_title(config_name.as_deref(), false, Some("Image-Folder"));
                title.push_str(&format!(" ({})", integrator::get_memory_usage_str()));
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
            } else {
                self.last_title_update_time = 0.0; // 次フレームで本来のタイトルに戻す
            }
        }
    }

    pub(super) fn handle_tree_navigation(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        let old = self.manager.tree.selected.clone();
        if k.up    { self.manager.tree.move_selection(-1); }
        if k.dn    { self.manager.tree.move_selection(1); }
        if k.right { self.manager.tree.expand_current(); }
        if k.left  { self.manager.tree.collapse_or_up(); }
        if self.manager.tree.selected != old {
            if let Some(p) = self.manager.tree.selected.clone() { self.open_path(p, ctx); }
        }
        if k.enter {
            if let Some(p) = self.manager.tree.activate_current() {
                let has = self.manager.tree.get_image_count(&p) > 0;
                self.open_path(p, ctx);
                if has {
                    self.ui.show_tree = false;
                    self.config.show_tree = false;
                    self.save_config();
                }
            }
        }
        if k.esc         { self.ui.show_tree = false; self.config.show_tree = false; self.save_config(); }
        if k.toggle_tree { self.ui.show_tree = false; self.config.show_tree = false; self.save_config(); }
    }

    pub(super) fn handle_viewer_keys(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        if k.prev_page        { self.go_prev(ctx); }
        if k.next_page        { self.go_next(ctx); }
        if k.prev_page_single { self.go_single_prev(ctx); }
        if k.next_page_single { self.go_single_next(ctx); }
        if k.first_page       { self.go_first(ctx); }
        if k.last_page        { self.go_last(ctx); }
        if k.prev_dir         { self.go_prev_dir(ctx); }
        if k.next_dir         { self.go_next_dir(ctx); }
        if k.zoom_in          { self.zoom_in(); }
        if k.zoom_out         { self.zoom_out(); }
        if k.zoom_reset       { self.zoom_reset(); }
        if k.rcw              { self.rotate_current(true, ctx); }
        if k.rccw             { self.rotate_current(false, ctx); }
        if k.toggle_manga     { self.toggle_manga(ctx); }
        if k.toggle_rtl       { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); }
        if k.toggle_linear    { self.toggle_filter_mode(ctx); }
        if k.toggle_debug     { self.ui.show_debug = !self.ui.show_debug; }
        if k.jump_page        { self.ui.show_jump_dialog = true; self.ui.jump_input.clear(); }
        if k.open_key_config  { self.ui.show_key_config = true; }
        for (i, &pressed) in k.open_external.iter().enumerate() {
            if pressed { self.open_external(i, ctx); }
        }
        if k.bs {
            if let Err(e) = shell::reveal_current_in_explorer(&self.manager) { self.add_toast(e, ctx); }
        }
        if k.toggle_fit {
            let next = match self.view.display_mode {
                DisplayMode::Fit       => DisplayMode::WindowFit,
                DisplayMode::WindowFit => DisplayMode::Manual,
                DisplayMode::Manual    => DisplayMode::Fit,
            };
            self.set_display_mode(next);
        }
        if k.toggle_bg {
            let next = match self.config.bg_mode {
                config::BackgroundMode::Theme        => config::BackgroundMode::Checkerboard,
                config::BackgroundMode::Checkerboard => config::BackgroundMode::Black,
                config::BackgroundMode::Black        => config::BackgroundMode::Gray,
                config::BackgroundMode::Gray         => config::BackgroundMode::White,
                config::BackgroundMode::White        => config::BackgroundMode::Green,
                config::BackgroundMode::Green        => config::BackgroundMode::Theme,
            };
            self.config.bg_mode = next;
            self.save_config();
        }
        if k.sort_settings {
            self.ui.show_sort_settings = true;
            self.ui.sort_focus_idx = 0;
        }
        if k.toggle_limiter {
            self.handle_action(ctx, widgets::ViewerAction::ToggleLimiterMode);
        }
        if k.toggle_tree {
            self.ui.show_tree = !self.ui.show_tree;
            if self.ui.show_tree { self.sync_tree_to_current(); }
            self.config.show_tree = self.ui.show_tree;
            self.save_config();
        }
    }

    pub(super) fn handle_mouse_input(&mut self, ctx: &egui::Context) {
        self.pending_scroll = None;

        // ポップアップメニュー（右クリックメニュー等）が開いている時だけ入力をガードする
        // wants_pointer_input() は画像ウィジェット自体も含まれるため、ここでは判定しない
        if ctx.memory(|m| m.any_popup_open()) {
            return;
        }

        // マウスが画像表示エリア（Backgroundレイヤー）以外にある場合は入力を無視する。
        // 各パネルに .sense(click()) を追加したことで、パネル上では Background 以外のレイヤーが返るようになる。
        if let Some(hover_pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if let Some(layer) = ctx.layer_id_at(hover_pos) {
                if layer.order != egui::Order::Background {
                    return;
                }
            }
        }

        // 小画面ボーダレス時、Alt + 左ドラッグでウィンドウを移動できるようにする
        // 単なる左クリック（ページ送り）と衝突しないよう、Altキーを必須にする
        if self.view.window_mode == WindowMode::Borderless && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary) && i.modifiers.alt) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            return; // 移動中は他の操作をさせない
        }

        let (wheel, secondary_down, primary_down) = ctx.input(|i| (
            i.smooth_scroll_delta.y,
            i.pointer.button_down(egui::PointerButton::Secondary),
            i.pointer.button_down(egui::PointerButton::Primary),
        ));

        if wheel != 0.0 {
            if secondary_down {
                self.is_mouse_gesture = true;
                let old_zoom = self.view.zoom;
                let new_zoom = (old_zoom * (1.0 + wheel * ui::WHEEL_ZOOM_SENSITIVITY)).clamp(ui::MIN_ZOOM, ui::MAX_ZOOM);
                self.view.zoom = new_zoom;
                let ratio = new_zoom / old_zoom;
                if let Some(mouse_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let rel = mouse_pos - self.viewport_origin;
                    let x = (rel.x + self.scroll_offset.x) * ratio - rel.x;
                    let y = (rel.y + self.scroll_offset.y) * ratio - rel.y;
                    self.pending_scroll = Some(egui::vec2(x.max(0.0), y.max(0.0)));
                }
            } else if !primary_down {
                self.wheel_accumulator += wheel;
                if self.wheel_accumulator.abs() >= ui::WHEEL_NAV_THRESHOLD {
                    if self.wheel_accumulator > 0.0 { self.go_prev(ctx); } else { self.go_next(ctx); }
                    self.wheel_accumulator = 0.0;
                }
            }
        } else {
            self.wheel_accumulator = 0.0;
        }

        // 右ボタンが離されたらジェスチャー状態をリセットする
        if !secondary_down {
            self.is_mouse_gesture = false;
        }

        let (extra1, extra2, middle) = ctx.input(|i| (
            i.pointer.button_pressed(egui::PointerButton::Extra1),
            i.pointer.button_pressed(egui::PointerButton::Extra2),
            i.pointer.button_pressed(egui::PointerButton::Middle),
        ));
        if extra1  { let act = self.config.mouse4_action.clone();        self.execute_mouse_action(&act, ctx); }
        if extra2  { let act = self.config.mouse5_action.clone();        self.execute_mouse_action(&act, ctx); }
        if middle  { let act = self.config.mouse_middle_action.clone();  self.execute_mouse_action(&act, ctx); }
    }

    pub(super) fn execute_mouse_action(&mut self, action_name: &str, ctx: &egui::Context) {
        match action_name {
            "PrevPage"       => self.go_prev(ctx),
            "NextPage"       => self.go_next(ctx),
            "PrevPageSingle" => self.go_single_prev(ctx),
            "NextPageSingle" => self.go_single_next(ctx),
            "PrevDir"        => self.go_prev_dir(ctx),
            "NextDir"        => self.go_next_dir(ctx),
            "ToggleFit"      => {
                let next = match self.view.display_mode {
                    DisplayMode::Fit       => DisplayMode::WindowFit,
                    DisplayMode::WindowFit => DisplayMode::Manual,
                    DisplayMode::Manual    => DisplayMode::Fit,
                };
                self.set_display_mode(next);
            }
            "ToggleManga"    => self.toggle_manga(ctx),
            "ToggleMangaRtl" => { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); ctx.request_repaint(); },
            "WindSizeLock"   => self.toggle_window_resizable(ctx),
            _ => {}
        }
    }
}
