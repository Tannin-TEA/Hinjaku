use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::archive::ArchiveReader;
use crate::utils;

pub struct NavTree {
    pub nodes: HashMap<PathBuf, Vec<PathBuf>>,
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub image_counts: HashMap<PathBuf, usize>,
    pub scroll_to_selected: bool,
    archive_reader: Arc<dyn ArchiveReader>,
}

impl NavTree {
    pub fn new(archive_reader: Arc<dyn ArchiveReader>) -> Self {
        Self {
            nodes: HashMap::new(),
            expanded: HashSet::new(),
            selected: None,
            image_counts: HashMap::new(),
            scroll_to_selected: false,
            archive_reader,
        }
    }

    /// ツリーのキャッシュ（ノードリストと画像数）をクリアする。
    /// メモリ使用量が気になる場合や、ドライブを跨ぐ移動時に呼ぶ。
    pub fn clear_metadata_cache(&mut self) {
        self.nodes.clear();
        self.image_counts.clear();
    }

    pub fn get_roots(&self) -> Vec<PathBuf> {
        self.archive_reader.get_roots()
    }

    pub fn get_image_count(&mut self, path: &Path) -> usize {
        let path = utils::clean_path(path);
        if let Some(&count) = self.image_counts.get(&path) { return count; }
        let count = self.archive_reader.list_images(&path).map(|e| e.len()).unwrap_or(0);
        self.image_counts.insert(path, count);
        count
    }

    pub fn get_children(&mut self, dir_path: &Path) -> Vec<PathBuf> {
        let dir_path = utils::clean_path(dir_path);
        if let Some(cached) = self.nodes.get(&dir_path) { return cached.clone(); }
        let targets = self.archive_reader.list_nav_targets(&dir_path).unwrap_or_default();
        self.nodes.insert(dir_path, targets.clone());
        targets
    }

    pub fn get_siblings(&mut self, path: &Path) -> Vec<PathBuf> {
        if let Some(p) = path.parent() {
            self.get_children(p)
        } else {
            self.archive_reader.get_roots()
        }
    }

    pub fn expand_to_path(&mut self, path: &Path) {
        let mut curr = Some(utils::clean_path(path));
        while let Some(p) = curr {
            if let Some(parent) = p.parent() {
                self.expanded.insert(parent.to_path_buf());
                curr = Some(parent.to_path_buf());
            } else {
                break;
            }
        }
    }

    pub fn get_relative_target(&mut self, current: &Path, forward: bool) -> Option<PathBuf> {
        let curr = utils::clean_path(current);
        let siblings = self.get_siblings(&curr);
        let pos = siblings.iter().position(|p| p == &curr)?;
        let next_pos = if forward { pos + 1 } else { pos.checked_sub(1)? };
        siblings.get(next_pos).cloned()
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.selected.is_none() {
            if let Some(first_root) = self.archive_reader.get_roots().first() {
                self.selected = Some(utils::clean_path(first_root));
                self.scroll_to_selected = true;
            }
            return;
        }

        if let Some(sel) = self.selected.clone() {
            let sel = utils::clean_path(&sel);
            let siblings = self.get_siblings(&sel);
            if let Some(pos) = siblings.iter().position(|p| p == &sel) {
                let new_pos = (pos as isize + delta).clamp(0, siblings.len() as isize - 1) as usize;
                self.selected = Some(utils::clean_path(&siblings[new_pos]));
                self.scroll_to_selected = true;
            }
        }
    }

    pub fn expand_current(&mut self) {
        if let Some(sel) = self.selected.clone() {
            let sel = utils::clean_path(&sel);
            if sel.is_dir() {
                self.expanded.insert(sel.clone());
                let children = self.get_children(&sel);
                if let Some(first) = children.first() {
                    self.selected = Some(utils::clean_path(first));
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    pub fn collapse_or_up(&mut self) {
        if let Some(sel) = self.selected.clone() {
            let sel = utils::clean_path(&sel);
            if self.expanded.contains(&sel) {
                self.expanded.remove(&sel);
            } else if let Some(parent) = sel.parent() {
                let p = utils::clean_path(parent);
                if p != sel {
                    self.selected = Some(p.clone());
                    self.expanded.remove(&p);
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    pub fn activate_current(&mut self) -> Option<PathBuf> {
        self.selected.clone()
    }

    /// 指定されたパスまでツリーを展開し、選択状態にしてスクロール要求を出す
    pub fn reveal_path(&mut self, path: &Path) {
        let cleaned = utils::clean_path(path);
        self.expand_to_path(&cleaned);
        self.selected = Some(cleaned);
        self.scroll_to_selected = true;
    }
}
