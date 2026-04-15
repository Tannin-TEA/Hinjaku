use crate::{archive::{self, ArchiveReader}, utils};
use crate::error::HinjakuError;
use crate::config::{Config, SortMode, SortOrder, FilterMode};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque, HashSet};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use image::AnimationDecoder;

const CACHE_MAX: usize = 13;
const PREFETCH_AHEAD: usize = 5;
const PREFETCH_BEHIND: usize = 5;
const MAX_TEX_DIM: u32 = 4096;

/// アニメーションデコードを試みる最大ファイルサイズ (30MB)
const MAX_ANIM_DECODE_SIZE: usize = 30 * 1024 * 1024;
/// アニメーションの最小フレーム遅延 (これより短い場合は 100ms に補正)
const MIN_ANIM_FRAME_DELAY_MS: u32 = 20;
/// アニメーションのデフォルト遅延
const DEFAULT_ANIM_FRAME_DELAY_MS: u32 = 100;
/// 1メインループあたりにGPUへ転送する最大テクスチャ数 (スタッター防止)
const MAX_TEXTURE_UPLOADS_PER_FRAME: usize = 2;
/// 現在位置からこれ以上離れたリクエストは破棄する距離
const LOAD_SKIP_DISTANCE_THRESHOLD: isize = 12;
/// 画像デコード用ワーカースレッド数
const WORKER_THREADS: usize = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Rotation { R0, R90, R180, R270 }
impl Rotation {
    pub fn cw(self)  -> Self { match self { Self::R0=>Self::R90,  Self::R90=>Self::R180, Self::R180=>Self::R270, Self::R270=>Self::R0  } }
    pub fn ccw(self) -> Self { match self { Self::R0=>Self::R270, Self::R90=>Self::R0,   Self::R180=>Self::R90,  Self::R270=>Self::R180 } }
}

struct LoadRequest {
    index: usize,
    key: String,
    archive_path: PathBuf,
    entry_name: String,
    entry_index: Option<usize>,
    rotation: Rotation,
    max_dim: u32,
    filter_mode: FilterMode,
    generation: u64,
}

struct LoadResult {
    index: usize,
    key: String,
    data: std::result::Result<Vec<FrameData>, String>,
    generation: u64,
}

pub struct FrameData {
    pub image: image::RgbaImage,
    pub delay_ms: u32,
}

pub enum CachedImage {
    Static(TextureHandle),
    Animated {
        frames: Vec<(TextureHandle, u32)>,
        total_ms: u32,
        loop_start_time: f64,
    },
}

impl CachedImage {
    pub fn current_frame(&self, now: f64) -> (&TextureHandle, Option<f64>) {
        match self {
            CachedImage::Static(tex) => (tex, None),
            CachedImage::Animated { frames, total_ms, loop_start_time } => {
                if frames.is_empty() || *total_ms == 0 {
                    return (&frames[0].0, None);
                }
                let elapsed_ms = ((now - loop_start_time) * 1000.0) as u32 % total_ms;
                let mut acc = 0u32;
                for (tex, delay_ms) in frames {
                    acc += delay_ms;
                    if elapsed_ms < acc {
                        let next_sec = (acc - elapsed_ms) as f64 / 1000.0;
                        return (tex, Some(next_sec));
                    }
                }
                (&frames[0].0, None)
            }
        }
    }
    pub fn first_frame(&self) -> &TextureHandle {
        match self {
            CachedImage::Static(tex) => tex,
            CachedImage::Animated { frames, .. } => &frames[0].0,
        }
    }
}

/// 画像管理とナビゲーションを統合したマネージャー
pub struct Manager {
    // --- Navigator State ---
    pub archive_path: Option<PathBuf>,
    pub entries: Vec<String>,
    pub entries_meta: Vec<archive::ImageEntry>,
    pub current: usize,
    pub target_index: usize,
    pub rotations: HashMap<String, Rotation>,
    pub open_from_end: bool,
    pub tree: NavTree,
    pub pending_focus: Option<String>,
    pub is_listing: bool,

    // --- Loader State ---
    cache: HashMap<String, CachedImage>,
    cache_lru: VecDeque<String>,
    load_tx: Sender<LoadRequest>,
    load_rx: Receiver<LoadResult>,
    list_rx: Option<Receiver<std::result::Result<(PathBuf, Vec<archive::ImageEntry>), String>>>,
    pending: HashSet<String>,
    current_idx_shared: Arc<AtomicUsize>,
    generation: Arc<AtomicU64>,
    pub archive_reader: Arc<dyn ArchiveReader>, // 外部からも参照できるように pub に変更
}

