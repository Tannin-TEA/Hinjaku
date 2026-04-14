use crate::archive;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

struct LoadResult {
    index: usize,
    key: String,
    data: Result<image::RgbaImage, String>,
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
                // スレッドごとに直近のZIPアーカイブを保持する（スティッキーハンドル）
                let mut zip_cache: Option<(PathBuf, zip::ZipArchive<BufReader<std::fs::File>>)> = None;

                while let Ok(req) = rx.lock().unwrap().recv() {
                    let current = worker_idx.load(Ordering::Relaxed);
                    if (req.index as isize - current as isize).abs() > 10 { continue; }

                    let result_data = (|| -> Result<image::RgbaImage, String> {
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
                            archive::read_entry(&req.archive_path, &req.entry_name, None).map_err(|e| e.to_string())?
                        };

                        let img = image::load_from_memory(&bytes)
                            .map_err(|e| format!("Decode error: {}", e))?
                            .to_rgba8();
                        let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
                        let img = apply_rotation(img, req.rotation);
                        Ok(img)
                    })();
                    
                    let _ = tx.send(LoadResult { index: req.index, key: req.key, data: result_data });
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

    pub fn collect_results(&mut self, ctx: &egui::Context, linear_filter: bool, entries: &[String]) -> Vec<(usize, String)> {
        let mut failures = Vec::new();
        while let Ok(result) = self.load_rx.try_recv() {
            self.pending.remove(&result.key);
            if entries.get(result.index) != Some(&result.key.split(':').nth(1).unwrap_or_default().to_string()) {
                continue;
            }

            match result.data {
                Ok(img) => {
                    let size = [img.width() as usize, img.height() as usize];
                    let ci = ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                    let filter = if linear_filter { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
                    let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
                    
                    self.cache.insert(result.key.clone(), tex);
                    self.cache_lru.push_back(result.key);
                    while self.cache_lru.len() > CACHE_MAX {
                        if let Some(old_key) = self.cache_lru.pop_front() { self.cache.remove(&old_key); }
                    }
                }
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
        let key = format!("{}:{}", index, entry.name);
        if self.cache.contains_key(&key) || self.pending.contains(&key) { return; }

        let rotation = rotations.get(&entry.name).copied().unwrap_or(Rotation::R0);
        let entry_index = if matches!(archive::detect_kind(archive_path), archive::ArchiveKind::Zip) { Some(entry.archive_index) } else { None };
        self.pending.insert(key.clone());
        let _ = self.load_tx.send(LoadRequest {
            index, key, archive_path: archive_path.to_path_buf(), entry_index,
            entry_name: entry.name.clone(), rotation, max_dim: MAX_TEX_DIM, linear_filter,
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