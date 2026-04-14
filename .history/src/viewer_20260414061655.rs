use crate::{archive, config, loader, navigator};
use chrono::{TimeZone, Local};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::io::Read;

use loader::{ImageLoader, Rotation};
use navigator::Navigator;
use config::Config;

// ── メインアプリ ─────────────────────────────────────────────────────────────
pub struct App {
    navigator: Navigator,
    loader: ImageLoader,
    config: Config,

    /// マウスホイールの回転蓄積バッファ
    wheel_accumulator: f32,

    /// 外部インスタンスから送られてきたパスの受信
    path_rx: Receiver<PathBuf>,

    /// 設定画面の表示状態
    show_settings: bool,
    /// ツリー表示の表示状態
    show_tree: bool,
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

    /// 最後に画像が実際に切り替わった時刻
    last_display_change_time: f64,

    /// 前のフレームでフォーカスされていたか（誤クリック防止用）
    was_focused: bool,

    error: Option<String>,
    fit: bool, zoom: f32, manga_mode: bool, manga_shift: bool,
    is_fullscreen: bool,
    is_borderless: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>, listener: Option<std::net::TcpListener>) -> Self {
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

        let (config, config_path) = config::load_config_file();
        let settings_args_tmp = config.external_args.join(" ");
        let (path_tx, path_rx) = std::sync::mpsc::channel();

        // リスナーが渡された場合、通信待ち受けスレッドを起動
        if let Some(l) = listener {
            let tx = path_tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for stream in l.incoming() {
                    if let Ok(mut s) = stream {
                        let mut buf = String::new();
                        if s.read_to_string(&mut buf).is_ok() {
                            let _ = tx.send(PathBuf::from(buf.trim()));
                            ctx.request_repaint(); // UIを即座に更新
                        }
                    }
                }
            });
        }

        let mut app = Self {
            navigator: Navigator::new(),
            loader: ImageLoader::new(),
            config,
            wheel_accumulator: 0.0,
            path_rx,
            show_settings: false,
            show_tree: true,
            show_sort_settings: false,
            sort_focus_idx: 0,
            settings_args_tmp,
            config_path,
            is_loading_archive: false,
            last_display_change_time: 0.0,
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

    // ── 先読みリクエストを発行 ────────────────────────────────────────────
    fn schedule_prefetch(&mut self) {
        if let Some(ref path) = self.navigator.archive_path {
            self.loader.schedule_prefetch(self.navigator.target_index, path, &self.navigator.entries, &self.navigator.rotations, self.manga_mode, self.config.linear_filter);
        }
    }

    fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        let entry = self.navigator.entries.get(index)?;
        let key = format!("{}:{}", index, entry);
        self.loader.get_texture(&key)
    }

    fn is_spread(&self, index: usize) -> bool {
        self.get_texture(index).map(|t| t.size_vec2().x > t.size_vec2().y).unwrap_or(false)
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.loader.clear();
        self.navigator.open_path(path, &self.config);
        self.is_loading_archive = true;
        self.last_display_change_time = ctx.input(|i| i.time);
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    fn ui_dir_tree(&self, ui: &mut egui::Ui, path: PathBuf, ctx: &egui::Context, open_req: &mut Option<PathBuf>) {
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let kind = archive::detect_kind(&path);
        let is_archive = matches!(kind, archive::ArchiveKind::Zip | archive::ArchiveKind::SevenZ);
        let icon = if is_archive { "📦 " } else { "📁 " };
        let is_current = self.navigator.archive_path.as_ref() == Some(&path);
        
        let header = egui::CollapsingHeader::new(format!("{}{}", icon, filename))
            .id_source(&path)
            .selectable(true)
            .selected(is_current);

        let response = header.show(ui, |ui| {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                paths.sort_by(|a, b| archive::natord(&a.to_string_lossy(), &b.to_string_lossy()));
                for p in paths {
                    if p.is_dir() || matches!(archive::detect_kind(&p), archive::ArchiveKind::Zip | archive::ArchiveKind::SevenZ) {
                        self.ui_dir_tree(ui, p, ctx, open_req);
                    }
                }
            }
        });
        if response.header_response.clicked() { *open_req = Some(path); }
    }

    fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.manga_mode {
            vec![self.navigator.current, self.navigator.current + 1]
        } else { vec![self.navigator.current] };

        for idx in indices {
            if let Some(name) = self.navigator.entries.get(idx).cloned() {
                let rot = self.navigator.rotations.get(&name).copied().unwrap_or(Rotation::R0);
                self.navigator.rotations.insert(name, if cw { rot.cw() } else { rot.ccw() });
                self.loader.invalidate(&format!("{}:{}", idx, self.navigator.entries[idx]));
            }
        }
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    fn move_to_dir(&mut self, path: PathBuf, go_last: bool, ctx: &egui::Context) {
        self.loader.clear();
        self.navigator.move_to_dir(path, go_last, &self.config, self.manga_mode, self.manga_shift);
        self.is_loading_archive = true;
        self.last_display_change_time = ctx.input(|i| i.time);
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if let Some((siblings, idx)) = self.navigator.sibling_dirs() {
            if idx > 0 { self.move_to_dir(siblings[idx-1].clone(), self.navigator.open_from_end, ctx); }
        }
    }
    fn go_next_dir(&mut self, ctx: &egui::Context) {
        if let Some((siblings, idx)) = self.navigator.sibling_dirs() {
            if idx+1 < siblings.len() { self.move_to_dir(siblings[idx+1].clone(), false, ctx); }
        }
    }

    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.navigator.target_index == 0 { self.go_prev_dir(ctx); }
        else { self.navigator.target_index -= 1; self.schedule_prefetch(); ctx.request_repaint(); }
    }

    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.navigator.target_index + 1 >= self.navigator.entries.len() { self.go_next_dir(ctx); }
        else { self.navigator.target_index += 1; self.schedule_prefetch(); ctx.request_repaint(); }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if !self.navigator.go_prev(self.manga_mode, self.manga_shift, |idx| self.is_spread(idx)) {
            self.go_prev_dir(ctx);
        } else { self.schedule_prefetch(); ctx.request_repaint(); }
    }

    fn go_next(&mut self, ctx: &egui::Context) {
        if !self.navigator.go_next(self.manga_mode, self.manga_shift, |idx| self.is_spread(idx)) {
            self.go_next_dir(ctx);
        } else { self.schedule_prefetch(); ctx.request_repaint(); }
    }

    fn go_first(&mut self, ctx: &egui::Context) {
        self.navigator.target_index = 0; self.schedule_prefetch(); ctx.request_repaint();
    }

    fn go_last(&mut self, ctx: &egui::Context) {
        let last = self.navigator.entries.len().saturating_sub(1);
        self.navigator.target_index = if self.manga_mode && last > 0 && last % 2 == 0 { last.saturating_sub(1) } else { last };
        self.schedule_prefetch(); ctx.request_repaint();
    }

    fn open_external(&self) {
        let Some(path) = &self.navigator.archive_path else { return };
        if self.navigator.entries.is_empty() { return; }
        let entry = &self.navigator.entries[self.navigator.current];
        let combined = if path.is_dir() {
            path.join(entry).to_string_lossy().to_string()
        } else {
            // アーカイブパスと内部エントリを結合
            let base = path.to_string_lossy();
            format!("{}\\{}", base.trim_end_matches(|c| c == '\\' || c == '/'), entry.trim_start_matches(|c| c == '\\' || c == '/'))
        };

        // 全ての / を \ に統一し、前後の空白と末尾の \ を徹底的に除去
        let target_str = combined.replace('/', "\\").trim().trim_end_matches('\\').to_string();

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

        // ── 外部プロセスからのパス転送をチェック ────────────────────────
        while let Ok(path) = self.path_rx.try_recv() {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ── バックグラウンド結果を回収 ──────────────────────────────────
        self.loader.collect_results(ctx, self.config.linear_filter, &self.navigator.entries);

        // ── ページ同期（目標ページの準備ができていたら表示を更新） ──────
        if self.is_loading_archive || self.navigator.current != self.navigator.target_index {
            if self.get_texture(self.navigator.target_index).is_some() {
                self.navigator.current = self.navigator.target_index;
                self.is_loading_archive = false;
                self.last_display_change_time = ctx.input(|i| i.time);
            }
        }

        // ── ドラッグ＆ドロップ ──────────────────────────────────────────
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ── キーボード ──────────────────────────────────────────────────
        let (left, right, fit_t, zin, zout, manga_t, rcw, rccw, pgup, pgdn, up, dn, p_key, n_key, s_key, home, end, bs_key, e_key, i_key, enter_key, alt_pressed, esc_key, y_key, t_key) = ctx.input(|i| (
            i.key_pressed(egui::Key::ArrowLeft)  || i.key_pressed(egui::Key::A),
            i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D),
            i.key_pressed(egui::Key::F),
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            i.key_pressed(egui::Key::Minus),
            i.key_pressed(egui::Key::M) || i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::R) && !i.modifiers.ctrl,
            i.key_pressed(egui::Key::R) &&  i.modifiers.ctrl,
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::P),
            i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::I),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.alt,
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Y),
            i.key_pressed(egui::Key::T),
        ));

        // ソート/外部アプリ設定ウィンドウが開いている間はメイン操作を無効化
        let modal_open = self.show_sort_settings || self.show_settings;

        // 操作の反転を廃止：常に左(A/Left)は戻る、右(D/Right)は進む
        if !modal_open && (left  || p_key) { self.go_prev(ctx); }
        if !modal_open && (right || n_key) { self.go_next(ctx); }

        // ↑↓ はマンガモード時のみ1ページ単位、通常時は←→と同じ
        if !modal_open && up { if self.manga_mode { self.go_single_prev(ctx); } else { self.go_prev(ctx); } }
        if !modal_open && dn { if self.manga_mode { self.go_single_next(ctx); } else { self.go_next(ctx); } }
        if home { self.go_first(ctx); }
        if end  { self.go_last(ctx); }
        if fit_t  { self.fit = !self.fit; }
        if zin    { self.zoom = (self.zoom * 1.2).min(10.0); self.fit = false; }
        if zout   { self.zoom = (self.zoom / 1.2).max(0.1);  self.fit = false; }
        if rcw    { self.rotate_current(true,  ctx); }
        if rccw   { self.rotate_current(false, ctx); }
        if pgup   { self.go_prev_dir(ctx); }
        if pgdn   { self.go_next_dir(ctx); }
        if manga_t {
            self.manga_mode = !self.manga_mode;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
        if s_key {
            self.show_sort_settings = !self.show_sort_settings;
            if self.show_sort_settings { self.sort_focus_idx = 0; }
        }
        if t_key { self.show_tree = !self.show_tree; }
        if y_key {
            self.config.manga_rtl = !self.config.manga_rtl;
            self.save_config();
        }
        if i_key {
            self.config.linear_filter = !self.config.linear_filter;
            self.loader.clear();
            self.save_config();
            self.schedule_prefetch();
        }

        // BS: エクスプローラーで開く
        if !modal_open && bs_key {
            if let Some(path) = &self.navigator.archive_path {
                let _ = std::process::Command::new("explorer")
                    .arg("/select,")
                    .arg(path)
                    .spawn();
            }
        }

        // E: 外部アプリで開く（例としてシステム既定のアプリ。特定のパスへの書き換えも可能）
        if !modal_open && e_key { self.open_external(); }

        // ── 全画面 / ボーダレス切替 ──────────────────────────────────────
        if !modal_open && enter_key {
            // いずれのモードでも、OSの真の全画面（Fullscreen）コマンドによる干渉を避けるためオフにする
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));

            if alt_pressed {
                // Alt + Enter: ボーダレス (枠なし最大化・没入モード)
                self.is_borderless = !self.is_borderless;
                self.is_fullscreen = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_borderless));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_borderless));
            } else {
                // Enter: 全画面 (枠あり最大化・「ばってん」が残る標準的な全画面)
                self.is_fullscreen = !self.is_fullscreen;
                self.is_borderless = false;

                // 枠（タイトルバー）を常に表示した状態で最大化を切り替える
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_fullscreen));
            }
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
                        self.navigator.apply_sorting(&self.config);
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
            egui::SidePanel::left("tree_panel").default_width(200.0).show(ctx, |ui| {
                ui.heading("ディレクトリツリー");
                egui::ScrollArea::both().show(ui, |ui| {
                    if let Some(path) = &self.archive_path {
                        if let Some(parent) = path.parent() {
                            self.ui_dir_tree(ui, parent.to_path_buf(), ctx, &mut tree_open_req);
                        }
                    }
                });
            });
        }
        if let Some(p) = tree_open_req { self.open_path(p, ctx); }

        // ── メニューバー ────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("開く…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("アーカイブ・画像", &["zip","7z","jpg","jpeg","png","gif","bmp","webp","tiff","tif"])
                            .pick_file() { self.open_path(p, ctx); }
                    }
                    if ui.button("フォルダを開く…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); }
                    }
                    ui.separator();
                    if ui.add_enabled(self.navigator.archive_path.is_some(), egui::Button::new("エクスプローラーで表示 (BS)")).clicked() {
                        ui.close_menu();
                        if let Some(path) = &self.navigator.archive_path {
                            let _ = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
                        }
                    }
                    if ui.add_enabled(self.navigator.archive_path.is_some(), egui::Button::new("外部アプリで開く (E)")).clicked() {
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
                        self.schedule_prefetch(); ctx.request_repaint(); ui.close_menu();
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
                        self.loader.clear();
                        self.save_config();
                        self.schedule_prefetch();
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
                    ui.radio_value(&mut self.navigator.open_from_end, false, "先頭から開く");
                    ui.radio_value(&mut self.navigator.open_from_end, true, "末尾から開く");
                });
            });
        });

        // ── ツールバー ──────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let has = !self.navigator.entries.is_empty();
                if ui.add_enabled(has, egui::Button::new("◀ (P)")).clicked() { self.go_prev(ctx); }
                
                // ページシークスライダー
                if has {
                    let max_idx = self.navigator.entries.len().saturating_sub(1);
                    let mut slider_val = self.navigator.target_index;
                    ui.style_mut().spacing.slider_width = 160.0;
                    if ui.add(egui::Slider::new(&mut slider_val, 0..=max_idx)
                        .show_value(false)
                        .trailing_fill(true)).changed() {
                        self.navigator.target_index = slider_val;
                        self.schedule_prefetch();
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
                    self.schedule_prefetch(); ctx.request_repaint();
                }
                ui.separator();
                if has {
                    let meta = &self.navigator.entries_meta[self.navigator.target_index];
                    let short = std::path::Path::new(&meta.name).file_name()
                        .and_then(|f| f.to_str()).unwrap_or(meta.name.as_str()).to_string();

                    let count = format!("{}/{}", self.navigator.target_index + 1, self.navigator.entries.len());

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

                    let loading = self.get_texture(self.navigator.target_index).is_none();
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

            // ロード中スピナー
            let tex1 = self.get_texture(self.navigator.current).map(|t| (t.id(), t.size_vec2()));
            if tex1.is_none() {
                ui.centered_and_justified(|ui| {
                    if self.navigator.entries.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("画像が見つかりませんでした").size(20.0).strong());
                            ui.add_space(10.0);
                            
                            if let Some(p) = &self.navigator.archive_path {
                                if let Some(parent) = p.parent() {
                                    if ui.button(format!("⤴ 親フォルダへ: {}", parent.display())).clicked() {
                                        self.open_path(parent.to_path_buf(), ctx);
                                    }
                                }
                            }
                            

                            ui.add_space(10.0);
                            ui.label("移動候補:");
                            ui.label("周辺のディレクトリ構造:");
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for path in self.nav_items.clone() {
                                    let icon = if path.is_dir() { "📁" } else { "📦" };
                                    if ui.button(format!("{} {}", icon, path.file_name().unwrap_or_default().to_string_lossy())).clicked() {
                                        self.open_path(path, ctx);
                                    }
                                }
                            });
                        });
                    } else {
                        ui.label(egui::RichText::new("⏳ 読み込み中...").size(18.0).color(egui::Color32::GRAY));
                    }
                });
                return;
            }

            // マウス操作
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
                    if self.wheel_accumulator.abs() >= 40.0 {
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

            let (tex1_id, tex1_size) = tex1.unwrap();
            
            // 2枚目の取得判定: マンガモード かつ 表紙(0)以外 かつ 1枚目が見開きでない
            let can_pair = (self.manga_shift || self.navigator.current > 0) && tex1_size.x <= tex1_size.y;

            let tex2 = if self.manga_mode && can_pair {
                self.get_texture(self.navigator.current + 1).and_then(|t| {
                    let s = t.size_vec2();
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
                    self.navigator.current + 1 >= self.navigator.entries.len().saturating_sub(1)
                } else {
                    self.navigator.current >= self.navigator.entries.len().saturating_sub(1)
                };

                if is_at_end {
                    if let Some((siblings, idx)) = self.navigator.sibling_dirs() {
                        if idx + 1 < siblings.len() {
                            let next_path = &siblings[idx + 1];
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
