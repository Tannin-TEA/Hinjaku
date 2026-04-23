use crate::{archive, integrator, window, shell, toast, config::{self, Config}, manager::{self, Manager}, utils, painter, widgets, input};
pub use crate::types::{DisplayMode, ViewState};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use crate::constants::*;

// ── UI表示フラグ・一時状態 ────────────────────────────────────────────────────
struct UiState {
    show_settings:      bool,
    show_tree:          bool,
    show_sort_settings: bool,
    show_key_config:    bool,
    show_debug:         bool,
    show_about:         bool,
    show_limiter_settings: bool,
    pdf_warning_open:   bool,
    capturing_key_for:  Option<String>,
    sort_focus_idx:     usize,
    settings_args_tmp:  Vec<String>,
    boss_mode:          bool,
    show_jump_dialog:   bool,
    jump_input:         String,
}

impl UiState {
    fn new(config: &Config) -> Self {
        Self {
            show_settings:      false,
            show_tree:          config.show_tree,
            show_sort_settings: false,
            show_key_config:    false,
            show_debug:         false,
            show_about:         false,
            show_limiter_settings: false,
            pdf_warning_open:   false,
            capturing_key_for:  None,
            sort_focus_idx:     0,
            settings_args_tmp:  config.external_apps.iter().map(|a| a.args.join(" ")).collect(),
            boss_mode:          false,
            show_jump_dialog:   false,
            jump_input:         String::new(),
        }
    }
}

// ── アプリケーション本体 ──────────────────────────────────────────────────────
pub struct App {
    manager: Manager,
    config:  Config,
    ui:      UiState,
    view:    ViewState,

