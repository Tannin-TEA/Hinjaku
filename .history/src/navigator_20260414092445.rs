use crate::archive;
use crate::config::{Config, SortMode, SortOrder};
use crate::loader::Rotation;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// ツリー全体を管理する構造
#[derive(Default)]
pub struct NavTree {
    /// パスをキーとした各ディレクトリのキャッシュ
    pub nodes: HashMap<PathBuf, Vec<PathBuf>>,
    /// ツリー上で展開されているディレクトリのパス
    pub expanded: HashSet<PathBuf>,
    /// 現在ツリー上で選択（フォーカス）されているパス
    pub selected: Option<PathBuf>,
}

impl NavTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定したディレクトリ直下のターゲット（フォルダ・アーカイブ）リストを返す
    pub fn get_children(&mut self, dir_path: &std::path::Path) -> Vec<PathBuf> {
        if let Some(cached) = self.nodes.get(dir_path) {
            return cached.clone();
        }
        let targets = archive::list_nav_targets(dir_path).unwrap_or_default();
        self.nodes.insert(dir_path.to_path_buf(), targets.clone());
        targets
    }

    pub fn get_siblings(&mut self, path: &std::path::Path) -> Vec<PathBuf> {
        let Some(parent) = path.parent() else { return vec![] };
        self.get_children(parent)
    }

    /// 指定されたパスまでの親ディレクトリをすべて展開状態にする
    pub fn expand_to_path(&mut self, path: &Path) {
        let mut curr = path;
        while let Some(parent) = curr.parent() {
            self.expanded.insert(parent.to_path_buf());
            curr = parent;
        }
    }

    /// ツリー内での選択移動
    pub fn move_selection(&mut self, delta: isize) {
        let visible = self.get_visible_list();
        if visible.is_empty() { return; }
        
        let current_idx = self.selected.as_ref()
            .and_then(|p| visible.iter().position(|x| x == p))
            .unwrap_or(0);

        let next_idx = (current_idx as isize + delta).clamp(0, visible.len() as isize - 1) as usize;
        self.selected = Some(visible[next_idx].clone());
    }

    /// 右キーまたはEnter：展開、あるいは中身がない場合は何もせず
    pub fn expand_current(&mut self) {
        if let Some(p) = self.selected.clone() {
            if p.is_dir() && archive::detect_kind(&p) == archive::ArchiveKind::Plain {
                self.expanded.insert(p);
            }
        }
    }

    /// 左キー：折り畳み、あるいは親へ移動
    pub fn collapse_or_up(&mut self) {
        let Some(p) = self.selected.clone() else { return };
        if self.expanded.contains(&p) {
            self.expanded.remove(&p);
        } else if let Some(parent) = p.parent() {
            // ルート（ドライブ直下など）より上には行かない
            if archive::get_roots().iter().any(|r| r == &p) { return; }
            self.selected = Some(parent.to_path_buf());
        }
    }

    /// Enter：ディレクトリなら展開切替、アーカイブならパスを返して開く準備
    pub fn activate_current(&mut self) -> Option<PathBuf> {
        let p = self.selected.clone()?;
        let kind = archive::detect_kind(&p);
        if kind != archive::ArchiveKind::Plain {
            return Some(p);
        } else if p.is_dir() {
            if self.expanded.contains(&p) { self.expanded.remove(&p); }
            else { self.expanded.insert(p); }
        }
        None
    }

    /// 現在のパスから相対的な次のターゲット（フォルダ/アーカイブ）を返す
    pub fn get_relative_target(&mut self, current: &Path, forward: bool) -> Option<PathBuf> {
        let siblings = self.get_siblings(current);
        let pos = siblings.iter().position(|p| p == current)?;
        let next_pos = if forward { pos + 1 } else { pos.checked_sub(1)? };
        siblings.get(next_pos).cloned()
    }

    /// ツリー上で現在可視状態にあるパスのフラットなリストを返す
    pub fn get_visible_list(&mut self) -> Vec<PathBuf> {
        let mut list = Vec::new();
        let roots = archive::get_roots();
        for root in roots {
            self.fill_visible(root, &mut list);
        }
        list
    }

    fn fill_visible(&mut self, path: PathBuf, list: &mut Vec<PathBuf>) {
        list.push(path.clone());
        let kind = archive::detect_kind(&path);
        let is_archive = matches!(kind, archive::ArchiveKind::Zip | archive::ArchiveKind::SevenZ);
        
        if !is_archive && path.is_dir() && self.expanded.contains(&path) {
            let children = self.get_children(&path);
            for child in children {
                self.fill_visible(child, list);
            }
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.nodes.clear();
    }
}

pub struct Navigator {
    pub archive_path: Option<PathBuf>,
    pub entries: Vec<String>,
    pub entries_meta: Vec<archive::ImageEntry>,
    pub current: usize,
    pub target_index: usize,
    pub rotations: HashMap<String, Rotation>,
    pub open_from_end: bool,
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            archive_path: None, entries: Vec::new(), entries_meta: Vec::new(),
            current: 0, target_index: 0,
            rotations: HashMap::new(), open_from_end: false,
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

    pub fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, config: &Config, manga_mode: bool, manga_shift: bool) {
        self.rotations.clear();
        if let Ok(entries) = archive::list_images(&path) {
            self.entries_meta = entries;
            self.entries = self.entries_meta.iter().map(|e| e.name.clone()).collect();

            if !self.entries.is_empty() {
                self.apply_sorting(config);
            }

            if !self.entries.is_empty() {
                self.current = if go_last {
                    let last_idx = self.entries.len().saturating_sub(1);
                    if manga_mode && last_idx > 0 {
                        let is_pair_start = if manga_shift { last_idx % 2 == 0 } else { last_idx % 2 != 0 };
                        if is_pair_start { last_idx } else { last_idx.saturating_sub(1) }
                    } else { last_idx }
                } else if let Some(hint) = focus_hint {
                    let hint_name = hint.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    self.entries.iter().position(|n| n.contains(&hint_name)).unwrap_or(0)
                } else { 0 }
            } else {
                self.current = 0;
            }

            self.target_index = self.current;
            self.archive_path = Some(path);
        }
    }
}