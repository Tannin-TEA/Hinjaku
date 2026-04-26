use eframe::egui;
use crate::window;
use crate::types::WindowMode;
use super::App;

impl App {
    pub(super) fn set_window_mode(&mut self, mode: WindowMode, ctx: &egui::Context) {
        let _old_mode = self.view.window_mode;

        // 全画面以外のモードに移行する場合は、それを「直前のベースモード」として記憶する
        if mode != WindowMode::Fullscreen {
            self.view.last_base_mode = mode;
        }

        self.view.window_mode = mode;
        self.config.window_mode = mode;
        self.save_config();

        match mode {
            WindowMode::Standard => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                // 標準モードに戻る際は最大化状態を復元
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.view.is_maximized));
            }
            WindowMode::Borderless => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.view.is_maximized));
            }
            WindowMode::Fullscreen => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false)); // フルスクリーン時は最大化解除
            }
        }

        // モード切替後、target_display を維持するようウィンドウを再計算する
        if !self.view.is_maximized && mode != WindowMode::Fullscreen {
            self.apply_target_size(ctx);
        }
    }

    pub(super) fn toggle_maximized(&mut self, ctx: &egui::Context) {
        if self.view.window_mode == WindowMode::Fullscreen {
            // フルスクリーン時に Enter が押されたら、記憶していたベースモードに戻して最大化する
            let base = self.view.last_base_mode;
            self.view.is_maximized = true;
            self.set_window_mode(base, ctx);
        } else {
            // 標準またはボーダレス時は、現在のModeを維持したまま最大化のみトグル
            self.view.is_maximized = !self.view.is_maximized;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.view.is_maximized));
        }
    }

    pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        let next_mode = if self.view.window_mode == WindowMode::Fullscreen {
            // 全画面解除：記憶していた元のモード（標準 or ボーダレス）に戻す
            self.view.last_base_mode
        } else {
            // 全画面化：現在のモードを記憶しつつ全画面へ
            WindowMode::Fullscreen
        };
        self.set_window_mode(next_mode, ctx);
    }

    pub(super) fn toggle_borderless(&mut self, ctx: &egui::Context) {
        let next = if self.view.window_mode == WindowMode::Borderless { WindowMode::Standard } else { WindowMode::Borderless };
        self.set_window_mode(next, ctx);
    }

    pub(super) fn exit_to_base(&mut self, ctx: &egui::Context) {
        // Fullscreen のときだけ元のモード（Standard/Borderless）に戻す
        // Standard ↔ Borderless の切り替えは ESC では行わない
        if self.view.window_mode == WindowMode::Fullscreen {
            self.set_window_mode(self.view.last_base_mode, ctx);
        }
    }

    pub(super) fn toggle_always_on_top(&mut self, ctx: &egui::Context) {
        self.config.always_on_top = !self.config.always_on_top;
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            if self.config.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal }
        ));
        self.save_config();
    }

    pub(super) fn toggle_window_resizable(&mut self, ctx: &egui::Context) {
        self.config.window_resizable = !self.config.window_resizable;
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(self.config.window_resizable));
        self.save_config();
    }

    /// target_display_w/h を window サイズに変換してリサイズ命令を発行する
    pub(super) fn apply_target_size(&mut self, ctx: &egui::Context) {
        if self.target_display_w <= 0.0 || self.target_display_h <= 0.0 { return; }
        let (w, h) = match self.view.window_mode {
            WindowMode::Standard   => (
                self.target_display_w + self.ui_width_overhead,
                self.target_display_h + self.ui_height_overhead,
            ),
            WindowMode::Borderless => (self.target_display_w, self.target_display_h),
            WindowMode::Fullscreen => return,
        };
        window::request_resize(ctx, w as u32, h as u32);
        self.last_resize_time = ctx.input(|i| i.time);
    }

    pub(super) fn resize_window(&mut self, ctx: &egui::Context, w: u32, h: u32) {
        self.target_display_w = w as f32;
        self.target_display_h = h as f32;
        self.apply_target_size(ctx);
        // sync_config_with_window はリサイズ中をガードするため、期待する合計サイズを先に保存する
        let (tw, th) = match self.view.window_mode {
            WindowMode::Standard => (w as f32 + self.ui_width_overhead, h as f32 + self.ui_height_overhead),
            _                    => (w as f32, h as f32),
        };
        self.config.window_width  = tw;
        self.config.window_height = th;
        self.save_config();
    }
}