impl Manager {
    pub fn new(ctx: egui::Context, archive_reader: Arc<dyn ArchiveReader>) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();
        let current_idx_shared = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicU64::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));
        let ctx = ctx;

        for _ in 0..WORKER_THREADS {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);
            let worker_gen = Arc::clone(&generation);
            let worker_archive_reader = Arc::clone(&archive_reader); // スレッドに渡す
            let worker_ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut zip_cache: Option<(PathBuf, zip::ZipArchive<BufReader<std::fs::File>>)> = None;
                while let Ok(req) = rx.lock().map_err(|_| "Poisoned").and_then(|l| l.recv().map_err(|_| "RecvError")) {
                    let current_gen = worker_gen.load(Ordering::Relaxed);
                    if req.generation < current_gen { continue; }
                    let current_idx = worker_idx.load(Ordering::Relaxed);
                    if req.index != current_idx && (req.index as isize - current_idx as isize).abs() > LOAD_SKIP_DISTANCE_THRESHOLD {
                        let _ = tx.send(LoadResult { index: req.index, key: req.key, data: Err("SKIPPED".to_string()), generation: req.generation });
                        continue;
                    }
                    let result_data = (|| -> std::result::Result<Vec<FrameData>, String> {
                        let bytes = if let Some(idx) = req.entry_index {
                            if zip_cache.as_ref().map(|(p, _)| p != &req.archive_path).unwrap_or(true) {
                                let file = std::fs::File::open(&req.archive_path).map_err(|e| e.to_string())?;
                                let zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())?;
                                zip_cache = Some((req.archive_path.clone(), zip));
                            }
                            let (_, ref mut zip) = zip_cache.as_mut().ok_or("Cache error")?;
                            let mut entry = zip.by_index(idx).map_err(|e| e.to_string())?;
                            let mut buf = Vec::new();
                            std::io::copy(&mut entry, &mut buf).map_err(|e| e.to_string())?;
                            buf
                        } else { // Plain または 7z
                            if zip_cache.is_some() { zip_cache = None; }
                            worker_archive_reader.read_entry(&req.archive_path, &req.entry_name, None).map_err(|e| e.to_string())?
                        };

                        let ext = req.entry_name.to_ascii_lowercase();

                        if (ext.ends_with(".gif") || ext.ends_with(".webp")) && bytes.len() <= MAX_ANIM_DECODE_SIZE {
                            let frames_res = if ext.ends_with(".gif") {
                                image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                                    .and_then(|d| d.into_frames().collect_frames())
                            } else {
                                image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes))
                                    .and_then(|d| {
                                        if d.has_animation() {
                                            d.into_frames().collect_frames()
                                        } else {
                                            Err(image::ImageError::IoError(std::io::Error::new(
                                                std::io::ErrorKind::Other,
                                                "Not animated WebP",
                                            )))
                                        }
                                    })
                            };

                            if let Ok(frames) = frames_res {
                                if frames.len() > 1 {
                                    let mut output = Vec::new();
                                    for frame in frames {
                                        let delay_ms = {
                                            let (n, d) = frame.delay().numer_denom_ms();
                                            if d > 0 { n / d } else { DEFAULT_ANIM_FRAME_DELAY_MS }
                                        };
                                        let delay_ms = if delay_ms < MIN_ANIM_FRAME_DELAY_MS { DEFAULT_ANIM_FRAME_DELAY_MS } else { delay_ms };
                                        let img = frame.into_buffer();
                                        let img = downscale_if_needed(img, req.max_dim, req.filter_mode);
                                        let img = apply_rotation(img, req.rotation);
                                        output.push(FrameData {
                                            image: img,
                                            delay_ms,
                                        });
                                    }
                                    if !output.is_empty() {
                                        return Ok(output);
                                    }
                                }
                            }
                        }

                        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
                        let img = downscale_if_needed(img, req.max_dim, req.filter_mode);
                        let img = apply_rotation(img, req.rotation);
                        Ok(vec![FrameData { image: img, delay_ms: 0 }])
                    })();
                    let _ = tx.send(LoadResult { index: req.index, key: req.key, data: result_data, generation: req.generation });
                    worker_ctx.request_repaint(); // 処理完了をUIに通知
                }
            });
        }

        Self {
            archive_path: None, entries: Vec::new(), entries_meta: Vec::new(), current: 0, target_index: 0,
            rotations: HashMap::new(), open_from_end: false, tree: NavTree::new(Arc::clone(&archive_reader)),
            pending_focus: None, cache: HashMap::new(), cache_lru: VecDeque::new(), load_tx: req_tx, load_rx: res_rx,
            list_rx: None, is_listing: false,
            pending: HashSet::new(), current_idx_shared, generation, archive_reader,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, config: &Config) -> Vec<(usize, String)> {
        // アーカイブリストの取得完了をチェック
        if let Some(rx) = &self.list_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_listing = false;
                self.list_rx = None;
                match res {
                    Ok((path, entries)) => {
                        self.entries_meta = entries; // archive_reader から取得したメタデータ
                        self.archive_path = Some(path);
                        self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
                        if !self.entries.is_empty() {
                            self.apply_sorting(config);
                            if let Some(focus) = self.pending_focus.take() {
                                self.current = self.entries.iter().position(|n| n.contains(&focus)).unwrap_or(0);
                            } else {
                                self.current = if self.open_from_end { self.entries.len().saturating_sub(1) } else { 0 };
                            }
                            self.target_index = self.current;
                            // リストが確定したのでプリフェッチを開始
                            self.schedule_prefetch(config.linear_filter, false); 
                        }
                    }
                    Err(_) => {} // エラー処理は viewer 側で行う
                }
            }
        }

        let mut failures = Vec::new();
        let current_gen = self.generation.load(Ordering::Relaxed);
        let mut upload_count = 0;

        while let Ok(result) = self.load_rx.try_recv() {
            if result.generation != current_gen { continue; }
            self.pending.remove(&result.key);
            match result.data {
                Ok(frames) => {
                    let filter = if config.filter_mode == FilterMode::Nearest { TextureOptions::NEAREST } else { TextureOptions::LINEAR };
                    let cached = if frames.len() == 1 {
                        let img = &frames[0].image;
                        let ci = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                        let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
                        CachedImage::Static(tex)
                    } else {
                        let total_ms = frames.iter().map(|f| f.delay_ms).sum();
                        let tex_frames = frames.into_iter().enumerate().map(|(fi, f)| {
                            let ci = ColorImage::from_rgba_unmultiplied([f.image.width() as usize, f.image.height() as usize], f.image.as_raw());
                            (ctx.load_texture(format!("img_{}_{}", result.index, fi), ci, filter), f.delay_ms)
                        }).collect();
                        CachedImage::Animated {
                            frames: tex_frames,
                            total_ms,
                            loop_start_time: ctx.input(|i| i.time),
                        }
                    };

                    self.cache.insert(result.key.clone(), cached);
                    self.cache_lru.push_back(result.key);
                    if self.cache_lru.len() > CACHE_MAX {
                        if let Some(old) = self.cache_lru.pop_front() { self.cache.remove(&old); }
                    }
                }
                Err(e) if e == "SKIPPED" => {}
                Err(e) => failures.push((result.index, e)),
            }
            ctx.request_repaint();

            // 高解像度画像の転送によるスタッターを防ぐため、1フレームの転送数を制限
            upload_count += 1;
            if upload_count >= MAX_TEXTURE_UPLOADS_PER_FRAME { break; }
        }
        failures
    }

    pub fn get_tex(&self, index: usize, now: f64) -> Option<(&TextureHandle, Option<f64>)> {
        let entry = self.entries.get(index)?;
        let key = format!("{}:{}", index, entry);
        self.cache.get(&key).map(|c| c.current_frame(now))
    }

    pub fn get_first_tex(&self, index: usize) -> Option<&TextureHandle> {
        let entry = self.entries.get(index)?;
        let key = format!("{}:{}", index, entry);
        self.cache.get(&key).map(|c| c.first_frame())
    }

    pub fn is_spread(&self, index: usize) -> bool {
        self.get_first_tex(index).map(|t| t.size_vec2().x > t.size_vec2().y).unwrap_or(false)
    }

    pub fn open_path(&mut self, path: PathBuf, _config: &Config) {
        let path = utils::clean_path(&path);
        self.clear_cache();
        self.is_listing = true;

        let (base_path, start_name) = if path.is_file() && utils::is_image_ext(&path.to_string_lossy()) {
            let p = path.parent().map(|p| p.to_path_buf()).ok_or_else(|| HinjakuError::NotFound("親ディレクトリがありません".to_string())).unwrap_or_else(|_| path.clone());
            let n = path.file_name().map(|f| f.to_string_lossy().to_string());
            (p, n)
        } else { (path, None) };

        let (tx, rx) = mpsc::channel(); // list_images の結果を受け取るチャンネル
        self.list_rx = Some(rx);
        let bp_clone = base_path.clone();
        let reader = Arc::clone(&self.archive_reader);

        std::thread::spawn(move || {
            let res = reader.list_images(&bp_clone)
                .map(|entries| (bp_clone, entries))
                .map_err(|e| e.user_message());
            let _ = tx.send(res); // 結果を送信
        });

        // 暫定的にパスだけ設定し、リストは update で受け取る
        self.archive_path = Some(base_path);
        if let Some(name) = start_name {
            self.entries = vec![name.clone()]; // ロード中の表示用
            self.pending_focus = Some(name);
        }
    }

    pub fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, config: &Config, manga: bool, shift: bool) { // config, manga, shift は viewer からの引数
        let path = crate::utils::clean_path(&path); // utils へ移動
        self.clear_cache();
        if let Ok(entries) = self.archive_reader.list_images(&path) { // ArchiveReader 経由
            self.entries_meta = entries;
            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
            if !self.entries.is_empty() { self.apply_sorting(config); }
            if !self.entries.is_empty() {
                self.current = if go_last {
                    let last = self.entries.len().saturating_sub(1);
                    if manga && last > 0 { if (last % 2 == 0) == shift { last } else { last.saturating_sub(1) } } else { last }
                } else if let Some(hint) = focus_hint {
                    let name = hint.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    self.entries.iter().position(|n| n.contains(&name)).unwrap_or(0)
                } else { 0 };
            } else { self.current = 0; }
            self.target_index = self.current;
            self.archive_path = Some(path);
            self.schedule_prefetch(config.filter_mode, manga);
        }
    }

    pub fn go_next(&mut self, manga: bool, shift: bool, filter: FilterMode) -> bool {
        if self.entries.is_empty() { return false; }
        let step = if manga {
            if self.target_index + 1 >= self.entries.len() || (!shift && self.target_index == 0) { 1 }
            else if self.is_spread(self.target_index) || self.is_spread(self.target_index + 1) { 1 }
            else { 2 }
        } else { 1 };
        if self.target_index + step < self.entries.len() {
            self.target_index += step; self.schedule_prefetch(filter, manga); true
        } else { false }
    }

    pub fn go_prev(&mut self, manga: bool, shift: bool, filter: FilterMode) -> bool {
        if self.target_index == 0 { return false; }
        let step = if manga {
            let first_pair = if shift { 0 } else { 1 };
            if self.target_index <= first_pair || self.target_index < 2 { 1 }
            else if self.is_spread(self.target_index - 1) || self.is_spread(self.target_index - 2) { 1 }
            else { 2 }
        } else { 1 };
        self.target_index = self.target_index.saturating_sub(step);
        self.schedule_prefetch(filter, manga);
        true
    }

    pub fn go_relative_dir(&mut self, forward: bool, config: &Config, manga: bool, shift: bool) -> bool {
        let Some(curr) = &self.archive_path else { return false };
        if let Some(dest) = self.tree.get_relative_target(curr, forward) {
            let go_last = !forward && self.open_from_end;
            self.move_to_dir(dest, Some(curr.clone()), go_last, config, manga, shift);
            true
        } else { false }
    }

    pub fn get_current_full_path(&self) -> Option<String> {
        let path = self.archive_path.as_ref()?; // 現在開いているアーカイブ/ディレクトリのパス
        let entry = self.entries.get(self.target_index)?; // 現在表示中のエントリ名
        Some(crate::utils::join_entry_path(path, entry)) // utils へ移動
    }

    pub fn apply_sorting(&mut self, config: &Config) {
        let current_name = self.entries.get(self.current).cloned();
        self.entries_meta.sort_by(|a, b| {
            let res = match config.sort_mode { // config は viewer から渡される
                SortMode::Name => if config.sort_natural { crate::utils::natord(&a.name, &b.name) } else { a.name.cmp(&b.name) } // utils へ移動
                SortMode::Mtime => a.mtime.cmp(&b.mtime),
                SortMode::Size => a.size.cmp(&b.size),
            };
            if config.sort_order == SortOrder::Descending { res.reverse() } else { res }
        });
        self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
        if let Some(name) = current_name {
            if let Some(pos) = self.entries.iter().position(|n| n == &name) { self.current = pos; self.target_index = pos; }
        }
    }

    pub fn schedule_prefetch(&mut self, filter_mode: FilterMode, manga: bool) {
        let Some(path) = self.archive_path.as_ref() else { return };
        let len = self.entries.len();
        if len == 0 { return; }
        self.current_idx_shared.store(self.target_index, Ordering::Relaxed);
        let lo = self.target_index.saturating_sub(PREFETCH_BEHIND);
        let hi = (self.target_index + PREFETCH_AHEAD + 1).min(len);

        let gen = self.generation.load(Ordering::Relaxed);
        let mut req = |idx: usize| {
            let entry = &self.entries_meta[idx];
            let key = format!("{}:{}", idx, entry.name);
            if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }
            let rot = self.rotations.get(&entry.name).copied().unwrap_or(Rotation::R0); // Rotation は Manager 内で定義
            let entry_idx = if matches!(utils::detect_kind(path), utils::ArchiveKind::Zip) { Some(entry.archive_index) } else { None };
            self.pending.insert(key.clone());
            let _ = self.load_tx.send(LoadRequest {
                index: idx, key, archive_path: path.to_path_buf(), entry_name: entry.name.clone(),
                entry_index: entry_idx, rotation: rot, max_dim: MAX_TEX_DIM, filter_mode, generation: gen,
            });
        };

        req(self.target_index);
        if manga && self.target_index + 1 < len { req(self.target_index + 1); }
        for i in lo..hi { req(i); }
        self.cache.retain(|k, _| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
        self.pending.retain(|k| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
        
        // cache から消されたキーを LRU リストからも削除する（ここが漏れていた）
        self.cache_lru.retain(|k| self.cache.contains_key(k));
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear(); self.cache_lru.clear(); self.pending.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn invalidate_cache_for(&mut self, index: usize, entry_name: &str) {
        let key = format!("{}:{}", index, entry_name); // キャッシュキーは Manager 内で管理
        self.invalidate(&key);
    }

    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key); self.cache_lru.retain(|k| k != key); self.pending.remove(key);
    }
}

