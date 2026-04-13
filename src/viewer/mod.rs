mod cache;
mod nav;
mod render;
mod ui;

use crate::archive;
use crate::config::{Config, SortMode, SortOrder, load_config_file};
use cache::{
    MAX_TEX_DIM, PREFETCH_AHEAD, PREFETCH_BEHIND,
    collect_results, make_cache_key,
    Loader, PendingSet, TextureCache,
};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, TextureHandle};
use nav::{last_page_index, next_step, prev_step, sibling_dirs};
use render::Rotation;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};

// ── App 構造体 ────────────────────────────────────────────────────────────────

pub struct App {
    // ── アーカイブ状態 ────────────────────────────────────────────────────
    pub archive_path: Option<PathBuf>,
    /// ソート済みのエントリ名リスト
    pub entries: Vec<String>,
    /// ソート済みのメタデータリスト（entries と常にインデックスが一致する）
    pub entries_meta: Vec<archive::ImageEntry>,
    /// 移動先候補（画像がないフォルダを開いたとき）
    pub nav_items: Vec<PathBuf>,

    // ── ページ状態 ────────────────────────────────────────────────────────
    /// 現在表示中のインデックス（テクスチャ準備済み）
    pub current: usize,
    /// 移動先のインデックス（まだロード待ちの可能性がある）
    pub target_index: usize,
    /// アーカイブ読み込み直後で最初のテクスチャ待ちか
    pub is_loading_archive: bool,
    /// 最後に表示が切り替わった時刻（ページ送りガード用）
    pub last_display_change_time: f64,

    // ── キャッシュ・ローダー ──────────────────────────────────────────────
    pub cache: TextureCache,
    pub pending: PendingSet,
    pub loader: Loader,

    // ── 入力 / IPC ────────────────────────────────────────────────────────
    /// 外部インスタンスから送られてきたパスの受信チャンネル
    pub path_rx: Receiver<PathBuf>,
    /// マウスホイール蓄積バッファ（感度調整用）
    pub wheel_accumulator: f32,

    // ── 設定 ─────────────────────────────────────────────────────────────
    pub config: Config,
    pub config_path: Option<PathBuf>,

