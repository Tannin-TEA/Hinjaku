use crate::archive;
use eframe::egui::TextureHandle;
use std::path::PathBuf;

pub const CACHE_MAX: usize = 13;
pub const PREFETCH_AHEAD: usize = 5;
pub const PREFETCH_BEHIND: usize = 5;
pub const MAX_TEX_DIM: u32 = 4096;

pub struct ListResult {
    pub path: PathBuf,
    pub entries: Vec<archive::ImageEntry>,
    pub start_name: Option<String>,
    pub go_last: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Rotation {
    R0, R90, R180, R270,
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

pub struct LoadResult {
    pub index: usize,
    pub key: String,
    pub image: image::RgbaImage,
}

pub struct LoadRequest {
    pub index: usize,
    pub key: String,
    pub archive_path: PathBuf,
    pub entry_name: String,
    pub rotation: Rotation,
    pub max_dim: u32,
    pub linear_filter: bool,
}

pub struct CacheEntry {
    pub texture: TextureHandle,
}
