use super::render::{apply_rotation, downscale_if_needed, Rotation};
use crate::archive;
use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

// ── 定数 ─────────────────────────────────────────────────────────────────────

/// キャッシュ保持上限（前後5枚 + 現在 = 最大11、余裕を持って13）
pub const CACHE_MAX: usize = 13;
/// 先読み範囲
pub const PREFETCH_AHEAD: usize = 5;
pub const PREFETCH_BEHIND: usize = 5;
/// 表示解像度上限（これ以上は縮小してからテクスチャ化）
pub const MAX_TEX_DIM: u32 = 4096;

// ── ワーカーへのリクエスト ────────────────────────────────────────────────────

pub struct LoadRequest {
    pub index: usize,
    /// キャッシュ照合用キー
    pub key: String,
    pub archive_path: PathBuf,
    pub entry_name: String,
    pub rotation: Rotation,
    pub max_dim: u32,
    pub linear_filter: bool,
}

// ── ワーカーからの結果 ────────────────────────────────────────────────────────

pub struct LoadResult {
    pub index: usize,
    pub key: String,
    pub image: image::RgbaImage,
}

// ── キャッシュエントリ ────────────────────────────────────────────────────────

struct CacheEntry {
    texture: TextureHandle,
}

// ── テクスチャキャッシュ ──────────────────────────────────────────────────────

/// LRU 方式のテクスチャキャッシュ。
///
/// # メモリ管理の方針
/// - キャッシュ上限（`CACHE_MAX`）を超えたら最古エントリを削除する。
/// - `TextureHandle` が drop されると egui 側の GPU リソースも解放される。
/// - `schedule_prefetch` が呼ばれるたびに表示範囲外のエントリを能動的に削除し、
///   上限に頼るだけの遅延解放を防ぐ。
pub struct TextureCache {
    map: HashMap<String, CacheEntry>,
    /// 先頭 = 最古
    lru: VecDeque<String>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    /// エントリを挿入する。上限を超えた分は最古から削除する。
    pub fn insert(&mut self, key: String, texture: TextureHandle) {
        // 既存キーなら LRU 順位を更新
        if self.map.contains_key(&key) {
            self.lru.retain(|k| k != &key);
        }
        self.map.insert(key.clone(), CacheEntry { texture });
        self.lru.push_back(key);

        // 上限超えを除去（TextureHandle drop → GPU リソース解放）
        while self.lru.len() > CACHE_MAX {
            if let Some(old_key) = self.lru.pop_front() {
                self.map.remove(&old_key);
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&TextureHandle> {
        self.map.get(key).map(|e| &e.texture)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// 指定キーを削除する（回転変更などでキャッシュを無効化する際に使用）
    pub fn remove(&mut self, key: &str) {
        self.map.remove(key);
        self.lru.retain(|k| k != key);
    }

    /// 表示範囲 [lo, hi) 外のエントリを全て削除する。
    /// `schedule_prefetch` から呼ばれることで、不要テクスチャを即座に解放する。
    pub fn retain_range(&mut self, lo: usize, hi: usize) {
        self.map.retain(|k, _| {
            let idx = key_to_index(k);
            idx >= lo && idx < hi
        });
        self.lru.retain(|k| self.map.contains_key(k));
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.lru.clear();
    }
}

// ── バックグラウンドローダー ──────────────────────────────────────────────────

/// バックグラウンドで画像をデコードするワーカープールを管理する。
///
/// # スレッドの寿命
/// `Sender` を drop すると `channel` が閉じられ、ワーカースレッドは自然終了する。
/// `App` の drop 時に `load_tx` が自動で drop されるため、明示的なシャットダウン不要。
pub struct Loader {
    pub tx: mpsc::SyncSender<LoadRequest>,
    pub rx: mpsc::Receiver<LoadResult>,
    /// ワーカーと共有する「現在のインデックス」（古いリクエストのスキップ用）
    pub current_idx_shared: Arc<AtomicUsize>,
}

impl Loader {
    /// `worker_count` 本のワーカースレッドを起動する。
    pub fn spawn(worker_count: usize) -> Self {
        // SyncSender でバックプレッシャーをかけ、キューを無制限に膨らませない
        // （バッファサイズ = CACHE_MAX * 2 程度が妥当）
        let (req_tx, req_rx) = mpsc::sync_channel::<LoadRequest>(CACHE_MAX * 2);
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();

        let current_idx_shared = Arc::new(AtomicUsize::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));

        for _ in 0..worker_count {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);

            std::thread::spawn(move || {
                loop {
                    // Mutex を取得してすぐ解放し、他ワーカーをブロックしない
                    let req = {
                        let lock = rx.lock().unwrap();
                        lock.recv()
                    };
                    let Ok(req) = req else { break };

                    // キューに溜まっている間にページが遠くへ移動していたらスキップ
                    let current = worker_idx.load(Ordering::Relaxed);
                    let dist = (req.index as isize - current as isize).unsigned_abs();
                    if dist > 10 {
                        continue;
                    }

                    let result = decode_entry(&req);
                    if let Some(r) = result {
                        // チャンネルが閉じていたら（App が drop 済み）終了
                        if tx.send(r).is_err() {
                            break;
                        }
                    }
                }
            });
        }

        Self {
            tx: req_tx,
            rx: res_rx,
            current_idx_shared,
        }
    }
}

/// 1 件の LoadRequest をデコードして LoadResult を返す。失敗したら None。
fn decode_entry(req: &LoadRequest) -> Option<LoadResult> {
    let bytes = match archive::read_entry(&req.archive_path, &req.entry_name) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("Failed to read entry {}: {}", req.entry_name, e);
            return None;
        }
    };

