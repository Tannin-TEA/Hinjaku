use crate::{archive, integrator, config::{self, Config}, manager::{self, Manager}, utils, painter, widgets, input};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::path::PathBuf;
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
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>, config_name: Option<String>, archive_reader: std::sync::Arc<dyn archive::ArchiveReader>) -> Self {
        // 日本語フォント
        let mut fonts = FontDefinitions::default();
        // ポータビリティ向上のため、複数の候補を確認するか、
        // 実行ファイルと同じ場所にフォントを置くなどの検討をお勧めします
        for font_path in &["C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\msjh.ttc"] {
            if let Ok(bytes) = std::fs::read(font_path) {
                fonts.font_data.insert("japanese".to_owned(), FontData::from_owned(bytes));
                fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "japanese".to_owned());
                fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "japanese".to_owned());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        let (config, config_path) = config::load_config_file(config_name.as_deref());
        let settings_args_tmp = config.external_apps.iter().map(|app| app.args.join(" ")).collect();

        let mut app = Self {
            manager: Manager::new(cc.egui_ctx.clone(), archive_reader), // Manager に archive_reader を渡す
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
            is_loading_archive: false,
            folder_lock_until: 0.0,
            page_lock_until: 0.0,
            loading_retry_count: 0,
            last_display_change_time: 0.0,
            last_target_index: 0,
            last_archive_path: None,
            was_focused: true,
            error: None, display_mode: DisplayMode::Fit, zoom: 1.0, 
            toasts: Vec::new(),
            manga_mode: false, manga_shift: false,
            is_fullscreen: false,
            is_borderless: false,
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }

        app
    }

    fn update_title(&self, ctx: &egui::Context) {
        let config_name = self.config_path.as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config.ini".to_string());

        let folder_name = self.manager.archive_path.as_ref()
            .map(|p| utils::get_display_name(p))
            .unwrap_or_else(|| "---".to_string());

        let title = format!("Hinjaku {{{}}} - {} -", config_name, folder_name);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
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
        self.manager.open_path(path, &self.config);
        
        // ツリーのノードキャッシュが1000件を超えたら一度リセットしてメモリを節約する
        if self.manager.tree.nodes.len() > widgets::TREE_NODES_CACHE_LIMIT {
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
        self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode);
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

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        if !self.manager.go_prev(false, false, self.config.filter_mode) { self.go_prev_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        if !self.manager.go_next(false, false, self.config.filter_mode) { self.go_next_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        if !self.manager.go_prev(self.manga_mode, self.manga_shift, self.config.filter_mode) {
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        if !self.manager.go_next(self.manga_mode, self.manga_shift, self.config.filter_mode) {
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        self.manager.target_index = 0; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode); ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.folder_lock_until = 0.0; self.page_lock_until = 0.0;
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.manga_mode && last > 0 && last % 2 == 0 { last.saturating_sub(1) } else { last };
        self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode); ctx.request_repaint();
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // ── タイトルの更新検知 ──────────────────────────────────────────
        if self.manager.archive_path != self.last_archive_path {
            self.last_archive_path = self.manager.archive_path.clone();
            self.update_title(ctx);
        }

        let is_focused = ctx.input(|i| i.focused);
        // ウィンドウがフォーカスを得た瞬間のクリックは無視するためのフラグ
        let click_allowed = is_focused && self.was_focused;

        // ── ターゲット変更の検知 ────────────────────────────────────────
        if self.manager.target_index != self.last_target_index {
            self.last_target_index = self.manager.target_index;
            self.loading_retry_count = 0; // ページが変わったらリトライカウントをリセット
        }

        // ── バックグラウンド結果を回収 ──────────────────────────────────
        let failures = self.manager.update(ctx, &self.config);
        for (idx, err) in failures {
            // 目標のページ、またはマンガモード時の隣のページがエラーなら停止
            let target = self.manager.target_index;
            if idx == target || (self.manga_mode && idx == target + 1) {
                self.error = Some(err);
                self.is_loading_archive = false;
            }
        }

        // ── ページ同期（目標ページの準備ができていたら表示を更新） ──────
        let target = self.manager.target_index;
        if self.is_loading_archive || self.manager.current != target {
            let mut is_ready = false;
            if let Some(tex1) = self.get_texture(target) {
                if self.manga_mode {
                    let s1 = tex1.size_vec2();
                    // 2枚目が必要な条件: 1枚目が見開きでない 且つ (シフト中 または 2枚目以降) 且つ 次のページが存在
                    let needs_2nd = s1.x <= s1.y 
                        && (self.manga_shift || target > 0) 
                        && target + 1 < self.manager.entries.len();
                    
                    if needs_2nd {
                        if self.get_texture(target + 1).is_some() {
                            is_ready = true;
                        }
                    } else {
                        is_ready = true; // 1枚のみで完結するケース
                    }
                } else {
                    is_ready = true; // 通常モード
                }
            }

            if is_ready {
                let was_loading = self.is_loading_archive;
                self.manager.current = target;
                self.error = None;
                self.is_loading_archive = false;
                self.loading_retry_count = 0;
                let now = ctx.input(|i| i.time);
                self.last_display_change_time = now;

                if was_loading {
                    // フォルダ/アーカイブを開いた直後のみ、専用の長いウェイトをかける
                    self.folder_lock_until = now + FOLDER_NAV_GUARD_DURATION;
                } else {
                    // 通常のページめくり（上下1ページ送り含む）完了時にも短いウェイトをかける
                    self.page_lock_until = now + PAGE_NAV_GUARD_DURATION;
                }
            } else if self.loading_retry_count < LOADING_MAX_RETRIES {
                // 15ms間隔で最大3回だけ自動リフレッシュ（バックグラウンド通知の予備）
                self.loading_retry_count += 1;
                ctx.request_repaint_after(std::time::Duration::from_millis(LOADING_RETRY_DELAY_MS));
            }
        }

        // エラー時や画像がない時はロックを解除する
        if self.error.is_some() || (self.manager.entries.is_empty() && !self.manager.is_listing) {
            self.is_loading_archive = false;
        }

        // ── ドラッグ＆ドロップ ──────────────────────────────────────────
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ── キーボード ──────────────────────────────────────────────────
        let k = input::gather_input(ctx, &self.config);

        let is_typing = ctx.wants_keyboard_input();
        let is_capturing = self.capturing_key_for.is_some();
        // 設定・キーコン・初回起動は「重い」モーダルとして、他のトグル操作を制限する
        let is_heavy_modal = self.show_settings || self.show_key_config || self.config.is_first_run;

        // ソート/外部アプリ設定ウィンドウが開いている間はメイン操作を無効化
        // ※show_sort_settings 自体も操作無効化の対象だが、自身のトグル（閉じる）は許可する必要がある
        let modal_open = is_heavy_modal || self.show_sort_settings;

        // ── モード別のキー入力処理 ──────────────────────────────────────
        if self.show_tree && !modal_open && !is_typing && !is_capturing {
            // ツリー操作モード：メイン画面の操作を完全に遮断
            let old_selected = self.manager.tree.selected.clone();

            if k.up { self.manager.tree.move_selection(-1); }
            if k.dn { self.manager.tree.move_selection(1); }
            if k.right { self.manager.tree.expand_current(); }
            if k.left { self.manager.tree.collapse_or_up(); }

            // 選択が変更された場合、プレビューとしてそのパスを開く（一番最初の画像を表示）
            let new_selected = self.manager.tree.selected.clone();
            if new_selected != old_selected {
                if let Some(path) = new_selected {
                    // ツリー表示を維持したまま、バックグラウンドのビューアの中身を更新
                    self.open_path(path, ctx);
                }
            }

            if k.enter {
                if let Some(path) = self.manager.tree.activate_current() {
                    self.open_path(path, ctx);
                    self.show_tree = false;
                }
            }
            if k.esc { self.show_tree = false; }
        } else if !modal_open && !is_typing && !is_capturing {
            // 通常ビューアモード
            if k.prev_page { self.go_prev(ctx); }
            if k.next_page { self.go_next(ctx); }

            if k.prev_page_single { self.go_single_prev(ctx); }
            if k.next_page_single { self.go_single_next(ctx); }

            if k.first_page { self.go_first(ctx); }
            if k.last_page { self.go_last(ctx); }
            if k.toggle_fit {
                self.display_mode = match self.display_mode {
                    DisplayMode::Fit => DisplayMode::WindowFit,
                    DisplayMode::WindowFit => { self.zoom = 1.0; DisplayMode::Manual },
                    DisplayMode::Manual => DisplayMode::Fit,
                };
            }
            if k.zoom_in { self.zoom = (self.zoom * ZOOM_STEP).min(MAX_ZOOM); self.display_mode = DisplayMode::Manual; }
            if k.zoom_out { self.zoom = (self.zoom / ZOOM_STEP).max(MIN_ZOOM); self.display_mode = DisplayMode::Manual; }
            if k.rcw    { self.rotate_current(true,  ctx); }
            if k.rccw   { self.rotate_current(false, ctx); }
            if k.prev_dir { self.go_prev_dir(ctx); }
            if k.next_dir { self.go_next_dir(ctx); }
            if k.toggle_manga {
                self.manga_mode = !self.manga_mode;
                self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode);
                ctx.request_repaint();
            }
            if k.toggle_rtl {
                self.config.manga_rtl = !self.config.manga_rtl;
                self.save_config();
            }
            if k.toggle_linear {
                self.config.filter_mode = match self.config.filter_mode {
                    config::FilterMode::Nearest => config::FilterMode::Bilinear,
                    config::FilterMode::Bilinear => config::FilterMode::Bicubic,
                    config::FilterMode::Bicubic => config::FilterMode::Lanczos,
                    config::FilterMode::Lanczos => config::FilterMode::Nearest,
                };
                self.manager.clear_cache();
                self.save_config();
                self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode);
            }
            if k.toggle_bg {
                self.config.bg_mode = match self.config.bg_mode {
                    config::BackgroundMode::Theme => config::BackgroundMode::Checkerboard,
                    config::BackgroundMode::Checkerboard => config::BackgroundMode::Black,
                    config::BackgroundMode::Black => config::BackgroundMode::Gray,
                    config::BackgroundMode::Gray => config::BackgroundMode::White,
                    config::BackgroundMode::White => config::BackgroundMode::Green,
                    config::BackgroundMode::Green => config::BackgroundMode::Theme,
                };
                self.save_config();
            }
            if k.bs {
                if let Some(path) = &self.manager.archive_path {
                    if let Err(e) = integrator::reveal_in_explorer(path) {
                        self.error = Some(e);
                    }
                }
            }
            if k.open_external_1 { self.open_external(0, ctx); }
            if k.open_external_2 { self.open_external(1, ctx); }
            if k.open_external_3 { self.open_external(2, ctx); }
            if k.open_external_4 { self.open_external(3, ctx); }
            if k.open_external_5 { self.open_external(4, ctx); }

            if k.quit { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }

            // 全画面切替
            if k.fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                self.is_fullscreen = !self.is_fullscreen;
                self.is_borderless = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_fullscreen));
            }
            if k.borderless {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                self.is_borderless = !self.is_borderless;
                self.is_fullscreen = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_borderless));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_borderless));
            }
        }

        // ── ウィンドウ表示・切替系のショートカット ──────────────────────
        // テキスト入力中やキー録画中、あるいは「重い」設定画面が開いているときは無効化
        if !is_heavy_modal && !is_typing && !is_capturing {
            if k.open_key_config {
            self.show_key_config = true;
        }

            if k.sort_settings {
                self.show_sort_settings = !self.show_sort_settings;
                if self.show_sort_settings { self.sort_focus_idx = 0; }
            }
            if k.toggle_tree { 
                self.show_tree = !self.show_tree; 
                if self.show_tree { self.sync_tree_to_current(); }
                ctx.request_repaint();
            }
        }

        // Escapeで全画面・ボーダレスを抜ける
        if !modal_open && k.esc {
            if self.is_fullscreen || self.is_borderless {
                self.is_fullscreen = false;
                self.is_borderless = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
        }

        // ── 設定ウィンドウ ──────────────────────────────────────────────
        if self.show_settings {
            if widgets::settings_window(ctx, &mut self.show_settings, &mut self.config, &mut self.settings_args_tmp) {
                self.save_config();
            }
        }

        // ── キーコンフィグウィンドウ ──────────────────────────────────────
        if self.show_key_config {
            // キーキャプチャの実行
            if let Some(action_id) = self.capturing_key_for.clone() {
                if let Some(combo) = input::detect_key_combination(ctx) {
                    self.config.keys.insert(action_id, combo);
                    self.capturing_key_for = None;
                    self.save_config();
                }
            }

            if widgets::key_config_window(ctx, &mut self.show_key_config, &mut self.config, &mut self.capturing_key_for) {
                self.save_config();
            }
        } else {
            self.capturing_key_for = None;
        }

        // ── 初回起動時のウェルカムダイアログ ────────────────────────────────
        if self.config.is_first_run {
            egui::Window::new("Hinjakuへようこそ！")
                .collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("初期設定ファイル(config.ini)を作成しました。");
                    ui.label("主要なショートカットキー：");
                    ui.label("・「S」：並べ替えの設定");
                    ui.label("・「K」：キーコンフィグ（録画機能付き！）");
                    ui.label("・「E」：外部アプリで開く（設定から変更可能）");
                    ui.add_space(12.0);
                    if ui.button("はじめる").clicked() {
                        self.config.is_first_run = false;
                        self.save_config();
                    }
                });
        }

        // ── ソート設定ウィンドウ ──────────────────────────────────────────
        if self.show_sort_settings {
            if widgets::sort_settings_window(ctx, &mut self.show_sort_settings, &mut self.config, &mut self.sort_focus_idx, k.enter) {
                self.manager.apply_sorting(&self.config);
                self.manager.clear_cache();
                self.save_config();
            }
        }

        // ── メインメニューとツールバーの処理 ────────────────────────────
        let menu_action = widgets::main_menu_bar(ctx, &self.config, &self.manager, self.display_mode, self.show_tree);

        // ── サイドパネル（ツリー表示） ────────────────────────────────────
        // ステータスバー（下部パネル）より先に定義することで、ツリーが画面左端をフルに占有するようにします
        let mut tree_open_req = None;
        if self.show_tree {
            let half_width = ctx.screen_rect().width() / 2.0;
            egui::SidePanel::left("tree_panel")
                .resizable(true)
                .default_width(half_width)
                .width_range(200.0..=half_width * 1.5)
                .show(ctx, |ui| {
                    widgets::sidebar_ui(ui, &mut self.manager.tree, &self.manager.archive_path, ctx, &mut tree_open_req);
                });
            
            // ツリーの描画が終わったので、スクロール要求フラグを下ろす
            self.manager.tree.scroll_to_selected = false;
        }

        let toolbar_action = widgets::bottom_toolbar(ctx, &self.manager, &self.config, self.display_mode, self.zoom, self.manga_mode, self.is_nav_locked(ctx));

        // 集約されたアクションを一括処理
        if let Some(act) = menu_action.or(toolbar_action) {
            match act {
                widgets::ViewerAction::OpenFolder => { if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); } },
                widgets::ViewerAction::RevealInExplorer => {
                    if let Some(path) = &self.manager.archive_path {
                        if let Err(e) = integrator::reveal_in_explorer(path) {
                            self.add_toast(e, ctx);
                        }
                    }
                },
                widgets::ViewerAction::OpenExternal(idx) => self.open_external(idx, ctx),
                widgets::ViewerAction::OpenExternalSettings => { 
                    self.settings_args_tmp = self.config.external_apps.iter().map(|app| app.args.join(" ")).collect(); 
                    self.show_settings = true; 
                },
                widgets::ViewerAction::OpenKeyConfig => { self.show_key_config = true; },
                widgets::ViewerAction::Exit => { ctx.send_viewport_cmd(egui::ViewportCommand::Close); },
                widgets::ViewerAction::SetDisplayMode(m) => { self.display_mode = m; if m == DisplayMode::Manual { self.zoom = 1.0; } },
                widgets::ViewerAction::ZoomIn => { self.zoom = (self.zoom * ZOOM_STEP).min(MAX_ZOOM); self.display_mode = DisplayMode::Manual; },
                widgets::ViewerAction::ZoomOut => { self.zoom = (self.zoom / ZOOM_STEP).max(MIN_ZOOM); self.display_mode = DisplayMode::Manual; },
                widgets::ViewerAction::ToggleManga => { self.manga_mode = !self.manga_mode; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode); },
                widgets::ViewerAction::ToggleMangaRtl => { self.config.manga_rtl = !self.config.manga_rtl; self.save_config(); },
                widgets::ViewerAction::ToggleTree => { self.show_tree = !self.show_tree; if self.show_tree { self.sync_tree_to_current(); } },
                widgets::ViewerAction::OpenSortSettings => { self.show_sort_settings = true; self.sort_focus_idx = 0; },
                widgets::ViewerAction::ToggleLinear => {
                    self.config.filter_mode = match self.config.filter_mode {
                        config::FilterMode::Nearest => config::FilterMode::Bilinear,
                        config::FilterMode::Bilinear => config::FilterMode::Bicubic,
                        config::FilterMode::Bicubic => config::FilterMode::Lanczos,
                        config::FilterMode::Lanczos => config::FilterMode::Nearest,
                    };
                    self.manager.clear_cache(); self.save_config(); self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode);
                },
                widgets::ViewerAction::ToggleMultipleInstances => { self.config.allow_multiple_instances = !self.config.allow_multiple_instances; self.save_config(); },
                widgets::ViewerAction::Rotate(cw) => self.rotate_current(cw, ctx),
                widgets::ViewerAction::GoPrevDir => self.go_prev_dir(ctx),
                widgets::ViewerAction::GoNextDir => self.go_next_dir(ctx),
                widgets::ViewerAction::SetOpenFromEnd(b) => { self.manager.open_from_end = b; },
                widgets::ViewerAction::SetBgMode(m) => { self.config.bg_mode = m; self.save_config(); },
                widgets::ViewerAction::PrevPage => self.go_prev(ctx),
                widgets::ViewerAction::NextPage => self.go_next(ctx),
                widgets::ViewerAction::NextDir => self.go_next_dir(ctx),
                widgets::ViewerAction::Seek(idx) => { self.manager.target_index = idx; self.folder_lock_until = 0.0; self.page_lock_until = 0.0; self.manager.schedule_prefetch(self.config.filter_mode, self.manga_mode); },
            }
        }
        if let Some(p) = tree_open_req { self.open_path(p, ctx); }

        // ── メイン表示エリア ────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            // 表示領域全体を背景設定で塗りつぶす（ロード中やエラー時も表示される）
            painter::paint_background(ui, ui.available_rect_before_wrap(), self.config.bg_mode);

            if let Some(err) = self.error.clone() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new(format!("⚠ {err}")).color(egui::Color32::RED));
                });
                return;
            }

            // ロード中スピナー（テクスチャ存在確認のみ。参照は取得しない）
            if self.get_texture(self.manager.current).is_none() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        if self.manager.entries.is_empty() && !self.manager.is_listing {
                            if self.manager.archive_path.is_none() {
                                ui.label(egui::RichText::new("画像をドラッグ＆ドロップ、またはメニューから開いてください").size(20.0).strong());
                            } else {
                                ui.label(egui::RichText::new("画像が見つかりませんでした").size(20.0).strong());
                                ui.add_space(10.0);
                                
                                if let Some(p) = &self.manager.archive_path {
                                    if let Some(parent) = p.as_path().parent() {
                                        if ui.button(format!("⤴ 上の階層へ: {}", parent.display())).clicked() {
                                            let current = p.clone();
                                            self.move_to_dir(parent.to_path_buf(), Some(current), false, ctx);
                                        }
                                    }
                                }
                                ui.add_space(10.0);
                            }
                        } else {
                            let current_file = self.manager.entries.get(self.manager.current)
                                .map(|s: &String| s.as_str())
                                .unwrap_or("");
                            ui.label(egui::RichText::new("⏳ 読み込み中...").size(18.0).color(egui::Color32::GRAY));
                            ui.label(egui::RichText::new(current_file).size(14.0).color(egui::Color32::DARK_GRAY));
                        }
                    });
                });
                return;
            }

            // マウス操作（self の可変借用が必要なのでテクスチャ参照の前に処理）
            let (wheel, ctrl, secondary) = ctx.input(|i| (
                i.smooth_scroll_delta.y,
                i.modifiers.ctrl,
                i.pointer.button_down(egui::PointerButton::Secondary)
            ));

            if wheel != 0.0 {
                if ctrl || secondary {
                    self.zoom = (self.zoom * (1.0 + wheel * WHEEL_ZOOM_SENSITIVITY)).clamp(MIN_ZOOM, MAX_ZOOM);
                    self.display_mode = DisplayMode::Manual;
                } else {
                    // スクロール感度の調整: 蓄積バッファがしきい値(40.0)を超えた時だけ発火
                    self.wheel_accumulator += wheel;
                    if self.wheel_accumulator.abs() >= WHEEL_NAV_THRESHOLD {
                        if self.wheel_accumulator > 0.0 { self.go_prev(ctx); }
                        else { self.go_next(ctx); }
                        // ページ移動時にサイズをリセット
                        self.display_mode = DisplayMode::Fit;
                        self.wheel_accumulator = 0.0;
                    }
                }
            } else {
                self.wheel_accumulator = 0.0; // 静止したらバッファリセット
            }

            let mode  = self.display_mode;
            let zoom  = self.zoom;

            // 最後の一枚（または最後のペア）を表示しているか判定
            let is_at_end = self.manager.current >= self.manager.entries.len().saturating_sub(2);

            // 描画ロジックを painter に委譲
            let (resp, p_action) = painter::draw_main_area(
                ui,
                &self.manager,
                mode,
                zoom,
                self.manga_mode,
                self.config.manga_rtl,
                self.manga_shift,
                ctx,
                is_at_end,
            );
            if let Some(widgets::ViewerAction::NextDir) = p_action { self.go_next_dir(ctx); }

            // クリックによるページ送り判定
            if click_allowed && resp.secondary_clicked() {
                self.go_prev(ctx);
            } else if click_allowed && resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let is_left = pos.x < resp.rect.center().x;
                    if is_left { self.go_prev(ctx); } else { self.go_next(ctx); }
                }
            }
        });

        // ── トースト通知の描画 ──────────────────────────────────────────
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|(_, expires)| *expires > now);

        if !self.toasts.is_empty() {
            egui::Area::new(egui::Id::new("toast_area"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        for (msg, _) in &self.toasts {
                            egui::Frame::group(ui.style())
                                .fill(egui::Color32::from_black_alpha(200))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(100)))
                                .rounding(4.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("⚠").color(egui::Color32::YELLOW).strong());
                                        ui.label(egui::RichText::new(msg).color(egui::Color32::WHITE));
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    });
                });
        }

        self.was_focused = is_focused;
    }
}