    config_path:          Option<PathBuf>,
    wheel_accumulator:    f32,
    is_loading_archive:   bool,
    folder_lock_until:    f64,
    page_lock_until:      f64,
    last_title_update_time: f64,
    last_debug_log_time:  f64,
    last_resize_time:     f64,
    debug_cli:            bool,
    last_archive_path:    Option<PathBuf>,
    error:                Option<String>,
    toasts:               toast::ToastManager,
    path_tx:              std::sync::mpsc::Sender<(PathBuf, bool)>, // 内部D&DイベントをIPCチャネルに送る用
    path_rx:              Receiver<(PathBuf, bool)>, // IPCとD&Dイベントを受け取る用
    applied_initial_center: bool,
    initial_center_frame:  u8,
    pro_mode:             bool,
    is_mouse_gesture:     bool,
    scroll_offset:        egui::Vec2,
    viewport_origin:      egui::Pos2,
    pending_scroll:       Option<egui::Vec2>,
    hook_installed:       bool, // WM_COPYDATAフックがインストールされたか
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        config: Config,
        config_path: Option<PathBuf>,
        archive_reader: std::sync::Arc<dyn archive::ArchiveReader>,
        debug_cli: bool,
        pro_mode: bool,
    ) -> Self {
        // 日本語フォント
        let mut fonts = FontDefinitions::default();
        let font_candidates = ["C:\\Windows\\Fonts\\msgothic.ttc", "C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\msjh.ttc"];
        for path in &font_candidates {
            #[cfg(target_os = "windows")]
            if let Some(data) = integrator::mmap_font_file(path) {
                fonts.font_data.insert("japanese".to_owned(), FontData::from_static(data));
                if let Some(f) = fonts.families.get_mut(&FontFamily::Proportional) { f.insert(0, "japanese".to_owned()); }
                if let Some(f) = fonts.families.get_mut(&FontFamily::Monospace)    { f.insert(0, "japanese".to_owned()); }
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        cc.egui_ctx.options_mut(|opt| { opt.tessellation_options.feathering = false; });
        cc.egui_ctx.set_pixels_per_point(1.0);

        if !config.window_maximized {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(config.window_width, config.window_height)));
            if !config.window_centered {
                cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(config.window_x, config.window_y)));
            }
        }

        let mut manager = Manager::new(cc.egui_ctx.clone(), archive_reader);
        manager.open_from_end = config.open_from_end;
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
            let w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as u32;
            let h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as u32;
            manager.display_max_dim = w.max(h).max(1920);
        }

        if config.is_fullscreen {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        } else if config.is_small_borderless {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        }

        if config.always_on_top {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        }

        let (tx, rx) = integrator::setup_ipc_channels(&cc.egui_ctx);
        let ui = UiState::new(&config);
        let mut app = Self {
            manager,
            ui,
            view: ViewState {
                display_mode: config.display_mode,
                zoom: 1.0,
                manga_mode: config.manga_mode,
                manga_shift: false,
                is_maximized: config.window_maximized,
                is_fullscreen: config.is_fullscreen,
                is_small_borderless: config.is_small_borderless,
                effective_zoom: 1.0,
            },
            config_path,
            wheel_accumulator:      0.0,
            is_loading_archive:     false,
            folder_lock_until:      0.0,
            page_lock_until:        0.0,
            last_title_update_time: 0.0,
            last_debug_log_time:    0.0,
            last_resize_time:       0.0,
            debug_cli,
            last_archive_path:      None,
            error:                  None,
            toasts:                 toast::ToastManager::new(),
            path_tx:                tx, // setup_ipc_channels から受け取った Sender
            path_rx:                rx, // setup_ipc_channels から受け取った Receiver
            applied_initial_center: false,
            initial_center_frame:   0,
            pro_mode,
            config,
            is_mouse_gesture:       false,
            scroll_offset:          egui::Vec2::ZERO,
            viewport_origin:        egui::Pos2::ZERO,
            pending_scroll:         None,
            hook_installed:         false, // 初期状態ではフックはインストールされていない
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }
        app
    }

    fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        self.manager.get_first_tex(index)
    }

    fn sync_tree_to_current(&mut self) {
        if let Some(path) = self.manager.archive_path.clone() {
            let cleaned = utils::clean_path(&path);
            self.manager.tree.expand_to_path(&cleaned);
            self.manager.tree.selected = Some(cleaned);
            self.manager.tree.reveal_path(&path);
        }
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.error = None;
        let path_str = path.to_string_lossy().into_owned();
        self.config.recent_paths.retain(|p| p != &path_str); // 既存のパスを削除
        self.config.recent_paths.insert(0, path_str);
        if self.config.recent_paths.len() > 10 { self.config.recent_paths.pop(); }
        self.save_config();

        if utils::detect_kind(&path) == utils::ArchiveKind::Pdf && self.config.show_pdf_warning {
            self.ui.pdf_warning_open = true;
        }

        self.manager.open_path(path, &self.config);
        if self.manager.tree.nodes.len() > crate::constants::cache::TREE_NODES_CACHE_LIMIT {
            self.manager.tree.clear_metadata_cache();
        }
        self.sync_tree_to_current();
        self.is_loading_archive = self.manager.is_listing;
        ctx.request_repaint();
    }

    fn get_effective_filter_mode(&self) -> config::FilterMode {
        if self.pro_mode { config::FilterMode::Nearest } else { self.config.filter_mode }
    }

    fn get_effective_max_dim(&mut self, ctx: &egui::Context) -> u32 {
        let kind = self.manager.archive_path.as_ref().map(|p| utils::detect_kind(p)).unwrap_or(utils::ArchiveKind::Plain);
        if kind == utils::ArchiveKind::Pdf {
            self.config.pdf_render_dpi
        } else {
            let ppp = ctx.pixels_per_point();
            let r = ctx.screen_rect();
            let dim = ((r.width().max(r.height()) * ppp) as u32).max(1920);
            self.manager.display_max_dim = dim;
            dim
        }
    }

    fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.view.manga_mode {
            vec![self.manager.current, self.manager.current + 1]
        } else {
            vec![self.manager.current]
        };
        for idx in indices {
            if let Some(name) = self.manager.entries.get(idx).cloned() {
                let rot = self.manager.rotations.get(&name).copied().unwrap_or(manager::Rotation::R0);
                self.manager.rotations.insert(name.clone(), if cw { rot.cw() } else { rot.ccw() });
                self.manager.invalidate_cache_for(idx, &name);
            }
        }
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, ctx: &egui::Context) {
        self.error = None;
        self.manager.move_to_dir(path, focus_hint, go_last, &self.config, self.view.manga_mode, self.view.manga_shift);
        self.sync_tree_to_current();
        self.is_loading_archive = self.manager.is_listing;
        ctx.request_repaint();
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.navigate_relative_dir(false, ctx);
    }
    fn go_next_dir(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.navigate_relative_dir(true, ctx);
    }

    fn navigate_relative_dir(&mut self, forward: bool, ctx: &egui::Context) {
        if self.manager.go_relative_dir(forward, &self.config, self.view.manga_mode, self.view.manga_shift) {
            self.sync_tree_to_current();
            self.error = None;
            self.is_loading_archive = true;
            ctx.request_repaint();
        }
    }

    fn is_nav_locked(&self, ctx: &egui::Context) -> bool {
        let now = ctx.input(|i| i.time);
        if self.is_loading_archive { return true; }

        // 表示が追いついていない（current != target）間は、次の操作をロックする（非同期スキップ防止）
        if self.manager.current != self.manager.target_index {
            return true;
        }

        if now < self.folder_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.folder_lock_until - now));
            return true;
        }
        if now < self.page_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.page_lock_until - now));
            return true;
        }
        false
    }

    /// ウィンドウタイトルを更新する。
    ///
    /// ⚠️ ガード条件（archive_path と last_title_update_time）を変更すると
    /// 毎フレームタイトル更新が走る repaint ループが発生する可能性がある。
    /// 過去に last_target_index を条件に加えたことでループが発生した経緯がある (0.1.2)。
    /// 条件を追加・変更する前にユーザーへ確認すること。
    fn update_title(&mut self, ctx: &egui::Context) {
        if self.ui.boss_mode { return; } // boss_mode 中は上書きしない
        let now = ctx.input(|i| i.time);
        // アーカイブが変わった、または2秒経過（メモリ更新用）のいずれかで更新
        if self.manager.archive_path == self.last_archive_path && now - self.last_title_update_time <= 2.0 { return; }

        self.last_archive_path = self.manager.archive_path.clone();
        self.last_title_update_time = now;

        let renderer_str = match self.config.renderer {
            config::RendererMode::Glow => "OpenGL",
            config::RendererMode::Wgpu => "Wgpu",
        };
        let pro_part = if self.pro_mode { "ProMode - " } else { "" };
        let config_part = self.config_path.as_ref()
            .and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
            .filter(|n| n != "config.ini").map(|n| format!(" {{{}}}", n)).unwrap_or_default();

        let container_name = self.manager.archive_path.as_ref()
            .map(|p| format!(" [{}]", utils::get_display_name(p))).unwrap_or_default();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            format!("Hinjaku - {}{}{}{} ({})", pro_part, renderer_str, config_part, container_name, integrator::get_memory_usage_str())
        ));
    }

    // ── ページ移動ヘルパー ────────────────────────────────────────────────────

    fn prepare_nav(&mut self) {
        self.reset_nav_locks();
        // Fit/WinFit の時は倍率をデフォルト(1.0)に戻す。等倍(Manual)の時は維持する。
        if self.view.display_mode != DisplayMode::Manual {
            self.view.zoom = 1.0;
        }
    }

    fn reset_nav_locks(&mut self) {
        self.folder_lock_until = 0.0;
        self.page_lock_until = 0.0;
    }

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_prev(false, false, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_start { return; }
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_next(false, false, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_end { return; }
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_prev(self.view.manga_mode, self.view.manga_shift, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_start { return; }
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_next(self.view.manga_mode, self.view.manga_shift, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_end { return; }
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        self.manager.target_index = 0;
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.view.manga_mode && last > 0 && last % 2 == 0 {
            last.saturating_sub(1)
        } else { last };
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    fn seek(&mut self, idx: usize, ctx: &egui::Context) {
        // 描写が追いついていない（current != target）間は、新しい移動リクエストを完全に無視する
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        self.manager.target_index = idx;
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    // ── 表示・ズームヘルパー ──────────────────────────────────────────────────

    fn zoom_in(&mut self) {
        self.view.zoom = (self.view.zoom * ui::ZOOM_STEP).min(ui::MAX_ZOOM);
    }

    fn zoom_out(&mut self) {
        self.view.zoom = (self.view.zoom / ui::ZOOM_STEP).max(ui::MIN_ZOOM);
    }

    fn zoom_reset(&mut self) {
        self.view.zoom = 1.0;
    }

    fn set_display_mode(&mut self, m: DisplayMode) {
        self.view.display_mode = m;
        self.config.display_mode = m;
        self.save_config();
        if m == DisplayMode::Manual { self.view.zoom = 1.0; }
    }

    fn toggle_manga(&mut self, ctx: &egui::Context) {
        self.view.manga_mode = !self.view.manga_mode;
        self.config.manga_mode = self.view.manga_mode;
        self.save_config();
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    fn toggle_filter_mode(&mut self, ctx: &egui::Context) {
        self.config.filter_mode = match self.config.filter_mode {
            config::FilterMode::Nearest  => config::FilterMode::Bilinear,
            config::FilterMode::Bilinear => config::FilterMode::Bicubic,
            config::FilterMode::Bicubic  => config::FilterMode::Lanczos,
            config::FilterMode::Lanczos  => config::FilterMode::Nearest,
        };
        self.manager.clear_cache();
        self.save_config();
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    // ── ウィンドウヘルパー ────────────────────────────────────────────────────

    fn toggle_maximized(&mut self, ctx: &egui::Context) {
        self.view.is_maximized = !self.view.is_maximized;
        self.view.is_fullscreen = false;
        self.view.is_small_borderless = false;
        
        self.config.is_fullscreen = false;
        self.config.is_small_borderless = false;
        self.save_config();

        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.view.is_maximized));
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.view.is_fullscreen = !self.view.is_fullscreen;
        self.view.is_maximized = false;
        self.view.is_small_borderless = false;

        self.config.is_fullscreen = self.view.is_fullscreen;
        self.config.is_small_borderless = false;
        self.save_config();

        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.view.is_fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.view.is_fullscreen));
    }

    fn toggle_small_borderless(&mut self, ctx: &egui::Context) {
        self.view.is_small_borderless = !self.view.is_small_borderless;
        self.view.is_fullscreen = false;

        self.config.is_small_borderless = self.view.is_small_borderless;
        self.config.is_fullscreen = false;
        self.save_config();

        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.view.is_small_borderless));
    }

    fn exit_fullscreen(&mut self, ctx: &egui::Context) {
        self.view.is_fullscreen = false;
        self.view.is_small_borderless = false;

        self.config.is_fullscreen = false;
        self.config.is_small_borderless = false;
        self.save_config();

        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
    }

    fn toggle_always_on_top(&mut self, ctx: &egui::Context) {
        self.config.always_on_top = !self.config.always_on_top;
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            if self.config.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal }
        ));
        self.save_config();
    }

    fn toggle_window_resizable(&mut self, ctx: &egui::Context) {
        self.config.window_resizable = !self.config.window_resizable;
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(self.config.window_resizable));
        self.save_config();
    }

    fn resize_window(&mut self, ctx: &egui::Context, w: u32, h: u32) {
        window::request_resize(ctx, w, h);
        self.config.window_width = w as f32;
        self.config.window_height = h as f32;
        self.last_resize_time = ctx.input(|i| i.time);
        self.save_config();
    }

    // ── その他ヘルパー ────────────────────────────────────────────────────────

    fn add_toast(&mut self, msg: String, ctx: &egui::Context) {
        self.toasts.add(msg, ctx);
    }

    fn open_external(&mut self, index: usize, ctx: &egui::Context) {
        let app = &self.config.external_apps[index];
        if let Err(e) = shell::open_external(&self.manager, app) {
            self.add_toast(e, ctx);
        } else if app.close_after_launch {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save_config(&self) {
        if let Some(ref path) = self.config_path {
            if let Err(e) = config::save_config_file(&self.config, path) {
                log::error!("設定の保存に失敗しました: {}", e);
            }
        }
    }

    // ── 入力処理 ─────────────────────────────────────────────────────────────

    fn handle_input(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
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
                if k.fullscreen { self.toggle_maximized(ctx); }
                if k.borderless { self.toggle_fullscreen(ctx); }
                if k.small_borderless { self.toggle_small_borderless(ctx); }
                self.handle_viewer_keys(ctx, k);
            }
        }
        if !modal_open && k.esc { self.exit_fullscreen(ctx); }

        // イースターエッグ: Ctrl+Shift+F12 固定（キーコンフィグ非公開）
        if ctx.input(|i| i.key_pressed(egui::Key::F12) && i.modifiers.ctrl && i.modifiers.shift) {
            self.ui.boss_mode = !self.ui.boss_mode;
            if self.ui.boss_mode {
                let renderer_str = match self.config.renderer {
                    config::RendererMode::Glow => "OpenGL",
                    config::RendererMode::Wgpu => "Wgpu",
                };
                let config_part = self.config_path.as_ref()
                    .and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
                    .filter(|n| n != "config.ini").map(|n| format!(" {{{}}}", n)).unwrap_or_default();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    format!("Hinjaku - {}{} [Image-Folder] ({})", renderer_str, config_part, integrator::get_memory_usage_str())
                ));
            } else {
                self.last_title_update_time = 0.0; // 次フレームで本来のタイトルに戻す
            }
        }
    }

    fn handle_tree_navigation(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
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

    fn handle_viewer_keys(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
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

    fn handle_mouse_input(&mut self, ctx: &egui::Context) {
        self.pending_scroll = None;

        // ポップアップメニュー（右クリックメニュー等）が開いている時だけ入力をガードする
        // wants_pointer_input() は画像ウィジェット自体も含まれるため、ここでは判定しない
        if ctx.memory(|m| m.any_popup_open()) {
            return;
        }

        // 小画面ボーダレス時、Alt + 左ドラッグでウィンドウを移動できるようにする
        // 単なる左クリック（ページ送り）と衝突しないよう、Altキーを必須にする
        if self.view.is_small_borderless && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary) && i.modifiers.alt) {
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

        // クリックによるページ送り（UI操作中ではない場合のみ）
        let (p_clicked, s_clicked) = ctx.input(|i| (
            i.pointer.button_clicked(egui::PointerButton::Primary),
            i.pointer.button_clicked(egui::PointerButton::Secondary),
        ));
        if p_clicked { self.go_next(ctx); }
        if s_clicked && !self.is_mouse_gesture { self.go_prev(ctx); }

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

    fn execute_mouse_action(&mut self, action_name: &str, ctx: &egui::Context) {
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
            "ToggleManga" => self.toggle_manga(ctx),
            _ => {}
        }
    }

    // ── 内部状態の更新 ────────────────────────────────────────────────────────

    fn handle_debug_logging(&mut self, ctx: &egui::Context) {
        if !self.debug_cli { return; }
        let now = ctx.input(|i| i.time);
        if now - self.last_debug_log_time > 1.0 {
            println!("\n--- Debug Stats ({:.1}s) ---", now);
            println!("Memory: {}", integrator::get_memory_usage_str());
            println!("Cache: {} items ({} KB)", self.manager.cache_len(), self.manager.total_cache_size_bytes() / 1024);
            self.last_debug_log_time = now;
        }
    }

    fn process_manager_update(&mut self, ctx: &egui::Context) {
        let (failures, list_error) = self.manager.update(ctx, &self.config, self.view.manga_mode, self.view.manga_shift);
        
        if let Some(err) = list_error {
            self.add_toast(err, ctx);
            self.is_loading_archive = false;
        }

        for (idx, err) in failures {
            if idx == self.manager.target_index || (self.view.manga_mode && idx == self.manager.target_index + 1) {
                self.error = Some(err);
                self.is_loading_archive = false;
            }
        }
        self.sync_display_to_target(ctx);
        if self.error.is_some() || (self.manager.entries.is_empty() && !self.manager.is_listing) {
            self.is_loading_archive = false;
        }
    }

    /// テクスチャの準備が整い次第 current を target に追いつかせる。
    ///
    /// ⚠️ ここは描写タイミング制御のコア。以下を変更するとロック/フリッカーの原因になる：
    /// - is_ready の判定条件（マンガモードのペアリング判断を含む）
    /// - folder_lock_until / page_lock_until のセットタイミング
    /// - ctx.request_repaint() の呼び出し位置
    /// 変更前にユーザーへ確認すること。
    fn sync_display_to_target(&mut self, ctx: &egui::Context) {
        let target = self.manager.target_index;
        if self.is_loading_archive || self.manager.current != target {
            let mut is_ready = false;
            if let Some(tex) = self.get_texture(target) {
                if self.view.manga_mode {
                    // マンガモードの場合、ペアリングの必要があるか判定
                    let tex_size = tex.size_vec2();
                    let is_portrait = tex_size.x <= tex_size.y;
                    let can_pair = (self.view.manga_shift || target > 0) && is_portrait;
                    let has_next = target + 1 < self.manager.entries.len();

                    if can_pair && has_next {
                        // 2枚目が必要な構成なので、2枚目が準備できるまで待つ
                        if self.get_texture(target + 1).is_some() {
                            is_ready = true;
                        }
                    } else {
                        // 1枚で完結する表示（表紙やスプレッド画像）
                        is_ready = true;
                    }
                } else {
                    // 通常モードは1枚あればOK
                    is_ready = true;
                }
            }
            if is_ready {
                let was_loading = self.is_loading_archive;
                self.manager.current = target;
                self.error = None;
                self.is_loading_archive = false;
                let now = ctx.input(|i| i.time);
                let (p_dur, f_dur) = if self.config.limiter_mode {
                    (self.config.limiter_page_duration as f64, self.config.limiter_folder_duration as f64)
                } else {
                    (ui::PAGE_NAV_GUARD_DURATION, ui::FOLDER_NAV_GUARD_DURATION)
                };
                self.folder_lock_until = if was_loading { now + f_dur } else { 0.0 };
                self.page_lock_until   = if !was_loading { now + p_dur } else { 0.0 };
                ctx.request_repaint(); // ターゲットが確定した瞬間に次のフレームを描画
            }
        }
    }

    // ── 描画 ─────────────────────────────────────────────────────────────────

    fn draw_ui(&mut self, ctx: &egui::Context) {
        self.draw_windows(ctx);

        let mut menu_act = None;
        let mut tool_act = None;

        if self.view.is_fullscreen || self.view.is_small_borderless {
            // ボーダレスモード：マウスホバーでオーバーレイ表示
            let mouse_pos = ctx.input(|i| i.pointer.hover_pos());
            let screen_rect = ctx.screen_rect();

            let in_menu_zone   = mouse_pos.map_or(false, |p| p.y < 40.0);
            let in_status_zone = mouse_pos.map_or(false, |p| p.y > screen_rect.height() - 40.0);

            // 前フレームのレイヤー情報からマウスがオーバーレイ（ドロップダウン含む）上にいるか検出
            let mouse_over_overlay = mouse_pos.map_or(false, |p| {
                ctx.layer_id_at(p).map_or(false, |id| id.order == egui::Order::Foreground)
            });

            // メニューとステータスバーはどちらかの条件が満たされたら両方表示
            let show_overlay = in_menu_zone || in_status_zone || mouse_over_overlay;

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
                            let (act, _) = widgets::main_menu_bar_inner(ui, &self.config, &self.manager, &self.view, self.ui.show_tree, self.ui.show_debug);
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
            let (act, _) = widgets::main_menu_bar(ctx, &self.config, &self.manager, &self.view, self.ui.show_tree, self.ui.show_debug);
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
    }

    fn draw_windows(&mut self, ctx: &egui::Context) {
        if self.ui.show_settings {
            if widgets::settings_window(ctx, &mut self.ui.show_settings, &mut self.config, &mut self.ui.settings_args_tmp) {
                self.save_config();
            }
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
        if self.ui.show_limiter_settings {
            if widgets::limiter_settings_window(ctx, &mut self.ui.show_limiter_settings, &mut self.config) {
                self.save_config();
            }
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

    fn draw_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            painter::paint_background(ui, ui.available_rect_before_wrap(), self.config.bg_mode);
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
            let (_, act, eff_zoom, scroll_off, vp_origin) = painter::draw_main_area(ui, &self.manager, &self.view, self.config.manga_rtl, ctx, is_at_end, sec_down, self.pending_scroll);
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

    fn draw_loading_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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

    fn draw_boss_mode(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("boss_mode"))
            .order(egui::Order::TOP)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                // 全画面を半透明の黒で塗りつぶす
                let painter = ui.painter();
                painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(200));

                // 中央にスピナーとテキスト
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

                // クリックで解除
                let resp = ui.allocate_rect(screen, egui::Sense::click());
                if resp.clicked() { self.ui.boss_mode = false; }
            });
        ctx.request_repaint(); // スピナーのアニメーションを維持
    }

    // ── アクションディスパッチ ────────────────────────────────────────────────

    fn handle_action(&mut self, ctx: &egui::Context, act: widgets::ViewerAction) {
        use widgets::ViewerAction::*;
        match act {
            // ナビゲーション
            PrevPage          => self.go_prev(ctx),
            NextPage          => self.go_next(ctx),
            NextDir           => self.go_next_dir(ctx),
            GoPrevDir         => self.go_prev_dir(ctx),
            GoNextDir         => self.go_next_dir(ctx),
            Seek(idx)         => self.seek(idx, ctx),
            SetOpenFromEnd(b) => { self.config.open_from_end = b; self.manager.open_from_end = b; self.save_config(); }

            // 表示・ズーム
            SetDisplayMode(m) => self.set_display_mode(m),
            ZoomIn            => self.zoom_in(),
            ZoomOut           => self.zoom_out(),
            ZoomReset         => self.zoom_reset(),
            ToggleManga       => self.toggle_manga(ctx),
            ToggleMangaRtl    => { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); }
            ToggleLinear      => self.toggle_filter_mode(ctx),
            Rotate(cw)        => self.rotate_current(cw, ctx),
            SetBgMode(m)      => { self.config.bg_mode = m; self.save_config(); }

            // ウィンドウ
            ToggleAlwaysOnTop     => self.toggle_always_on_top(ctx),
            ToggleWindowResizable => self.toggle_window_resizable(ctx),
            ToggleWindowCentered  => {
                self.config.window_centered = !self.config.window_centered;
                if self.config.window_centered { self.applied_initial_center = false; }
                self.save_config();
            }
            ResizeWindow(w, h) => self.resize_window(ctx, w, h),
            MoveToCenter       => { window::move_to_center(ctx, self.config.window_width, self.config.window_height); },

            // ファイル・システム
            OpenRecent(p) => {
                let path = PathBuf::from(&p);
                if path.exists() { self.open_path(path, ctx); }
                else { self.add_toast("対象のパスが見つかりません。".to_string(), ctx); }
            }
            OpenFolder => {
                if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); }
            }
            RevealInExplorer => {
                if let Err(e) = shell::reveal_current_in_explorer(&self.manager) { self.add_toast(e, ctx); }
            }
            OpenExternal(idx)    => self.open_external(idx, ctx),
            OpenExternalSettings => {
                self.ui.settings_args_tmp = self.config.external_apps.iter().map(|a| a.args.join(" ")).collect();
                self.ui.show_settings = true;
            }

            // 設定・その他
            OpenKeyConfig   => self.ui.show_key_config = true,
            OpenSortSettings => { self.ui.show_sort_settings = true; self.ui.sort_focus_idx = 0; }
            ToggleMultipleInstances => { self.config.allow_multiple_instances = !self.config.allow_multiple_instances; self.save_config(); }
            ToggleDebug     => self.ui.show_debug = !self.ui.show_debug,
            About           => self.ui.show_about = true,
            SetRenderer(m)  => { self.config.renderer = m; self.save_config(); self.add_toast("設定を反映するには再起動が必要です。".to_string(), ctx); }
            SetMouseAction(btn, act) => {
                match btn {
                    3 => self.config.mouse_middle_action = act,
                    4 => self.config.mouse4_action = act,
                    _ => self.config.mouse5_action = act,
                }
                self.save_config();
            }
            ToggleLimiterMode => {
                self.config.limiter_mode = !self.config.limiter_mode;
                let msg = if self.config.limiter_mode { "リミッターモード: ON" } else { "通常モード" };
                self.add_toast(msg.to_string(), ctx);
                self.save_config();
            }
            SetPdfRenderSize(s) => {
                self.config.pdf_render_dpi = s;
                self.save_config();
                self.manager.clear_cache();
                let max_dim = self.get_effective_max_dim(ctx);
                self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
            }
            TogglePdfWarning => { self.config.show_pdf_warning = !self.config.show_pdf_warning; self.save_config(); }
            OpenLimiterSettings => {
                self.ui.show_limiter_settings = true;
            }
            SetLimiterPageDuration(d) => { self.config.limiter_page_duration = d; self.save_config(); }
            SetLimiterFolderDuration(d) => { self.config.limiter_folder_duration = d; self.save_config(); }
            ToggleFullscreen => self.toggle_maximized(ctx),
            ToggleBorderless => self.toggle_fullscreen(ctx),
            ToggleSmallBorderless => self.toggle_small_borderless(ctx),
            Exit => { self.save_config(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            ToggleTree => {
                self.ui.show_tree = !self.ui.show_tree;
                if self.ui.show_tree { self.sync_tree_to_current(); }
                self.config.show_tree = self.ui.show_tree;
                self.save_config();
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. WM_COPYDATA フックの遅延インストール
        // ウィンドウハンドルが確定するまで try_install_hook を呼び続ける
        if !self.hook_installed {
            self.hook_installed = integrator::try_install_hook();
        }

        // 2. 外部（二重起動）および D&D からのメッセージを安全に一括処理
        let mut path_to_open = None;
        let mut should_focus = false;

        // IPCチャネルからパスを受信
        while let Ok(req) = self.path_rx.try_recv() {
            let (path, is_external) = req;
            path_to_open = Some(path);
            if is_external { should_focus = true; }
        }

        // D&D の検知：IPCメッセージがない場合のみ、現在のフレームのドロップを確認
        // 複数のドロップファイルがある場合、最初の1つだけを処理する
        if path_to_open.is_none() {
            if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone())) {
                path_to_open = Some(path);
            }
        }

        // 通信イベントが終わった後の「この場所」で初めて重い処理を実行する
        if let Some(path) = path_to_open {
            // ロード中でなく、かつ新しいパスである場合のみ実行（重複処理によるスタック破壊を防止）
            let is_new = !self.is_loading_archive && self.manager.archive_path.as_ref().map_or(true, |p| p != &path);
            if is_new {
                if should_focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                }
                self.open_path(path, ctx);
            }
        }

        // 3. 起動時の中央配置
        // 成功するまで EnumWindows が走り続けるのを防ぐため、回数制限を設ける
        if self.config.window_centered && !self.applied_initial_center {
            self.initial_center_frame = self.initial_center_frame.saturating_add(1);
            if self.initial_center_frame >= 3 && self.initial_center_frame < 10 { // 10フレームまで試行
                if window::move_to_center(ctx, self.config.window_width, self.config.window_height) {
                    self.applied_initial_center = true;
                }
            }
        }

        // 4. ウィンドウクローズ要求時の設定保存
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_config();
        }

        // 5. 内部状態の更新
        self.update_title(ctx);
        window::sync_config_with_window(ctx, &mut self.config, self.last_resize_time);
        self.view.is_maximized = self.config.window_maximized;
        self.handle_debug_logging(ctx);
        self.process_manager_update(ctx);

        // 6. 入力処理
        let k = input::gather_input(ctx, &self.config);
        self.handle_input(ctx, &k);

        // 7. 描画
        self.draw_ui(ctx);

        // 8. マウス入力（draw_ui 後に処理: 描画でポップアップが開いた同フレームも正しくガードできる）
        {
            let is_typing    = ctx.wants_keyboard_input();
            let is_capturing = self.ui.capturing_key_for.is_some();
            let modal_open   = self.ui.show_settings || self.ui.show_key_config
                            || self.config.is_first_run || self.ui.show_sort_settings || self.ui.show_limiter_settings;
            if !modal_open && !is_typing && !is_capturing && !self.ui.show_tree {
                self.handle_mouse_input(ctx);
            }
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_config();
    }
}
