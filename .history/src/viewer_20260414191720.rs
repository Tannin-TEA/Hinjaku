use crate::{archive, config::{self, Config, SortMode, SortOrder}, manager::{self, Manager}};
use chrono::{TimeZone, Local};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle, RichText, Color32};
use std::path::PathBuf;

/// アーカイブを開いた直後の誤操作防止ウェイト (秒)
const NAV_GUARD_DURATION: f64 = 2.0;
/// マウスホイールでページをめくる際のしきい値（大きいほど鈍感になる）
const WHEEL_NAV_THRESHOLD: f32 = 40.0;
/// 画像ロード待ち時の自動リトライ最大回数
const LOADING_MAX_RETRIES: u8 = 3;
/// 自動リトライの間隔 (ミリ秒)
const LOADING_RETRY_DELAY_MS: u64 = 15;
/// ズーム操作時の倍率ステップ
const ZOOM_STEP: f32 = 1.2;
/// ツリーのノードキャッシュをクリアするしきい値
const TREE_NODES_CACHE_LIMIT: usize = 1000;

pub struct App {
    manager: Manager,
    config: Config,

    /// マウスホイールの回転蓄積バッファ
    wheel_accumulator: f32,

    /// 設定画面の表示状態
    show_settings: bool,
    /// ツリー表示の表示状態
    show_tree: bool,
    /// ツリー表示の起点ディレクトリ
    tree_root: Option<PathBuf>,
    /// ソート設定画面の表示状態
    show_sort_settings: bool,
    /// ソート設定ウィンドウ内のフォーカス行 (0:基準, 1:順序, 2:自然順)
    sort_focus_idx: usize,
    /// 設定画面用の引数編集バッファ
    settings_args_tmp: String,
    /// config.ini のパス保持
    config_path: Option<PathBuf>,

    /// アーカイブ切り替え中で、最初の画像がロードされるのを待っている状態か
    is_loading_archive: bool,
    /// フォルダ移動直後の最初のページ表示中か（ウェイト判定用）
    is_first_page_of_folder: bool,

    /// ロード待ちのリフレッシュ試行回数
    loading_retry_count: u8,

    /// 最後に画像が実際に切り替わった時刻
    last_display_change_time: f64,

    last_target_index: usize,

    /// 前のフレームでフォーカスされていたか（誤クリック防止用）
    was_focused: bool,

