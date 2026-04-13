use crate::archive;
use eframe::egui::TextureHandle;
use std::path::PathBuf;

// ── 定数 ────────────────────────────────────────────────────────────────────
/// キャッシュ保持上限（前後5枚 + 現在 = 最大11、余裕を持って13）
pub const CACHE_MAX: usize = 13;
/// 先読み範囲
pub const PREFETCH_AHEAD: usize = 5;
pub const PREFETCH_BEHIND: usize = 5;
/// 表示解像度上限（これ以上は縮小してからテクスチャ化）
pub const MAX_TEX_DIM: u32 = 4096; // 4K解像度まではリサイズせずGPUに委ねる

// ── アーカイブリスト取得の結果 ─────────────────────────────────────────────
pub struct ListResult {
    pub path: PathBuf,
    pub entries: Vec<archive::ImageEntry>,
    pub start_name: Option<String>,
    pub go_last: bool,
    pub error: Option<String>,
}

// ── 回転 ────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn cw(self) -> Self {
        match self {
            Self::R0 => Self::R90,
            Self::R90 => Self::R180,
            Self::R180 => Self::R270,
            Self::R270 => Self::R0,
        }
    }

    pub fn ccw(self) -> Self {
        match self {
            Self::R0 => Self::R270,
            Self::R90 => Self::R0,
            Self::R180 => Self::R90,
            Self::R270 => Self::R180,
        }
    }
}

// ── バックグラウンドロードの結果 ─────────────────────────────────────────────
pub struct LoadResult {
    pub index: usize,
    /// エントリキー（キャッシュの照合用）
    pub key: String,
    pub image: image::RgbaImage,
}

// ── デコードワーカーへのリクエスト ───────────────────────────────────────────
pub struct LoadRequest {
    pub index: usize,
    pub key: String,
    pub archive_path: PathBuf,
    pub entry_name: String,
    pub rotation: Rotation,
    pub max_dim: u32,
    pub linear_filter: bool,
}

// ── キャッシュエントリ ───────────────────────────────────────────────────────
pub struct CacheEntry {
    pub texture: TextureHandle,
}
