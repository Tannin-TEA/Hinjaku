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
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            archive_path: None, entries: Vec::new(), entries_meta: Vec::new(),
            current: 0, target_index: 0, nav_items: Vec::new(),
            rotations: HashMap::new(), open_from_end: false,
        }
    }

    pub fn get_current_key(&self) -> Option<String> {
        self.entries.get(self.current).map(|e| format!("{}:{}", self.current, e))
    }

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
            else {
                let s1 = is_spread(self.target_index, &self.entries[self.target_index]);
                let s2 = is_spread(self.target_index + 1, &self.entries[self.target_index + 1]);
                if s1 || s2 { 1 } else { 2 }
            }
        } else { 1 };

        if self.target_index + step < self.entries.len() {
            self.target_index += step; true
        } else { false }
    }

    pub fn go_prev(&mut self, manga_mode: bool, manga_shift: bool, is_spread: impl Fn(usize) -> bool) -> bool {
        if self.target_index == 0 { return false; }
        let step = if manga_mode {
            let first_pair_idx = if manga_shift { 0 } else { 1 };
            if self.target_index <= first_pair_idx || self.target_index < 2 { 1 }
            else {
                if is_spread(self.target_index - 1) || is_spread(self.target_index - 2) { 1 } else { 2 }
            }
        } else { 1 };
        self.target_index = self.target_index.saturating_sub(step);
        true
    }

    pub fn sibling_dirs(&self) -> Option<(Vec<PathBuf>, usize)> {
        let path = self.archive_path.as_ref()?;
        let parent = path.parent()?;
        let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent).ok()?
            .filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.is_dir() || matches!(archive::detect_kind(p), archive::ArchiveKind::Zip | archive::ArchiveKind::SevenZ))
            .collect();
        siblings.sort_by(|a, b| archive::natord(&a.to_string_lossy(), &b.to_string_lossy()));
        let idx = siblings.iter().position(|p| p == path)?;
        Some((siblings, idx))
    }

    pub fn move_to_dir(&mut self, path: PathBuf, go_last: bool, config: &Config, manga_mode: bool, manga_shift: bool) {
        self.rotations.clear();
        if let Ok(entries) = archive::list_images(&path) {
            self.entries_meta = entries;
            if self.entries_meta.is_empty() {
                if let Ok(targets) = archive::list_nav_targets(&path) {
                    self.nav_items = targets;
                }
            }
            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();
            self.apply_sorting(config);

            self.current = if go_last && !self.entries.is_empty() {
                let last_idx = self.entries.len().saturating_sub(1);
                if manga_mode && last_idx > 0 {
                    let is_pair_start = if manga_shift { last_idx % 2 == 0 } else { last_idx % 2 != 0 };
                    if is_pair_start { last_idx } else { last_idx.saturating_sub(1) }
                } else { last_idx }
            } else { 0 };

            self.target_index = self.current;
            self.archive_path = Some(path);
        }
    }
}