    error: Option<String>,
    fit: bool, zoom: f32, manga_mode: bool, manga_shift: bool,
    is_fullscreen: bool,
    is_borderless: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>, config_name: Option<String>) -> Self {
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
        let settings_args_tmp = config.external_args.join(" ");

        let mut app = Self {
            manager: Manager::new(cc.egui_ctx.clone()),
            config,
            wheel_accumulator: 0.0,
            show_settings: false,
            show_tree: false,
            tree_root: None,
            show_sort_settings: false,
            sort_focus_idx: 0,
            settings_args_tmp,
            config_path,
            is_loading_archive: false,
            is_first_page_of_folder: true,
            loading_retry_count: 0,
            last_display_change_time: 0.0,
            last_target_index: 0,
            was_focused: true,
            error: None, fit: true, zoom: 1.0, manga_mode: false, manga_shift: false,
            is_fullscreen: false,
            is_borderless: false,
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }

        app
    }

    fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        self.manager.get_first_tex(index)
    }

    /// アニメーション対応のテクスチャ取得。
    fn get_texture_animated(&self, index: usize, ctx: &egui::Context) -> Option<(&TextureHandle, egui::Vec2)> {
        let now = ctx.input(|i| i.time);
        let (tex, next_frame_sec) = self.manager.get_tex(index, now)?;
        if let Some(secs) = next_frame_sec {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(secs));
        }
        Some((tex, tex.size_vec2()))
    }

    /// ツリーの選択状態と展開状態を現在のアーカイブパスに強制同期する
    fn sync_tree_to_current(&mut self) {
        if let Some(path) = self.manager.archive_path.clone() {
            let cleaned = archive::clean_path(&path);
            self.manager.tree.expand_to_path(&cleaned);
            self.manager.tree.selected = Some(cleaned);
        }
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.error = None;
        self.manager.open_path(path, &self.config);
        
        // ツリーのノードキャッシュが1000件を超えたら一度リセットしてメモリを節約する
        if self.manager.tree.nodes.len() > TREE_NODES_CACHE_LIMIT {
            self.manager.tree.clear_metadata_cache();
        }

        // 初回ロード時やツリーの外のファイルを開いた時、起点を親フォルダに設定
        if self.tree_root.is_none() {
            self.tree_root = self.manager.archive_path.as_ref()
                .and_then(|p| p.parent().map(|parent| parent.to_path_buf()));
        }
        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        self.is_first_page_of_folder = true;
        self.last_display_change_time = ctx.input(|i| i.time);
        ctx.request_repaint();
    }

    fn ui_dir_tree(
        nav_tree: &mut manager::NavTree,
        current_path: &Option<PathBuf>,
        ui: &mut egui::Ui,
        path: PathBuf,
        ctx: &egui::Context,
        open_req: &mut Option<PathBuf>,
    ) {
        // この関数に渡される path は既に clean 済みであると想定する（呼び出し側で一度だけ行う）
        let filename = archive::get_display_name(&path);

        let kind = archive::detect_kind(&path);
        let is_archive = matches!(kind, archive::ArchiveKind::Zip | archive::ArchiveKind::SevenZ);
        let icon = if is_archive { "📦 " } else { "📁 " };

        let is_current = current_path.as_ref() == Some(&path);
        let is_selected = nav_tree.selected.as_ref() == Some(&path);

        let text = RichText::new(format!("{}{}", icon, filename));
        let text = if is_current { text.color(Color32::YELLOW) } else { text };
        let text = if is_selected { text.background_color(ui.visuals().selection.bg_fill.linear_multiply(0.3)) } else { text };

        if is_archive {
            let resp = ui.selectable_label(is_selected, text);
            if is_selected { resp.scroll_to_me(Some(egui::Align::Center)); }
            if resp.clicked() { *open_req = Some(path); }
        } else {
            let is_expanded = nav_tree.expanded.contains(&path);
            let response = egui::CollapsingHeader::new(text)
                .id_source(&path)
                .open(Some(is_expanded))
                .show(ui, |ui| {
                    let children = nav_tree.get_children(&path);
                    for p in children {
                        Self::ui_dir_tree(nav_tree, current_path, ui, p, ctx, open_req);
                    }
                });

            if is_selected { response.header_response.scroll_to_me(Some(egui::Align::Center)); }

            if response.header_response.clicked() {
                nav_tree.selected = Some(path.clone());
                if is_expanded {
                    nav_tree.expanded.remove(&path);
                } else {
                    nav_tree.expanded.insert(path);
                }
            }
        }
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
        self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode);
        ctx.request_repaint();
    }

    fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, ctx: &egui::Context) {
        if let Some(root) = &self.tree_root {
            if !path.starts_with(root) { self.tree_root = path.parent().map(|p| p.to_path_buf()); }
        }
        self.error = None;
        self.manager.move_to_dir(path, focus_hint, go_last, &self.config, self.manga_mode, self.manga_shift);
        self.sync_tree_to_current();
        self.is_loading_archive = !self.manager.entries.is_empty();
        self.is_first_page_of_folder = true;
        self.last_display_change_time = ctx.input(|i| i.time);
        ctx.request_repaint();
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) {
        self.navigate_relative_dir(false, ctx);
    }
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
        // フォルダ移動後の最初の画像が表示されてから一定時間内は操作をロックする
        self.is_first_page_of_folder && (now - self.last_display_change_time) < NAV_GUARD_DURATION
    }

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        if !self.manager.go_prev(false, false, self.config.linear_filter) { self.go_prev_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        if !self.manager.go_next(false, false, self.config.linear_filter) { self.go_next_dir(ctx); }
        else { ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        if !self.manager.go_prev(self.manga_mode, self.manga_shift, self.config.linear_filter) {
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        if !self.manager.go_next(self.manga_mode, self.manga_shift, self.config.linear_filter) {
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        self.manager.target_index = 0; self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode); ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.is_first_page_of_folder = false;
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.manga_mode && last > 0 && last % 2 == 0 { last.saturating_sub(1) } else { last };
        self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode); ctx.request_repaint();
    }

    fn open_external(&self) {
        let Some(target_str) = self.manager.get_current_full_path() else { return };

        if !self.config.external_app.is_empty() {
            let mut cmd = std::process::Command::new(&self.config.external_app);
            if self.config.external_args.is_empty() {
                cmd.arg(&target_str);
            } else {
                for arg in &self.config.external_args {
                    cmd.arg(arg.replace("%P", &target_str));
                }
            }
            let _ = cmd.spawn();
        }
    }

    fn save_config(&self) {
        if let Some(ref path) = self.config_path { let _ = std::fs::write(path, toml::to_string_pretty(&self.config).unwrap_or_default()); }
    }
}

