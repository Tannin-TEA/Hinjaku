use crate::{archive::{self, ArchiveReader}, utils};
use crate::config::{Config, SortMode, SortOrder, FilterMode};
pub use crate::nav_tree::NavTree;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use ::image::AnimationDecoder;
use crate::constants::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Rotation { R0, R90, R180, R270 }

impl Rotation {
    pub fn cw(self) -> Self {
        match self {
            Self::R0   => Self::R90,
            Self::R90  => Self::R180,
            Self::R180 => Self::R270,
            Self::R270 => Self::R0,
        }
    }
    pub fn ccw(self) -> Self {
        match self {
            Self::R0   => Self::R270,
            Self::R90  => Self::R0,
            Self::R180 => Self::R90,
            Self::R270 => Self::R180,
        }
    }
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
    pub image: ::image::RgbaImage,
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
                if *total_ms == 0 || frames.is_empty() {
                    return (&frames.first().expect("BUG: Animated with 0 frames").0, None);
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
    pub pending_go_last: bool,
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
    ctx: egui::Context,
}

impl Manager {
    /// 画像読み込みワーカースレッドを作成する
    fn spawn_worker_threads(
        req_rx: Arc<Mutex<Receiver<LoadRequest>>>,
        res_tx: Sender<LoadResult>,
        current_idx_shared: Arc<AtomicUsize>,
        generation: Arc<AtomicU64>,
        archive_reader: Arc<dyn ArchiveReader>,
        worker_ctx: egui::Context,
    ) {
        for _ in 0..loading::WORKER_THREADS {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);
            let worker_gen = Arc::clone(&generation);
            let worker_archive_reader = Arc::clone(&archive_reader);
            let ctx = worker_ctx.clone();
            
            std::thread::spawn(move || {
                let mut zip_cache: Option<(PathBuf, zip::ZipArchive<std::fs::File>)> = None;
                loop {
                    // ロックをこのスコープ内だけで取得・解放する
                    let req = {
                        let lock = rx.lock().ok();
                        match lock.and_then(|l| l.recv().ok()) {
                            Some(r) => r,
                            None => break,
                        }
                    };

                    let current_gen = worker_gen.load(Ordering::Relaxed);
                    if req.generation < current_gen { continue; }
                    let current_idx = worker_idx.load(Ordering::Relaxed);
                    
                    // スキップ対象なら、メインスレッドに通知せず静かに捨てる (0.1.0 互換)
                    if req.index != current_idx && (req.index as isize - current_idx as isize).abs() > loading::LOAD_SKIP_DISTANCE_THRESHOLD {
                        continue;
                    }
                    
                    let result_data = Self::process_load_request(&req, &mut zip_cache, &worker_archive_reader);
                    let _ = tx.send(LoadResult { index: req.index, key: req.key, data: result_data, generation: req.generation });
                    ctx.request_repaint();
                }
            });
        }
    }

