use crate::archive;
use std::path::PathBuf;

// ── ページ送りステップ計算 ────────────────────────────────────────────────────

/// マンガモードでの「次へ」ステップを計算する（1 または 2）。
/// `is_spread(index)` はそのインデックスが横長（見開き）かを返す関数。
pub fn next_step(
    manga_mode: bool,
    manga_shift: bool,
    entries_len: usize,
    current: usize,
    is_spread: impl Fn(usize) -> bool,
) -> usize {
    if !manga_mode {
        return 1;
    }
    if current + 1 >= entries_len {
        return 1;
    }
    if !manga_shift && current == 0 {
        return 1;
    }
    if is_spread(current) || is_spread(current + 1) {
        1
    } else {
        2
    }
}

/// マンガモードでの「前へ」ステップを計算する（1 または 2）
pub fn prev_step(
    manga_mode: bool,
    manga_shift: bool,
    current: usize,
    is_spread: impl Fn(usize) -> bool,
) -> usize {
    if !manga_mode {
        return 1;
    }
    let first_pair_idx = if manga_shift { 0 } else { 1 };
    if current <= first_pair_idx || current < 2 {
        return 1;
    }
    let prev_is_spread = is_spread(current.saturating_sub(1));
    let prev_prev_is_spread = is_spread(current.saturating_sub(2));
    if prev_is_spread || prev_prev_is_spread {
        1
    } else {
        2
    }
}

// ── 兄弟ディレクトリ列挙 ──────────────────────────────────────────────────────

/// 現在のアーカイブと同じ親を持つ兄弟ディレクトリ・アーカイブの一覧と、
/// 現在のインデックスを返す。
pub fn sibling_dirs(archive_path: &PathBuf) -> Option<(Vec<PathBuf>, usize)> {
    let parent = archive_path.parent()?;
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                || matches!(
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_lowercase())
                        .as_deref(),
                    Some("zip" | "7z")
                )
        })
        .collect();
    siblings.sort_by(|a, b| {
        archive::natord(&a.to_string_lossy(), &b.to_string_lossy())
    });
    let idx = siblings.iter().position(|p| p == archive_path)?;
    Some((siblings, idx))
}

// ── 末尾インデックス計算 ──────────────────────────────────────────────────────

/// 末尾ページへ移動する際の開始インデックスを計算する。
/// マンガモード時は見開きが崩れないよう調整する。
pub fn last_page_index(len: usize, manga_mode: bool, manga_shift: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len.saturating_sub(1);
    if manga_mode && last > 0 {
        let is_pair_start = if manga_shift {
            last % 2 == 0
        } else {
            last % 2 != 0
        };
        if is_pair_start {
            last
        } else {
            last.saturating_sub(1)
        }
    } else {
        last
    }
}
