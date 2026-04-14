use crate::archive;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::AnimationDecoder;
use std::collections::{HashMap, VecDeque};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

const CACHE_MAX: usize = 13;
const PREFETCH_AHEAD: usize = 5;
const PREFETCH_BEHIND: usize = 5;
const MAX_TEX_DIM: u32 = 4096;

#[derive(Clone, Copy, PartialEq)]
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
    linear_filter: bool,
    generation: u64,
}

pub struct FrameData {
    pub image: image::RgbaImage,
    pub delay_ms: u32,
}

struct LoadResult {
    index: usize,
    key: String,
    data: Result<Vec<FrameData>, String>,
    generation: u64,
}

/// キャッシュに格納される画像エントリ。
/// 静止画とアニメーションを統一的に扱う。
pub enum CachedImage {
    /// 静止画（JPEGやPNGなど）
    Static(TextureHandle),
    /// アニメーション画像（GIF/WebPなど）
    /// frames: (テクスチャ, そのフレームの表示時間ms)
    /// loop_start_time: アニメーション開始時刻（egui の time）
    Animated {
        frames: Vec<(TextureHandle, u32)>,
        /// 全フレームの合計時間（ms）
        total_ms: u32,
        /// このエントリがキャッシュに登録された時刻
        loop_start_time: f64,
    },
}

impl CachedImage {
    /// 現在時刻に対応するテクスチャを返す。
    /// アニメーションはループ再生し、次のフレーム切替までの秒数も返す。
    pub fn current_frame(&self, now: f64) -> (&TextureHandle, Option<f64>) {
        match self {
            CachedImage::Static(tex) => (tex, None),
            CachedImage::Animated { frames, total_ms, loop_start_time } => {
                if frames.len() == 1 || *total_ms == 0 {
                    return (&frames[0].0, None);
                }
                // 経過時間をループ周期で折り返す
                let elapsed_ms = ((now - loop_start_time) * 1000.0) as u32 % total_ms;
                let mut acc = 0u32;
                for (i, (tex, delay_ms)) in frames.iter().enumerate() {
                    acc += delay_ms;
                    if elapsed_ms < acc {
                        // 次のフレームまでの残り時間
                        let remain_sec = (acc - elapsed_ms) as f64 / 1000.0;
                        // 最終フレームなら次ループ先頭までの時間
                        let next_sec = if i + 1 < frames.len() {
                            remain_sec
                        } else {
                            (total_ms - elapsed_ms) as f64 / 1000.0
                        };
                        return (tex, Some(next_sec));
                    }
                }
                // フォールバック
                (&frames[0].0, None)
            }
        }
    }

    /// 代表テクスチャ（先頭フレーム）を返す。見開き判定など非時刻依存の用途に使う。
    pub fn first_frame(&self) -> &TextureHandle {
        match self {
            CachedImage::Static(tex) => tex,
            CachedImage::Animated { frames, .. } => &frames[0].0,
        }
    }
}

