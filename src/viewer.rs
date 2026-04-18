use crate::{archive, integrator, config::{self, Config}, manager::{self, Manager}, utils, painter, widgets, input};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use crate::constants::*;

#[derive(PartialEq, Copy, Clone)]
pub enum DisplayMode {
    /// 画像が大きい場合のみ縮小（最大1.0倍）
    Fit,
    /// ウィンドウに合わせて拡大縮小（1.0倍を超えて拡大）
    WindowFit,
    /// ズーム倍率に基づく表示（100%など）
    Manual,
}

pub struct App {
    manager: Manager,
    config: Config,

    /// マウスホイールの回転蓄積バッファ
    wheel_accumulator: f32,

    /// 設定画面の表示状態
    show_settings: bool,
    /// ツリー表示の表示状態
    show_tree: bool,
    /// ソート設定画面の表示状態
    show_sort_settings: bool,
    /// キーコンフィグ画面の表示状態
    show_key_config: bool,
    /// キーコンフィグで現在録画中のアクションID
    capturing_key_for: Option<String>,
    /// ソート設定ウィンドウ内のフォーカス行 (0:基準, 1:順序, 2:自然順)
    sort_focus_idx: usize,
    /// 設定画面用の引数編集バッファ
    settings_args_tmp: Vec<String>,
    /// config.ini のパス保持
    config_path: Option<PathBuf>,
    /// デバッグ画面の表示状態
    show_debug: bool,
    /// バージョン・ライセンス情報の表示状態
    show_about: bool,

    /// アーカイブ切り替え中で、最初の画像がロードされるのを待っている状態か
    is_loading_archive: bool,
    /// フォルダ移動による操作ロック解除時刻
    folder_lock_until: f64,
    /// ページ移動・同期による操作ロック解除時刻
    page_lock_until: f64,

    /// ロード待ちのリフレッシュ試行回数
    loading_retry_count: u8,

    /// 最後に画像が実際に切り替わった時刻
    last_display_change_time: f64,
    /// 最後にタイトルを更新した時刻
    last_title_update_time: f64,
    /// 最後にコンソールへログを出力した時刻
    last_debug_log_time: f64,
    /// 最後にウィンドウサイズを明示的に変更した時刻（干渉防止用）
    last_resize_time: f64,
    /// コマンドラインからのデバッグフラグ
    debug_cli: bool,

    last_target_index: usize,
    last_archive_path: Option<PathBuf>,

    /// 前のフレームでフォーカスされていたか（誤クリック防止用）
    was_focused: bool,

    error: Option<String>,
    /// トースト通知のリスト (メッセージ, 消滅時刻)
    toasts: Vec<(String, f64)>,
    display_mode: DisplayMode, zoom: f32, manga_mode: bool, manga_shift: bool,
    is_fullscreen: bool,
    is_borderless: bool,

