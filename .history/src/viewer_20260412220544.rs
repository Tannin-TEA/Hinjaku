use crate::archive;
use eframe::egui::{
    self, ColorImage, FontData, FontDefinitions, FontFamily,
    TextureHandle, TextureOptions,
};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

// ── 定数 ────────────────────────────────────────────────────────────────────
/// キャッシュ保持上限（前後5枚 + 現在 = 最大11、余裕を持って13）
const CACHE_MAX: usize = 13;
/// 先読み範囲
const PREFETCH_AHEAD: usize = 5;
const PREFETCH_BEHIND: usize = 5;
/// 表示解像度上限（これ以上は縮小してからテクスチャ化）
const MAX_TEX_DIM: u32 = 2048;

// ── 回転 ────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum Rotation { R0, R90, R180, R270 }
impl Rotation {
    fn cw(self)  -> Self { match self { Self::R0=>Self::R90,  Self::R90=>Self::R180, Self::R180=>Self::R270, Self::R270=>Self::R0  } }
    fn ccw(self) -> Self { match self { Self::R0=>Self::R270, Self::R90=>Self::R0,   Self::R180=>Self::R90,  Self::R270=>Self::R180 } }
}

// ── バックグラウンドロードの結果 ─────────────────────────────────────────────
struct LoadResult {
    index: usize,
    /// エントリキー（キャッシュの照合用）
    key: String,
    image: image::RgbaImage,
}

// ── デコードワーカーへのリクエスト ───────────────────────────────────────────
struct LoadRequest {
    index: usize,
    key: String,
    archive_path: PathBuf,
    entry_name: String,
    rotation: Rotation,
    max_dim: u32,
}

// ── キャッシュエントリ ───────────────────────────────────────────────────────
struct CacheEntry {
    texture: TextureHandle,
}

// ── メインアプリ ─────────────────────────────────────────────────────────────
pub struct App {
    archive_path: Option<PathBuf>,
    entries: Vec<String>,
    current: usize,

    /// テクスチャキャッシュ  key = "{index}:{entry_name}"
    cache: HashMap<String, CacheEntry>,
    /// LRU順でキーを管理（先頭=最古）
    cache_lru: VecDeque<String>,

    /// 非同期ロード用チャンネル
    load_tx: Sender<LoadRequest>,
    load_rx: Receiver<LoadResult>,

    /// 現在リクエスト中のキー集合（重複リクエスト防止）
    pending: std::collections::HashSet<String>,

