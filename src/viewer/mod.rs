use crate::{archive, integrator, window, shell, toast, config::{self, Config}, manager::Manager, utils, widgets, input, startup};
pub use crate::types::{ViewState, WindowMode};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use crate::constants::*;

mod navigation;
mod display;
mod window_mgr;
mod input_handler;
mod render;

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
    #[allow(dead_code)]
    path_tx:              std::sync::mpsc::Sender<(PathBuf, bool)>, // 内部D&DイベントをIPCチャネルに送る用
    path_rx:              Receiver<(PathBuf, bool)>,
    ui_width_overhead:    f32,
    ui_height_overhead:   f32,
    target_display_w:     f32,
    target_display_h:     f32,
    applied_initial_center: bool,
    initial_center_frame:  u8,
    pro_mode:             bool,
    is_mouse_gesture:     bool,
    scroll_offset:        egui::Vec2,
    viewport_origin:      egui::Pos2,
    pending_scroll:       Option<egui::Vec2>,
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

        match config.window_mode {
            WindowMode::Standard => {}
            WindowMode::Borderless => {
                cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            }
            WindowMode::Fullscreen => {
                cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            }
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
                window_mode: config.window_mode,
                last_base_mode: if config.window_mode == WindowMode::Fullscreen { WindowMode::Standard } else { config.window_mode },
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
            path_tx:                tx,
            path_rx:                rx,
            ui_width_overhead:      0.0,
            ui_height_overhead:     0.0,
            target_display_w:       0.0,
            target_display_h:       0.0,
            applied_initial_center: false,
            initial_center_frame:   0,
            pro_mode,
            config,
            is_mouse_gesture:       false,
            scroll_offset:          egui::Vec2::ZERO,
            viewport_origin:        egui::Pos2::ZERO,
            pending_scroll:         None,
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }
        app
    }

    pub(super) fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        self.manager.get_first_tex(index)
    }

    pub(super) fn get_effective_filter_mode(&self) -> config::FilterMode {
        if self.pro_mode { config::FilterMode::Nearest } else { self.config.filter_mode }
    }

    pub(super) fn get_effective_max_dim(&mut self, ctx: &egui::Context) -> u32 {
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

    /// ウィンドウタイトルを更新する。
    ///
    /// ⚠️ ガード条件（archive_path と last_title_update_time）を変更すると
    /// 毎フレームタイトル更新が走る repaint ループが発生する可能性がある。
    /// 過去に last_target_index を条件に加えたことでループが発生した経緯がある (0.1.2)。
    /// 条件を追加・変更する前にユーザーへ確認すること。
    pub(super) fn update_title(&mut self, ctx: &egui::Context) {
        if self.ui.boss_mode { return; }
        let now = ctx.input(|i| i.time);
        if self.manager.archive_path == self.last_archive_path && now - self.last_title_update_time <= 2.0 { return; }

        self.last_archive_path = self.manager.archive_path.clone();
        self.last_title_update_time = now;

        let config_name = self.config_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy());
        let container_name = self.manager.archive_path.as_ref().map(|p| utils::get_display_name(p));
        let mut title = startup::build_window_title(config_name.as_deref(), self.pro_mode, container_name.as_deref());
        title.push_str(&format!(" ({})", integrator::get_memory_usage_str()));

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    pub(super) fn add_toast(&mut self, msg: String, ctx: &egui::Context) {
        self.toasts.add(msg, ctx);
    }

    pub(super) fn open_external(&mut self, index: usize, ctx: &egui::Context) {
        let app = &self.config.external_apps[index];
        if let Err(e) = shell::open_external(&self.manager, app) {
            self.add_toast(e, ctx);
        } else if app.close_after_launch {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn save_config(&self) {
        if let Some(ref path) = self.config_path {
            if let Err(e) = config::save_config_file(&self.config, path) {
                log::error!("設定の保存に失敗しました: {}", e);
            }
        }
    }

    pub(super) fn handle_debug_logging(&mut self, ctx: &egui::Context) {
        if !self.debug_cli { return; }
        let now = ctx.input(|i| i.time);
        if now - self.last_debug_log_time > 1.0 {
            println!("\n--- Debug Stats ({:.1}s) ---", now);
            println!("Memory: {}", integrator::get_memory_usage_str());
            println!("Cache: {} items ({} KB)", self.manager.cache_len(), self.manager.total_cache_size_bytes() / 1024);
            self.last_debug_log_time = now;
        }
    }

    pub(super) fn process_manager_update(&mut self, ctx: &egui::Context) {
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
    ///   変更前にユーザーへ確認すること。
    pub(super) fn sync_display_to_target(&mut self, ctx: &egui::Context) {
        let target = self.manager.target_index;
        if self.is_loading_archive || self.manager.current != target {
            let mut is_ready = false;
            if let Some(tex) = self.get_texture(target) {
                if self.view.manga_mode {
                    let tex_size = tex.size_vec2();
                    let is_portrait = tex_size.x <= tex_size.y;
                    let can_pair = (self.view.manga_shift || target > 0) && is_portrait;
                    let has_next = target + 1 < self.manager.entries.len();

                    if can_pair && has_next {
                        if self.get_texture(target + 1).is_some() {
                            is_ready = true;
                        }
                    } else {
                        is_ready = true;
                    }
                } else {
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
                ctx.request_repaint();
            }
        }
    }

    // ── アクションディスパッチ ────────────────────────────────────────────────
    pub(super) fn handle_action(&mut self, ctx: &egui::Context, act: widgets::ViewerAction) {
        use widgets::ViewerAction::*;
        match act {
            PrevPage          => self.go_prev(ctx),
            NextPage          => self.go_next(ctx),
            NextDir           => self.go_next_dir(ctx),
            GoPrevDir         => self.go_prev_dir(ctx),
            GoNextDir         => self.go_next_dir(ctx),
            Seek(idx)         => self.seek(idx, ctx),
            SetOpenFromEnd(b) => { self.config.open_from_end = b; self.manager.open_from_end = b; self.save_config(); }

            SetDisplayMode(m) => self.set_display_mode(m),
            ZoomIn            => self.zoom_in(),
            ZoomOut           => self.zoom_out(),
            ZoomReset         => self.zoom_reset(),
            ToggleManga       => self.toggle_manga(ctx),
            ToggleMangaRtl    => { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); }
            ToggleLinear      => self.toggle_filter_mode(ctx),
            Rotate(cw)        => self.rotate_current(cw, ctx),
            SetBgMode(m)      => { self.config.bg_mode = m; self.save_config(); }

            ToggleAlwaysOnTop     => self.toggle_always_on_top(ctx),
            WindSizeLock          => self.toggle_window_resizable(ctx),
            ToggleWindowCentered  => {
                self.config.window_centered = !self.config.window_centered;
                if self.config.window_centered { self.applied_initial_center = false; }
                self.save_config();
            }
            ResizeWindow(w, h) => self.resize_window(ctx, w, h),
            MoveToCenter       => { window::move_to_center(ctx, self.config.window_width, self.config.window_height); },

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

            OpenKeyConfig    => self.ui.show_key_config = true,
            OpenSortSettings => { self.ui.show_sort_settings = true; self.ui.sort_focus_idx = 0; }
            ToggleMultipleInstances => { self.config.allow_multiple_instances = !self.config.allow_multiple_instances; self.save_config(); }
            ToggleDebug      => self.ui.show_debug = !self.ui.show_debug,
            About            => self.ui.show_about = true,
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
            TogglePdfWarning    => { self.config.show_pdf_warning = !self.config.show_pdf_warning; self.save_config(); }
            OpenLimiterSettings => { self.ui.show_limiter_settings = true; }
            SetLimiterPageDuration(d)   => { self.config.limiter_page_duration = d; self.save_config(); }
            SetLimiterFolderDuration(d) => { self.config.limiter_folder_duration = d; self.save_config(); }
            SetWindowMode(mode) => self.set_window_mode(mode, ctx),
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
        let mut path_to_open = None;
        let mut should_focus = false;

        while let Ok(req) = self.path_rx.try_recv() {
            let (path, is_external) = req;
            path_to_open = Some(path);
            if is_external { should_focus = true; }
        }

        if path_to_open.is_none() {
            if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone())) {
                path_to_open = Some(path);
            }
        }

        if let Some(path) = path_to_open {
            let is_new = self.manager.archive_path.as_ref() != Some(&path);
            let is_ipc = should_focus;
            if is_new || is_ipc {
                if should_focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                }
                self.open_path(path, ctx);
            }
        }

        if self.config.window_centered && !self.applied_initial_center {
            self.initial_center_frame = self.initial_center_frame.saturating_add(1);
            if self.initial_center_frame >= 3 && self.initial_center_frame < 10
                && window::move_to_center(ctx, self.config.window_width, self.config.window_height) {
                self.applied_initial_center = true;
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_config();
        }

        self.update_title(ctx);
        window::sync_config_with_window(ctx, &mut self.config, self.last_resize_time);
        self.view.is_maximized = self.config.window_maximized;
        self.handle_debug_logging(ctx);
        self.process_manager_update(ctx);

        let k = input::gather_input(ctx, &self.config);
        self.handle_input(ctx, &k);

        self.draw_ui(ctx);

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
