use anyhow::{anyhow, Result};
use std::io::Read;
use std::io::BufReader;
use std::path::Path;

/// 拡張子が画像かどうかを判定する
pub fn is_image_ext(name: &str) -> bool {
    let Some(pos) = name.rfind('.') else { return false };
    let ext = &name[pos + 1..];
    // eq_ignore_ascii_case を使って String 生成を完全に排除
    ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") ||
    ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("webp") ||
    ext.eq_ignore_ascii_case("gif") || ext.eq_ignore_ascii_case("bmp") ||
    ext.eq_ignore_ascii_case("tiff") || ext.eq_ignore_ascii_case("tif")
}

/// パスの区切り文字を Windows 形式に統一し、比較を確実にする
pub fn clean_path(path: &Path) -> std::path::PathBuf {
    // RustのPathBufはWindows上で / と \ を同一視するが、
    // 表示の一貫性とドライブレターの扱いのために最低限の正規化を行う
    let mut p = std::path::PathBuf::from(path.to_string_lossy().replace('/', "\\"));
    if p.to_string_lossy().ends_with(':') {
        p = std::path::PathBuf::from(format!("{}\\", p.display()));
    }
    p
}

/// 表示用にパスからファイル名（またはルート名）を抽出する
pub fn get_display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// アーカイブのパスとエントリ名を結合して、OS標準の形式（Windowsなら \）で返す
pub fn join_entry_path(archive_path: &Path, entry_name: &str) -> String {
    if archive_path.is_dir() {
        archive_path.join(entry_name).to_string_lossy().into_owned()
    } else {
        // アーカイブ内のパス表示用
        format!("{}\\{}", archive_path.display(), entry_name.replace('/', "\\"))
    }
}

/// ファイルをエクスプローラーで選択した状態で開く (Windows専用)
#[cfg(target_os = "windows")]
pub fn reveal_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub name: String,
    pub mtime: u64,
    pub size: u64,
    pub archive_index: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum NavEntry {
    Directory(std::path::PathBuf),
    Archive(std::path::PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
    Plain,
}

pub fn detect_kind(path: &Path) -> ArchiveKind {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return ArchiveKind::Plain; };
    if ext.eq_ignore_ascii_case("zip") {
        ArchiveKind::Zip
    } else if ext.eq_ignore_ascii_case("7z") {
        ArchiveKind::SevenZ
    } else {
        ArchiveKind::Plain
    }
}

pub fn list_images(path: &Path) -> Result<Vec<ImageEntry>> {
    match detect_kind(path) {
        ArchiveKind::Zip => list_zip(path),
        ArchiveKind::SevenZ => list_7z(path),
        ArchiveKind::Plain => list_plain(path),
    }
}

pub fn read_entry(archive_path: &Path, entry_name: &str, entry_index: Option<usize>) -> Result<Vec<u8>> {
    match detect_kind(archive_path) {
        ArchiveKind::Zip => read_zip(archive_path, entry_name, entry_index),
        ArchiveKind::SevenZ => read_7z(archive_path, entry_name),
        ArchiveKind::Plain => {
            let target = if archive_path.is_file() {
                archive_path.to_path_buf()
            } else {
                archive_path.join(entry_name)
            };
            Ok(std::fs::read(target)?)
        }
    }
}

// ── ZIP ──────────────────────────────────────────────────────────────────────

fn list_zip(path: &Path) -> Result<Vec<ImageEntry>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader)?;

    // 1. file_names() を使って画像ファイルだけのインデックスを先に抽出する。
    // 中央ディレクトリの文字列参照だけで判定するため、画像以外のファイルに対して
    // メタデータ解析（by_index）を行うコストを完全にスキップできます。
    let image_indices: Vec<usize> = zip.file_names()
        .enumerate()
        .filter(|(_, name)| is_image_ext(name))
        .map(|(i, _)| i)
        .collect();

    let mut entries = Vec::with_capacity(image_indices.len());
    for i in image_indices {
        let entry = zip.by_index(i)?;
            let mtime = entry
                .last_modified()
                .and_then(|t| {
                    chrono::NaiveDate::from_ymd_opt(
                        t.year() as i32,
                        t.month() as u32,
                        t.day() as u32,
                    )
                    .and_then(|d| {
                        d.and_hms_opt(t.hour() as u32, t.minute() as u32, t.second() as u32)
                    })
                    .map(|dt| dt.and_utc().timestamp() as u64)
                })
                .unwrap_or(0);
            entries.push(ImageEntry {
                name: entry.name().to_string(), // 画像確定後のみ String を作成
                mtime,
                size: entry.size(),
                archive_index: i,
            });
    }
    Ok(entries)
}