    // ── UI 状態 ───────────────────────────────────────────────────────────
    pub show_settings: bool,
    pub show_sort_settings: bool,
    /// ソート設定ウィンドウ内のキーボードフォーカス行
    pub sort_focus_idx: usize,
    /// 設定画面用の引数編集バッファ
    pub settings_args_tmp: String,
    /// フォーカス取得直後のクリックを無視するためのフラグ
    pub was_focused: bool,
    pub error: Option<String>,
    pub fit: bool,
    pub zoom: f32,
    pub manga_mode: bool,
    /// 表紙ずらし（先頭を右ページ扱いにするか）
    pub manga_shift: bool,
    /// 各エントリの回転状態（エントリ名 → Rotation）
    pub rotations: HashMap<String, Rotation>,
    /// 前のフォルダへ移動した際に末尾から開くか
    pub open_from_end: bool,
    pub is_fullscreen: bool,
    pub is_borderless: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        listener: Option<std::net::TcpListener>,
    ) -> Self {
        // ── 日本語フォント設定 ─────────────────────────────────────────
        let mut fonts = FontDefinitions::default();
        for font_path in &[
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msjh.ttc",
        ] {
            if let Ok(bytes) = std::fs::read(font_path) {
                fonts
                    .font_data
                    .insert("japanese".to_owned(), FontData::from_owned(bytes));
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "japanese".to_owned());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "japanese".to_owned());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        // ── バックグラウンドローダー起動 ──────────────────────────────
        // worker 4 本：デコード並列化
        let loader = Loader::spawn(4);

        // ── 設定読み込み ───────────────────────────────────────────────
        let (config, config_path) = load_config_file();
        let settings_args_tmp = config.external_args.join(" ");

        // ── IPC: 外部インスタンスからのパス転送 ──────────────────────
        // channel の Sender は必要な間だけ保持し、listener スレッドが終了したら
        // 自動で drop → path_rx への送信が止まる（リーク防止）
        let (path_tx, path_rx) = mpsc::channel();
        if let Some(l) = listener {
            let tx = path_tx;
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for stream in l.incoming() {
                    let Ok(mut s) = stream else { continue };
                    let mut buf = String::new();
                    if s.read_to_string(&mut buf).is_ok() {
                        let path = PathBuf::from(buf.trim());
                        if tx.send(path).is_err() {
                            break; // App が drop 済み → スレッド終了
                        }
                        ctx.request_repaint();
                    }
                }
            });
        }
        // listener が None の場合、path_tx はここで drop される（問題なし）

        let mut app = Self {
            archive_path: None,
            entries: Vec::new(),
            entries_meta: Vec::new(),
            nav_items: Vec::new(),
            current: 0,
            target_index: 0,
            is_loading_archive: false,
            last_display_change_time: 0.0,
            cache: TextureCache::new(),
            pending: PendingSet::new(),
            loader,
            path_rx,
            wheel_accumulator: 0.0,
            config,
            config_path,
            show_settings: false,
            show_sort_settings: false,
            sort_focus_idx: 0,
            settings_args_tmp,
            was_focused: true,
            error: None,
            fit: true,
            zoom: 1.0,
            manga_mode: false,
            manga_shift: false,
            rotations: HashMap::new(),
            open_from_end: false,
            is_fullscreen: false,
            is_borderless: false,
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }

        app
    }

    // ── キャッシュキー ────────────────────────────────────────────────────

    pub fn cache_key(&self, index: usize) -> Option<String> {
        self.entries
            .get(index)
            .map(|e| make_cache_key(index, e))
    }

    // ── ロードリクエスト送信 ──────────────────────────────────────────────

    pub fn request_load(&mut self, index: usize) {
        let Some(path) = &self.archive_path else { return };
        let Some(key) = self.cache_key(index) else { return };

        if self.cache.contains(&key) || self.pending.contains(&key) {
            return;
        }

        let entry_name = self.entries[index].clone();
        let rotation = self
            .rotations
            .get(&entry_name)
            .copied()
            .unwrap_or_default();

        let req = cache::LoadRequest {
            index,
            key: key.clone(),
            archive_path: path.clone(),
            entry_name,
            rotation,
            max_dim: MAX_TEX_DIM,
            linear_filter: self.config.linear_filter,
        };

        // SyncSender::try_send を使い、キューが詰まっていたらスキップ
        // （ブロッキングによる UI フリーズを防ぐ）
        match self.loader.tx.try_send(req) {
            Ok(()) => {
                self.pending.insert(key);
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                // キュー満杯：次フレームで再試行する（pending には入れない）
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                // ワーカー終了（通常は発生しない）
            }
        }
    }

    // ── バックグラウンド結果回収 ──────────────────────────────────────────

    pub fn collect_results(&mut self, ctx: &egui::Context) {
        let entries = &self.entries;
        collect_results(
            &self.loader,
            &mut self.cache,
            &mut self.pending,
            self.config.linear_filter,
            ctx,
            |index| {
                entries
                    .get(index)
                    .map(|e| make_cache_key(index, e))
            },
        );
    }

    // ── 先読みスケジューリング ────────────────────────────────────────────

    /// 現在の target_index を中心に先読み範囲を計算し、リクエストを発行する。
    /// 範囲外のテクスチャはキャッシュから能動的に解放する。
    pub fn schedule_prefetch(&mut self) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }

        self.loader
            .current_idx_shared
            .store(self.target_index, Ordering::Relaxed);

        let lo = self.target_index.saturating_sub(PREFETCH_BEHIND);
        let hi = (self.target_index + PREFETCH_AHEAD + 1).min(len);

        // 最優先：今すぐ必要なページ
        self.request_load(self.target_index);
        if self.manga_mode && self.target_index + 1 < len {
            self.request_load(self.target_index + 1);
        }

        // 残りの先読み範囲
        for i in lo..hi {
            self.request_load(i);
        }

        // 範囲外テクスチャの即時解放（メモリリーク防止の核心）
        self.cache.retain_range(lo, hi);
        self.pending.retain_range(lo, hi);
    }

    // ── テクスチャ取得 ────────────────────────────────────────────────────

    pub fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        let key = self.cache_key(index)?;
        self.cache.get(&key)
    }

    // ── 見開き判定（横長 = true） ─────────────────────────────────────────

    pub fn is_spread(&self, index: usize) -> bool {
        self.get_texture(index)
            .map(|t| {
                let sz = t.size_vec2();
                sz.x > sz.y
            })
            .unwrap_or(false)
    }

    // ── アーカイブを開く ──────────────────────────────────────────────────

    pub fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.reset_state();

        let (archive_path, start_name) =
            if path.is_file() && archive::is_image_ext(&path.to_string_lossy()) {
                let name = path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let dir = path.parent().unwrap().to_path_buf();
                (dir, Some(name))
            } else {
                (path, None)
            };

        match archive::list_images(&archive_path) {
            Ok(entries) => {
                if entries.is_empty() {
                    if let Ok(targets) = archive::list_nav_targets(&archive_path) {
                        self.nav_items = targets;
                    }
                }
                self.entries_meta = entries;
                self.apply_sorting();

                self.current = start_name
                    .as_deref()
                    .and_then(|name| {
                        self.entries.iter().position(|n| {
                            std::path::Path::new(n)
                                .file_name()
                                .map(|f| f.to_string_lossy() == name)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(0);

                self.target_index = self.current;
                self.archive_path = Some(archive_path);
                self.is_loading_archive = true;
                self.last_display_change_time = ctx.input(|i| i.time);
                self.schedule_prefetch();
                ctx.request_repaint();
            }
            Err(e) => {
                self.error = Some(format!("開けませんでした: {e}"));
            }
        }
    }

    // ── フォルダ間移動 ────────────────────────────────────────────────────

    pub fn move_to_dir(&mut self, path: PathBuf, go_last: bool, ctx: &egui::Context) {
        self.reset_state();

        match archive::list_images(&path) {
            Ok(entries) => {
                if entries.is_empty() {
                    if let Ok(targets) = archive::list_nav_targets(&path) {
                        self.nav_items = targets;
                    }
                }
                self.entries_meta = entries;
                self.apply_sorting();

                self.current = if go_last && !self.entries.is_empty() {
                    last_page_index(self.entries.len(), self.manga_mode, self.manga_shift)
                } else {
                    0
                };

                self.target_index = self.current;
                self.archive_path = Some(path);
                self.is_loading_archive = true;
                self.last_display_change_time = ctx.input(|i| i.time);
                self.schedule_prefetch();
                ctx.request_repaint();
            }
            Err(e) => {
                self.error = Some(format!("開けませんでした: {e}"));
            }
        }
    }

    /// 全キャッシュ・ペンディングをクリアし、状態をリセットする。
    /// open_path / move_to_dir の共通前処理。
    fn reset_state(&mut self) {
        self.cache.clear();
        self.pending.clear();
        self.error = None;
        self.current = 0;
        self.target_index = 0;
        self.nav_items.clear();
        self.is_loading_archive = false;
        self.rotations.clear();
    }

    // ── 回転 ──────────────────────────────────────────────────────────────

    fn invalidate_cache_for(&mut self, index: usize) {
        if let Some(key) = self.cache_key(index) {
            self.cache.remove(&key);
            self.pending.remove(&key);
        }
    }

    pub fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.manga_mode {
            vec![self.current, self.current + 1]
        } else {
            vec![self.current]
        };
        for idx in indices {
            if let Some(name) = self.entries.get(idx).cloned() {
                let rot = self
                    .rotations
                    .get(&name)
                    .copied()
                    .unwrap_or_default();
                self.rotations
                    .insert(name, if cw { rot.cw() } else { rot.ccw() });
                self.invalidate_cache_for(idx);
            }
        }
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    // ── ソート ────────────────────────────────────────────────────────────

    pub fn apply_sorting(&mut self) {
        if self.entries_meta.is_empty() {
            return;
        }
        // ソート前の表示位置を記憶
        let current_name = self.entries.get(self.current).cloned();

        let mode = self.config.sort_mode;
        let order = self.config.sort_order;
        let natural = self.config.sort_natural;

        self.entries_meta.sort_by(|a, b| {
            let res = match mode {
                SortMode::Name => {
                    if natural {
                        archive::natord(&a.name, &b.name)
                    } else {
                        a.name.cmp(&b.name)
                    }
                }
                SortMode::Mtime => a.mtime.cmp(&b.mtime),
                SortMode::Size => a.size.cmp(&b.size),
            };
            if order == SortOrder::Descending {
                res.reverse()
            } else {
                res
            }
        });

        self.entries = self
            .entries_meta
            .iter()
            .map(|e| e.name.clone())
            .collect();

        // ソート後に表示ファイルの新しい位置を復元
        if let Some(name) = current_name {
            if let Some(pos) = self.entries.iter().position(|n| n == &name) {
                self.current = pos;
                self.target_index = pos;
            }
        }
    }

    // ── ページ送りメソッド ────────────────────────────────────────────────

    fn page_guard(&self, ctx: &egui::Context) -> bool {
        if self.entries.is_empty() || self.is_loading_archive {
            return false;
        }
        if self.current != self.target_index {
            return false;
        }
        let elapsed = ctx.input(|i| i.time) - self.last_display_change_time;
        elapsed >= 0.05
    }

    pub fn go_prev(&mut self, ctx: &egui::Context) {
        if !self.page_guard(ctx) {
            return;
        }
        if self.target_index == 0 {
            self.go_prev_dir(ctx);
            return;
        }
        let step = prev_step(
            self.manga_mode,
            self.manga_shift,
            self.target_index,
            |idx| self.is_spread(idx),
        );
        self.target_index = self.target_index.saturating_sub(step);
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_next(&mut self, ctx: &egui::Context) {
        if !self.page_guard(ctx) {
            return;
        }
        let step = next_step(
            self.manga_mode,
            self.manga_shift,
            self.entries.len(),
            self.target_index,
            |idx| self.is_spread(idx),
        );
        if self.target_index + step >= self.entries.len() {
            self.go_next_dir(ctx);
        } else {
            self.target_index += step;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }

    /// マンガモード専用：1 ページだけ戻る
    pub fn go_single_prev(&mut self, ctx: &egui::Context) {
        if !self.page_guard(ctx) {
            return;
        }
        if self.target_index == 0 {
            self.go_prev_dir(ctx);
        } else {
            self.target_index -= 1;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }

    /// マンガモード専用：1 ページだけ進む
    pub fn go_single_next(&mut self, ctx: &egui::Context) {
        if !self.page_guard(ctx) {
            return;
        }
        if self.target_index + 1 >= self.entries.len() {
            self.go_next_dir(ctx);
        } else {
            self.target_index += 1;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }

    pub fn go_first(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() || self.is_loading_archive {
            return;
        }
        self.target_index = 0;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_last(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() || self.is_loading_archive {
            return;
        }
        self.target_index =
            last_page_index(self.entries.len(), self.manga_mode, self.manga_shift);
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    // ── フォルダ移動 ──────────────────────────────────────────────────────

    fn dir_guard(&self, ctx: &egui::Context) -> bool {
        if self.is_loading_archive {
            return false;
        }
        let elapsed = ctx.input(|i| i.time) - self.last_display_change_time;
        elapsed >= 0.1
    }

    pub fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if !self.dir_guard(ctx) {
            return;
        }
        let Some(path) = &self.archive_path else { return };
        let Some((siblings, idx)) = sibling_dirs(path) else { return };
        if idx > 0 {
            let dest = siblings[idx - 1].clone();
            let from_end = self.open_from_end;
            self.move_to_dir(dest, from_end, ctx);
        }
    }

    pub fn go_next_dir(&mut self, ctx: &egui::Context) {
        if !self.dir_guard(ctx) {
            return;
        }
        let Some(path) = &self.archive_path else { return };
        let Some((siblings, idx)) = sibling_dirs(path) else { return };
        if idx + 1 < siblings.len() {
            let dest = siblings[idx + 1].clone();
            self.move_to_dir(dest, false, ctx);
        }
    }

    // ── 外部アプリ ────────────────────────────────────────────────────────

    pub fn open_external(&self) {
        let Some(path) = &self.archive_path else { return };
        if self.entries.is_empty() {
            return;
        }

        let entry = &self.entries[self.current];
        let combined = if path.is_dir() {
            path.join(entry).to_string_lossy().to_string()
        } else {
            let base = path.to_string_lossy();
            format!(
                "{}\\{}",
                base.trim_end_matches(['\\', '/']),
                entry.trim_start_matches(['\\', '/'])
            )
        };
        let target_str = combined
            .replace('/', "\\")
            .trim()
            .trim_end_matches('\\')
            .to_string();

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

    // ── 設定保存 ──────────────────────────────────────────────────────────

    pub fn save_config(&self) {
        if let Some(ref path) = self.config_path {
            if let Ok(toml_str) = toml::to_string_pretty(&self.config) {
                let _ = std::fs::write(path, toml_str);
            }
        }
    }

    // ── 兄弟ディレクトリ（UI から使用） ──────────────────────────────────

    pub fn sibling_dirs(&self) -> Option<(Vec<PathBuf>, usize)> {
        sibling_dirs(self.archive_path.as_ref()?)
    }
}

// ── eframe::App 実装 ──────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ui::update(self, ctx, frame);
    }
}