    ipc_rx: Receiver<PathBuf>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>, config_name: Option<String>, archive_reader: std::sync::Arc<dyn archive::ArchiveReader>, window_title: &str, debug_cli: bool) -> Self {
        // 日本語フォント
        let mut fonts = FontDefinitions::default();
        // メモリ消費を抑えるため、MS ゴシックなどの比較的軽量なフォントを優先候補に追加
        let font_candidates = ["C:\\Windows\\Fonts\\msgothic.ttc", "C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\msjh.ttc"];
        for path in &font_candidates {
            #[cfg(target_os = "windows")]
            if let Some(data) = integrator::mmap_font_file(path) {
                fonts.font_data.insert("japanese".to_owned(), FontData::from_static(data));
                fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "japanese".to_owned());
                fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "japanese".to_owned());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        // テクスチャアトラスや描画負荷の軽減設定
        cc.egui_ctx.options_mut(|opt| {
            // 境界のボケ（フェザリング）を無効にして描画バッファと計算量を節約
            opt.tessellation_options.feathering = false;
        });
        // 高DPI環境でのテクスチャ肥大化を防ぐ（1.0に固定。UIは小さくなるがメモリには優しい）
        cc.egui_ctx.set_pixels_per_point(1.0);

        let (config, config_path) = config::load_config_file(config_name.as_deref());
        let settings_args_tmp = config.external_apps.iter().map(|app| app.args.join(" ")).collect();

        // 1.0固定のスケーリングを適用した状態で、保存されていた位置とサイズを再適用する
        // (eframeの初期化時点ではOSのスケーリングが効いている可能性があるため)
        if !config.window_maximized {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                config.window_width,
                config.window_height,
            )));
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                config.window_x,
                config.window_y,
            )));
        }

        // 設定値を反映
        let mut manager = Manager::new(cc.egui_ctx.clone(), archive_reader);
        manager.open_from_end = config.open_from_end;

        if config.always_on_top {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        }

        let mut app = Self {
            manager,
            config,
            wheel_accumulator: 0.0,
            show_settings: false,
            show_tree: false,
            show_sort_settings: false,
            show_key_config: false,
            capturing_key_for: None,
            sort_focus_idx: 0,
            settings_args_tmp,
            config_path,
            show_debug: false,
            show_about: false,
            is_loading_archive: false,
            folder_lock_until: 0.0,
            page_lock_until: 0.0,
            loading_retry_count: 0,
            last_display_change_time: 0.0,
            last_title_update_time: 0.0,
            last_debug_log_time: 0.0,
            last_resize_time: 0.0,
            debug_cli,
            last_target_index: 0,
            last_archive_path: None,
            was_focused: true,
            error: None, display_mode: DisplayMode::Fit, zoom: 1.0, 
            toasts: Vec::new(),
            manga_mode: false, manga_shift: false,
            is_fullscreen: false,
            is_borderless: false,
            ipc_rx: integrator::install_message_hook(&cc.egui_ctx, window_title),
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }

        app
    }

    fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        self.manager.get_first_tex(index)
    }

    /// ツリーの選択状態と展開状態を現在のアーカイブパスに強制同期する
    fn sync_tree_to_current(&mut self) {
        if let Some(path) = self.manager.archive_path.clone() { // manager のフィールド
            let cleaned = utils::clean_path(&path); // utils::clean_path を使用
            self.manager.tree.expand_to_path(&cleaned);
            self.manager.tree.selected = Some(cleaned);
            self.manager.tree.reveal_path(&path);
        }
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.error = None;
        
        // 最近開いたパスの履歴を更新 (最大10件)
        let path_str = path.to_string_lossy().into_owned();
        self.config.recent_paths.retain(|p| p != &path_str);
        self.config.recent_paths.insert(0, path_str);
        if self.config.recent_paths.len() > 10 {
            self.config.recent_paths.pop();
        }
        self.save_config();

        self.manager.open_path(path, &self.config);
        
        // ツリーのノードキャッシュが1000件を超えたら一度リセットしてメモリを節約する
        if self.manager.tree.nodes.len() > crate::constants::cache::TREE_NODES_CACHE_LIMIT {
            self.manager.tree.clear_metadata_cache();
        }

        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        ctx.request_repaint();
    }

    fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.manga_mode {
            vec![self.manager.current, self.manager.current + 1]
        } else { vec![self.manager.current] };

        for idx in indices {
            if let Some(name) = self.manager.entries.get(idx).cloned() {
                let rot = self.manager.rotations.get(&name).copied().unwrap_or(manager::Rotation::R0);
                let new_rot = if cw { rot.cw() } else { rot.ccw() };
                self.manager.rotations.insert(name.clone(), new_rot);
                self.manager.invalidate_cache_for(idx, &name);
            }
        }
        self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM);
        ctx.request_repaint();
    }

    fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, ctx: &egui::Context) {
        self.error = None;
        self.manager.move_to_dir(path, focus_hint, go_last, &self.config, self.manga_mode, self.manga_shift);
        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        ctx.request_repaint();
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) {
        self.navigate_relative_dir(false, ctx);
    } // manager のメソッド
    fn go_next_dir(&mut self, ctx: &egui::Context) {
        self.navigate_relative_dir(true, ctx);
    }

    fn navigate_relative_dir(&mut self, forward: bool, ctx: &egui::Context) {
        if self.manager.go_relative_dir(forward, &self.config, self.manga_mode, self.manga_shift) {
            self.sync_tree_to_current();
            self.error = None; self.is_loading_archive = true; ctx.request_repaint();
        }
    }

    fn is_nav_locked(&self, ctx: &egui::Context) -> bool {
        let now = ctx.input(|i| i.time);

        // 1. フォルダ/アーカイブの最初の読み込み待ち
        if self.is_loading_archive { return true; }

        // 2. ページロード待ち（目標に到達するまでロック）
        if self.manager.current != self.manager.target_index { return true; }

        // 3. フォルダ移動直後の長いガード
        if now < self.folder_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.folder_lock_until - now));
            return true;
        }

        // 4. ページ移動・揃え直後のガード
        if now < self.page_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.page_lock_until - now));
            return true;
        }

        false
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.manager.archive_path == self.last_archive_path && now - self.last_title_update_time <= 2.0 {
            return;
        }
        self.last_archive_path = self.manager.archive_path.clone();
        self.last_title_update_time = now;

        let renderer_str = match self.config.renderer {
            config::RendererMode::Glow => "OpenGL",
            config::RendererMode::Wgpu => "Wgpu",
        };
        let config_part = self.config_path.as_ref()
            .and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
            .filter(|n| n != "config.ini").map(|n| format!(" {{{}}}", n)).unwrap_or_default();

        let folder_name = self.manager.archive_path.as_ref()
            .map(|p| utils::get_display_name(p)).unwrap_or_else(|| "---".to_string());

        let title = format!("Hinjaku - {}{} - {} - [{}]", renderer_str, config_part, folder_name, integrator::get_memory_usage_str());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        if !self.manager.go_prev(false, false, self.config.filter_mode, image::MAX_TEX_DIM) { self.go_prev_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        if !self.manager.go_next(false, false, self.config.filter_mode, image::MAX_TEX_DIM) { self.go_next_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        if !self.manager.go_prev(self.manga_mode, self.manga_shift, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        if !self.manager.go_next(self.manga_mode, self.manga_shift, self.config.filter_mode, image::MAX_TEX_DIM) {
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        self.manager.target_index = 0; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM); ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.display_mode = DisplayMode::Fit;
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.manga_mode && last > 0 && last % 2 == 0 { last.saturating_sub(1) } else { last };
        self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM); ctx.request_repaint();
    }

    fn add_toast(&mut self, msg: String, ctx: &egui::Context) {
        let expires = ctx.input(|i| i.time) + ui::TOAST_DURATION;
        self.toasts.push((msg, expires));
        // 通知が消えるタイミングで再描画を予約
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(ui::TOAST_DURATION));
    }

    fn open_external(&mut self, index: usize, ctx: &egui::Context) {
        let Some(path_p) = self.manager.get_current_full_path() else { return };
        let path_a = self.manager.archive_path.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let app = &self.config.external_apps[index];
        if let Err(e) = integrator::launch_external(&app.exe, &app.args, &path_p, &path_a) {
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

    fn update_window_state(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let viewport_info = ctx.input(|i| i.viewport().clone());
        let maximized = viewport_info.maximized.unwrap_or(false);
        let minimized = viewport_info.minimized.unwrap_or(false);
        let fullscreen = viewport_info.fullscreen.unwrap_or(false);

        // フルスクリーン時は「最大化」として保存しない
        if !fullscreen {
            self.config.window_maximized = maximized;
        }

        // 通常状態（最大化・最小化・フルスクリーンではない）の時のみ、座標とサイズを記録する
        if !maximized && !minimized && !fullscreen && (self.last_resize_time == 0.0 || now - self.last_resize_time > 0.5) {
            if let Some(rect) = viewport_info.outer_rect {
                // スクリーン座標上のウィンドウ位置
                self.config.window_x = rect.min.x;
                self.config.window_y = rect.min.y;
            }
            if let Some(rect) = viewport_info.inner_rect {
                // eframe::NativeOptions が期待する「描画領域のサイズ」
                if rect.width() > 10.0 && rect.height() > 10.0 {
                    self.config.window_width = rect.width();
                    self.config.window_height = rect.height();
                }
            }
        }
    }

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
        let failures = self.manager.update(ctx, &self.config, self.manga_mode, self.manga_shift);
        for (idx, err) in failures {
            if idx == self.manager.target_index || (self.manga_mode && idx == self.manager.target_index + 1) {
                self.error = Some(err);
                self.is_loading_archive = false;
            }
        }
        if self.manager.target_index != self.last_target_index {
            self.last_target_index = self.manager.target_index;
            self.loading_retry_count = 0;
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
                if self.manga_mode {
                    let s1 = tex1.size_vec2();
                    let needs_2nd = s1.x <= s1.y && (self.manga_shift || target > 0) && target + 1 < self.manager.entries.len();
                    if !needs_2nd || self.get_texture(target + 1).is_some() { is_ready = true; }
                } else { is_ready = true; }
            }
            if is_ready {
                let was_loading = self.is_loading_archive;
                self.manager.current = target;
                self.error = None;
                self.is_loading_archive = false;
                let now = ctx.input(|i| i.time);
                self.folder_lock_until = if was_loading { now + ui::FOLDER_NAV_GUARD_DURATION } else { 0.0 };
                self.page_lock_until = if !was_loading { now + ui::PAGE_NAV_GUARD_DURATION } else { 0.0 };
            } else if self.loading_retry_count < loading::LOADING_MAX_RETRIES {
                self.loading_retry_count += 1;
                ctx.request_repaint_after(std::time::Duration::from_millis(loading::LOADING_RETRY_DELAY_MS));
            }
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        let is_typing = ctx.wants_keyboard_input();
        let is_capturing = self.capturing_key_for.is_some();
        let modal_open = self.show_settings || self.show_key_config || self.config.is_first_run || self.show_sort_settings;

        if self.show_tree && !modal_open && !is_typing && !is_capturing {
            self.handle_tree_navigation(ctx, k);
        } else if !modal_open && !is_typing && !is_capturing {
            self.handle_viewer_keys(ctx, k);
            self.handle_mouse_input(ctx);
        }
        if !modal_open && k.esc { self.exit_fullscreen(ctx); }
    }

    fn handle_tree_navigation(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        let old = self.manager.tree.selected.clone();
        if k.up { self.manager.tree.move_selection(-1); }
        if k.dn { self.manager.tree.move_selection(1); }
        if k.right { self.manager.tree.expand_current(); }
        if k.left { self.manager.tree.collapse_or_up(); }
        if self.manager.tree.selected != old {
            if let Some(p) = self.manager.tree.selected.clone() { self.open_path(p, ctx); }
        }
        if k.enter {
            if let Some(p) = self.manager.tree.activate_current() {
                let has = self.manager.tree.get_image_count(&p) > 0;
                self.open_path(p, ctx);
                if has { self.show_tree = false; }
            }
        }
        if k.esc { self.show_tree = false; }
        if k.toggle_tree { self.show_tree = false; }
    }

    fn handle_viewer_keys(&mut self, ctx: &egui::Context, k: &input::KeyboardState) {
        if k.prev_page { self.go_prev(ctx); }
        if k.next_page { self.go_next(ctx); }
        if k.prev_page_single { self.go_single_prev(ctx); }
        if k.next_page_single { self.go_single_next(ctx); }
        if k.first_page { self.go_first(ctx); }
        if k.last_page { self.go_last(ctx); }
        if k.toggle_fit { self.handle_action(ctx, widgets::ViewerAction::SetDisplayMode(match self.display_mode { DisplayMode::Fit => DisplayMode::WindowFit, DisplayMode::WindowFit => DisplayMode::Manual, DisplayMode::Manual => DisplayMode::Fit })); }
        if k.zoom_in { self.handle_action(ctx, widgets::ViewerAction::ZoomIn); }
        if k.zoom_out { self.handle_action(ctx, widgets::ViewerAction::ZoomOut); }
        if k.rcw { self.rotate_current(true, ctx); }
        if k.rccw { self.rotate_current(false, ctx); }
        if k.prev_dir { self.go_prev_dir(ctx); }
        if k.next_dir { self.go_next_dir(ctx); }
        if k.toggle_manga { self.handle_action(ctx, widgets::ViewerAction::ToggleManga); }
        if k.toggle_rtl { self.handle_action(ctx, widgets::ViewerAction::ToggleMangaRtl); }
        if k.toggle_linear { self.handle_action(ctx, widgets::ViewerAction::ToggleLinear); }
        if k.toggle_bg { self.handle_action(ctx, widgets::ViewerAction::SetBgMode(match self.config.bg_mode { config::BackgroundMode::Theme => config::BackgroundMode::Checkerboard, config::BackgroundMode::Checkerboard => config::BackgroundMode::Black, config::BackgroundMode::Black => config::BackgroundMode::Gray, config::BackgroundMode::Gray => config::BackgroundMode::White, config::BackgroundMode::White => config::BackgroundMode::Green, config::BackgroundMode::Green => config::BackgroundMode::Theme })); }
        if k.toggle_debug { self.show_debug = !self.show_debug; }
        if k.bs { if let Some(p) = &self.manager.archive_path { if let Err(e) = integrator::reveal_in_explorer(p) { self.error = Some(e); } } }
        if k.open_external_1 { self.open_external(0, ctx); }
        if k.quit { self.handle_action(ctx, widgets::ViewerAction::Exit); }
        if k.fullscreen { self.toggle_fullscreen(ctx); }
        if k.borderless { self.toggle_borderless(ctx); }
        if k.open_key_config { self.show_key_config = true; }
        if k.sort_settings {
            if self.show_sort_settings {
                self.manager.apply_sorting(&self.config);
                self.manager.clear_cache();
                self.save_config();
            }
            self.show_sort_settings = !self.show_sort_settings;
            if self.show_sort_settings { self.sort_focus_idx = 0; }
        }
        if k.toggle_tree { self.show_tree = !self.show_tree; if self.show_tree { self.sync_tree_to_current(); } }
    }

    fn handle_mouse_input(&mut self, ctx: &egui::Context) {
        let (wheel, ctrl, secondary) = ctx.input(|i| (i.smooth_scroll_delta.y, i.modifiers.ctrl, i.pointer.button_down(egui::PointerButton::Secondary)));
        if wheel != 0.0 {
            if ctrl || secondary {
                self.zoom = (self.zoom * (1.0 + wheel * ui::WHEEL_ZOOM_SENSITIVITY)).clamp(ui::MIN_ZOOM, ui::MAX_ZOOM);
                self.display_mode = DisplayMode::Manual;
            } else {
                self.wheel_accumulator += wheel;
                if self.wheel_accumulator.abs() >= ui::WHEEL_NAV_THRESHOLD {
                    if self.wheel_accumulator > 0.0 { self.go_prev(ctx); } else { self.go_next(ctx); }
                    self.display_mode = DisplayMode::Fit; self.wheel_accumulator = 0.0;
                }
            }
        } else { self.wheel_accumulator = 0.0; }
        ctx.input(|i| {
            if i.pointer.button_pressed(egui::PointerButton::Extra1) { self.go_prev(ctx); }
            if i.pointer.button_pressed(egui::PointerButton::Extra2) { self.go_next(ctx); }
        });
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.is_fullscreen = !self.is_fullscreen; self.is_borderless = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
    }

    fn toggle_borderless(&mut self, ctx: &egui::Context) {
        self.is_borderless = !self.is_borderless; self.is_fullscreen = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_borderless));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_borderless));
    }

    fn exit_fullscreen(&mut self, ctx: &egui::Context) {
        self.is_fullscreen = false; self.is_borderless = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
    }

    fn draw_ui(&mut self, ctx: &egui::Context) {
        self.draw_windows(ctx);
        let menu_act = widgets::main_menu_bar(ctx, &self.config, &self.manager, self.display_mode, self.manga_mode, self.show_tree, self.show_debug);
        let mut tree_req = None;
        if self.show_tree {
            egui::SidePanel::left("tree")
                .resizable(true)
                .default_width(ctx.screen_rect().width() * 0.5)
                .max_width(ctx.screen_rect().width() * 0.5)
                .show(ctx, |ui| widgets::sidebar_ui(ui, &mut self.manager.tree, &self.manager.archive_path, ctx, &mut tree_req));
            self.manager.tree.scroll_to_selected = false;
        }
        let tool_act = widgets::bottom_toolbar(ctx, &self.manager, &self.config, self.display_mode, self.zoom, self.manga_mode, self.is_nav_locked(ctx));
        if let Some(act) = menu_act.or(tool_act) { self.handle_action(ctx, act); }
        if let Some(p) = tree_req { self.open_path(p, ctx); }
        self.draw_main_panel(ctx);
        self.draw_toasts(ctx);
    }

    fn draw_windows(&mut self, ctx: &egui::Context) {
        if self.show_settings && widgets::settings_window(ctx, &mut self.show_settings, &mut self.config, &mut self.settings_args_tmp) { self.save_config(); }
        if self.show_key_config {
            if let Some(id) = self.capturing_key_for.clone() { if let Some(c) = input::detect_key_combination(ctx) { self.config.keys.insert(id, c); self.capturing_key_for = None; self.save_config(); } }
            if widgets::key_config_window(ctx, &mut self.show_key_config, &mut self.config, &mut self.capturing_key_for) { self.save_config(); }
        }
        if self.config.is_first_run {
            egui::Window::new("Hinjaku へようこそ")
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .collapsible(false)
                .resizable(false)
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
        if self.show_sort_settings {
            widgets::sort_settings_window(ctx, &mut self.show_sort_settings, &mut self.config, &mut self.sort_focus_idx, false, ctx.input(|i| i.key_pressed(egui::Key::Space)));
            if !self.show_sort_settings {
                self.manager.apply_sorting(&self.config);
                self.manager.clear_cache();
                self.save_config();
            }
        }
        if self.show_debug { widgets::debug_window(ctx, &mut self.show_debug, &self.manager); }
        if self.show_about { widgets::about_window(ctx, &mut self.show_about); }
    }

    fn draw_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            painter::paint_background(ui, ui.available_rect_before_wrap(), self.config.bg_mode);
            if let Some(err) = self.error.clone() { ui.centered_and_justified(|ui| ui.label(egui::RichText::new(format!("エラー: {err}")).color(egui::Color32::RED))); return; }
            if self.get_texture(self.manager.current).is_none() { self.draw_loading_screen(ui, ctx); return; }
            let is_at_end = self.manager.current >= self.manager.entries.len().saturating_sub(2);
            let (_, act) = painter::draw_main_area(ui, &self.manager, self.display_mode, self.zoom, self.manga_mode, self.config.manga_rtl, self.manga_shift, ctx, is_at_end);
            if let Some(widgets::ViewerAction::NextDir) = act { self.go_next_dir(ctx); }
        });
    }

    fn draw_loading_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.centered_and_justified(|ui| {
            if self.manager.entries.is_empty() && !self.manager.is_listing {
                ui.vertical_centered(|ui| {
                    let faint_color = ui.visuals().weak_text_color().linear_multiply(0.1);
                    ui.label(egui::RichText::new("H").size(140.0).strong().color(faint_color));
                    ui.add_space(8.0);
                    ui.label("フォルダやアーカイブをドラッグ＆ドロップしてください。");
                    if let Some(p) = &self.manager.archive_path { if let Some(parent) = p.parent() { if ui.button("一つ上の階層へ").clicked() { let c = p.clone(); self.move_to_dir(parent.to_path_buf(), Some(c), false, ctx); } } }
                });
            } else { ui.label("読み込み中..."); }
        });
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time); self.toasts.retain(|(_, e)| *e > now);
        if !self.toasts.is_empty() { egui::Area::new(egui::Id::new("toasts")).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.,-10.)).interactable(false).show(ctx, |ui| { for (m, _) in &self.toasts { egui::Frame::group(ui.style()).fill(egui::Color32::from_black_alpha(200)).show(ui, |ui| ui.label(m)); } }); }
    }

    fn handle_action(&mut self, ctx: &egui::Context, act: widgets::ViewerAction) {
        match act {
            widgets::ViewerAction::OpenRecent(p) => { let path = PathBuf::from(p); if path.exists() { self.open_path(path, ctx); } else { self.add_toast("対象のパスが見つかりません。".to_string(), ctx); } }
            widgets::ViewerAction::OpenFolder => { if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); } }
            widgets::ViewerAction::RevealInExplorer => { if let Some(p) = &self.manager.archive_path { if let Err(e) = integrator::reveal_in_explorer(p) { self.add_toast(e, ctx); } } }
            widgets::ViewerAction::OpenExternal(idx) => self.open_external(idx, ctx),
            widgets::ViewerAction::OpenExternalSettings => { self.settings_args_tmp = self.config.external_apps.iter().map(|a| a.args.join(" ")).collect(); self.show_settings = true; }
            widgets::ViewerAction::OpenKeyConfig => self.show_key_config = true,
            widgets::ViewerAction::Exit => { self.save_config(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            widgets::ViewerAction::SetDisplayMode(m) => { self.display_mode = m; if m == DisplayMode::Manual { self.zoom = 1.0; } }
            widgets::ViewerAction::ZoomIn => { self.zoom = (self.zoom * ui::ZOOM_STEP).min(ui::MAX_ZOOM); self.display_mode = DisplayMode::Manual; }
            widgets::ViewerAction::ZoomOut => { self.zoom = (self.zoom / ui::ZOOM_STEP).max(ui::MIN_ZOOM); self.display_mode = DisplayMode::Manual; }
            widgets::ViewerAction::ToggleManga => { self.manga_mode = !self.manga_mode; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM); }
            widgets::ViewerAction::ToggleMangaRtl => { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); }
            widgets::ViewerAction::ToggleTree => { self.show_tree = !self.show_tree; if self.show_tree { self.sync_tree_to_current(); } }
            widgets::ViewerAction::OpenSortSettings => { self.show_sort_settings = true; self.sort_focus_idx = 0; }
            widgets::ViewerAction::ToggleLinear => { self.config.filter_mode = match self.config.filter_mode { config::FilterMode::Nearest => config::FilterMode::Bilinear, config::FilterMode::Bilinear => config::FilterMode::Bicubic, config::FilterMode::Bicubic => config::FilterMode::Lanczos, config::FilterMode::Lanczos => config::FilterMode::Nearest }; self.manager.clear_cache(); self.save_config(); self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM); }
            widgets::ViewerAction::ToggleAlwaysOnTop => { self.config.always_on_top = !self.config.always_on_top; ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.config.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal })); self.save_config(); }
            widgets::ViewerAction::Rotate(cw) => self.rotate_current(cw, ctx),
            widgets::ViewerAction::GoPrevDir => self.go_prev_dir(ctx),
            widgets::ViewerAction::GoNextDir => self.go_next_dir(ctx),
            widgets::ViewerAction::SetOpenFromEnd(b) => { self.config.open_from_end = b; self.manager.open_from_end = b; self.save_config(); }
            widgets::ViewerAction::SetBgMode(m) => { self.config.bg_mode = m; self.save_config(); }
            widgets::ViewerAction::PrevPage => self.go_prev(ctx),
            widgets::ViewerAction::NextPage => self.go_next(ctx),
            widgets::ViewerAction::NextDir => self.go_next_dir(ctx),
            widgets::ViewerAction::Seek(idx) => { self.manager.target_index = idx; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode, image::MAX_TEX_DIM); }
            widgets::ViewerAction::ToggleDebug => self.show_debug = !self.show_debug,
            widgets::ViewerAction::SetRenderer(m) => { self.config.renderer = m; self.save_config(); self.add_toast("設定を反映するには再起動が必要です。".to_string(), ctx); }
            widgets::ViewerAction::ToggleWindowResizable => { self.config.window_resizable = !self.config.window_resizable; ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(self.config.window_resizable)); self.save_config(); }
            widgets::ViewerAction::ResizeWindow(w, h) => { let s = egui::vec2(w as f32, h as f32); ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(s)); self.config.window_width = s.x; self.config.window_height = s.y; self.last_resize_time = ctx.input(|i| i.time); self.save_config(); }
            widgets::ViewerAction::About => self.show_about = true,
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 外部・システムからのイベント処理 (IPC / ドラッグ&ドロップ)
        while let Ok(path) = self.ipc_rx.try_recv() {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }

        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ウィンドウの「×」ボタン、システムメニュー、Alt+F4 等での終了要求を検知した瞬間に保存
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_config();
        }

        // 2. 内部状態の更新
        self.update_title(ctx);
        self.update_window_state(ctx);
        self.handle_debug_logging(ctx);
        self.process_manager_update(ctx);

        // 3. 入力のハンドリング
        let k = input::gather_input(ctx, &self.config);
        self.handle_input(ctx, &k);

        // 4. 描画
        self.draw_ui(ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // アプリ終了時（ウィンドウを閉じた際）に現在の位置・サイズを含む設定を保存
        self.save_config();
    }
}
