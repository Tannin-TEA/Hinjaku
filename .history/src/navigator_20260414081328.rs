use crate::archive;
use crate::config::{Config, SortMode, SortOrder};
use crate::loader::Rotation;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Navigator {
    pub archive_path: Option<PathBuf>,
    pub entries: Vec<String>,
    pub entries_meta: Vec<archive::ImageEntry>,
    pub current: usize,
    pub target_index: usize,
    pub nav_items: Vec<PathBuf>,
    pub rotations: HashMap<String, Rotation>,
    pub open_from_end: bool,
    /// 現在のディレクトリ/アーカイブが有効な状態か
    pub is_ready: bool,
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            archive_path: None, entries: Vec::new(), entries_meta: Vec::new(),
            current: 0, target_index: 0, nav_items: Vec::new(),
            rotations: HashMap::new(), open_from_end: false,
            is_ready: false,
        }
    }

    #[allow(dead_code)] // Currently unused, but potentially useful for external access or future features
    pub fn get_current_key(&self) -> Option<String> {
        self.entries.get(self.current).map(|e| format!("{}:{}", self.current, e))
    }

    #[allow(dead_code)] // Currently unused, but potentially useful for external access or future features
    pub fn get_target_key(&self) -> Option<String> {
        self.entries.get(self.target_index).map(|e| format!("{}:{}", self.target_index, e))
    }

    pub fn apply_sorting(&mut self, config: &Config) {
        if self.entries_meta.is_empty() { return; }
        let current_name = self.entries.get(self.current).cloned();

        self.entries_meta.sort_by(|a, b| {
            let res = match config.sort_mode {
                SortMode::Name => {
                    if config.sort_natural { archive::natord(&a.name, &b.name) } 
                    else { a.name.cmp(&b.name) }
                }
                SortMode::Mtime => a.mtime.cmp(&b.mtime),
                SortMode::Size => a.size.cmp(&b.size),
            };
            if config.sort_order == SortOrder::Descending { res.reverse() } else { res }
        });

        self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
        if let Some(name) = current_name {
            if let Some(pos) = self.entries.iter().position(|n| n == &name) {
                self.current = pos;
                self.target_index = pos;
            }
        }
    }

    pub fn open_path(&mut self, path: PathBuf, config: &Config) {
        self.rotations.clear();
        let (base_path, start_name) = if path.is_file() && archive::is_image_ext(&path.to_string_lossy()) {
            (path.parent().unwrap().to_path_buf(), Some(path.file_name().unwrap().to_string_lossy().to_string()))
        } else { (path, None) };

        if let Ok(entries) = archive::list_images(&base_path) {
            self.entries_meta = entries;
            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
            self.apply_sorting(config);
            
            if let Some(ref name) = start_name {
                self.current = self.entries.iter().position(|n| n.contains(name)).unwrap_or(0);
            } else { self.current = 0; }
            self.target_index = self.current;
            self.archive_path = Some(base_path);
        }
    }

    pub fn go_next(&mut self, manga_mode: bool, manga_shift: bool, mut is_spread: impl FnMut(usize, &str) -> bool) -> bool {
        if self.entries.is_empty() { return false; }
        let step = if manga_mode {
            if self.target_index + 1 >= self.entries.len() { 1 }
            else if !manga_shift && self.target_index == 0 { 1 }
            else { // Check if current or next image is a spread
                let s1 = is_spread(self.target_index, &self.entries[self.target_index]); // Pass name for key generation
                let s2 = if self.target_index + 1 < self.entries.len() {
                    is_spread(self.target_index + 1, &self.entries[self.target_index + 1])
                } else { false };
                if s1 || s2 { 1 } else { 2 } // If either is spread, move by 1, else by 2
            }
        } else { 1 };

        if self.target_index + step < self.entries.len() {
            self.target_index += step; true
        } else { false }
    }

    pub fn go_prev(&mut self, manga_mode: bool, manga_shift: bool, mut is_spread: impl FnMut(usize, &str) -> bool) -> bool {
        if self.target_index == 0 { return false; }
        let step = if manga_mode {
            let first_pair_idx = if manga_shift { 0 } else { 1 };
            if self.target_index <= first_pair_idx || self.target_index < 2 { 1 }
            else {
                let s1 = is_spread(self.target_index - 1, &self.entries[self.target_index - 1]); // Pass name for key generation
                let s2 = if self.target_index >= 2 { // Ensure index is valid for s2
                    is_spread(self.target_index - 2, &self.entries[self.target_index - 2])
                } else { false };
                if s1 || s2 { 1 } else { 2 } // If either is spread, move by 1, else by 2
            }
        } else { 1 };
        self.target_index = self.target_index.saturating_sub(step);
        true
    }

    /// 現在表示しているファイルまたはアーカイブ内のエントリの、外部アプリ用フルパスを構築する
    pub fn get_current_full_path(&self) -> Option<String> {
        let path = self.archive_path.as_ref()?;
        let entries = &self.entries;
        if entries.is_empty() { return None; }
        let entry = &entries[self.current];

        let combined = if path.is_dir() {
            path.join(entry).to_string_lossy().to_string()
        } else {
            let base = path.to_string_lossy();
            format!("{}\\{}", base.trim_end_matches(|c| c == '\\' || c == '/'), entry.trim_start_matches(|c| c == '\\' || c == '/'))
        };
        Some(combined.replace('/', "\\").trim().trim_end_matches('\\').to_string())
    }

    /// 現在の場所から「次」または「前」のフォルダ/アーカイブを探す。
    /// 現在の階層に行き止まれば、親階層に遡って隣を探す（hoge1/zip の次は hoge2）。
    pub fn find_neighbor(&self, forward: bool) -> Option<(PathBuf, PathBuf)> {
        let mut current = self.archive_path.as_ref()?.clone();
        loop {
            let parent = current.parent()?;
            let targets = archive::list_nav_targets(parent).ok()?;
            let pos = targets.iter().position(|p| p == &current)?;
            
            let next_idx = if forward { pos + 1 } else { pos.checked_sub(1)? };
            if let Some(next) = targets.get(next_idx) {
                return Some((next.clone(), current)); // (移動先, どこから来たか)
            }
            // この階層に次がないので、一つ上の階層へ（hoge1 自体の次を hoge0 で探す）
            current = parent.to_path_buf();
            if current.parent().is_none() { break; }
        }
        None
    }

    pub fn sibling_dirs(&self) -> Option<(Vec<PathBuf>, usize)> {
        let path = self.archive_path.as_ref()?;
        let parent = path.parent()?;
        let targets = archive::list_nav_targets(parent).ok()?;
        let idx = targets.iter().position(|p| p == path)?;
        Some((targets, idx))
    }

    pub fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, config: &Config, manga_mode: bool, manga_shift: bool) {
        self.rotations.clear();
        if let Ok(entries) = archive::list_images(&path) {
            self.entries_meta = entries;
            self.nav_items.clear();
            
            // 実ディレクトリの場合のみ、その中のサブターゲットを検索する。
            // アーカイブファイルの場合は、OS上のディレクトリではないため中身を走査しない。
            if self.entries_meta.is_empty() && path.is_dir() {
                if let Ok(targets) = archive::list_nav_targets(&path) {
                    self.nav_items = targets;
                }
            }
            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();

            // 画像が存在する場合のみソートとインデックス構築を行う
            if !self.entries.is_empty() {
                self.apply_sorting(config);
            }

            self.current = if go_last && !self.entries.is_empty() {
                let last_idx = self.entries.len().saturating_sub(1);
                if manga_mode && last_idx > 0 {
                    let is_pair_start = if manga_shift { last_idx % 2 == 0 } else { last_idx % 2 != 0 };
                    if is_pair_start { last_idx } else { last_idx.saturating_sub(1) }
                } else { last_idx }
            } else if let Some(hint) = focus_hint {
                // 「どこから来たか」のヒントがある場合、そのフォルダ/ファイルの位置を探す
                if !self.entries.is_empty() {
                    let hint_name = hint.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    self.entries.iter().position(|n| n.contains(&hint_name)).unwrap_or(0)
                } else {
                    self.nav_items.iter().position(|p| p == &hint).unwrap_or(0)
                }
            } else { 0 };

            self.target_index = self.current;
            self.archive_path = Some(path);
        }
    }
}