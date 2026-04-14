use crate::archive;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::fs::OpenOptions;
use std::io::Write;

const CACHE_MAX: usize = 13;
const PREFETCH_AHEAD: usize = 5;
const PREFETCH_BEHIND: usize = 5;
const MAX_TEX_DIM: u32 = 4096;

struct LoadRequest {
    index: usize,
    key: String,
    archive_path: PathBuf,
    entry_name: String,
    rotation: Rotation,
    max_dim: u32,
    linear_filter: bool,
}

struct LoadResult {
    index: usize,
    key: String,
    image: image::RgbaImage,
}

pub struct ImageLoader {
    cache: HashMap<String, TextureHandle>,
    cache_lru: VecDeque<String>,
    load_tx: Sender<LoadRequest>,
    load_rx: Receiver<LoadResult>,
    pending: std::collections::HashSet<String>,
    current_idx_shared: Arc<AtomicUsize>,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();
        let current_idx_shared = Arc::new(AtomicUsize::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));

        for _ in 0..4 {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);
            std::thread::spawn(move || {
                while let Ok(req) = rx.lock().unwrap().recv() {
                    let current = worker_idx.load(Ordering::Relaxed);
                    if (req.index as isize - current as isize).abs() > 10 { continue; }

                    let result = (|| -> Option<LoadResult> {
                        let bytes = archive::read_entry(&req.archive_path, &req.entry_name).ok()?;
                        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                        let img = apply_rotation(img, req.rotation);
                        Some(LoadResult { index: req.index, key: req.key, image: img })
                    })();
                    if let Some(r) = result { let _ = tx.send(r); }
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
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
    }

    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
        self.cache_lru.retain(|k| k != key);
        self.pending.remove(key);
    }

    pub fn get_texture(&self, key: &str) -> Option<&TextureHandle> {
        self.cache.get(key)
    }

    pub fn collect_results(&mut self, ctx: &egui::Context, linear_filter: bool, entries: &[String]) {
        while let Ok(result) = self.load_rx.try_recv() {
            self.pending.remove(&result.key);
            if entries.get(result.index) != Some(&result.key.split(':').nth(1).unwrap_or_default().to_string()) {
                continue;
            }

            let size = [result.image.width() as usize, result.image.height() as usize];
            let ci = ColorImage::from_rgba_unmultiplied(size, &result.image.into_raw());
            let filter = if linear_filter { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
            let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
            
            self.cache.insert(result.key.clone(), tex);
            self.cache_lru.push_back(result.key);
            while self.cache_lru.len() > CACHE_MAX {
                if let Some(old_key) = self.cache_lru.pop_front() { self.cache.remove(&old_key); }
            }
            ctx.request_repaint();
        }
    }

    pub fn schedule_prefetch(&mut self, target: usize, archive_path: &Path, entries: &[String], rotations: &HashMap<String, Rotation>, manga_mode: bool, linear_filter: bool) {
        let len = entries.len();
        if len == 0 { return; }
        self.current_idx_shared.store(target, Ordering::Relaxed);

        let lo = target.saturating_sub(PREFETCH_BEHIND);
        let hi = (target + PREFETCH_AHEAD + 1).min(len);

        self.request_load(target, archive_path, entries, rotations, linear_filter);
        if manga_mode && target + 1 < len {
            self.request_load(target + 1, archive_path, entries, rotations, linear_filter);
        }
        for i in lo..hi { self.request_load(i, archive_path, entries, rotations, linear_filter); }

        self.cache.retain(|k, _| Self::key_in_range(k, lo, hi));
        self.cache_lru.retain(|k| self.cache.contains_key(k));
        self.pending.retain(|k| Self::key_in_range(k, lo, hi));
    }

    fn request_load(&mut self, index: usize, archive_path: &Path, entries: &[String], rotations: &HashMap<String, Rotation>, linear_filter: bool) {
        let Some(entry_name) = entries.get(index) else { return };
        let key = format!("{}:{}", index, entry_name);
        if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }

        let rotation = rotations.get(entry_name).copied().unwrap_or(Rotation::R0);
        self.pending.insert(key.clone());
        let _ = self.load_tx.send(LoadRequest {
            index, key, archive_path: archive_path.to_path_buf(),
            entry_name: entry_name.clone(), rotation, max_dim: MAX_TEX_DIM, linear_filter,
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