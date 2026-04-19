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
    capturing_key_for:  Option<String>,
    sort_focus_idx:     usize,
    settings_args_tmp:  Vec<String>,
}

impl UiState {
    fn new(config: &Config) -> Self {
        Self {
            show_settings:      false,
            show_tree:          false,
            show_sort_settings: false,
            show_key_config:    false,
            show_debug:         false,
            show_about:         false,
            capturing_key_for:  None,
            sort_focus_idx:     0,
            settings_args_tmp:  config.external_apps.iter().map(|a| a.args.join(" ")).collect(),
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
    last_target_index:    usize,
    last_archive_path:    Option<PathBuf>,
    error:                Option<String>,
    toasts:               toast::ToastManager,
    path_rx:              Receiver<PathBuf>,
    applied_initial_center: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        config: Config,
        config_path: Option<PathBuf>,
        archive_reader: std::sync::Arc<dyn archive::ArchiveReader>,
        window_title: &str,
        debug_cli: bool,
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
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(config.window_x, config.window_y)));
        }

        let mut manager = Manager::new(cc.egui_ctx.clone(), archive_reader);
        manager.open_from_end = config.open_from_end;

        if config.always_on_top {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        }

        let ui = UiState::new(&config);
        let mut app = Self {
            manager,
            ui,
            view:                   ViewState::new(),
            config_path,
            wheel_accumulator:      0.0,
            is_loading_archive:     false,
            folder_lock_until:      0.0,
            page_lock_until:        0.0,
            last_title_update_time: 0.0,
            last_debug_log_time:    0.0,
            last_resize_time:       0.0,
            debug_cli,
            last_target_index:      0,
            last_archive_path:      None,
            error:                  None,
            toasts:                 toast::ToastManager::new(),
            path_rx:                integrator::install_message_hook(&cc.egui_ctx, window_title),
            applied_initial_center: false,
            config,
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
        self.config.recent_paths.retain(|p| p != &path_str);
        self.config.recent_paths.insert(0, path_str);
        if self.config.recent_paths.len() > 10 { self.config.recent_paths.pop(); }
        self.save_config();

        self.manager.open_path(path, &self.config);
        if self.manager.tree.nodes.len() > crate::constants::cache::TREE_NODES_CACHE_LIMIT {
            self.manager.tree.clear_metadata_cache();
        }
        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        ctx.request_repaint();
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
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, ctx: &egui::Context) {
        self.error = None;
        self.manager.move_to_dir(path, focus_hint, go_last, &self.config, self.view.manga_mode, self.view.manga_shift);
        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        ctx.request_repaint();
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) { self.navigate_relative_dir(false, ctx); }
    fn go_next_dir(&mut self, ctx: &egui::Context) { self.navigate_relative_dir(true, ctx); }

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
        if self.manager.current != self.manager.target_index { return true; }
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

    fn update_title(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.manager.archive_path == self.last_archive_path && now - self.last_title_update_time <= 2.0 { return; }
        self.last_archive_path = self.manager.archive_path.clone();
        self.last_title_update_time = now;

        let renderer_str = match self.config.renderer {
            config::RendererMode::Glow => "OpenGL",
            config::RendererMode::Wgpu => "Wgpu",
        };
        let config_part = self.config_path.as_ref()
            .and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
            .filter(|n| n != "config.ini").map(|n| format!(" {{{}}}", n)).unwrap_or_default();
        
        let current_file = self.manager.entries.get(self.manager.target_index)
            .map(|s| format!("{} - ", s))
            .unwrap_or_default();

        let container_name = self.manager.archive_path.as_ref()
            .map(|p| utils::get_display_name(p)).unwrap_or_else(|| "Hinjaku".to_string());

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            format!("{}{}{} [{}] ({})", current_file, container_name, config_part, renderer_str, integrator::get_memory_usage_str())
        ));
    }

    // ── ページ移動ヘルパー ────────────────────────────────────────────────────

    fn reset_nav_locks(&mut self) {
        self.folder_lock_until = 0.0;
        self.page_lock_until = 0.0;
    }

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        if !self.manager.go_prev(false, false, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        if !self.manager.go_next(false, false, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        if !self.manager.go_prev(self.view.manga_mode, self.view.manga_shift, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        if !self.manager.go_next(self.view.manga_mode, self.view.manga_shift, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        self.manager.target_index = 0;
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.reset_nav_locks();
        self.view.display_mode = DisplayMode::Fit;
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.view.manga_mode && last > 0 && last % 2 == 0 {
            last.saturating_sub(1)
        } else { last };
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    fn seek(&mut self, idx: usize, ctx: &egui::Context) {
        self.manager.target_index = idx;
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    // ── 表示・ズームヘルパー ──────────────────────────────────────────────────

    fn zoom_in(&mut self) {
        self.view.zoom = (self.view.zoom * ui::ZOOM_STEP).min(ui::MAX_ZOOM);
        self.view.display_mode = DisplayMode::Manual;
    }

    fn zoom_out(&mut self) {
        self.view.zoom = (self.view.zoom / ui::ZOOM_STEP).max(ui::MIN_ZOOM);
        self.view.display_mode = DisplayMode::Manual;
    }

    fn set_display_mode(&mut self, m: DisplayMode) {
        self.view.display_mode = m;
        if m == DisplayMode::Manual { self.view.zoom = 1.0; }
    }

    fn toggle_manga(&mut self, ctx: &egui::Context) {
        self.view.manga_mode = !self.view.manga_mode;
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
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
        self.manager.schedule_prefetch(self.config.filter_mode, self.view.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    // ── ウィンドウヘルパー ────────────────────────────────────────────────────

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.view.is_fullscreen = !self.view.is_fullscreen;
        self.view.is_borderless = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.view.is_fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.view.is_fullscreen));
    }

    fn toggle_borderless(&mut self, ctx: &egui::Context) {
        self.view.is_borderless = !self.view.is_borderless;
        self.view.is_fullscreen = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.view.is_borderless));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.view.is_borderless));
    }

    fn exit_fullscreen(&mut self, ctx: &egui::Context) {
        self.view.is_fullscreen = false;
        self.view.is_borderless = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
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
                        || self.config.is_first_run || self.ui.show_sort_settings;

        if self.ui.show_tree && !modal_open && !is_typing && !is_capturing {
            self.handle_tree_navigation(ctx, k);
        } else if !modal_open && !is_typing && !is_capturing {
            self.handle_viewer_keys(ctx, k);
            self.handle_mouse_input(ctx);
        }
        if !modal_open && k.esc { self.exit_fullscreen(ctx); }
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
                if has { self.ui.show_tree = false; }
            }
        }
        if k.esc         { self.ui.show_tree = false; }
        if k.toggle_tree { self.ui.show_tree = false; }
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
        if k.rcw              { self.rotate_current(true, ctx); }
        if k.rccw             { self.rotate_current(false, ctx); }
        if k.toggle_manga     { self.toggle_manga(ctx); }
        if k.toggle_rtl       { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); }
        if k.toggle_linear    { self.toggle_filter_mode(ctx); }
        if k.toggle_debug     { self.ui.show_debug = !self.ui.show_debug; }
        if k.quit             { self.save_config(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        if k.fullscreen       { self.toggle_fullscreen(ctx); }
        if k.borderless       { self.toggle_borderless(ctx); }
        if k.open_key_config  { self.ui.show_key_config = true; }
        if k.open_external_1  { self.open_external(0, ctx); }
        if k.open_external_2  { self.open_external(1, ctx); }
        if k.open_external_3  { self.open_external(2, ctx); }
        if k.open_external_4  { self.open_external(3, ctx); }
        if k.open_external_5  { self.open_external(4, ctx); }
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
            if self.ui.show_sort_settings {
                self.manager.apply_sorting(&self.config);
                self.manager.clear_cache();
                self.save_config();
            }
            self.ui.show_sort_settings = !self.ui.show_sort_settings;
            if self.ui.show_sort_settings { self.ui.sort_focus_idx = 0; }
        }
        if k.toggle_tree {
            self.ui.show_tree = !self.ui.show_tree;
            if self.ui.show_tree { self.sync_tree_to_current(); }
        }
    }

    fn handle_mouse_input(&mut self, ctx: &egui::Context) {
        let (wheel, ctrl, secondary) = ctx.input(|i| (
            i.smooth_scroll_delta.y, i.modifiers.ctrl, i.pointer.button_down(egui::PointerButton::Secondary)
        ));
        if wheel != 0.0 {
            if ctrl || secondary {
                self.view.zoom = (self.view.zoom * (1.0 + wheel * ui::WHEEL_ZOOM_SENSITIVITY)).clamp(ui::MIN_ZOOM, ui::MAX_ZOOM);
                self.view.display_mode = DisplayMode::Manual;
            } else {
                self.wheel_accumulator += wheel;
                if self.wheel_accumulator.abs() >= ui::WHEEL_NAV_THRESHOLD {
                    if self.wheel_accumulator > 0.0 { self.go_prev(ctx); } else { self.go_next(ctx); }
                    self.view.display_mode = DisplayMode::Fit;
                    self.wheel_accumulator = 0.0;
                }
            }
        } else {
            self.wheel_accumulator = 0.0;
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
        let failures = self.manager.update(ctx, &self.config, self.view.manga_mode, self.view.manga_shift);
        for (idx, err) in failures {
            if idx == self.manager.target_index || (self.view.manga_mode && idx == self.manager.target_index + 1) {
                self.error = Some(err);
                self.is_loading_archive = false;
            }
        }
        if self.manager.target_index != self.last_target_index {
            self.last_target_index = self.manager.target_index;
        }
        self.sync_display_to_target(ctx);
        if self.error.is_some() || (self.manager.entries.is_empty() && !self.manager.is_listing) {
            self.is_loading_archive = false;
        }
    }

    fn sync_display_to_target(&mut self, ctx: &egui::Context) {
        let target = self.manager.target_index;
        if self.is_loading_archive || self.manager.current != target {
            let mut is_ready = false;
            if let Some(tex1) = self.get_texture(target) {
                // 1枚目さえあれば表示は切り替えて良い（2枚目は painter 側で「読み込み中」表示をハンドルする）
                is_ready = true;
            }
            if is_ready {
                let was_loading = self.is_loading_archive;
                self.manager.current = target;
                self.error = None;
                self.is_loading_archive = false;
                let now = ctx.input(|i| i.time);
                self.folder_lock_until = if was_loading { now + ui::FOLDER_NAV_GUARD_DURATION } else { 0.0 };
                self.page_lock_until   = if !was_loading { now + ui::PAGE_NAV_GUARD_DURATION } else { 0.0 };
            } else {
                // workerのrequest_repaintで再描画される。取りこぼし対策として低頻度でフォールバック
                ctx.request_repaint_after(std::time::Duration::from_millis(loading::LOADING_FALLBACK_POLL_MS));
            }
        }
    }

    // ── 描画 ─────────────────────────────────────────────────────────────────

    fn draw_ui(&mut self, ctx: &egui::Context) {
        self.draw_windows(ctx);
        let menu_act = widgets::main_menu_bar(ctx, &self.config, &self.manager, &self.view, self.ui.show_tree, self.ui.show_debug);
        let mut tree_req = None;
        if self.ui.show_tree {
            egui::SidePanel::left("tree")
                .resizable(true)
                .default_width(ctx.screen_rect().width() * 0.5)
                .max_width(ctx.screen_rect().width() * 0.5)
                .show(ctx, |ui| widgets::sidebar_ui(ui, &mut self.manager.tree, &self.manager.archive_path, ctx, &mut tree_req));
            self.manager.tree.scroll_to_selected = false;
        }
        let tool_act = widgets::bottom_toolbar(ctx, &self.manager, &self.config, &self.view, self.is_nav_locked(ctx));
        if let Some(act) = menu_act.or(tool_act) { self.handle_action(ctx, act); }
        if let Some(p) = tree_req { self.open_path(p, ctx); }
        self.draw_main_panel(ctx);
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
                self.save_config();
            }
        }
        if self.ui.show_debug { widgets::debug_window(ctx, &mut self.ui.show_debug, &self.manager); }
        if self.ui.show_about { widgets::about_window(ctx, &mut self.ui.show_about); }
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
            let (_, act) = painter::draw_main_area(ui, &self.manager, &self.view, self.config.manga_rtl, ctx, is_at_end);
            if let Some(widgets::ViewerAction::NextDir) = act { self.go_next_dir(ctx); }
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
            MoveToCenter       => window::move_to_center(ctx, self.config.window_width, self.config.window_height),

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
            Exit => { self.save_config(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            ToggleTree => {
                self.ui.show_tree = !self.ui.show_tree;
                if self.ui.show_tree { self.sync_tree_to_current(); }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 外部・システムからのイベント処理
        while let Ok(path) = self.path_rx.try_recv() {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }

        // 2. 起動時の中央配置
        if self.config.window_centered && !self.applied_initial_center {
            window::move_to_center(ctx, self.config.window_width, self.config.window_height);
            self.applied_initial_center = true;
        }

        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_config();
        }

        // 3. 内部状態の更新
        self.update_title(ctx);
        window::sync_config_with_window(ctx, &mut self.config, self.last_resize_time);
        self.handle_debug_logging(ctx);
        self.process_manager_update(ctx);

        // 4. 入力処理
        let k = input::gather_input(ctx, &self.config);
        self.handle_input(ctx, &k);

        // 5. 描画
        self.draw_ui(ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_config();
    }
}
