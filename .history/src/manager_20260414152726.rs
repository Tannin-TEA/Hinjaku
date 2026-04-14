use crate::archive;
use crate::config::{Config, SortMode, SortOrder};
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
    linear_filter: bool,
    generation: u64,
}

struct LoadResult {
    index: usize,
    key: String,
    data: Result<Vec<FrameData>, String>,
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

    // --- Loader State ---
    cache: HashMap<String, CachedImage>,
    cache_lru: VecDeque<String>,
    load_tx: Sender<LoadRequest>,
    load_rx: Receiver<LoadResult>,
    pending: HashSet<String>,
    current_idx_shared: Arc<AtomicUsize>,
    generation: Arc<AtomicU64>,
}

impl Manager {
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
                let mut zip_cache: Option<(PathBuf, zip::ZipArchive<BufReader<std::fs::File>>)> = None;
                while let Ok(req) = rx.lock().unwrap().recv() {
                    let current_gen = worker_gen.load(Ordering::Relaxed);
                    if req.generation < current_gen { continue; }
                    let current_idx = worker_idx.load(Ordering::Relaxed);
                    if req.index != current_idx && (req.index as isize - current_idx as isize).abs() > 12 {
                        let _ = tx.send(LoadResult { index: req.index, key: req.key, data: Err("SKIPPED".to_string()), generation: req.generation });
                        continue;
                    }
                    let result_data = (|| -> Result<Vec<FrameData>, String> {
                        let bytes = if let Some(idx) = req.entry_index {
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
                            if zip_cache.is_some() { zip_cache = None; }
                            archive::read_entry(&req.archive_path, &req.entry_name, None).map_err(|e| e.to_string())?
                        };

                        let ext = req.entry_name.to_ascii_lowercase();
                        let size_limit = 30 * 1024 * 1024; // 30MB

                        if (ext.ends_with(".gif") || ext.ends_with(".webp")) && bytes.len() <= size_limit {
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
                                            if d > 0 { n / d } else { 100 }
                                        };
                                        let delay_ms = if delay_ms < 20 { 100 } else { delay_ms };
                                        let img = frame.into_buffer();
                                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
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
                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                        let img = apply_rotation(img, req.rotation);
                        Ok(vec![FrameData { image: img, delay_ms: 0 }])
                    })();
                    let _ = tx.send(LoadResult { index: req.index, key: req.key, data: result_data, generation: req.generation });
                }
            });
        }

        Self {
            archive_path: None, entries: Vec::new(), entries_meta: Vec::new(), current: 0, target_index: 0,
            rotations: HashMap::new(), open_from_end: false, tree: NavTree::default(),
            cache: HashMap::new(), cache_lru: VecDeque::new(), load_tx: req_tx, load_rx: res_rx,
            pending: HashSet::new(), current_idx_shared, generation,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, linear_filter: bool) -> Vec<(usize, String)> {
        let mut failures = Vec::new();
        let current_gen = self.generation.load(Ordering::Relaxed);
        while let Ok(result) = self.load_rx.try_recv() {
            if result.generation != current_gen { continue; }
            self.pending.remove(&result.key);
            match result.data {
                Ok(frames) => {
                    let filter = if linear_filter { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
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
        }
        failures
    }

    pub fn get_tex(&self, index: usize, now: f64) -> Option<(&TextureHandle, Option<f64>)> {
        let path = self.archive_path.as_ref()?;
        let entry = self.entries.get(index)?;
        let key = format!("{}:{}:{}", index, path.to_string_lossy(), entry);
        self.cache.get(&key).map(|c| c.current_frame(now))
    }

    pub fn get_first_tex(&self, index: usize) -> Option<&TextureHandle> {
        let path = self.archive_path.as_ref()?;
        let entry = self.entries.get(index)?;
        let key = format!("{}:{}:{}", index, path.to_string_lossy(), entry);
        self.cache.get(&key).map(|c| c.first_frame())
    }

    pub fn is_spread(&self, index: usize) -> bool {
        self.get_first_tex(index).map(|t| t.size_vec2().x > t.size_vec2().y).unwrap_or(false)
    }

    pub fn open_path(&mut self, path: PathBuf, config: &Config) {
        let path = archive::clean_path(&path);
        self.clear_cache();
        let (base_path, start_name) = if path.is_file() && archive::is_image_ext(&path.to_string_lossy()) {
            (path.parent().unwrap().to_path_buf(), Some(path.file_name().unwrap().to_string_lossy().to_string()))
        } else { (path, None) };

        if let Ok(entries) = archive::list_images(&base_path) {
            self.entries_meta = entries;
            self.apply_sorting(config);
            if let Some(ref name) = start_name {
                self.current = self.entries.iter().position(|n| n.contains(name)).unwrap_or(0);
            } else { self.current = 0; }
            self.target_index = self.current;
            self.archive_path = Some(base_path);
            self.schedule_prefetch(config.linear_filter, false);
        }
    }

    pub fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, config: &Config, manga: bool, shift: bool) {
        let path = archive::clean_path(&path);
        self.clear_cache();
        if let Ok(entries) = archive::list_images(&path) {
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
            self.schedule_prefetch(config.linear_filter, manga);
        }
    }

    pub fn go_next(&mut self, manga: bool, shift: bool, linear: bool) -> bool {
        if self.entries.is_empty() { return false; }
        let step = if manga {
            if self.target_index + 1 >= self.entries.len() || (!shift && self.target_index == 0) { 1 }
            else if self.is_spread(self.target_index) || self.is_spread(self.target_index + 1) { 1 }
            else { 2 }
        } else { 1 };
        if self.target_index + step < self.entries.len() {
            self.target_index += step; self.schedule_prefetch(linear, manga); true
        } else { false }
    }

    pub fn go_prev(&mut self, manga: bool, shift: bool, linear: bool) -> bool {
        if self.target_index == 0 { return false; }
        let step = if manga {
            let first_pair = if shift { 0 } else { 1 };
            if self.target_index <= first_pair || self.target_index < 2 { 1 }
            else if self.is_spread(self.target_index - 1) || self.is_spread(self.target_index - 2) { 1 }
            else { 2 }
        } else { 1 };
        self.target_index = self.target_index.saturating_sub(step);
        self.schedule_prefetch(linear, manga);
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
        let path = self.archive_path.as_ref()?;
        let entry = self.entries.get(self.current)?;
        Some(archive::join_entry_path(path, entry))
    }

    pub fn apply_sorting(&mut self, config: &Config) {
        let current_name = self.entries.get(self.current).cloned();
        self.entries_meta.sort_by(|a, b| {
            let res = match config.sort_mode {
                SortMode::Name => if config.sort_natural { archive::natord(&a.name, &b.name) } else { a.name.cmp(&b.name) }
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

    pub fn schedule_prefetch(&mut self, linear: bool, manga: bool) {
        let Some(path) = self.archive_path.as_ref() else { return };
        let len = self.entries.len();
        if len == 0 { return; }
        self.current_idx_shared.store(self.target_index, Ordering::Relaxed);
        let lo = self.target_index.saturating_sub(PREFETCH_BEHIND);
        let hi = (self.target_index + PREFETCH_AHEAD + 1).min(len);

        let gen = self.generation.load(Ordering::Relaxed);
        let mut req = |idx: usize| {
            let entry = &self.entries_meta[idx];
            let key = format!("{}:{}:{}", idx, path.to_string_lossy(), entry.name);
            if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }
            let rot = self.rotations.get(&entry.name).copied().unwrap_or(Rotation::R0);
            let entry_idx = if matches!(archive::detect_kind(path), archive::ArchiveKind::Zip) { Some(entry.archive_index) } else { None };
            self.pending.insert(key.clone());
            let _ = self.load_tx.send(LoadRequest {
                index: idx, key, archive_path: path.clone(), entry_name: entry.name.clone(),
                entry_index: entry_idx, rotation: rot, max_dim: MAX_TEX_DIM, linear_filter: linear, generation: gen,
            });
        };

        req(self.target_index);
        if manga && self.target_index + 1 < len { req(self.target_index + 1); }
        for i in lo..hi { req(i); }
        self.cache.retain(|k, _| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
        self.pending.retain(|k| k.split(':').next().and_then(|s| s.parse::<usize>().ok()).map(|i| i >= lo && i < hi).unwrap_or(false));
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear(); self.cache_lru.clear(); self.pending.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn invalidate_cache_for(&mut self, index: usize, path: &Path, entry_name: &str) {
        let key = format!("{}:{}:{}", index, path.to_string_lossy(), entry_name);
        self.invalidate(&key);
    }

    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key); self.cache_lru.retain(|k| k != key); self.pending.remove(key);
    }
}

fn downscale_if_needed(img: image::RgbaImage, max_dim: u32, linear: bool) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let (nw, nh) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
    if linear { image::imageops::thumbnail(&img, nw, nh) } else { image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Nearest) }
}

fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot { Rotation::R0=>img, Rotation::R90=>image::imageops::rotate90(&img), Rotation::R180=>image::imageops::rotate180(&img), Rotation::R270=>image::imageops::rotate270(&img) }
}

#[derive(Default)]
pub struct NavTree {
    pub nodes: HashMap<PathBuf, Vec<PathBuf>>,
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub image_counts: HashMap<PathBuf, usize>,
}

impl NavTree {
    pub fn get_children(&mut self, dir_path: &Path) -> Vec<PathBuf> {
        let dir_path = archive::clean_path(dir_path);
        if let Some(cached) = self.nodes.get(&dir_path) { return cached.clone(); }
        let targets = archive::list_nav_targets(&dir_path).unwrap_or_default();
        self.nodes.insert(dir_path, targets.clone());
        targets
    }
    pub fn get_siblings(&mut self, path: &Path) -> Vec<PathBuf> {
        path.parent().map(|p| self.get_children(p)).unwrap_or_default()
    }
    pub fn get_image_count(&mut self, path: &Path) -> usize {
        let path = archive::clean_path(path);
        if let Some(&count) = self.image_counts.get(&path) { return count; }
        let count = archive::list_images(&path).map(|e| e.len()).unwrap_or(0);
        self.image_counts.insert(path, count);
        count
    }
    pub fn expand_to_path(&mut self, path: &Path) {
        let mut curr = Some(archive::clean_path(path));
        while let Some(p) = curr {
            if let Some(parent) = p.parent() { self.expanded.insert(parent.to_path_buf()); curr = Some(parent.to_path_buf()); }
            else { break; }
        }
    }

    pub fn get_relative_target(&mut self, current: &Path, forward: bool) -> Option<PathBuf> {
        let curr = archive::clean_path(current);
        let siblings = self.get_siblings(&curr);
        let pos = siblings.iter().position(|p| p == &curr)?;
        let next_pos = if forward { pos + 1 } else { pos.checked_sub(1)? };
        siblings.get(next_pos).cloned()
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.selected.is_none() {
            if let Some(first_root) = archive::get_roots().first() {
                self.selected = Some(first_root.clone());
            }
            return;
        }

        if let Some(sel) = self.selected.clone() {
            let siblings = self.get_siblings(&sel);
            if let Some(pos) = siblings.iter().position(|p| p == &sel) {
                let new_pos = (pos as isize + delta).clamp(0, siblings.len() as isize - 1) as usize;
                self.selected = Some(siblings[new_pos].clone());
            }
        }
    }

    pub fn expand_current(&mut self) {
        if let Some(sel) = self.selected.clone() {
            let kind = archive::detect_kind(&sel);
            if kind == archive::ArchiveKind::Plain {
                // 展開して即座に中に入る
                self.expanded.insert(sel.clone());
                let children = self.get_children(&sel);
                if let Some(first) = children.first() {
                    self.selected = Some(first.clone());
                }
            }
        }
    }

    pub fn collapse_or_up(&mut self) {
        if let Some(sel) = self.selected.clone() {
            if self.expanded.contains(&sel) {
                // 開いているディレクトリなら閉じる
                self.expanded.remove(&sel);
            } else {
                // 閉じている、あるいはファイルなら親ディレクトリを選択する
                if let Some(parent) = sel.parent() {
                    let p = archive::clean_path(parent);
                    // ループ防止: ルートディレクトリ（C:\等）で止まるようにする
                    if p != sel {
                        self.selected = Some(p);
                    }
                }
            }
        }
    }

    pub fn activate_current(&mut self) -> Option<PathBuf> {
        self.selected.clone()
    }
}