fn downscale_if_needed(img: image::RgbaImage, max_dim: u32, filter: FilterMode) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let (nw, nh) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
    let filter_type = match filter {
        FilterMode::Nearest => image::imageops::FilterType::Nearest,
        FilterMode::Bilinear => image::imageops::FilterType::Triangle,
        FilterMode::Bicubic => image::imageops::FilterType::CatmullRom,
        FilterMode::Lanczos => image::imageops::FilterType::Lanczos3,
    };
    image::imageops::resize(&img, nw, nh, filter_type)
}

fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot { Rotation::R0=>img, Rotation::R90=>image::imageops::rotate90(&img), Rotation::R180=>image::imageops::rotate180(&img), Rotation::R270=>image::imageops::rotate270(&img) }
}

pub struct NavTree {
    pub nodes: HashMap<PathBuf, Vec<PathBuf>>,
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub image_counts: HashMap<PathBuf, usize>,
    archive_reader: Arc<dyn ArchiveReader>,
    pub scroll_to_selected: bool,
}

impl NavTree {
    pub fn new(archive_reader: Arc<dyn ArchiveReader>) -> Self {
        Self {
            nodes: HashMap::new(),
            expanded: HashSet::new(),
            selected: None,
            image_counts: HashMap::new(),
            archive_reader,
            scroll_to_selected: false,
        }
    }
    /// ツリーのキャッシュ（ノードリストと画像数）をクリアする。
    /// メモリ使用量が気になる場合や、ドライブを跨ぐ移動時に呼ぶ。
    pub fn clear_metadata_cache(&mut self) {
        self.nodes.clear();
        self.image_counts.clear(); // image_counts は NavTree 内で管理
    }