    error: Option<String>,
    fit: bool,
    zoom: f32,
    manga_mode: bool,
    rotations: HashMap<String, Rotation>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 日本語フォント
        let mut fonts = FontDefinitions::default();
        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\meiryo.ttc") {
            fonts.font_data.insert("meiryo".to_owned(), FontData::from_owned(bytes));
            fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "meiryo".to_owned());
            fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "meiryo".to_owned());
        }
        cc.egui_ctx.set_fonts(fonts);

        // バックグラウンドワーカースレッド起動
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();

        std::thread::spawn(move || {
            while let Ok(req) = req_rx.recv() {
                let result = (|| -> Option<LoadResult> {
                    let bytes = archive::read_entry(&req.archive_path, &req.entry_name).ok()?;
                    let img   = image::load_from_memory(&bytes).ok()?.to_rgba8();

                    // 解像度制限（縮小）
                    let img = downscale_if_needed(img, req.max_dim);

                    // 回転適用
                    let img = apply_rotation(img, req.rotation);

                    Some(LoadResult { index: req.index, key: req.key, image: img })
                })();

                if let Some(r) = result {
                    let _ = res_tx.send(r);
                }
            }
        });

        Self {
            archive_path: None,
            entries: Vec::new(),
            current: 0,
            cache: HashMap::new(),
            cache_lru: VecDeque::new(),
            load_tx: req_tx,
            load_rx: res_rx,
            pending: std::collections::HashSet::new(),
            error: None,
            fit: true,
            zoom: 1.0,
            manga_mode: false,
            rotations: HashMap::new(),
        }
    }

    // ── キャッシュキー ────────────────────────────────────────────────────
    fn cache_key(&self, index: usize) -> Option<String> {
        self.entries.get(index).map(|e| format!("{}:{}", index, e))
    }

    // ── キャッシュ追加（LRU管理・上限超えたら古いものを削除） ──────────────
    fn cache_insert(&mut self, key: String, entry: CacheEntry) {
        // 既存なら LRU を更新
        if self.cache.contains_key(&key) {
            self.cache_lru.retain(|k| k != &key);
        }
        self.cache.insert(key.clone(), entry);
        self.cache_lru.push_back(key);

        // 上限超えたら最古を削除
        while self.cache_lru.len() > CACHE_MAX {
            if let Some(old_key) = self.cache_lru.pop_front() {
                self.cache.remove(&old_key);
                // TextureHandle が drop されることでGPUリソースも解放
            }
        }
    }

    // ── ロードリクエストを送る ────────────────────────────────────────────
    fn request_load(&mut self, index: usize) {
        let path = match &self.archive_path { Some(p) => p.clone(), None => return };
        let key  = match self.cache_key(index) { Some(k) => k, None => return };

        // キャッシュ済み or リクエスト中ならスキップ
        if self.cache.contains_key(&key) { return; }
        if self.pending.contains(&key)   { return; }

        let entry_name = self.entries[index].clone();
        let rotation   = self.rotations.get(&entry_name).copied().unwrap_or(Rotation::R0);

        self.pending.insert(key.clone());
        let _ = self.load_tx.send(LoadRequest {
            index,
            key,
            archive_path: path,
            entry_name,
            rotation,
            max_dim: MAX_TEX_DIM,
        });
    }

    // ── 受信済み結果をキャッシュに反映 ───────────────────────────────────
    fn collect_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.load_rx.try_recv() {
            self.pending.remove(&result.key);

            // アーカイブが切り替わっていたら捨てる
            let expected_key = self.cache_key(result.index);
            if expected_key.as_deref() != Some(&result.key) { continue; }

            let size = [result.image.width() as usize, result.image.height() as usize];
            let ci   = ColorImage::from_rgba_unmultiplied(size, &result.image.into_raw());

            // 回転は load 時に焼き込み済み
            let tex = ctx.load_texture(format!("img_{}", result.index), ci, TextureOptions::LINEAR);
            self.cache_insert(result.key, CacheEntry { texture: tex });
            ctx.request_repaint();
        }
    }

    // ── 先読みリクエストを発行 ────────────────────────────────────────────
    fn schedule_prefetch(&mut self) {
        let len = self.entries.len();
        if len == 0 { return; }

        // 現在 + 先後 PREFETCH 範囲
        let lo = self.current.saturating_sub(PREFETCH_BEHIND);
        let hi = (self.current + PREFETCH_AHEAD + 1).min(len);

        for i in lo..hi {
            self.request_load(i);
        }

        // 先読み範囲外のキャッシュを削除（メモリ節約）
        let keep_lo = self.current.saturating_sub(PREFETCH_BEHIND + 1);
        let keep_hi = (self.current + PREFETCH_AHEAD + 2).min(len);
        let to_remove: Vec<String> = self.cache_lru.iter()
            .filter(|k| {
                // キーから index を取り出す
                let idx: Option<usize> = k.split(':').next().and_then(|s| s.parse().ok());
                idx.map(|i| i < keep_lo || i >= keep_hi).unwrap_or(false)
            })
            .cloned()
            .collect();
        for k in to_remove {
            self.cache.remove(&k);
            self.cache_lru.retain(|lk| lk != &k);
        }
    }

    // ── テクスチャ取得（キャッシュから） ──────────────────────────────────
    fn get_texture(&self, index: usize) -> Option<&TextureHandle> {
        let key = self.cache_key(index)?;
        self.cache.get(&key).map(|e| &e.texture)
    }

    // ── アーカイブを開く ──────────────────────────────────────────────────
    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        // 全キャッシュ・ペンディングをクリア
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.error   = None;
        self.current = 0;
        self.rotations.clear();

        let (archive_path, start_name) = if path.is_file() && archive::is_image_ext(&path.to_string_lossy()) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let dir  = path.parent().unwrap().to_path_buf();
            (dir, Some(name))
        } else {
            (path, None)
        };

        match archive::list_images(&archive_path) {
            Ok(entries) if entries.is_empty() => {
                self.error = Some("画像ファイルが見つかりません".to_string());
            }
            Ok(entries) => {
                if let Some(ref name) = start_name {
                    self.current = entries.iter().position(|e| {
                        std::path::Path::new(e).file_name()
                            .map(|f| f.to_string_lossy().as_ref() == name.as_str())
                            .unwrap_or(false)
                    }).unwrap_or(0);
                }
                self.entries = entries;
                self.archive_path = Some(archive_path);
                self.schedule_prefetch();
                ctx.request_repaint();
            }
            Err(e) => {
                self.error = Some(format!("開けませんでした: {e}"));
            }
        }
    }

    // ── 回転変更（回転が変わったエントリのキャッシュを無効化） ──────────────
    fn invalidate_cache_for(&mut self, index: usize) {
        if let Some(key) = self.cache_key(index) {
            self.cache.remove(&key);
            self.cache_lru.retain(|k| k != &key);
            self.pending.remove(&key);
        }
    }

    fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.manga_mode {
            vec![self.current, self.current + 1]
        } else {
            vec![self.current]
        };
        for idx in indices {
            if let Some(name) = self.entries.get(idx).cloned() {
                let rot = self.rotations.get(&name).copied().unwrap_or(Rotation::R0);
                self.rotations.insert(name, if cw { rot.cw() } else { rot.ccw() });
                self.invalidate_cache_for(idx);
            }
        }
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    // ── ディレクトリ移動 ──────────────────────────────────────────────────
    fn sibling_dirs(&self) -> Option<(Vec<PathBuf>, usize)> {
        let path   = self.archive_path.as_ref()?;
        let parent = path.parent()?;
        let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent).ok()?
            .filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| {
                if p.is_dir() { return true; }
                matches!(p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(), Some("zip"|"7z"))
            })
            .collect();
        siblings.sort();
        let idx = siblings.iter().position(|p| p == path)?;
        Some((siblings, idx))
    }

    fn move_to_dir(&mut self, path: PathBuf, go_last: bool, ctx: &egui::Context) {
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.error = None;
        self.rotations.clear();

        match archive::list_images(&path) {
            Ok(entries) if entries.is_empty() => {
                self.error = Some("画像ファイルが見つかりません".to_string());
            }
            Ok(entries) => {
                self.current = if go_last && !entries.is_empty() {
                    let last = entries.len().saturating_sub(1);
                    if self.manga_mode { last.saturating_sub(last % 2) } else { last }
                } else { 0 };
                self.entries = entries;
                self.archive_path = Some(path);
                self.schedule_prefetch();
                ctx.request_repaint();
            }
            Err(e) => { self.error = Some(format!("開けませんでした: {e}")); }
        }
    }

    fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if let Some((siblings, idx)) = self.sibling_dirs() {
            if idx > 0 { self.move_to_dir(siblings[idx-1].clone(), true,  ctx); }
        }
    }
    fn go_next_dir(&mut self, ctx: &egui::Context) {
        if let Some((siblings, idx)) = self.sibling_dirs() {
            if idx+1 < siblings.len() { self.move_to_dir(siblings[idx+1].clone(), false, ctx); }
        }
    }

    /// マンガモード専用：1ページだけ戻る
    fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() { return; }
        if self.current == 0 {
            self.go_prev_dir(ctx);
        } else {
            self.current -= 1;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }

    /// マンガモード専用：1ページだけ進む
    fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() { return; }
        if self.current + 1 >= self.entries.len() {
            self.go_next_dir(ctx);
        } else {
            self.current += 1;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }

    fn go_prev(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() { return; }
        let step = if self.manga_mode { 2 } else { 1 };
        if self.current < step {
            self.go_prev_dir(ctx);
        } else {
            self.current -= step;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }
    fn go_next(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() { return; }
        let step = if self.manga_mode { 2 } else { 1 };
        if self.current + step >= self.entries.len() {
            self.go_next_dir(ctx);
        } else {
            self.current += step;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
    }
}

// ── 解像度制限（縦横いずれかが max_dim を超えたら縮小） ──────────────────────
fn downscale_if_needed(img: image::RgbaImage, max_dim: u32) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale  = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let nw     = ((w as f32 * scale) as u32).max(1);
    let nh     = ((h as f32 * scale) as u32).max(1);
    image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3)
}

fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot {
        Rotation::R0   => img,
        Rotation::R90  => image::imageops::rotate90(&img),
        Rotation::R180 => image::imageops::rotate180(&img),
        Rotation::R270 => image::imageops::rotate270(&img),
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

        // ── バックグラウンド結果を回収 ──────────────────────────────────
        self.collect_results(ctx);

        // ── ドラッグ＆ドロップ ──────────────────────────────────────────
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(path) = dropped { self.open_path(path, ctx); }

        // ── キーボード ──────────────────────────────────────────────────
        let (prev, next, fit_t, zin, zout, manga_t, rcw, rccw, pgup, pgdn, up, dn) = ctx.input(|i| (
            i.key_pressed(egui::Key::ArrowLeft)  || i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::F),
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            i.key_pressed(egui::Key::Minus),
            i.key_pressed(egui::Key::M),
            i.key_pressed(egui::Key::R) && !i.modifiers.ctrl,
            i.key_pressed(egui::Key::R) &&  i.modifiers.ctrl,
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
        ));
        if prev   { self.go_prev(ctx); }
        if next   { self.go_next(ctx); }
        // ↑↓ はマンガモード時のみ1ページ単位、通常時は←→と同じ
        if up  { if self.manga_mode { self.go_single_prev(ctx); } else { self.go_prev(ctx); } }
        if dn  { if self.manga_mode { self.go_single_next(ctx); } else { self.go_next(ctx); } }
        if fit_t  { self.fit = !self.fit; }
        if zin    { self.zoom = (self.zoom * 1.2).min(10.0); self.fit = false; }
        if zout   { self.zoom = (self.zoom / 1.2).max(0.1);  self.fit = false; }
        if rcw    { self.rotate_current(true,  ctx); }
        if rccw   { self.rotate_current(false, ctx); }
        if pgup   { self.go_prev_dir(ctx); }
        if pgdn   { self.go_next_dir(ctx); }
        if manga_t {
            self.manga_mode = !self.manga_mode;
            if self.manga_mode && self.current % 2 == 1 { self.current = self.current.saturating_sub(1); }
            self.schedule_prefetch();
            ctx.request_repaint();
        }

        // ── メニューバー ────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("開く…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("アーカイブ・画像", &["zip","7z","jpg","jpeg","png","gif","bmp","webp"])
                            .pick_file() { self.open_path(p, ctx); }
                    }
                    if ui.button("フォルダを開く…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().pick_folder() { self.open_path(p, ctx); }
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
                    ui.separator();
                    if ui.button("右回転 (R)").clicked()      { self.rotate_current(true,  ctx); ui.close_menu(); }
                    if ui.button("左回転 (Ctrl+R)").clicked() { self.rotate_current(false, ctx); ui.close_menu(); }
                });
                ui.menu_button("フォルダ", |ui| {
                    if ui.button("前のフォルダ (PgUp)").clicked() { self.go_prev_dir(ctx); ui.close_menu(); }
                    if ui.button("次のフォルダ (PgDn)").clicked() { self.go_next_dir(ctx); ui.close_menu(); }
                });
            });
        });

        // ── ツールバー ──────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let has = !self.entries.is_empty();
                if ui.add_enabled(has, egui::Button::new("◀")).clicked() { self.go_prev(ctx); }
                if ui.add_enabled(has, egui::Button::new("▶")).clicked() { self.go_next(ctx); }
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
                    let name  = self.entries[self.current].clone();
                    let short = std::path::Path::new(&name).file_name()
                        .and_then(|f| f.to_str()).unwrap_or(name.as_str()).to_string();
                    let count = if self.manga_mode && self.current+1 < self.entries.len() {
                        format!("{}-{} / {}", self.current+1, self.current+2, self.entries.len())
                    } else {
                        format!("{} / {}", self.current+1, self.entries.len())
                    };
                    // ファイルサイズ取得
                    let file_size = if let Some(ref path) = self.archive_path {
                        let size = if path.is_dir() {
                            std::fs::metadata(path.join(&name)).map(|m| m.len()).unwrap_or(0)
                        } else {
                            // アーカイブ内はエントリサイズ不明なのでアーカイブ全体サイズ
                            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                        };
                        if size >= 1024*1024 {
                            format!("{:.1}MB", size as f64 / (1024.0*1024.0))
                        } else if size >= 1024 {
                            format!("{:.0}KB", size as f64 / 1024.0)
                        } else {
                            format!("{}B", size)
                        }
                    } else { String::new() };

                    let loading = self.get_texture(self.current).is_none();
                    let status  = if loading { " ⏳" } else { "" };
                    ui.label(format!("{}{}  |  {}  [{}]", count, status, short, file_size));
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
            let tex1 = self.get_texture(self.current).map(|t| (t.id(), t.size_vec2()));
            if tex1.is_none() {
                ui.centered_and_justified(|ui| {
                    if self.entries.is_empty() {
                        ui.label(egui::RichText::new(
                            "ここにファイル・フォルダをドロップ\nまたはメニュー → 開く"
                        ).size(18.0).color(egui::Color32::GRAY));
                    } else {
                        ui.label(egui::RichText::new("⏳ 読み込み中...").size(18.0).color(egui::Color32::GRAY));
                        ctx.request_repaint(); // ロード完了まで再描画し続ける
                    }
                });
                return;
            }

            // Ctrl+ホイールでズーム
            let (wheel, ctrl) = ctx.input(|i| (i.smooth_scroll_delta.y, i.modifiers.ctrl));
            if ctrl && wheel != 0.0 {
                self.zoom = (self.zoom * (1.0 + wheel * 0.002)).clamp(0.05, 10.0);
                self.fit  = false;
            }

            let avail = ui.available_size();
            let fit   = self.fit;
            let zoom  = self.zoom;

            let (tex1_id, tex1_size) = tex1.unwrap();
            let tex2 = self.get_texture(self.current + 1).map(|t| (t.id(), t.size_vec2()));

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
                        ui.painter().image(tex1_id, egui::Rect::from_min_size(egui::pos2(cx, rect.min.y+(total_h-ds1.y)/2.0), ds1), uv, egui::Color32::WHITE);
                        ui.painter().image(tex2_id, egui::Rect::from_min_size(egui::pos2(cx-ds2.x, rect.min.y+(total_h-ds2.y)/2.0), ds2), uv, egui::Color32::WHITE);
                        if resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if pos.x < rect.center().x { self.go_next(ctx); } else { self.go_prev(ctx); }
                            }
                        }
                    } else {
                        // 2枚目まだロード中
                        let resp = draw_centered(ui, tex1_id, tex1_size, avail, fit, zoom);
                        if resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if pos.x < resp.rect.center().x { self.go_prev(ctx); } else { self.go_next(ctx); }
                            }
                        }
                        ctx.request_repaint();
                    }
                } else {
                    let resp = draw_centered(ui, tex1_id, tex1_size, avail, fit, zoom);
                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            if pos.x < resp.rect.center().x { self.go_prev(ctx); } else { self.go_next(ctx); }
                        }
                    }
                }
            });
        });
    }
}