pub struct ImageLoader {
    cache: HashMap<String, CachedImage>,
    cache_lru: VecDeque<String>,
    load_tx: Sender<LoadRequest>,
    load_rx: Receiver<LoadResult>,
    pending: std::collections::HashSet<String>,
    current_idx_shared: Arc<AtomicUsize>,
    generation: Arc<AtomicU64>,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();
        let current_idx_shared = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicU64::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));

        for _ in 0..4 {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);
            let worker_gen = Arc::clone(&generation);
            std::thread::spawn(move || {
                // スレッドごとに直近のZIPアーカイブを保持する（スティッキーハンドル）
                let mut zip_cache: Option<(PathBuf, zip::ZipArchive<BufReader<std::fs::File>>)> = None;

                while let Ok(req) = rx.lock().unwrap().recv() {
                    let current_gen = worker_gen.load(Ordering::Relaxed);
                    if req.generation < current_gen {
                        // 古い世代のリクエストは完全に無視
                        continue;
                    }

                    let current_idx = worker_idx.load(Ordering::Relaxed);
                    if (req.index as isize - current_idx as isize).abs() > 12 {
                        // 遠すぎるリクエストをスキップ。
                        // メインスレッドの pending を解除させるため通知を送る。
                        let _ = tx.send(LoadResult { index: req.index, key: req.key, data: Err("SKIPPED".to_string()), generation: req.generation });
                        continue;
                    }

                    let result_data = (|| -> Result<Vec<FrameData>, String> {
                        let bytes = if let Some(idx) = req.entry_index {
                            // ZIPリクエストかつキャッシュが異なる場合は開き直す
                            if zip_cache.as_ref().map(|(p, _)| p != &req.archive_path).unwrap_or(true) {
                                let file = std::fs::File::open(&req.archive_path).map_err(|e| e.to_string())?;
                                let zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())?;
                                zip_cache = Some((req.archive_path.clone(), zip));
                            }
                            let (_, ref mut zip) = zip_cache.as_mut().unwrap();
                            let mut entry = zip.by_index(idx).map_err(|e| e.to_string())?;
                            let mut buf = Vec::new();
                            std::io::copy(&mut entry, &mut buf).map_err(|e| e.to_string())?;
                            buf
                        } else {
                            // 通常フォルダのリクエストが来たら、念のためZIPハンドルを解放してロックを解く
                            if zip_cache.is_some() {
                                zip_cache = None;
                            }
                            archive::read_entry(&req.archive_path, &req.entry_name, None).map_err(|e| e.to_string())?
                        };

                        let ext = req.entry_name.to_ascii_lowercase();
                        let is_gif = ext.ends_with(".gif");
                        let is_webp = ext.ends_with(".webp");
                        let size_limit = 30 * 1024 * 1024; // 30MB

                        // GIF アニメーション（30MB以下）
                        if is_gif && bytes.len() <= size_limit {
                            let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                                .map_err(|e| e.to_string())?;
                            let frames = decoder.into_frames().collect_frames().map_err(|e| e.to_string())?;
                            if frames.len() > 1 {
                                let mut output = Vec::new();
                                for frame in frames {
                                    let delay_ms = {
                                        let (numer, denom) = frame.delay().numer_denom_ms();
                                        if denom > 0 { numer / denom } else { 100 }
                                    };
                                    // delay が 0 や極端に小さい場合は 10fps 相当に補正
                                    let delay_ms = if delay_ms < 20 { 100 } else { delay_ms };
                                    let img = frame.into_buffer();
                                    let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                                    let img = apply_rotation(img, req.rotation);
                                    output.push(FrameData { image: img, delay_ms });
                                }
                                if !output.is_empty() { return Ok(output); }
                            }
                        }

                        // WebP アニメーション（30MB以下）
                        if is_webp && bytes.len() <= size_limit {
                            let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes))
                                .map_err(|e| e.to_string())?;
                            if decoder.has_animation() {
                                let frames = decoder.into_frames().collect_frames().map_err(|e| e.to_string())?;
                                if frames.len() > 1 {
                                    let mut output = Vec::new();
                                    for frame in frames {
                                        let delay_ms = {
                                            let (numer, denom) = frame.delay().numer_denom_ms();
                                            if denom > 0 { numer / denom } else { 100 }
                                        };
                                        let delay_ms = if delay_ms < 20 { 100 } else { delay_ms };
                                        let img = frame.into_buffer();
                                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                                        let img = apply_rotation(img, req.rotation);
                                        output.push(FrameData { image: img, delay_ms });
                                    }
                                    if !output.is_empty() { return Ok(output); }
                                }
                            }
                        }

                        // 静止画（または制限を超えたアニメーション）としてのデコード
                        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                        let img = apply_rotation(img, req.rotation);
                        Ok(vec![FrameData { image: img, delay_ms: 0 }])
                    })();
                    
                    let _ = tx.send(LoadResult { index: req.index, key: req.key, data: result_data, generation: req.generation });
                }
            });
        }

        Self {
            cache: HashMap::new(),
            cache_lru: VecDeque::new(),
            load_tx: req_tx,
            load_rx: res_rx,
            pending: std::collections::HashSet::new(),
            current_idx_shared,
            generation,
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
        self.cache_lru.retain(|k| k != key);
        self.pending.remove(key);
    }

    /// 静止画・アニメーション共通のテクスチャ取得。
    /// アニメーションの場合は現在時刻に対応するフレームと、次フレームまでの秒数を返す。
    pub fn get_image(&self, key: &str, now: f64) -> Option<(&TextureHandle, Option<f64>)> {
        self.cache.get(key).map(|img| img.current_frame(now))
    }

    /// 見開き判定など、時刻に依存しない先頭フレーム取得。
    pub fn get_first_frame(&self, key: &str) -> Option<&TextureHandle> {
        self.cache.get(key).map(|img| img.first_frame())
    }

    pub fn collect_results(&mut self, ctx: &egui::Context, linear_filter: bool, entries: &[String]) -> Vec<(usize, String)> {
        let mut failures = Vec::new();
        let current_gen = self.generation.load(Ordering::Relaxed);
        while let Ok(result) = self.load_rx.try_recv() {
            if result.generation != current_gen { continue; }
            self.pending.remove(&result.key);
            
            match result.data {
                Ok(frames) if !frames.is_empty() => {
                    let filter = if linear_filter { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
                    let now = ctx.input(|i| i.time);

                    let cached = if frames.len() == 1 {
                        // 静止画
                        let img = &frames[0].image;
                        let size = [img.width() as usize, img.height() as usize];
                        let ci = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                        let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
                        CachedImage::Static(tex)
                    } else {
                        // アニメーション：全フレームをテクスチャ化
                        let total_ms: u32 = frames.iter().map(|f| f.delay_ms).sum();
                        let tex_frames: Vec<(TextureHandle, u32)> = frames
                            .iter()
                            .enumerate()
                            .map(|(fi, f)| {
                                let size = [f.image.width() as usize, f.image.height() as usize];
                                let ci = ColorImage::from_rgba_unmultiplied(size, f.image.as_raw());
                                let tex = ctx.load_texture(
                                    format!("img_{}_{}", result.index, fi),
                                    ci,
                                    filter,
                                );
                                (tex, f.delay_ms)
                            })
                            .collect();
                        CachedImage::Animated {
                            frames: tex_frames,
                            total_ms,
                            loop_start_time: now,
                        }
                    };

                    self.cache.insert(result.key.clone(), cached);
                    self.cache_lru.push_back(result.key);
                    while self.cache_lru.len() > CACHE_MAX {
                        if let Some(old_key) = self.cache_lru.pop_front() {
                            self.cache.remove(&old_key);
                        }
                    }
                }
                Ok(_) => {} // フレームが空（通常は発生しない）
                Err(e) if e == "SKIPPED" => {} // 距離制限によるスキップ。pendingは解除済み。
                Err(e) => { failures.push((result.index, e)); }
            }
            ctx.request_repaint();
        }
        failures
    }

    pub fn schedule_prefetch(&mut self, target: usize, archive_path: &Path, entries: &[archive::ImageEntry], rotations: &HashMap<String, Rotation>, manga_mode: bool, linear_filter: bool) {
        let len = entries.len();
        if len == 0 { return; }
        self.current_idx_shared.store(target, Ordering::Relaxed);

        let lo = target.saturating_sub(PREFETCH_BEHIND);
        let hi = (target + PREFETCH_AHEAD + 1).min(len);

        // 現在表示すべきページを最優先でリクエスト
        self.request_load(target, archive_path, entries, rotations, linear_filter);
        if manga_mode && target + 1 < len {
            self.request_load(target + 1, archive_path, entries, rotations, linear_filter);
        }

        for i in lo..hi { self.request_load(i, archive_path, entries, rotations, linear_filter); }

        self.cache.retain(|k, _| Self::key_in_range(k, lo, hi));
        self.cache_lru.retain(|k| self.cache.contains_key(k));
        self.pending.retain(|k| Self::key_in_range(k, lo, hi));
    }

    fn request_load(&mut self, index: usize, archive_path: &Path, entries: &[archive::ImageEntry], rotations: &HashMap<String, Rotation>, linear_filter: bool) {
        let Some(entry) = entries.get(index) else { return };
        let key = format!("{}:{}:{}", index, archive_path.to_string_lossy(), entry.name);
        if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }

        let rotation = rotations.get(&entry.name).copied().unwrap_or(Rotation::R0);
        let entry_index = if matches!(archive::detect_kind(archive_path), archive::ArchiveKind::Zip) { Some(entry.archive_index) } else { None };
        let generation = self.generation.load(Ordering::Relaxed);
        self.pending.insert(key.clone());
        let _ = self.load_tx.send(LoadRequest {
            index, key, archive_path: archive_path.to_path_buf(), entry_index,
            entry_name: entry.name.clone(), rotation, max_dim: MAX_TEX_DIM, linear_filter,
            generation,
        });
    }

    fn key_in_range(key: &str, lo: usize, hi: usize) -> bool {
        key.split(':').next().and_then(|s| s.parse::<usize>().ok())
            .map(|idx| idx >= lo && idx < hi).unwrap_or(false)
    }
}

fn downscale_if_needed(img: image::RgbaImage, max_dim: u32, linear: bool) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);
    if linear { image::imageops::thumbnail(&img, nw, nh) }
    else { image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Nearest) }
}

fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot {
        Rotation::R0 => img,
        Rotation::R90 => image::imageops::rotate90(&img),
        Rotation::R180 => image::imageops::rotate180(&img),
        Rotation::R270 => image::imageops::rotate270(&img),
    }
}