    pub fn get_roots(&self) -> Vec<PathBuf> {
        self.archive_reader.get_roots()
    }

    pub fn get_image_count(&mut self, path: &Path) -> usize {
        let path = utils::clean_path(path);
        if let Some(&count) = self.image_counts.get(&path) { return count; }
        let count = self.archive_reader.list_images(&path).map(|e| e.len()).unwrap_or(0);
        self.image_counts.insert(path, count);
        count
    }

    pub fn get_children(&mut self, dir_path: &Path) -> Vec<PathBuf> {
        let dir_path = crate::utils::clean_path(dir_path); // utils へ移動
        if let Some(cached) = self.nodes.get(&dir_path) { return cached.clone(); }
        let targets = self.archive_reader.list_nav_targets(&dir_path).unwrap_or_default(); // ArchiveReader 経由
        self.nodes.insert(dir_path, targets.clone());
        targets
    }
    pub fn get_siblings(&mut self, path: &Path) -> Vec<PathBuf> {
        if let Some(p) = path.parent() {
            self.get_children(p)
        } else { // 親がなければドライブ一覧を兄弟とする
            self.archive_reader.get_roots() // ArchiveReader 経由
        }
    }
    pub fn expand_to_path(&mut self, path: &Path) {
        let mut curr = Some(utils::clean_path(path));
        while let Some(p) = curr {
            if let Some(parent) = p.parent() {
                self.expanded.insert(parent.to_path_buf());
                curr = Some(parent.to_path_buf());
            } else {
                break;
            }
        }
    }

