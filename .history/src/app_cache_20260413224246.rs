use crate::types::{CacheEntry, LoadRequest, Rotation, CACHE_MAX, MAX_TEX_DIM, PREFETCH_AHEAD, PREFETCH_BEHIND};
use crate::App;
use eframe::egui::{self, ColorImage, TextureOptions};

impl App {
    // ── キャッシュキー ────────────────────────────────────────────────────
    pub fn cache_key(&self, index: usize) -> Option<String> {
        self.entries.get(index).map(|e| format!("{}:{}", index, e))
    }

    // ── キャッシュ追加（LRU管理・上限超えたら古いものを削除） ──────────────
    pub fn cache_insert(&mut self, key: String, entry: CacheEntry) {
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
    pub fn request_load(&mut self, index: usize) {
        let path = match &self.archive_path {
            Some(p) => p.clone(),
            None => return,
        };
        let key = match self.cache_key(index) {
            Some(k) => k,
            None => return,
        };

        // キャッシュ済み or リクエスト中ならスキップ
        if self.cache.contains_key(&key) {
            return;
        }
        if self.pending.contains(&key) {
            return;
        }

        let entry_name = self.entries[index].clone();
        let rotation = self.rotations
            .get(&entry_name)
            .copied()
            .unwrap_or(Rotation::R0);

        self.pending.insert(key.clone());
        let _ = self.load_tx.send(LoadRequest {
            index,
            key,
            archive_path: path,
            entry_name,
            rotation,
            max_dim: MAX_TEX_DIM,
            linear_filter: self.config.linear_filter,
        });
    }

    // ── 受信済み結果をキャッシュに反映 ───────────────────────────────────
    pub fn collect_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.load_rx.try_recv() {
            self.pending.remove(&result.key);

            // アーカイブが切り替わっていたら捨てる
            let expected_key = self.cache_key(result.index);
            if expected_key.as_deref() != Some(&result.key) {
                continue;
            }

            let size = [
                result.image.width() as usize,
                result.image.height() as usize,
            ];
            let ci = ColorImage::from_rgba_unmultiplied(size, &result.image.into_raw());

            // 回転は load 時に焼き込み済み
            let filter = if self.config.linear_filter {
                TextureOptions::LINEAR
            } else {
                TextureOptions::NEAREST
            };
            let tex = ctx.load_texture(format!("img_{}", result.index), ci, filter);
            self.cache_insert(result.key, CacheEntry { texture: tex });
            ctx.request_repaint();
        }
    }

    // ── 先読みリクエストを発行 ────────────────────────────────────────────
    pub fn schedule_prefetch(&mut self) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }

        // 共有インデックスを「目標位置」で更新
        self.current_idx_shared
            .store(self.target_index, std::sync::atomic::Ordering::Relaxed);

        // 目標位置を中心に先読み範囲を計算
        let lo = self.target_index.saturating_sub(PREFETCH_BEHIND);
        let hi = (self.target_index + PREFETCH_AHEAD + 1).min(len);

        // 今まさに必要なページを最優先でリクエスト
        self.request_load(self.target_index);
        if self.manga_mode && self.target_index + 1 < len {
            self.request_load(self.target_index + 1);
        }

        // その他の範囲内を予約
        for i in lo..hi {
            self.request_load(i);
        }

        // 範囲外のキャッシュとペンディングを掃除（メモリ節約と再要求の許可）
        self.cache.retain(|k, _| {
            let idx = k
                .split(':')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            idx >= lo && idx < hi
        });
        self.cache_lru.retain(|k| self.cache.contains_key(k));
        self.pending.retain(|k| {
            let idx = k
                .split(':')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            idx >= lo && idx < hi
        });
    }

    // ── テクスチャ取得（キャッシュから） ──────────────────────────────────
    pub fn get_texture(&self, index: usize) -> Option<&eframe::egui::TextureHandle> {
        let key = self.cache_key(index)?;
        self.cache.get(&key).map(|e| &e.texture)
    }

    // ── 見開き判定（横長なら true） ──────────────────────────────────────
    pub fn is_spread(&self, index: usize) -> bool {
        if let Some(tex) = self.get_texture(index) {
            let sz = tex.size_vec2();
            sz.x > sz.y
        } else {
            false
        }
    }
}