    let img = match image::load_from_memory(&bytes) {
        Ok(dyn_img) => dyn_img.to_rgba8(),
        Err(e) => {
            log::warn!("Failed to decode image {}: {}", req.entry_name, e);
            return None;
        }
    };

    let img = downscale_if_needed(img, req.max_dim, req.linear_filter);
    let img = apply_rotation(img, req.rotation);

    Some(LoadResult {
        index: req.index,
        key: req.key.clone(),
        image: img,
    })
}

// ── ペンディングセット ────────────────────────────────────────────────────────

/// 現在リクエスト中のキーを管理する（重複送信防止）。
pub struct PendingSet(HashSet<String>);

impl PendingSet {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
    pub fn insert(&mut self, key: String) {
        self.0.insert(key);
    }
    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
    pub fn contains(&self, key: &str) -> bool {
        self.0.contains(key)
    }
    pub fn retain_range(&mut self, lo: usize, hi: usize) {
        self.0.retain(|k| {
            let idx = key_to_index(k);
            idx >= lo && idx < hi
        });
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

// ── キーユーティリティ ────────────────────────────────────────────────────────

/// `"{index}:{entry_name}"` 形式のキーからインデックスを取り出す
pub fn key_to_index(key: &str) -> usize {
    key.split(':')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// テクスチャキャッシュキーを生成する
pub fn make_cache_key(index: usize, entry_name: &str) -> String {
    format!("{}:{}", index, entry_name)
}

// ── バックグラウンド結果をキャッシュへ反映 ───────────────────────────────────

/// ローダーから受信済みの結果を全て処理してキャッシュに登録する。
/// `expected_key_fn` は `(index) -> Option<String>` で、アーカイブ切替後の古い結果を
/// 捨てるために使う。
pub fn collect_results(
    loader: &Loader,
    cache: &mut TextureCache,
    pending: &mut PendingSet,
    linear_filter: bool,
    ctx: &Context,
    expected_key_fn: impl Fn(usize) -> Option<String>,
) {
    while let Ok(result) = loader.rx.try_recv() {
        pending.remove(&result.key);

        // アーカイブが切り替わっていたら破棄
        if expected_key_fn(result.index).as_deref() != Some(&result.key) {
            continue;
        }

        let size = [result.image.width() as usize, result.image.height() as usize];
        let ci = ColorImage::from_rgba_unmultiplied(size, &result.image.into_raw());
        let filter = if linear_filter {
            TextureOptions::LINEAR
        } else {
            TextureOptions::NEAREST
        };
        let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
        cache.insert(result.key, tex);
        ctx.request_repaint();
    }
}