    pub fn get_relative_target(&mut self, current: &Path, forward: bool) -> Option<PathBuf> {
        let curr = crate::utils::clean_path(current); // utils へ移動
        let siblings = self.get_siblings(&curr);
        let pos = siblings.iter().position(|p| p == &curr)?;
        let next_pos = if forward { pos + 1 } else { pos.checked_sub(1)? };
        siblings.get(next_pos).cloned() // siblings は PathBuf の Vec
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.selected.is_none() {
            if let Some(first_root) = self.archive_reader.get_roots().first() { // ArchiveReader 経由
                self.selected = Some(crate::utils::clean_path(first_root)); // utils へ移動
                self.scroll_to_selected = true;
            }
            return;
        }

        if let Some(sel) = self.selected.clone() { // selected は PathBuf
            let sel = crate::utils::clean_path(&sel); // utils へ移動
            let siblings = self.get_siblings(&sel);
            if let Some(pos) = siblings.iter().position(|p| p == &sel) {
                let new_pos = (pos as isize + delta).clamp(0, siblings.len() as isize - 1) as usize;
                self.selected = Some(crate::utils::clean_path(&siblings[new_pos])); // utils へ移動
                self.scroll_to_selected = true;
            }
        }
    }

    pub fn expand_current(&mut self) {
        if let Some(sel) = self.selected.clone() {
            let sel = crate::utils::clean_path(&sel); // utils へ移動
            let kind = crate::utils::detect_kind(&sel); // utils へ移動
            if kind == crate::utils::ArchiveKind::Plain {
                // 展開して即座に中に入る
                self.expanded.insert(sel.clone());
                let children = self.get_children(&sel);
                if let Some(first) = children.first() {
                    self.selected = Some(crate::utils::clean_path(first)); // utils へ移動
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    pub fn collapse_or_up(&mut self) {
        if let Some(sel) = self.selected.clone() {
            let sel = crate::utils::clean_path(&sel); // utils へ移動
            if self.expanded.contains(&sel) {
                // 現在のフォルダが開いているなら、まずそれを閉じる
                self.expanded.remove(&sel);
            } else if let Some(parent) = sel.parent() {
                // すでに閉じている（またはファイル）なら親へ戻り、
                // 移動先の親フォルダ自体も閉じる（階層を遡る挙動に合わせる）
                let p = crate::utils::clean_path(parent); // utils へ移動
                if p != sel {
                    self.selected = Some(p.clone());
                    self.expanded.remove(&sel);
                    self.expanded.remove(&p);   // 親に戻った際、その階層を折りたたむ
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    pub fn activate_current(&mut self) -> Option<PathBuf> {
        self.selected.clone()
    }

    /// 指定されたパスまでツリーを展開し、選択状態にしてスクロール要求を出す
    pub fn reveal_path(&mut self, path: &Path) {
        let cleaned = utils::clean_path(path);
        self.expand_to_path(&cleaned);
        self.selected = Some(cleaned);
        self.scroll_to_selected = true;
    }
}