fn draw_centered(
    ui: &mut egui::Ui,
    tex_id: egui::TextureId,
    tex_size: egui::Vec2,
    avail: egui::Vec2,
    fit: bool,
    zoom: f32,
) -> egui::Response {
    let display_size = if fit {
        let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y).min(1.0);
        tex_size * scale
    } else {
        tex_size * zoom
    };
    let area = egui::vec2(display_size.x.max(avail.x), display_size.y.max(avail.y));
    let off  = egui::vec2(((area.x - display_size.x)*0.5).max(0.0), ((area.y - display_size.y)*0.5).max(0.0));
    let (rect, resp) = ui.allocate_exact_size(area, egui::Sense::click());
    let img_rect = egui::Rect::from_min_size(rect.min + off, display_size);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0));
    ui.painter().image(tex_id, img_rect, uv, egui::Color32::WHITE);
    resp
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

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
                self.manager.current = target;
                self.error = None;
                self.is_loading_archive = false;
                self.loading_retry_count = 0;
                self.last_display_change_time = ctx.input(|i| i.time);
            } else if self.loading_retry_count < LOADING_MAX_RETRIES {
                // 15ms間隔で最大3回だけ自動リフレッシュ（バックグラウンド通知の予備）
                self.loading_retry_count += 1;
                ctx.request_repaint_after(std::time::Duration::from_millis(LOADING_RETRY_DELAY_MS));
            }
        }

        // ── ドラッグ＆ドロップ ──────────────────────────────────────────
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ── キーボード ──────────────────────────────────────────────────
        let (left, right, up, dn, enter_key, esc_key, t_key, fit_t, zin, zout, manga_t, rcw, rccw, pgup, pgdn, p_key, n_key, s_key, home, end, bs_key, e_key, i_key, alt_pressed, y_key, q_key, ctrl_w) = ctx.input(|i| (
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::T),
            i.key_pressed(egui::Key::F),
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            i.key_pressed(egui::Key::Minus),
            i.key_pressed(egui::Key::M) || i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::R) && !i.modifiers.ctrl,
            i.key_pressed(egui::Key::R) &&  i.modifiers.ctrl,
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::P),
            i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::I),
            i.modifiers.alt,
            i.key_pressed(egui::Key::Y),
            i.key_pressed(egui::Key::Q),
            i.modifiers.ctrl && i.key_pressed(egui::Key::W),
        ));

        // ソート/外部アプリ設定ウィンドウが開いている間はメイン操作を無効化
        let modal_open = self.show_sort_settings || self.show_settings;

        // ── モード別のキー入力処理 ──────────────────────────────────────
        if self.show_tree && !modal_open {
            // ツリー操作モード：メイン画面の操作を完全に遮断
            let old_selected = self.manager.tree.selected.clone();

            if up { self.manager.tree.move_selection(-1); }
            if dn { self.manager.tree.move_selection(1); }
            if right { self.manager.tree.expand_current(); }
            if left { self.manager.tree.collapse_or_up(); }

            // 選択が変更された場合、プレビューとしてそのパスを開く（一番最初の画像を表示）
            let new_selected = self.manager.tree.selected.clone();
            if new_selected != old_selected {
                if let Some(path) = new_selected {
                    // ツリー表示を維持したまま、バックグラウンドのビューアの中身を更新
                    self.open_path(path, ctx);
                }
            }

            if enter_key {
                if let Some(path) = self.manager.tree.activate_current() {
                    self.open_path(path, ctx);
                    self.show_tree = false;
                }
            }
            if esc_key { self.show_tree = false; }
        } else if !modal_open {
            // 通常ビューアモード
            if left || p_key { self.go_prev(ctx); }
            if right || n_key { self.go_next(ctx); }

            if up { if self.manga_mode { self.go_single_prev(ctx); } else { self.go_prev(ctx); } }
            if dn { if self.manga_mode { self.go_single_next(ctx); } else { self.go_next(ctx); } }

            if home { self.go_first(ctx); }
            if end  { self.go_last(ctx); }
            if fit_t  { self.fit = !self.fit; }
            if zin    { self.zoom = (self.zoom * ZOOM_STEP).min(10.0); self.fit = false; }
            if zout   { self.zoom = (self.zoom / ZOOM_STEP).max(0.1);  self.fit = false; }
            if rcw    { self.rotate_current(true,  ctx); }
            if rccw   { self.rotate_current(false, ctx); }
            if pgup   { self.go_prev_dir(ctx); }
            if pgdn   { self.go_next_dir(ctx); }
            if manga_t {
                self.manga_mode = !self.manga_mode;
                self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode);
                ctx.request_repaint();
            }
            if y_key {
                self.config.manga_rtl = !self.config.manga_rtl;
                self.save_config();
            }
            if i_key {
                self.config.linear_filter = !self.config.linear_filter;
                self.manager.clear_cache();
                self.save_config();
                self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode);
            }
            if bs_key {
                if let Some(path) = &self.manager.archive_path {
                    archive::reveal_in_explorer(path);
                }
            }
            if e_key { self.open_external(); }

            if q_key || ctrl_w { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }

            // 全画面切替
            if enter_key {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                if alt_pressed {
                    self.is_borderless = !self.is_borderless;
                    self.is_fullscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_borderless));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_borderless));
                } else {
                    self.is_fullscreen = !self.is_fullscreen;
                    self.is_borderless = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_fullscreen));
                }
            }
        }

        // TキーとSキーはどのモードからでも（あるいは特定条件下で）受け付ける
        if s_key {
            self.show_sort_settings = !self.show_sort_settings;
            if self.show_sort_settings { self.sort_focus_idx = 0; }
        }
        if t_key { 
            self.show_tree = !self.show_tree; 
            if self.show_tree { self.sync_tree_to_current(); }
            ctx.request_repaint();
        }

        // Escapeで全画面・ボーダレスを抜ける
        if !modal_open && esc_key {
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
            egui::Window::new("外部アプリ連携の設定")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Eキーを押した時に起動するソフトを設定します。");
                    ui.add_space(8.0);

                    egui::Grid::new("config_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
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
                            self.config.external_args = self.settings_args_tmp.split_whitespace().map(|s| s.to_string()).collect();
                            self.save_config();
                            self.show_settings = false;
                        }
                        if ui.button("キャンセル").clicked() { self.show_settings = false; }
                    });
                });
        }

        // ── ソート設定ウィンドウ ──────────────────────────────────────────
        if self.show_sort_settings {
            egui::Window::new("並べ替えの設定 (S)")
                .collapsible(false).resizable(false)
                .show(ctx, |ui| {
                    let mut changed = false;

                    // キーボードナビゲーション
                    let (arr_up, arr_dn, arr_left, arr_right) = ctx.input(|i| (
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::ArrowLeft),
                        i.key_pressed(egui::Key::ArrowRight),
                    ));

                    if arr_up { self.sort_focus_idx = (self.sort_focus_idx + 2) % 3; }
                    if arr_dn { self.sort_focus_idx = (self.sort_focus_idx + 1) % 3; }
                    if enter_key { self.show_sort_settings = false; }

                    ui.label("矢印キーで選択 / Enterで戻る");
                    ui.add_space(8.0);
                    
                    ui.horizontal(|ui| {
                        let active = self.sort_focus_idx == 0;
                        let label = if active { egui::RichText::new("▶ 基準:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  基準:") };
                        ui.label(label);
                        changed |= ui.radio_value(&mut self.config.sort_mode, SortMode::Name, "ファイル名").changed();
                        changed |= ui.radio_value(&mut self.config.sort_mode, SortMode::Mtime, "更新日時").changed();
                        changed |= ui.radio_value(&mut self.config.sort_mode, SortMode::Size, "サイズ").changed();
                        
                        if active {
                            if arr_right {
                                self.config.sort_mode = match self.config.sort_mode {
                                    SortMode::Name => SortMode::Mtime, SortMode::Mtime => SortMode::Size, SortMode::Size => SortMode::Name,
                                };
                                changed = true;
                            }
                            if arr_left {
                                self.config.sort_mode = match self.config.sort_mode {
                                    SortMode::Name => SortMode::Size, SortMode::Mtime => SortMode::Name, SortMode::Size => SortMode::Mtime,
                                };
                                changed = true;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        let active = self.sort_focus_idx == 1;
                        let label = if active { egui::RichText::new("▶ 順序:").color(egui::Color32::YELLOW) } else { egui::RichText::new("  順序:") };
                        ui.label(label);
                        changed |= ui.radio_value(&mut self.config.sort_order, SortOrder::Ascending, "昇順").changed();
                        changed |= ui.radio_value(&mut self.config.sort_order, SortOrder::Descending, "降順").changed();

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
                    let check_text = if active { egui::RichText::new("自然順（数字の大きさを考慮）").color(egui::Color32::YELLOW) } else { egui::RichText::new("自然順（数字の大きさを考慮）") };
                    if ui.checkbox(&mut self.config.sort_natural, check_text).on_hover_text("1, 2, 10 の順に並べます。").changed() {
                        changed = true;
                    }
                    if active && (arr_left || arr_right) {
                        self.config.sort_natural = !self.config.sort_natural;
                        changed = true;
                    }

                    if changed {
                        self.manager.apply_sorting(&self.config);
                        self.manager.clear_cache();
                        self.save_config();
                    }

                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        if ui.button("閉じる").clicked() { self.show_sort_settings = false; }
                    });
                });
        }

        // ── サイドパネル（ツリー表示） ────────────────────────────────────
        let mut tree_open_req = None;
        if self.show_tree {
            egui::SidePanel::left("tree_panel")
                .default_width(ctx.screen_rect().width() / 2.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.heading("ツリー");
                            if let Some(root) = &self.tree_root {
                                if let Some(parent) = root.parent() {
                                    if ui.button("⤴ 上へ").on_hover_text("親ディレクトリを起点にする").clicked() {
                                        self.tree_root = Some(parent.to_path_buf());
                                    }
                                }
                            }
                        });
                        ui.separator();

                        // 下部のステータスエリアを先に定義（スクロールエリアの外）
                        let status_height = 24.0;
                        let scroll_height = ui.available_height() - status_height - 10.0;

                        egui::ScrollArea::both()
                            .max_height(scroll_height)
                            .show(ui, |ui| {
                                let roots = archive::get_roots();
                                for root in roots {
                                    Self::ui_dir_tree(&mut self.manager.tree, &self.manager.archive_path, ui, root, ctx, &mut tree_open_req);
                                }
                            });

                        ui.separator();
                        // ステータス表示
                        if let Some(sel) = self.manager.tree.selected.clone() {
                            let count = self.manager.tree.get_image_count(&sel);
                            let name = sel.file_name().map(|f: &std::ffi::OsStr| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| sel.to_string_lossy().to_string());
                            
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("選択中: {} ({} 枚の画像)", name, count)).small());
                            });
                        }
                    });
                });
        }
        if let Some(p) = tree_open_req { self.open_path(p, ctx); }

        // ── メニューバー ────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("フォルダを開く…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); }
                    }
                    ui.separator();
                    if ui.add_enabled(self.manager.archive_path.is_some(), egui::Button::new("エクスプローラーで表示 (BS)")).clicked() {
                        ui.close_menu();
                        if let Some(path) = &self.manager.archive_path {
                            archive::reveal_in_explorer(path);
                        }
                    }
                    if ui.add_enabled(self.manager.archive_path.is_some(), egui::Button::new("外部アプリで開く (E)")).clicked() {
                        ui.close_menu();
                        self.open_external();
                    }
                    if ui.button("外部アプリ設定…").clicked() {
                        ui.close_menu();
                        self.settings_args_tmp = self.config.external_args.join(" ");
                        self.show_settings = true;
                    }
                    ui.separator();
                    if ui.button("終了").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button("表示", |ui| {
                    if ui.selectable_label(self.fit,  "フィット表示 (F)").clicked() { self.fit=true;  ui.close_menu(); }
                    if ui.selectable_label(!self.fit, "等倍表示").clicked()         { self.fit=false; self.zoom=1.0; ui.close_menu(); }
                    ui.separator();
                    if ui.button("拡大 (+)").clicked() { self.zoom=(self.zoom*1.2).min(10.0); self.fit=false; ui.close_menu(); }
                    if ui.button("縮小 (-)").clicked() { self.zoom=(self.zoom/1.2).max(0.1);  self.fit=false; ui.close_menu(); }
                    ui.separator();
                    if ui.selectable_label(self.manga_mode, "マンガモード (M)").clicked() {
                        self.manga_mode = !self.manga_mode;
                        self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode); ctx.request_repaint(); ui.close_menu();
                    }
                    if ui.selectable_label(self.config.manga_rtl, "右開き表示 (Y)").clicked() {
                        self.config.manga_rtl = !self.config.manga_rtl;
                        self.save_config();
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.show_tree, "ツリー表示 (T)").clicked() { self.show_tree = !self.show_tree; ui.close_menu(); }
                    if ui.button("並べ替えの設定 (S)").clicked() {
                        self.show_sort_settings = true;
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.config.linear_filter, "画像の補正(スムージング) (I)").clicked() {
                        self.config.linear_filter = !self.config.linear_filter;
                        self.manager.clear_cache();
                        self.save_config();
                        self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode);
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.config.allow_multiple_instances, "複数起動を許可").clicked() {
                        self.save_config();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("右回転 (R)").clicked()      { self.rotate_current(true,  ctx); ui.close_menu(); }
                    if ui.button("左回転 (Ctrl+R)").clicked() { self.rotate_current(false, ctx); ui.close_menu(); }
                });
                ui.menu_button("フォルダ", |ui| {
                    if ui.button("前のフォルダ (PgUp)").clicked() { self.go_prev_dir(ctx); ui.close_menu(); }
                    if ui.button("次のフォルダ (PgDn)").clicked() { self.go_next_dir(ctx); ui.close_menu(); }
                    ui.separator();
                    ui.label("フォルダ移動時の設定:");
                    ui.radio_value(&mut self.manager.open_from_end, false, "先頭から開く");
                    ui.radio_value(&mut self.manager.open_from_end, true, "末尾から開く");
                });
            });
        });

        // ── ツールバー ──────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let has = !self.manager.entries.is_empty();
                if ui.add_enabled(has, egui::Button::new("◀ (P)")).clicked() { self.go_prev(ctx); }
                
                // ページシークスライダー
                if has {
                    let max_idx = self.manager.entries.len().saturating_sub(1);
                    let mut slider_val = self.manager.target_index;
                    ui.style_mut().spacing.slider_width = 160.0;
                    let slider = egui::Slider::new(&mut slider_val, 0..=max_idx)
                        .show_value(false)
                        .trailing_fill(true);
                    if ui.add_enabled(!self.is_nav_locked(ctx), slider).changed() {
                        self.manager.target_index = slider_val;
                        self.is_first_page_of_folder = false;
                        self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode);
                    }
                }

                if ui.add_enabled(has, egui::Button::new("▶ (N)")).clicked() { self.go_next(ctx); }
                ui.separator();
                if ui.add_enabled(has, egui::Button::new("⟲")).on_hover_text("左回転 Ctrl+R").clicked() { self.rotate_current(false, ctx); }
                if ui.add_enabled(has, egui::Button::new("⟳")).on_hover_text("右回転 R").clicked()      { self.rotate_current(true,  ctx); }
                ui.separator();
                let ml = if self.manga_mode { "📖 2P" } else { "📄 1P" };
                if ui.button(ml).on_hover_text("マンガモード (M)").clicked() {
                    self.manga_mode = !self.manga_mode;
                    self.manager.schedule_prefetch(self.config.linear_filter, self.manga_mode); ctx.request_repaint();
                }
                ui.separator();
                if has {
                    let meta = &self.manager.entries_meta[self.manager.target_index];
                    let short = archive::get_display_name(std::path::Path::new(&meta.name));

                    let count = format!("{}/{}", self.manager.target_index + 1, self.manager.entries.len());

                    let file_size = if meta.size >= 1024*1024 {
                        format!("{:.1} MB", meta.size as f64 / (1024.0*1024.0))
                    } else if meta.size >= 1024 {
                        format!("{:.0} KB", meta.size as f64 / 1024.0)
                    } else {
                        format!("{} B", meta.size)
                    };

                    let day_str = if meta.mtime > 0 {
                        Local.timestamp_opt(meta.mtime as i64, 0).unwrap().format("%Y/%m/%d").to_string()
                    } else {
                        "----/--/--".to_string()
                    };

                    let sort_label = match self.config.sort_mode {
                        SortMode::Name => "Name",
                        SortMode::Mtime => "Day",
                        SortMode::Size => "Size",
                    };
                    let sort_icon = if self.config.sort_order == SortOrder::Ascending { "▲" } else { "▼" };

                    let loading = self.get_texture(self.manager.target_index).is_none();
                    let status  = if loading { " ⏳" } else { "" };
                    ui.label(format!("{}{} | {} | {} | {} | [{} {}]", 
                        count, status, short, day_str, file_size, sort_label, sort_icon));
                    if !self.fit {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("{:.0}%", self.zoom*100.0));
                        });
                    }
                } else {
                    ui.label("ファイルをドラッグ＆ドロップ、またはメニューから開いてください");
                }
            });
        });

        // ── メイン表示エリア ────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
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
                    self.zoom = (self.zoom * (1.0 + wheel * 0.002)).clamp(0.05, 10.0);
                    self.fit  = false;
                } else {
                    // スクロール感度の調整: 蓄積バッファがしきい値(40.0)を超えた時だけ発火
                    self.wheel_accumulator += wheel;
                    if self.wheel_accumulator.abs() >= WHEEL_NAV_THRESHOLD {
                        if self.wheel_accumulator > 0.0 { self.go_prev(ctx); }
                        else { self.go_next(ctx); }
                        // ページ移動時にサイズをリセット
                        self.fit = true;
                        self.zoom = 1.0;
                        self.wheel_accumulator = 0.0;
                    }
                }
            } else {
                self.wheel_accumulator = 0.0; // 静止したらバッファリセット
            }

            let avail = ui.available_size();
            let fit   = self.fit;
            let zoom  = self.zoom;

            // ホイール処理が終わった後で参照を取得（借用の競合を回避）
            let tex1 = self.get_texture_animated(self.manager.current, ctx);
            let (tex1_ref, tex1_size) = tex1.unwrap(); // is_none チェック済み
            let tex1_id = tex1_ref.id();
            
            // 2枚目の取得判定: マンガモード かつ 表紙(0)以外 かつ 1枚目が見開きでない
            let can_pair = (self.manga_shift || self.manager.current > 0) && tex1_size.x <= tex1_size.y;

            let tex2 = if self.manga_mode && can_pair {
                self.get_texture_animated(self.manager.current + 1, ctx).and_then(|(t, s)| {
                    if s.x <= s.y { Some((t.id(), s)) } else { None } // 2枚目も見開きならペアにしない
                })
            } else {
                None
            };

            egui::ScrollArea::both().show(ui, |ui| {
                if self.manga_mode {
                    if let Some((tex2_id, tex2_size)) = tex2 {
                        // 2枚並べ（右→左）
                        let half = egui::vec2(avail.x / 2.0, avail.y);
                        let s1 = if fit { (half.x/tex1_size.x).min(half.y/tex1_size.y).min(1.0) } else { zoom };
                        let s2 = if fit { (half.x/tex2_size.x).min(half.y/tex2_size.y).min(1.0) } else { zoom };
                        let ds1 = tex1_size * s1;
                        let ds2 = tex2_size * s2;
                        let total_w = (ds1.x + ds2.x).max(avail.x);
                        let total_h = ds1.y.max(ds2.y).max(avail.y);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::click());
                        let cx = rect.min.x + total_w / 2.0;
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0));
                        
                        if self.config.manga_rtl {
                            // 右開き: 右に1枚目(n)、左に2枚目(n+1)
                            ui.painter().image(tex1_id, egui::Rect::from_min_size(egui::pos2(cx, rect.min.y+(total_h-ds1.y)/2.0), ds1), uv, egui::Color32::WHITE);
                            ui.painter().image(tex2_id, egui::Rect::from_min_size(egui::pos2(cx-ds2.x, rect.min.y+(total_h-ds2.y)/2.0), ds2), uv, egui::Color32::WHITE);
                        } else {
                            // 左開き: 左に1枚目(n)、右に2枚目(n+1)
                            ui.painter().image(tex1_id, egui::Rect::from_min_size(egui::pos2(cx-ds1.x, rect.min.y+(total_h-ds1.y)/2.0), ds1), uv, egui::Color32::WHITE);
                            ui.painter().image(tex2_id, egui::Rect::from_min_size(egui::pos2(cx, rect.min.y+(total_h-ds2.y)/2.0), ds2), uv, egui::Color32::WHITE);
                        }

                        if click_allowed && resp.secondary_clicked() {
                            self.go_prev(ctx);
                        } else if click_allowed && resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let is_left = pos.x < rect.center().x;
                                if is_left { self.go_prev(ctx); } else { self.go_next(ctx); }
                            }
                        }
                    } else {
                        // 2枚目まだロード中
                        let resp = draw_centered(ui, tex1_id, tex1_size, avail, fit, zoom);
                        if click_allowed && resp.secondary_clicked() {
                            self.go_prev(ctx);
                        } else if click_allowed && resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let is_left = pos.x < resp.rect.center().x;
                                if is_left { self.go_prev(ctx); } else { self.go_next(ctx); }
                            }
                        }
                        ctx.request_repaint();
                    }
                } else {
                    let resp = draw_centered(ui, tex1_id, tex1_size, avail, fit, zoom);
                    if click_allowed && resp.secondary_clicked() {
                        self.go_prev(ctx);
                    } else if click_allowed && resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let is_left = pos.x < resp.rect.center().x;
                            if is_left { self.go_prev(ctx); } else { self.go_next(ctx); }
                        }
                    }
                }

                // 最後の一枚（または最後のペア）を表示している場合、次のフォルダへの案内を出す
                let is_at_end = if tex2.is_some() {
                    self.manager.current + 1 >= self.manager.entries.len().saturating_sub(1)
                } else {
                    self.manager.current >= self.manager.entries.len().saturating_sub(1)
                };

                if is_at_end {
                    if let Some(curr) = &self.manager.archive_path {
                        let siblings = self.manager.tree.get_siblings(curr);
                        if let Some(pos) = siblings.iter().position(|p| p == curr).filter(|&i| i + 1 < siblings.len()) {
                            let next_path = &siblings[pos + 1];
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                let btn_text = format!("次のフォルダへ: {} ➡", next_path.file_name().unwrap_or_default().to_string_lossy());
                                if ui.button(egui::RichText::new(btn_text).size(20.0).strong()).clicked() {
                                    self.go_next_dir(ctx);
                                }
                            });
                            ui.add_space(48.0);
                        }
                    }
                }
            });
        });

        self.was_focused = is_focused;
    }
}
