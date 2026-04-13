use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;

pub fn is_image_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        Path::new(&lower).extension().and_then(|e| e.to_str()),
        Some("jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff" | "tif")
    )
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

pub fn list_images(path: &Path) -> Result<Vec<String>> {
    let kind = detect_kind(path);
    let mut names: Vec<String> = match kind {
        ArchiveKind::Zip => list_zip(path)?,
        ArchiveKind::SevenZ => list_7z(path)?,
        ArchiveKind::Plain => list_plain(path)?,
    };
    names.sort_by(|a, b| natord(a, b));
    Ok(names)
}

pub fn read_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let kind = detect_kind(archive_path);
    match kind {
        ArchiveKind::Zip => read_zip(archive_path, entry_name),
        ArchiveKind::SevenZ => read_7z(archive_path, entry_name),
        ArchiveKind::Plain => {
            if archive_path.is_file() {
                Ok(std::fs::read(archive_path)?)
            } else {
                Ok(std::fs::read(archive_path.join(entry_name))?)
            }
        }
    }
}

// ── ZIP ──────────────────────────────────────────────────────────────────────

fn list_zip(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if is_image_ext(&name) {
            names.push(name);
        }
    }
    Ok(names)
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

fn list_7z(path: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    sevenz_rust::decompress_file_with_extract_fn(path, Path::new("."), |entry, _reader, _dest| {
        let name = entry.name().to_string();
        if is_image_ext(&name) {
            names.push(name);
        }
        Ok(false)
    })
    .map_err(|e| anyhow!("7z error: {e}"))?;
    Ok(names)
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

fn list_plain(path: &Path) -> Result<Vec<String>> {
    if path.is_file() {
        let dir = path.parent().ok_or_else(|| anyhow!("No parent dir"))?;
        return list_plain(dir);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let e = entry?;
        let name = e.file_name().to_string_lossy().to_string();
        if is_image_ext(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

// ── 自然順ソート ─────────────────────────────────────────────────────────────

fn natord(a: &str, b: &str) -> std::cmp::Ordering {
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
