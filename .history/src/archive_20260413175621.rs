use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;

pub fn is_image_ext(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff" | "tif")
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub name: String,
    pub mtime: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
    Plain,
}

pub fn detect_kind(path: &Path) -> ArchiveKind {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
        Some("zip") => ArchiveKind::Zip,
        Some("7z") => ArchiveKind::SevenZ,
        _ => ArchiveKind::Plain,
    }
}

pub fn list_images(path: &Path) -> Result<Vec<ImageEntry>> {
    let kind = detect_kind(path);
    let entries = match kind {
        ArchiveKind::Zip => list_zip(path)?,
        ArchiveKind::SevenZ => list_7z(path)?,
        ArchiveKind::Plain => list_plain(path)?,
    };
    Ok(entries)
}

pub fn read_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    match detect_kind(archive_path) {
        ArchiveKind::Zip => read_zip(archive_path, entry_name),
        ArchiveKind::SevenZ => read_7z(archive_path, entry_name),
        ArchiveKind::Plain => {
            let target = if archive_path.is_file() { archive_path.to_path_buf() } 
                         else { archive_path.join(entry_name) };
            Ok(std::fs::read(target)?)
        }
    }
}

// ── ZIP ──────────────────────────────────────────────────────────────────────

fn list_zip(path: &Path) -> Result<Vec<ImageEntry>> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if is_image_ext(&name) {
            // zip の日時を Unix タイムスタンプに変換
            let mtime = entry.last_modified().and_then(|t| {
                chrono::NaiveDate::from_ymd_opt(t.year() as i32, t.month() as u32, t.day() as u32)
                    .and_then(|d| d.and_hms_opt(t.hour() as u32, t.minute() as u32, t.second() as u32))
                    .map(|dt| dt.and_utc().timestamp() as u64)
            }).unwrap_or(0);

            entries.push(ImageEntry { name, mtime, size: entry.size() });
        }
    }
    Ok(entries)
}

fn read_zip(path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut entry = zip.by_name(entry_name)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

// ── 7z ───────────────────────────────────────────────────────────────────────

fn list_7z(path: &Path) -> Result<Vec<ImageEntry>> {
    let mut entries = Vec::new();
    sevenz_rust::decompress_file_with_extract_fn(path, Path::new("."), |entry, _reader, _dest| {
        let name = entry.name().to_string();
        if is_image_ext(&name) {
            entries.push(ImageEntry { 
                name, 
                // NT FileTime (100ns intervals since 1601) を Unix 秒に変換
                mtime: {
                    let ticks: u64 = entry.last_modified_date.into();
                    (ticks / 10_000_000).saturating_sub(11_644_473_600)
                },
                size: entry.size
            });
        }
        Ok(false)
    })
    .map_err(|e| anyhow!("7z error: {e}"))?;
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
    if path.is_file() {
        let dir = path.parent().ok_or_else(|| anyhow!("No parent dir"))?;
        return list_plain(dir);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let e = entry?;
        let name_str = e.file_name().to_string_lossy().to_string();
        if is_image_ext(&name_str) {
            let meta = e.metadata()?;
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH)
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            entries.push(ImageEntry { name: name_str, mtime, size: meta.len() });
        }
    }
    Ok(entries)
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
    Path::new(s).file_name().and_then(|f| f.to_str()).unwrap_or(s)
}