    /// 画像読み込みリクエストを処理する
    fn process_load_request(
        req: &LoadRequest,
        zip_cache: &mut Option<(PathBuf, zip::ZipArchive<std::fs::File>)>,
        archive_reader: &Arc<dyn ArchiveReader>,
    ) -> std::result::Result<Vec<FrameData>, String> {
        let kind = utils::detect_kind(&req.archive_path);
        let bytes = if let Some(idx) = req.entry_index {
            if matches!(kind, utils::ArchiveKind::Zip) {
                if zip_cache.as_ref().map(|(p, _)| p != &req.archive_path).unwrap_or(true) {
                    let file = std::fs::File::open(&req.archive_path).map_err(|e| e.to_string())?;
                    // ZipArchive は自身でシークを管理するため、BufReader は不要。File を直接渡す。
                    let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                    *zip_cache = Some((req.archive_path.clone(), zip));
                }
                let (_, ref mut zip) = zip_cache.as_mut().ok_or("Cache error")?;
                let mut entry = zip.by_index(idx).map_err(|e| e.to_string())?;
                let mut buf = Vec::with_capacity(entry.size() as usize);
                if let Err(e) = entry.read_to_end(&mut buf) {
                    // CRC不一致はデータ自体は読めているので続行、それ以外はエラー
                    if !e.to_string().contains("Invalid checksum") || buf.is_empty() {
                        return Err(e.to_string());
                    }
                }
                buf
            } else if matches!(kind, utils::ArchiveKind::Pdf) {
                if zip_cache.is_some() { *zip_cache = None; }
                archive_reader.read_entry(&req.archive_path, &req.entry_name, Some(idx), req.max_dim).map_err(|e| e.to_string())?
            } else {
                // 通常画像は定数の 1920 を使用
                archive_reader.read_entry(&req.archive_path, &req.entry_name, Some(idx), image::MAX_TEX_DIM).map_err(|e| e.to_string())?
            }
        } else {
            // Plainファイルまたは7zアーカイブからの読み込み
            if zip_cache.is_some() { *zip_cache = None; }
            // PDF以外は定数 1920 を上限として扱う
            let limit = if kind == utils::ArchiveKind::Pdf { req.max_dim } else { image::MAX_TEX_DIM };
            archive_reader.read_entry(&req.archive_path, &req.entry_name, None, limit).map_err(|e| e.to_string())?
        };

        let ext = req.entry_name.to_ascii_lowercase();

        if (ext.ends_with(".gif") || ext.ends_with(".webp")) && bytes.len() <= loading::MAX_ANIM_DECODE_SIZE {
            let frames_res: ::image::ImageResult<Vec<::image::Frame>> = if ext.ends_with(".gif") {
                ::image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                    .and_then(|d| d.into_frames().collect::<::image::ImageResult<Vec<_>>>())
            } else {
                ::image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes))
                    .and_then(|d| {
                        if d.has_animation() {
                            d.into_frames().collect::<::image::ImageResult<Vec<_>>>()
                        } else {
                            Err(::image::ImageError::IoError(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Not animated WebP",
                            )))
                        }
                    })
            };

            match frames_res {
                Ok(frames) => {
                    // メモリ保護: フレーム数が極端に多い場合は静止画として扱う
                    if frames.len() > 200 {
                        return Ok(vec![FrameData { image: frames[0].clone().into_buffer(), delay_ms: 0 }]);
                    }

                    let mut result_frames = Vec::new();
                    for frame in frames {
                        let delay = frame.delay();
                        let (n, d) = delay.numer_denom_ms();
                        let delay_ms = if d > 0 { n / d } else { loading::DEFAULT_ANIM_FRAME_DELAY_MS };
                        let delay_ms = if delay_ms < loading::MIN_ANIM_FRAME_DELAY_MS { loading::DEFAULT_ANIM_FRAME_DELAY_MS } else { delay_ms };

                        let img = frame.into_buffer();
                        let img = downscale_if_needed(img, req.max_dim, req.filter_mode);
                        let img = apply_rotation(img, req.rotation);
                        result_frames.push(FrameData { image: img, delay_ms });
                    }
                    return Ok(result_frames);
                }
                Err(_) => {} // フォールバックして静的画像として扱う
            }
        }

        let img = ::image::load_from_memory(&bytes).map_err(|e| e.to_string())?.into_rgba8();

        let img = if kind == utils::ArchiveKind::Pdf {
            img
        } else {
            downscale_if_needed(img, req.max_dim, req.filter_mode)
        };
        let img = apply_rotation(img, req.rotation);
        
        Ok(vec![FrameData { image: img, delay_ms: 0 }])
    }

    pub fn new(ctx: egui::Context, archive_reader: Arc<dyn ArchiveReader>) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();
        let current_idx_shared = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicU64::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));

        // ワーカースレッドを作成
        Self::spawn_worker_threads(
            Arc::clone(&req_rx),
            res_tx,
            Arc::clone(&current_idx_shared),
            Arc::clone(&generation),
            Arc::clone(&archive_reader),
            ctx.clone(),
        );

        Self {
            archive_path: None,
            entries: Vec::new(),
            entries_meta: Vec::new(),
            current: 0,
            target_index: 0,
            rotations: HashMap::new(),
            open_from_end: false,
            tree: NavTree::new(Arc::clone(&archive_reader)),
            pending_focus: None,
            pending_go_last: false,
            cache: HashMap::new(),
            cache_lru: VecDeque::new(),
            load_tx: req_tx,
            load_rx: res_rx,
            list_rx: None,
            is_listing: false,
            pending: HashSet::new(),
            current_idx_shared,
            generation,
            archive_reader,
            ctx,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, config: &Config, manga: bool, shift: bool) -> (Vec<(usize, String)>, Option<String>) {
        // アーカイブリストの取得完了をチェック
        let mut list_error = None;
        let mut list_finished = false;
        if let Some(ref rx) = self.list_rx {
            match rx.try_recv() {
                Ok(res) => {
                    list_finished = true;
                    match res {
                        Ok((path, entries)) => {
                            self.entries_meta = entries;
                            self.archive_path = Some(path);
                            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
                            if !self.entries.is_empty() {
                                self.apply_sorting(config);
                                let go_last = self.pending_go_last;
                                self.pending_go_last = false;
                                if let Some(focus) = self.pending_focus.take() {
                                    self.current = self.entries.iter().position(|n| n.contains(&focus)).unwrap_or(0);
                                } else {
                                    self.current = if go_last {
                                        let last = self.entries.len().saturating_sub(1);
                                        if manga && last > 0 {
                                            if (last % 2 == 0) == shift { last } else { last.saturating_sub(1) }
                                        } else { last }
                                    } else { 0 };
                                }
                                self.target_index = self.current;
                                self.schedule_prefetch(config.filter_mode, manga, config.pdf_render_size);
                            }
                        }
                        Err(e) => { list_error = Some(e); }
                }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    list_finished = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if list_finished {
            self.is_listing = false;
            self.list_rx = None;
            ctx.request_repaint();
        }
        
        let mut failures = Vec::new();
        let current_gen = self.generation.load(Ordering::Relaxed);
        let mut upload_count = 0;

        while let Ok(result) = self.load_rx.try_recv() {
            // 世代が違っても pending からは削除しないと、新しい世代のリクエストがブロックされる
            self.pending.remove(&result.key);
            if result.generation != current_gen {
                continue;
            }

            match result.data {
                Ok(frames) => {
                    let filter = if config.filter_mode == FilterMode::Nearest { TextureOptions::NEAREST } else { TextureOptions::LINEAR };
                    let cached = if frames.len() == 1 {
                        let img = &frames[0].image;
                        let ci = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                        let tex = ctx.load_texture(format!("img_{}", result.key), ci, filter);
                        CachedImage::Static(tex)
                    } else if !frames.is_empty() {
                        let total_ms = frames.iter().map(|f| f.delay_ms).sum();
                        let tex_frames = frames.into_iter().enumerate().map(|(fi, f)| {
                            let ci = ColorImage::from_rgba_unmultiplied([f.image.width() as usize, f.image.height() as usize], f.image.as_raw());
                            (ctx.load_texture(format!("img_{}_{}", result.key, fi), ci, filter), f.delay_ms)
                        }).collect();
                        CachedImage::Animated {
                            frames: tex_frames,
                            total_ms,
                            loop_start_time: ctx.input(|i| i.time),
                        }
                    } else {
                        continue;
                    };

                    self.cache.insert(result.key.clone(), cached);
                    self.cache_lru.push_back(result.key);

                    upload_count += 1;
                    if self.cache_lru.len() > cache::CACHE_MAX {
                        if let Some(old) = self.cache_lru.pop_front() {
                            self.cache.remove(&old);
                        }
                    }
                }
                Err(e) => failures.push((result.index, e)),
            }
            if upload_count >= loading::MAX_TEXTURE_UPLOADS_PER_FRAME { break; }
        }
        if upload_count > 0 { ctx.request_repaint(); }
        (failures, list_error)
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

    /// 現在キャッシュに保持されている画像数を返す
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// 現在のキャッシュの推定合計サイズ（バイト）を返す
    pub fn total_cache_size_bytes(&self) -> usize {
        self.cache.values().map(|img| {
            match img {
                CachedImage::Static(tex) => tex.size()[0] * tex.size()[1] * 4,
                CachedImage::Animated { frames, .. } => {
                    frames.iter().map(|(t, _)| t.size()[0] * t.size()[1] * 4).sum::<usize>()
                }
            }
        }).sum()
    }

    pub fn is_spread(&self, index: usize) -> bool {
        self.get_first_tex(index).map(|t| t.size_vec2().x > t.size_vec2().y).unwrap_or(false)
    }

    pub fn open_path(&mut self, path: PathBuf, _config: &Config) {
        let path = utils::clean_path(&path);
        self.clear_cache();
        self.entries.clear();
        self.entries_meta.clear();
        self.current = 0;
        self.target_index = 0;
        self.is_listing = true;
        self.pending_go_last = self.open_from_end;

        let (base_path, start_name) = if path.is_file() && utils::is_image_ext(&path.to_string_lossy()) {
            let parent = path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| path.clone());
            let name = path.file_name().map(|f| f.to_string_lossy().to_string());
            (parent, name)
        } else {
            (path, None)
        };

        let (tx, rx) = mpsc::channel(); // list_images の結果を受け取るチャンネル
        self.list_rx = Some(rx);
        let bp_clone = base_path.clone();
        let reader = Arc::clone(&self.archive_reader);
        let ctx = self.ctx.clone();

        std::thread::spawn(move || {
            let res = reader.list_images(&bp_clone)
                .map(|entries| (bp_clone, entries))
                .map_err(|e| e.user_message());
            let _ = tx.send(res); // 結果を送信
            ctx.request_repaint();
        });

        // 暫定的にパスだけ設定し、リストは update で受け取る
        self.archive_path = Some(base_path);
        if let Some(name) = start_name {
            self.entries = vec![name.clone()]; // ロード中の表示用
            self.pending_focus = Some(name);
        }
    }

    pub fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, _config: &Config, _manga: bool, _shift: bool) {
        let path = crate::utils::clean_path(&path);
        self.clear_cache();
        self.entries.clear();
        self.entries_meta.clear();
        self.current = 0;
        self.target_index = 0;
        self.is_listing = true;
        self.pending_go_last = go_last;
        self.pending_focus = focus_hint
            .and_then(|h| h.file_name().map(|f| f.to_string_lossy().to_string()));

        let (tx, rx) = mpsc::channel();
        self.list_rx = Some(rx);
        let bp_clone = path.clone();
        let reader = Arc::clone(&self.archive_reader);
        let ctx = self.ctx.clone();

        std::thread::spawn(move || {
            let res = reader.list_images(&bp_clone)
                .map(|entries| (bp_clone, entries))
                .map_err(|e| e.user_message());
            let _ = tx.send(res);
            ctx.request_repaint();
        });

        self.archive_path = Some(path);
    }

    pub fn go_next(&mut self, manga: bool, shift: bool, filter: FilterMode, max_dim: u32) -> bool {
        if self.entries.is_empty() { return false; }
        let step = if manga {
            if self.target_index + 1 >= self.entries.len() || (!shift && self.target_index == 0) { 1 }
            else if self.is_spread(self.target_index) || self.is_spread(self.target_index + 1) { 1 }
            else { 2 }
        } else { 1 };
        if self.target_index + step < self.entries.len() {
            self.target_index += step; self.schedule_prefetch(filter, manga, max_dim); true
        } else { false }
    }

    pub fn go_prev(&mut self, manga: bool, shift: bool, filter: FilterMode, max_dim: u32) -> bool {
        if self.target_index == 0 { return false; }
        let step = if manga {
            let first_pair = if shift { 0 } else { 1 };
            if self.target_index <= first_pair || self.target_index < 2 { 1 }
            else if self.is_spread(self.target_index - 1) || self.is_spread(self.target_index - 2) { 1 }
            else { 2 }
        } else { 1 };
        self.target_index = self.target_index.saturating_sub(step);
        self.schedule_prefetch(filter, manga, max_dim);
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

    pub fn schedule_prefetch(&mut self, filter_mode: FilterMode, manga: bool, max_dim: u32) {
        let Some(path) = self.archive_path.as_ref() else { return };
        let len = self.entries.len();
        if len == 0 { return; }
        self.current_idx_shared.store(self.target_index, Ordering::Relaxed);
        
        // マンガモードを考慮したプリフェッチ範囲の調整
        let step = if manga { 2 } else { 1 };
        let lo = self.target_index.saturating_sub(cache::PREFETCH_BEHIND * step);
        let hi = (self.target_index + (cache::PREFETCH_AHEAD + 1) * step).min(len);

        let gen = self.generation.load(Ordering::Relaxed);
        let mut req = |idx: usize| {
            if idx >= len { return; }
            let entry = &self.entries_meta[idx];
            let key = format!("{}:{}", idx, entry.name);
            if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }
            let rot = self.rotations.get(&entry.name).copied().unwrap_or(Rotation::R0); // Rotation は Manager 内で定義
            let kind = utils::detect_kind(path);
            let entry_idx = if matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::Pdf) { Some(entry.archive_index) } else { None };
            self.pending.insert(key.clone());
            let _ = self.load_tx.send(LoadRequest {
                index: idx, key, archive_path: path.to_path_buf(), entry_name: entry.name.clone(),
                entry_index: entry_idx, rotation: rot, max_dim, filter_mode, generation: gen,
            });
        };

        // 1. 最優先：現在表示すべきページ
        req(self.target_index);
        if manga { req(self.target_index + 1); }
        
        // 2. 次点：周辺ページ
        for i in lo..hi { req(i); }
        
        // 3. 0.1.0 互換の積極的なキャッシュクリーンアップ
        self.cache.retain(|k, _| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
        self.pending.retain(|k| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
        self.cache_lru.retain(|k| self.cache.contains_key(k));
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn invalidate_cache_for(&mut self, _index: usize, entry_name: &str) {
        self.invalidate(entry_name);
    }

    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
        self.cache_lru.retain(|k| k != key);
        self.pending.remove(key);
    }
}

fn downscale_if_needed(img: ::image::RgbaImage, max_dim: u32, filter: FilterMode) -> ::image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let (nw, nh) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
    
    let filter_type = match filter {
        FilterMode::Nearest => ::image::imageops::FilterType::Nearest,
        FilterMode::Bilinear => ::image::imageops::FilterType::Triangle,
        FilterMode::Bicubic => ::image::imageops::FilterType::CatmullRom,
        FilterMode::Lanczos => ::image::imageops::FilterType::Lanczos3,
    };
    ::image::imageops::resize(&img, nw, nh, filter_type)
}

fn apply_rotation(img: ::image::RgbaImage, rot: Rotation) -> ::image::RgbaImage {
    match rot {
        Rotation::R0   => img,
        Rotation::R90  => ::image::imageops::rotate90(&img),
        Rotation::R180 => ::image::imageops::rotate180(&img),
        Rotation::R270 => ::image::imageops::rotate270(&img),
    }
}