fn read_zip(path: &Path, entry_name: &str, entry_index: Option<usize>) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader)?;
    let mut entry = if let Some(idx) = entry_index {
        zip.by_index(idx)?
    } else {
        zip.by_name(entry_name)?
    };
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

// ── 7z ───────────────────────────────────────────────────────────────────────

fn list_7z(path: &Path) -> Result<Vec<ImageEntry>> {
    let mut entries = Vec::new();
    let file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    
    let archive = sevenz_rust::Archive::read(&mut reader, len, &[])
        .map_err(|e| anyhow!("7z read error: {e}"))?;

    for entry in &archive.files {
        let name = entry.name().to_string();
        if !entry.is_directory() && is_image_ext(&name) {
            let mtime = (u64::from(entry.last_modified_date) / 10_000_000)
                .saturating_sub(11_644_473_600);
            entries.push(ImageEntry {
                name,
                mtime,
                size: entry.size,
                archive_index: 0,
            });
        }
    }
    Ok(entries)
}

fn read_7z(path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let mut result: Option<Vec<u8>> = None;
    sevenz_rust::decompress_file_with_extract_fn(path, Path::new("."), |entry, reader, _dest| {
        if entry.name() == entry_name {
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            result = Some(buf);
        }
        Ok(false)
    })
    .map_err(|e| anyhow!("7z read error: {e}"))?;
    result.ok_or_else(|| anyhow!("Entry not found: {entry_name}"))
}

// ── Plain folder / single file ───────────────────────────────────────────────

fn list_plain(path: &Path) -> Result<Vec<ImageEntry>> {
    // 単一ファイルが渡された場合は親ディレクトリのリストを返す
    if path.is_file() {
        let dir = path.parent().ok_or_else(|| anyhow!("No parent dir"))?;
        return list_plain(dir);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let e = entry?;
        let name_os = e.file_name();
        // ここでも String 生成を避け、参照 (&str) で判定。
        let Some(name_str) = name_os.to_str() else { continue; };

        if is_image_ext(name_str) {
            let meta = e.metadata()?;
            let mtime = meta
                .modified()
                .unwrap_or(std::time::UNIX_EPOCH)
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entries.push(ImageEntry {
                name: name_str.to_string(),
                mtime,
                size: meta.len(),
                archive_index: 0,
            });
        }
    }
    Ok(entries)
}

/// ナビゲーション用に、指定パス内のサブディレクトリとアーカイブをリストアップする
pub fn list_nav_targets(path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut targets = Vec::new();
    if !path.is_dir() {
        return Ok(targets);
    }
    for entry in std::fs::read_dir(path)? {
        let e = entry?;
        let p = e.path();
        if p.is_dir() || matches!(detect_kind(&p), ArchiveKind::Zip | ArchiveKind::SevenZ) {
            targets.push(clean_path(&p));
        }
    }
    targets.sort_by(|a, b| natord(&a.to_string_lossy(), &b.to_string_lossy()));
    Ok(targets)
}

/// システムのルート（Windowsならドライブ一覧、Unixなら /）を取得する
pub fn get_roots() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        for c in b'A'..=b'Z' {
            let p = std::path::PathBuf::from(format!("{}:\\", c as char));
            if p.exists() { roots.push(clean_path(&p)); }
        }
        roots
    }
    #[cfg(not(windows))]
    {
        vec![std::path::PathBuf::from("/")]
    }
}

// ── 自然順ソート ─────────────────────────────────────────────────────────────

pub fn natord(a: &str, b: &str) -> std::cmp::Ordering {
    let a = basename(a);
    let b = basename(b);
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let na: u64 = consume_num(&mut ai);
                let nb: u64 = consume_num(&mut bi);
                match na.cmp(&nb) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
            _ => {
                let ac = ai.next().unwrap().to_lowercase().next().unwrap();
                let bc = bi.next().unwrap().to_lowercase().next().unwrap();
                match ac.cmp(&bc) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
        }
    }
}

fn consume_num(iter: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut s = String::new();
    while iter.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s.push(iter.next().unwrap());
    }
    s.parse().unwrap_or(0)
}

fn basename(s: &str) -> &str {
    Path::new(s)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(s)
}
